//! 이동 트랜잭션과 커밋 전 수신 편집의 보존.

use super::{
    AppError, CHECKPOINT_LIMIT, Change, CloseReason, Command, CommittedMove, EditAuthorization,
    HashSet, Instant, MoveReply, MoveRequest, Outbound, ROOM_QUEUE, Room, documents, mpsc, oneshot,
};

impl Room {
    /// 문서를 옮긴다(`Command::Move`, 감사 S6·#71). 이 명령을 처리하는 동안 들어온 메시지는 대기열에서 기다린다(넣지도
    /// 확인하지도 않음). 옮겼으면 거짓(방을 닫는다).
    pub(super) async fn move_document(
        &mut self,
        request: MoveRequest,
        done: oneshot::Sender<Result<MoveReply, AppError>>,
        rx: &mut mpsc::Receiver<Command>,
    ) -> bool {
        // 기다리던 요청이 이미 끝났으면(연결이 끊김) 체크포인트 리비전도 남기지 않는다.
        if done.is_closed() {
            return true;
        }
        if let Err(err) = self.ensure_loaded(request.user).await {
            let _ignored = done.send(Err(err));
            return false;
        }
        self.flush().await;
        // 리비전으로 남기지 않은 편집을 먼저 남긴다. 남겼는지 확인되지 않으면 옮기지 않는다(그 편집은 방에 남는다).
        if self.dirty_since_snapshot
            && let Some(reason) = self.checkpoint(Change::Move, request.user).await
        {
            let _ignored = done.send(Err(AppError::Conflict(reason.into())));
            return true;
        }
        // 기다리던 요청이 그사이 끝났으면 옮기지 않는다.
        if done.is_closed() {
            return true;
        }
        // 커밋 전에 대기열에 들어온 편집은 옛 자리의 권한으로 판단한다. 바로 앞에서 세션과 옛 자리 권한을 다시 확인하고,
        // 이동 트랜잭션이 잠금을 기다리는 동안에도 재확인 간격마다 다시 본다(`move_rechecking`, #64 리뷰·W1b). 커밋 뒤에는
        // 옛 자리 권한을 볼 수 없으므로 남는 창은 마지막 재확인 ~ 커밋이고 그 길이는 재확인 간격을 넘지 않는다(문서화된
        // 권한 철회 지연과 같다). 세션은 `drain_after_move`가 한 번 더 본다.
        self.recheck().await;
        let (mut outcome, committed_at, commit) = match self.move_rechecking(&request).await {
            Ok(MoveReply::Moved {
                outcome,
                committed_at,
                commit,
            }) => (outcome, committed_at, commit),
            // 옮기지 않았다(확인한 미리보기가 달라짐·오류). 기다리던 메시지는 평소처럼 받는다.
            other => {
                let _ignored = done.send(other);
                return true;
            }
        };
        self.rooms.moved(self.doc_id, outcome.workspace_id.0);
        let shutdowns = self.drain_after_move(committed_at, commit, rx).await;
        // 커밋 전에 받은 편집까지 새 자리에서 리비전으로 남긴다. 작성자는 마지막으로 고친 사람·연결된 작성자 가운데 새
        // 자리에서 쓸 수 있는 사람, 없으면 옮긴 사람(대신 저장이면 실제 편집자를 따로 적는다, D-95). 남기면 그때 상태를
        // 저장하고 그 편집의 확인을 처음 보낸다.
        let before = self.base_revision;
        let kept = tokio::time::timeout(CHECKPOINT_LIMIT, self.snapshot_with(Some(request.user)))
            .await
            .unwrap_or(false);
        // 새 자리의 권한·설정으로 다시 열게 한다(4409). 리비전으로 남기지 못한 편집은 확인하지 않는다: 연결을 닫은 뒤에
        // 상태만 저장한다(브라우저가 초안으로 지닌다).
        for (_, c) in self.conns.drain() {
            let _ignored = c.peer.out.try_send(Outbound::Close(CloseReason::Reload));
        }
        self.persist().await;
        // 커밋 뒤에 받은 닫기 요청(새 자리의 실시간 끄기 등)에는 리비전을 남기고 상태를 저장한 뒤에 답한다.
        for saved in shutdowns {
            let _ignored = saved.send(kept);
        }
        if self.base_revision != before {
            // 이동의 답에는 그 리비전까지 담은 문서를 넣는다(화면이 그 판에서 이어 간다).
            match documents::document_for(&self.db, request.user, self.doc_id).await {
                Ok(document) => outcome.document = document,
                Err(err) => {
                    tracing::warn!(event = "collab.move_reload_failed", document = %self.doc_id.0, error_code = err.code(), detail = fvoci_core::logging::safe_error(&err), "could not reload the moved document");
                }
            }
        }
        let _ignored = done.send(Ok(MoveReply::Moved {
            outcome,
            committed_at,
            commit,
        }));
        false
    }

    /// 이동 트랜잭션을 하며, 그것이 잠금을 기다리는 동안 재확인 간격마다 연결의 세션·옛 자리 권한을 다른 DB 연결로 다시
    /// 본다(`recheck_peers`). 잃은 연결은 4403으로 닫아 그 대기 편집이 커밋 뒤에 들어가지 않게 한다(확인받지 않은 것뿐이다:
    /// 이 명령 동안에는 저장 확인을 보내지 않는다). 이동 트랜잭션이 문서를 잠그는 동안에도 재확인 기한은 유지한다.
    pub(super) async fn move_rechecking(
        &mut self,
        request: &MoveRequest,
    ) -> Result<MoveReply, AppError> {
        let db = self.db.clone();
        let moving = documents::move_in_db(&db, request.user, self.doc_id, &request.input);
        tokio::pin!(moving);
        let mut every = tokio::time::interval_at(
            tokio::time::Instant::now() + self.recheck_every,
            self.recheck_every,
        );
        loop {
            tokio::select! {
                reply = &mut moving => return reply,
                _ = every.tick() => {
                    // 커밋 직후 새 위치 확인과 경합해도, 큐의 대리 저장 근거를 확인하기 전 peer를 버리지 않는다.
                    self.refresh_peers().await;
                }
            }
        }
    }

    /// 문서를 옮긴 커밋 뒤: 대기열에 쌓인 것을 비운다. 커밋 전에 받은 메시지(그때 그 사람은 원래 자리의 권한이 있었다)만
    /// 평소처럼 검사해 넣고, 커밋 뒤에 받은 것은 버린다(새 자리에서 권한을 잃은 사람의 쓰기를 받지 않는다). 나가기는
    /// 따른다. 다른 명령은 답 없이 버린다: 방이 곧 닫히므로 참여는 새 방에서 새 자리의 권한으로 다시 하고, 다른 요청은
    /// 방이 닫힌 것으로 본다. 닫기 요청(`Shutdown`)의 답 자리만 돌려준다: 답 없이 버리면 리비전을 남긴 것으로 읽혀
    /// (`close_all`), 서버 종료가 리비전이나 상태 보존 전에 끝났다고 판단할 수 있다. 이동 중 대기열에 들어온 종료 요청도
    /// 이동 뒤 편집의 보존 결과로 답한다. 체크포인트 뒤 편집은 커밋 뒤 리비전을 남겨야 처음 확인받고, 실패하면 연결을
    /// 닫은 뒤 상태만 저장하며 브라우저 초안도 유지한다.
    ///
    /// 커밋 전 편집은 마지막 재확인(이동 바로 앞, 잠금 대기 중에는 재확인 간격마다, `move_rechecking`)에서 본 쓰기
    /// 권한으로 받는다. 그 재확인이 닫은 연결의 편집은 넣지 않는다. 넣기 전에 세션만 한 번 더 보고 끝났으면 버린다(연결을
    /// 4403으로 닫는다, #64 리뷰). 세션은 자리와 상관없어 옮긴 뒤에도 볼 수 있다. 옛 자리 권한은 커밋 뒤에는 다시 볼 수
    /// 없어 마지막 재확인으로 갈음한다.
    pub(super) async fn drain_after_move(
        &mut self,
        committed_at: Instant,
        commit: CommittedMove,
        rx: &mut mpsc::Receiver<Command>,
    ) -> Vec<oneshot::Sender<bool>> {
        let mut shutdowns = Vec::new();
        let mut leaving = Vec::new();
        for _ in 0..ROOM_QUEUE {
            match rx.try_recv() {
                Ok(Command::Incoming {
                    conn,
                    data,
                    received,
                    permit,
                }) => {
                    if received <= committed_at
                        && let Some(permit) = permit
                        && let Some(permit) = permit.freeze_move(commit)
                    {
                        self.incoming_with_authorization(
                            conn,
                            &data,
                            EditAuthorization::BeforeMove { permit },
                        );
                    }
                }
                // 나가기 전에 보낸 편집은 아래에서 넣은 뒤에 나간다(평소의 나가기와 같다).
                Ok(Command::Leave { conn }) => leaving.push(conn),
                Ok(Command::Shutdown {
                    saved: Some(saved), ..
                }) => shutdowns.push(saved),
                Ok(_) => {}
                Err(_) => break,
            }
        }
        self.drop_ended_sessions().await;
        self.flush().await;
        for conn in leaving {
            self.leave(conn);
        }
        shutdowns
    }

    /// 넣기를 기다리는 편집을 보낸 연결의 세션을 다시 본다. DB 오류와 기한 초과도 연결을 닫고 대기 편집을 버린다.
    pub(super) async fn drop_ended_sessions(&mut self) {
        let mut checked = HashSet::new();
        let conns: Vec<u64> = self
            .pending
            .iter()
            .map(|p| p.conn)
            .filter(|c| checked.insert(*c))
            .collect();
        for conn in conns {
            let Some(peer) = self.conns.get(&conn).map(|c| c.peer.clone()) else {
                continue;
            };
            if let Some(pending) = self.pending.iter().find(|pending| pending.conn == conn)
                && let Err(reason) = pending.authorization.check_session(&self.db, &peer).await
            {
                self.close(conn, reason);
            }
        }
    }
}
