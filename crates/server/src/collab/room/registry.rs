//! 열린 방 목록, 참가와 보류·종료 조정.

use super::{
    AppError, Change, CloseReason, Command, Db, DocumentId, Duration, Handle, HashMap, HashSet,
    HoldGuard, HoldKey, Instant, Limits, Membership, MoveReply, MoveRequest, Ordering, Peer,
    ROOM_QUEUE, RestoreRequest, Room, Rooms, TrySendError, UserId, Uuid, documents, mpsc, oneshot,
    recovery,
};

impl Drop for HoldGuard {
    fn drop(&mut self) {
        let _rooms = self.rooms.lock();
        if self.epoch != self.rooms.recovery.current() {
            return;
        }
        let mut held = self.rooms.held_set();
        for key in &self.keys {
            if let Some(count) = held.get_mut(key) {
                *count -= 1;
                if *count == 0 {
                    held.remove(key);
                }
            }
        }
    }
}

impl Membership {
    /// 방 작업이 끝나면 유휴 소켓도 더 이상 연결을 유지하지 않는다.
    pub(crate) async fn closed(&self) {
        self.tx.closed().await;
    }

    /// 받은 메시지를 방에 넘긴다(받은 때와 함께). 방이 닫혔으면 거짓.
    pub async fn send(&self, data: Vec<u8>) -> bool {
        let permit = self.access.write_permit();
        let received = Instant::now();
        self.tx
            .send(Command::Incoming {
                conn: self.conn,
                data,
                received,
                permit,
            })
            .await
            .is_ok()
    }
}

impl Drop for Membership {
    fn drop(&mut self) {
        // 대기열이 가득 차 못 보내도 방은 닫힌 연결을 주기적으로 걸러 낸다.
        let _ignored = self.tx.try_send(Command::Leave { conn: self.conn });
    }
}

impl Rooms {
    pub(super) fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<DocumentId, Handle>> {
        recovery::lock(self)
    }

    /// 옮기거나 방 없이 복원하는 중인 문서 목록. 늘 방 목록 잠금(`lock`)을 먼저 잡은 뒤에 잡는다.
    pub(super) fn held_set(&self) -> std::sync::MutexGuard<'_, HashMap<HoldKey, usize>> {
        recovery::list(&self.held, &self.recovery) // 오염된 이동·복원 막음도 옛 방과 함께 버린다(#172).
    }

    /// 열린(닫히는 중이 아닌) 방.
    pub(super) fn live_room(&self, doc: DocumentId) -> Option<Handle> {
        self.lock().get(&doc).filter(|h| !h.tx.is_closed()).cloned()
    }

    /// 방에 들어간다(없으면 연다). 문서를 읽을 수 없으면 오류.
    pub async fn join(
        &self,
        db: &Db,
        limits: &Limits,
        recheck_every: Duration,
        doc: DocumentId,
        workspace: Option<Uuid>,
        peer: Peer,
    ) -> Result<Membership, AppError> {
        let organization = if let Some(ws) = workspace {
            let mut tx = db.begin_as(peer.user).await?;
            let org = sqlx::query_scalar("SELECT organization_id FROM workspaces WHERE id=$1")
                .bind(ws)
                .fetch_optional(tx.conn())
                .await?;
            tx.commit().await?;
            org
        } else {
            None
        };
        for _ in 0..5 {
            let handle =
                self.acquire_room(db, limits, recheck_every, doc, workspace, organization)?;
            if let Some(membership) = self.request_join(&handle, &peer).await? {
                return Ok(membership);
            }
        }
        Err(AppError::Upstream(
            "could not open the live editing room".into(),
        ))
    }

    /// 방 목록 잠금 안에서 참가 가능 여부를 확인하고 기존 방을 찾거나 연다.
    fn acquire_room(
        &self,
        db: &Db,
        limits: &Limits,
        recheck_every: Duration,
        doc: DocumentId,
        workspace: Option<Uuid>,
        organization: Option<Uuid>,
    ) -> Result<Handle, AppError> {
        let mut rooms = self.lock();
        // 옮기거나 방 없이 복원하는 중이면 들어가지 않는다. 클라이언트는 잠시 뒤 다시 붙고, 그때 새 자리의
        // 권한·설정(복원했으면 복원한 본문)으로 연다.
        let held = self.held_set();
        if held.contains_key(&HoldKey::Document(doc))
            || workspace.is_some_and(|ws| held.contains_key(&HoldKey::Workspace(ws)))
            || organization.is_some_and(|org| held.contains_key(&HoldKey::Organization(org)))
        {
            return Err(AppError::Upstream(
                "the document is being moved or restored".into(),
            ));
        }
        drop(held);
        if let Some(h) = rooms.get(&doc).filter(|h| !h.tx.is_closed()) {
            Ok(h.clone())
        } else {
            let generation = self.next.fetch_add(1, Ordering::Relaxed);
            let (tx, rx) = mpsc::channel(ROOM_QUEUE);
            let handle = Handle {
                generation,
                workspace,
                organization,
                tx,
            };
            rooms.insert(doc, handle.clone());
            let room = Room::new(
                doc,
                generation,
                db.clone(),
                *limits,
                recheck_every,
                self.clone(),
            );
            tokio::spawn(recovery::run(room, rx));
            Ok(handle)
        }
    }

    /// 방 목록 잠금 밖에서 참가 답을 기다린다. 닫히던 방이면 호출자가 다시 확보한다.
    async fn request_join(
        &self,
        handle: &Handle,
        peer: &Peer,
    ) -> Result<Option<Membership>, AppError> {
        let conn = self.next.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        if handle
            .tx
            .send(Command::Join {
                conn,
                peer: peer.clone(),
                reply,
            })
            .await
            .is_err()
        {
            return Ok(None);
        }
        match answer.await {
            Ok(Ok(())) => {
                return Ok(Some(Membership {
                    conn,
                    tx: handle.tx.clone(),
                    access: peer.access.clone(),
                }));
            }
            Ok(Err(err)) => return Err(err),
            // 방이 닫히는 중이었다(답 없이 버려짐). 새로 연다.
            Err(_) => tokio::task::yield_now().await,
        }
        Ok(None)
    }

    /// 방이 문서를 다른 워크스페이스로 옮겼다. 방이 닫히기까지 그 워크스페이스에 속하게 한다(실시간 끄기·파일 버전
    /// 지우기가 워크스페이스로 방을 찾는다).
    pub(super) fn moved(&self, doc: DocumentId, workspace: Uuid) {
        if let Some(handle) = self.lock().get_mut(&doc) {
            handle.workspace = Some(workspace);
        }
    }

    /// 열린 방마다 모아 둔 변경을 넣고 상태를 지금 저장하게 한 뒤 기다린다(방마다 최대 2초). 파일 버전을 지우기 전에
    /// 불러, 방금 실시간 본문에 넣은 이미지의 참조가 주기적 저장(3초)을 기다리지 않고 DB에 남게 한다.
    pub async fn persist_all(&self) {
        self.persist_where(|_| true).await;
    }

    /// 한 워크스페이스의 열린 방만(그 워크스페이스 파일을 가리킬 수 있는 문서들). 파일 버전을 지울 때 쓴다.
    pub async fn persist_workspace(&self, workspace: Uuid) {
        self.persist_where(|h| h.workspace == Some(workspace)).await;
    }

    /// 방들에 동시에 저장을 시키고 모두 합쳐 최대 2초 기다린다. 대기열이 찬(바쁜) 방은 건너뛴다(곧 하는 주기적
    /// 저장이 대신한다). 방이 이미 닫히는 중이면 답이 바로 돌아온다.
    pub(super) async fn persist_where(&self, keep: impl Fn(&Handle) -> bool) {
        let handles: Vec<Handle> = self.lock().values().filter(|h| keep(h)).cloned().collect();
        ask_all(handles, Duration::from_secs(2), |done| Command::Persist {
            done,
        })
        .await;
    }

    /// 이 문서들의 열린 방이 아직 리비전으로 남지 않은 편집을 지금 리비전으로 남기게 하고 모두 합쳐 최대 5초
    /// 기다린다. 반출이 문서 본문(마지막 리비전)을 읽기 전에 쓴다. 계속 편집하는 방은 쉬는 시간 리비전이 없어서다.
    pub async fn snapshot_documents(&self, docs: &HashSet<DocumentId>) {
        let handles: Vec<Handle> = self
            .lock()
            .iter()
            .filter(|(doc, _)| docs.contains(doc))
            .map(|(_, h)| h.clone())
            .collect();
        ask_all(handles, Duration::from_secs(5), |done| Command::Snapshot {
            done,
        })
        .await;
    }

    /// 이 문서의 방이 새로 열리지 않게 막고(들어오는 연결은 4503으로 다시 붙는다) 지금 열린 방을 돌려준다. 막음과 방
    /// 찾기를 같은 잠금 아래에서 해, 방이 없다고 본 뒤에 방이 열려 저장 대기 상태를 이어받지 않는다.
    pub(super) fn hold(
        &self,
        doc: DocumentId,
        workspaces: &[Uuid],
        organizations: &[Uuid],
    ) -> Result<(HoldGuard, Option<Handle>), AppError> {
        let rooms = self.lock();
        let mut held = self.held_set();
        if held.contains_key(&HoldKey::Document(doc))
            || workspaces
                .iter()
                .any(|ws| held.contains_key(&HoldKey::Workspace(*ws)))
            || organizations
                .iter()
                .any(|org| held.contains_key(&HoldKey::Organization(*org)))
        {
            return Err(AppError::Conflict(
                "the document is being changed; retry after reconnecting".into(),
            ));
        }
        let keys: HashSet<_> = std::iter::once(HoldKey::Document(doc))
            .chain(workspaces.iter().copied().map(HoldKey::DocumentWorkspace))
            .chain(
                organizations
                    .iter()
                    .copied()
                    .map(HoldKey::DocumentOrganization),
            )
            .collect();
        for key in &keys {
            *held.entry(*key).or_default() += 1;
        }
        let room = rooms.get(&doc).filter(|h| !h.tx.is_closed()).cloned();
        Ok((
            HoldGuard {
                rooms: self.clone(),
                keys: keys.into_iter().collect(),
                epoch: self.recovery.current(),
            },
            room,
        ))
    }

    /// 이 문서의 방이 열려 있는지(방 목록에서 빠지기 전, 닫히며 저장하는 중도 열린 것으로 본다).
    #[must_use]
    pub fn has_room(&self, doc: DocumentId) -> bool {
        self.live_room(doc).is_some()
    }

    /// 문서를 옮긴다(감사 S6, #71). 옮기는 동안 이 문서의 방이 새로 열리지 않게 막는다(`hold`). 열린 방이 있으면 그 방이
    /// 자기 명령 안에서 옮긴다(체크포인트 → 이동 → 리비전 → 연결 닫기, `Command::Move`): 그동안 온 편집은 방의 대기열에서
    /// 기다려, 확인받은 편집이 옮긴 곳에서 리비전이 되지 못하는 일이 없다. 방이 없으면 방 없이 남은 실시간 상태를 먼저
    /// 리비전으로 남기고(남기지 못하면 옮기지 않고 409) 옮긴다. 방의 답은 복원처럼 시간 제한 없이 기다린다(여기서 그만두면
    /// 방이 옮겼는데 실패로 답할 수 있다). 방 안의 이동 트랜잭션은 잠금 하나를 `documents::LOCK_LIMIT`까지만 기다리므로
    /// 잠금 대기는 끝이 있다(요청 전체의 상한은 아니다). 방이 바쁘면 (대기열이 참) 409. 방이 이 명령을 받기 전에
    /// 닫혔으면 옮기지 않았으므로 다시 본다.
    pub async fn move_document(
        &self,
        db: &Db,
        limits: &Limits,
        request: MoveRequest,
    ) -> Result<MoveReply, AppError> {
        let (_guard, mut room) = self
            .hold_document(
                db,
                request.user,
                request.doc,
                request.input.target_workspace_id.map(|ws| ws.0),
                request.input.target_item_id.map(|item| item.0),
            )
            .await?;
        for _ in 0..5 {
            let Some(handle) = room else {
                // 막음 아래라 그사이 방이 열려 그 상태를 이어받지 않는다.
                self.save_pending_before(db, limits, request.doc, request.user, Change::Move)
                    .await?;
                return documents::move_in_db(db, request.user, request.doc, &request.input).await;
            };
            let (done, answer) = oneshot::channel();
            match handle.tx.try_send(Command::Move {
                request: request.clone(),
                done,
            }) {
                Err(TrySendError::Full(_)) => {
                    return Err(AppError::Conflict(
                        "the live editing room is busy; nothing was moved, try again".into(),
                    ));
                }
                Err(TrySendError::Closed(_)) => {}
                Ok(()) => {
                    if let Ok(reply) = answer.await {
                        return reply;
                    }
                }
            }
            // 방이 이 명령을 처리하기 전에 닫혔다(옮기지 않음). 막음 아래라 새 방은 열리지 않는다.
            tokio::task::yield_now().await;
            room = self.live_room(request.doc);
        }
        Err(AppError::Conflict(
            "the live editing room closed; nothing was moved, try again".into(),
        ))
    }

    /// 옛 리비전으로 복원한다(쓰기 권한을 확인한 요청). 복원하는 동안 이 문서의 방이 새로 열리지 않게 막는다(`hold`).
    /// 열린 방이 있으면 그 방이 복원한다(체크포인트 → 복원 → 방 초기화를 한 명령으로, `Command::Restore`). 방이 없으면 방
    /// 없이 남은 저장 대기 상태를 먼저 리비전으로 남긴 뒤 복원한다(D-95 조건 4, 이동과 같다). 그 작성자는 상태와 함께 저장한
    /// 편집자 가운데 아직 쓸 수 있는 사람뿐이다: 연결이 없으므로 복원하는 사람은 대신 저장하지 않는다(방 안 복원에서는
    /// 연결된 복원하는 사람도 연결된 작성자로서 대신 저장할 수 있다). 남기지 못하면 복원하지 않고 409이며 상태는 그대로
    /// 남는다. 남겼으면 기준 리비전이 지나 복원은 충돌(`conflict`)로 끝난다(화면이 지금 내용을 보고 다시 한다). 방의 답은
    /// 시간 제한 없이 기다린다: 여기서 그만두면 방이 복원을 마쳤는데 실패로 답할 수 있다. 방이 이 명령을 받기 전에
    /// 닫혔으면 복원하지 않았으므로 다시 본다.
    pub async fn restore(
        &self,
        db: &Db,
        limits: &Limits,
        request: RestoreRequest,
    ) -> Result<String, AppError> {
        let (_guard, mut room) = self
            .hold_document(db, request.user, request.doc, None, None)
            .await?;
        for _ in 0..5 {
            let Some(handle) = room else {
                self.save_pending_before(db, limits, request.doc, request.user, Change::Restore)
                    .await?;
                return crate::features::revisions::restore_in_db(db, limits, &request).await;
            };
            let (done, answer) = oneshot::channel();
            match handle.tx.try_send(Command::Restore {
                request: request.clone(),
                done,
            }) {
                Err(TrySendError::Full(_)) => {
                    return Err(AppError::Conflict(
                        "the live editing room is busy; nothing was restored, try again".into(),
                    ));
                }
                Err(TrySendError::Closed(_)) => {}
                Ok(()) => {
                    if let Ok(reply) = answer.await {
                        return reply;
                    }
                }
            }
            // 방이 이 명령을 처리하기 전에 닫혔다(복원하지 않음). 막음 아래라 새 방은 열리지 않는다.
            tokio::task::yield_now().await;
            room = self.live_room(request.doc);
        }
        Err(AppError::Conflict(
            "the live editing room closed; nothing was restored, try again".into(),
        ))
    }

    /// 방 없이 남은 실시간 상태를 옮기거나 복원하기 전에 리비전으로 남긴다(#71 리뷰, D-95 조건 4). 앞선 방이 리비전을
    /// 남기지 못하고 닫혔거나(쓸 수 있는 사람이 없음) 서버가 주기 저장 뒤 갑자기 멈춘 경우로, 그 편집은 이미 저장 확인을
    /// 받았을 수 있다. `hold` 아래에서 불러 그사이 새 방이 열려 그 상태를 이어받지 않게 한다.
    ///
    /// 새로 만든 방이라 연결이 없으므로 작성자 순서는 그 상태와 함께 저장된 편집자(마지막 리비전 뒤로 고친 사람, 마지막이
    /// 앞) 가운데 아직 쓸 수 있는 사람이다. 옮길 때는 그다음 옮기는 사람이 대신 남기고(방 안 체크포인트와 같다) 실제 편집자를 리비전에 따로 적는다(D-95, 편집자 기록이 없는 옛 상태면 빈 목록). 복원하는 사람은
    /// 대신 남기지 않는다(연결이 없으므로 연결된 작성자 자리도 없다, D-95 docs/decisions/documents.md#d-95). 남길 것이 없으면(저장된 상태가
    /// 없음·본문과 같은 내용) 아무것도 하지 않는다. 남기지 못하면 방 안 체크포인트와 같은 409로 하지 않고
    /// 상태는 그대로 둔다.
    pub(super) async fn save_pending_before(
        &self,
        db: &Db,
        limits: &Limits,
        doc: DocumentId,
        user: UserId,
        change: Change,
    ) -> Result<(), AppError> {
        if matches!(change, Change::Organization) {
            // 닫힌 방의 내용 로드는 명령 권한과 별개다. 읽기 공유만 가진 조직 관리자도 대리 저장하지 못한다.
            let mut tx = db.begin_as(user).await?;
            fvoci_db::access::require_write(&mut tx, fvoci_db::access::Kind::Document, doc).await?;
            tx.commit().await?;
        }
        let generation = self.next.fetch_add(1, Ordering::Relaxed);
        let mut room = Room::new(
            doc,
            generation,
            db.clone(),
            *limits,
            Duration::from_secs(30),
            self.clone(),
        );
        let loaded = room.load(user).await?;
        // 이어받은 상태에 아직 리비전이 아닌 편집이 있을 때만 남긴다(`load`가 본다). 본문에서 새로 만들었으면(저장된 상태가
        // 없거나 그 뒤로 본문이 바뀜) 남길 것이 없고 상태 행도 만들지 않는다.
        if !room.dirty_since_snapshot {
            return Ok(());
        }
        room.doc = Some(loaded);
        room.checkpoint(change, user)
            .await
            .map_or(Ok(()), |reason| Err(AppError::Conflict(reason.into())))
    }

    /// 서버를 끝낸다. 모든 방의 리비전을 남기고 닫는다(클라이언트는 다시 붙는다).
    pub async fn shutdown(&self) {
        let handles: Vec<Handle> = self.lock().values().cloned().collect();
        let _saved = close_all(handles, CloseReason::Restart, None).await;
    }

    pub(super) fn remove(&self, doc: DocumentId, generation: u64) {
        let mut rooms = self.lock();
        if rooms.get(&doc).is_some_and(|h| h.generation == generation) {
            rooms.remove(&doc);
        }
    }
}
/// 방마다 명령을 보내고 답을 모두 합쳐 `wait`까지 기다린다. 대기열이 찬(바쁜) 방은 건너뛴다.
async fn ask_all<T: Send + 'static>(
    handles: Vec<Handle>,
    wait: Duration,
    command: impl Fn(oneshot::Sender<T>) -> Command,
) {
    let waits = handles.into_iter().map(|handle| {
        let (done, answer) = oneshot::channel();
        let sent = handle.tx.try_send(command(done)).is_ok();
        async move {
            if sent {
                let _ignored = answer.await;
            }
        }
    });
    let _ignored = tokio::time::timeout(wait, futures_util::future::join_all(waits)).await;
}

/// 방마다 리비전을 남기고 닫게 한 뒤 답을 기다린다. 모두 남겼으면 참. 이미 닫힌 방은 남길 것이 없다.
async fn close_all(handles: Vec<Handle>, reason: CloseReason, fallback: Option<UserId>) -> bool {
    let mut waits = Vec::new();
    for handle in handles {
        let (saved, answer) = oneshot::channel();
        if handle
            .tx
            .send(Command::Shutdown {
                reason,
                snapshot: true,
                saved: Some(saved),
                fallback,
            })
            .await
            .is_ok()
        {
            // 답 없이 끝난 방(닫히는 중이던 방)은 이미 마지막 리비전을 남겼다.
            waits.push(async move { answer.await.unwrap_or(true) });
        }
    }
    // 리비전 쓰기가 오래 걸리면 끝까지 기다리지는 않는다(그때는 남겼다고 보지 않는다).
    tokio::time::timeout(
        Duration::from_secs(10),
        futures_util::future::join_all(waits),
    )
    .await
    .is_ok_and(|all| all.into_iter().all(|saved| saved))
}
