//! 방 작업의 수명과 명령 분배.

use super::{
    AppError, Attribution, Change, CloseReason, Command, Connection, Db, DocumentId, Duration,
    FLUSH_EVERY, HashMap, HashSet, Instant, Limits, Outbound, PERSIST_EVERY, Peer, RestoreRequest,
    Room, Rooms, SNAPSHOT_IDLE, UserId, Value, mpsc, oneshot,
};

impl Room {
    pub(super) fn new(
        doc_id: DocumentId,
        generation: u64,
        db: Db,
        limits: Limits,
        recheck_every: Duration,
        rooms: Rooms,
    ) -> Self {
        Self {
            doc_id,
            generation,
            db,
            limits,
            registry_epoch: rooms.recovery.subscribe(),
            rooms,
            doc: None,
            presence: HashMap::new(),
            presence_owner: HashMap::new(),
            conns: HashMap::new(),
            pending: Vec::new(),
            base_revision: 0,
            base_body: Value::Null,
            file_refs: Attribution::default(),
            lineage: String::new(),
            dirty_state: false,
            dirty_since_snapshot: false,
            changed_outside: false,
            failed_checks: false,
            last_change: Instant::now(),
            persisted: None,
            editors: Vec::new(),
            edited_since: Vec::new(),
            recheck_every,
        }
    }

    /// Yjs 상태 크기 한도: 본문 한도의 네 배(지운 흔적 등 Yjs의 덧붙는 몫을 둔다).
    pub(super) fn max_state_bytes(&self) -> usize {
        self.limits
            .max_document_bytes
            .saturating_mul(4)
            .max(1 << 20)
    }

    pub(super) async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let mut flush = tokio::time::interval(FLUSH_EVERY);
        let mut persist = tokio::time::interval(PERSIST_EVERY);
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if !self.handle(cmd, &mut rx).await {
                        break;
                    }
                }
                _ = flush.tick() => self.flush().await,
                _ = persist.tick() => {
                    self.flush().await;
                    self.prune();
                    self.persist().await;
                    if self.dirty_since_snapshot && self.last_change.elapsed() >= SNAPSHOT_IDLE {
                        self.snapshot().await;
                    }
                    if self.remove_closed_peers() {
                        self.stop_realtime().await;
                    }
                }
            }
            if self.doc.is_some() && self.conns.is_empty() {
                self.flush().await;
                self.snapshot().await;
                self.persist().await;
                break;
            }
        }
        self.rooms.remove(self.doc_id, self.generation);
        // 닫히는 동안 들어온 참여 요청은 답하지 않고 버려 새 방을 열게 한다.
        rx.close();
        while rx.try_recv().is_ok() {}
    }

    /// 거짓이면 방을 닫는다. `rx`: 문서를 옮긴 뒤 대기열에 쌓인 것을 비울 때 쓴다.
    pub(super) async fn handle(&mut self, cmd: Command, rx: &mut mpsc::Receiver<Command>) -> bool {
        match cmd {
            Command::Join { conn, peer, reply } => self.join(conn, peer, reply).await,
            Command::Incoming { conn, data, .. } => {
                self.incoming(conn, &data);
                true
            }
            Command::Leave { conn } => {
                // 나가기 직전에 보낸 변경(아직 모아 두기만 한 것)을 먼저 넣는다. 나간 연결의 변경은 버리기 때문이다.
                self.flush().await;
                self.leave(conn);
                true
            }
            Command::Persist { done } => {
                self.flush().await;
                self.persist().await;
                let _ignored = done.send(());
                true
            }
            Command::Snapshot { done } => {
                self.flush().await;
                // 리비전을 남기면 상태도 저장한다. 남기지 못하면(편집할 수 있는 사람이 없음 등) 방에 그대로 둔다.
                let _kept = self.snapshot().await;
                let _ignored = done.send(());
                true
            }
            Command::Move { request, done } => self.move_document(request, done, rx).await,
            Command::Restore { request, done } => self.restore(request, done).await,
            Command::Shutdown {
                reason,
                snapshot,
                saved,
                fallback,
            } => self.shutdown(reason, snapshot, saved, fallback).await,
        }
    }

    async fn join(
        &mut self,
        conn: u64,
        peer: Peer,
        reply: oneshot::Sender<Result<(), AppError>>,
    ) -> bool {
        if self.doc.is_none() {
            match self.load(peer.user).await {
                Ok(doc) => self.doc = Some(doc),
                Err(err) => {
                    let _ignored = reply.send(Err(err));
                    return false;
                }
            }
        }
        if peer.access.permission().is_none() {
            let _ignored = reply.send(Err(AppError::Unauthorized));
            return true;
        }
        let start = self.start_messages();
        if start
            .into_iter()
            .all(|frame| peer.out.try_send(Outbound::Binary(frame)).is_ok())
        {
            self.conns.insert(
                conn,
                Connection {
                    peer,
                    clients: HashSet::new(),
                    window: Instant::now(),
                    messages: 0,
                    bytes: 0,
                },
            );
        }
        let _ignored = reply.send(Ok(()));
        true
    }

    async fn restore(
        &mut self,
        request: RestoreRequest,
        done: oneshot::Sender<Result<String, AppError>>,
    ) -> bool {
        if let Err(err) = self.ensure_loaded(request.user).await {
            let _ignored = done.send(Err(err));
            return false;
        }
        self.flush().await;
        // 리비전으로 남기지 않은 편집을 먼저 남긴다. 남겼는지 확인되지 않으면 복원하지 않는다(그 편집은 방에 남는다).
        // 남겼으면 그 리비전 때문에 화면의 기준이 지나 아래 복원이 충돌로 알린다(사람이 지금 내용을 보고 다시 한다).
        // 남길 수 있는 사람은 고친 사람과 연결된 작성자다(연결된 복원하는 사람도 든다, `checkpoint`).
        if self.dirty_since_snapshot
            && let Some(reason) = self.checkpoint(Change::Restore, request.user).await
        {
            let _ignored = done.send(Err(AppError::Conflict(reason.into())));
            return true;
        }
        let outcome =
            crate::features::revisions::restore_in_db(&self.db, &self.limits, &request).await;
        // 본문이 바뀌었으면 방을 닫아 복원된 본문으로 다시 열게 한다. 이 명령 뒤에 들어와 있던 편집은 방과 함께
        // 버려진다(받았다고 확인하지 않았으므로 클라이언트가 초안으로 남긴다).
        let restored = matches!(outcome.as_deref(), Ok("restored"));
        if restored {
            for (_, c) in self.conns.drain() {
                let _ignored = c.peer.out.try_send(Outbound::Close(CloseReason::Reload));
            }
        }
        let _ignored = done.send(outcome);
        !restored
    }

    async fn shutdown(
        &mut self,
        reason: CloseReason,
        snapshot: bool,
        saved: Option<oneshot::Sender<bool>>,
        fallback: Option<UserId>,
    ) -> bool {
        // 리비전을 남기면 그때 상태를 저장하고 확인을 보낸다(`snapshot_with`).
        let kept = if snapshot {
            self.flush().await;
            self.snapshot_with(fallback).await
        } else {
            true
        };
        // 실시간을 끄려는데 리비전을 남기지 못했으면 방을 닫지 않는다. 끄기는 거절되고(켜진 채), 편집은 방에
        // 그대로 남아 다음 시도에서 다시 리비전으로 남긴다(방이 계속 지니므로 평소처럼 저장·확인한다).
        if !kept && reason == CloseReason::RealtimeOff {
            self.persist().await;
            if let Some(saved) = saved {
                let _ignored = saved.send(false);
            }
            return true;
        }
        for (_, c) in self.conns.drain() {
            let _ignored = c.peer.out.try_send(Outbound::Close(reason));
        }
        // 리비전으로 남기지 못한 편집은 확인하지 않는다: 연결을 닫은 뒤에 상태만 저장한다(#71).
        if snapshot {
            self.persist().await;
        }
        if let Some(saved) = saved {
            let _ignored = saved.send(kept);
        }
        false
    }
}
