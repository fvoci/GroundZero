//! 문서 하나의 실시간 편집 방(D-70). 방마다 작업 하나가 Yjs 문서를 혼자 맡고(잠금 없음), 연결은 메시지만 주고받는다.
//!
//! - **경계**: 쓰기 권한이 있는 연결의 변경만, 같은 내용 검사(`validate_body`)를 거친 뒤 방 문서에 넣고 다른
//!   연결에 보낸다. 검사는 방 문서를 복사한 임시 문서에 그 변경을 먼저 넣어 본다. 통과하지 못하면 버리고 그 연결을
//!   닫는다. 읽기 연결은 내용을 바꾸지 못한다(커서·참여자 표시만).
//! - **저장**: 몇 초마다 Yjs 상태를 저장하고, 60초 조용하거나 마지막 사람이 나가면 저장 API와 같은 길
//!   (`documents::write_revision`)으로 리비전을 만든다. 그 사이 본문이 다른 길(복원)로 바뀌었으면 쓰지 않고 방을 닫는다.
//! - **한 서버**: 방은 메모리에 있다. 셀프호스트의 한 앱 프로세스(`role=All`)를 전제로 한다.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
use yrs::sync::awareness::AwarenessUpdateEntry;
use yrs::sync::{AwarenessUpdate, Message, MessageReader, SyncMessage};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, Map, ReadTxn, StateVector, Transact, Update};

use super::convert::{self, FRAGMENT};
use crate::config::Limits;
use crate::db::Db;
use crate::error::AppError;
use crate::features::documents;
use crate::ids::{DocumentId, UserId};

/// 작은 변경을 모아 검사하는 간격.
const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// Yjs 상태를 저장하는 간격.
const PERSIST_EVERY: Duration = Duration::from_secs(3);
/// 이만큼 조용하면 리비전을 만든다.
const SNAPSHOT_IDLE: Duration = Duration::from_secs(60);

/// 닫는 이유(웹소켓 닫기 코드). 4000번대는 앱 정의.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// 내용 검사 실패, 읽기 연결의 쓰기 등 잘못된 메시지.
    Invalid,
    /// 권한·세션이 끝남.
    AccessLost,
    /// 문서가 다른 길로 바뀌어(복원) 다시 열어야 함.
    Reload,
    /// 워크스페이스가 실시간을 끔.
    RealtimeOff,
    /// 너무 느려 보낼 것이 쌓임.
    Slow,
}

impl CloseReason {
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Invalid => 4400,
            Self::AccessLost => 4403,
            Self::Reload => 4409,
            Self::RealtimeOff => 4410,
            Self::Slow => 4429,
        }
    }

    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Invalid => "invalid message",
            Self::AccessLost => "access ended",
            Self::Reload => "document changed; reload",
            Self::RealtimeOff => "realtime editing turned off",
            Self::Slow => "connection too slow",
        }
    }
}

/// 연결로 보내는 것.
#[derive(Debug)]
pub enum Outbound {
    Binary(Vec<u8>),
    Close(CloseReason),
}

/// 연결 하나.
#[derive(Debug, Clone)]
pub struct Peer {
    pub user: UserId,
    /// 세션 토큰의 해시. 주기적으로 세션이 살아 있는지 본다.
    pub session: Vec<u8>,
    pub can_write: bool,
    pub out: mpsc::Sender<Outbound>,
}

#[derive(Debug)]
enum Command {
    Join {
        conn: u64,
        peer: Peer,
        reply: oneshot::Sender<Result<(), AppError>>,
    },
    Incoming {
        conn: u64,
        data: Vec<u8>,
    },
    Leave {
        conn: u64,
    },
    Shutdown {
        reason: CloseReason,
        snapshot: bool,
    },
}

#[derive(Debug, Clone)]
struct Handle {
    generation: u64,
    workspace: Option<Uuid>,
    tx: mpsc::Sender<Command>,
}

/// 열린 방 목록.
#[derive(Debug, Clone, Default)]
pub struct Rooms {
    inner: Arc<Mutex<HashMap<DocumentId, Handle>>>,
    next: Arc<AtomicU64>,
}

/// 방에 들어간 연결. 떨어지면 방에서 나간다.
#[derive(Debug)]
pub struct Membership {
    conn: u64,
    tx: mpsc::Sender<Command>,
}

impl Membership {
    /// 받은 메시지를 방에 넘긴다. 방이 닫혔으면 거짓.
    pub async fn send(&self, data: Vec<u8>) -> bool {
        self.tx
            .send(Command::Incoming {
                conn: self.conn,
                data,
            })
            .await
            .is_ok()
    }
}

impl Drop for Membership {
    fn drop(&mut self) {
        let _ignored = self.tx.try_send(Command::Leave { conn: self.conn });
    }
}

impl Rooms {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<DocumentId, Handle>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        for _ in 0..3 {
            let handle = {
                let mut rooms = self.lock();
                if let Some(h) = rooms.get(&doc).filter(|h| !h.tx.is_closed()) {
                    h.clone()
                } else {
                    let generation = self.next.fetch_add(1, Ordering::Relaxed);
                    let (tx, rx) = mpsc::channel(1024);
                    let handle = Handle {
                        generation,
                        workspace,
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
                    tokio::spawn(room.run(rx));
                    handle
                }
            };
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
                continue;
            }
            match answer.await {
                Ok(Ok(())) => {
                    return Ok(Membership {
                        conn,
                        tx: handle.tx.clone(),
                    });
                }
                Ok(Err(err)) => return Err(err),
                // 방이 닫히는 중이었다. 새로 연다.
                Err(_) => {}
            }
        }
        Err(AppError::Conflict(
            "could not open the live editing room".into(),
        ))
    }

    /// 문서가 다른 길로 바뀌었다(복원). 방을 저장 없이 닫고 다시 열게 한다.
    pub async fn reset(&self, doc: DocumentId) {
        let handle = self.lock().get(&doc).cloned();
        if let Some(handle) = handle {
            let _ignored = handle
                .tx
                .send(Command::Shutdown {
                    reason: CloseReason::Reload,
                    snapshot: false,
                })
                .await;
        }
    }

    /// 워크스페이스가 실시간을 껐다. 그 워크스페이스의 방은 리비전을 남기고 닫는다.
    pub async fn close_workspace(&self, workspace: Uuid) {
        let handles: Vec<Handle> = self
            .lock()
            .values()
            .filter(|h| h.workspace == Some(workspace))
            .cloned()
            .collect();
        for handle in handles {
            let _ignored = handle
                .tx
                .send(Command::Shutdown {
                    reason: CloseReason::RealtimeOff,
                    snapshot: true,
                })
                .await;
        }
    }

    fn remove(&self, doc: DocumentId, generation: u64) {
        let mut rooms = self.lock();
        if rooms.get(&doc).is_some_and(|h| h.generation == generation) {
            rooms.remove(&doc);
        }
    }
}

struct Connection {
    peer: Peer,
    /// 이 연결이 알린 참여자(awareness) id. 나가면 지운다.
    clients: HashSet<ClientID>,
}

struct Pending {
    conn: u64,
    user: UserId,
    update: Vec<u8>,
}

struct Room {
    doc_id: DocumentId,
    generation: u64,
    db: Db,
    limits: Limits,
    rooms: Rooms,
    doc: Option<Doc>,
    /// 참여자 표시(커서·이름). 서버는 내용을 보지 않고 각 참여자의 최신 항목(시계가 큰 것)만 들고 있다가 전한다.
    presence: HashMap<ClientID, AwarenessUpdateEntry>,
    conns: HashMap<u64, Connection>,
    pending: Vec<Pending>,
    /// 방 문서가 이어받은 리비전과 그때의 본문(본문이 다른 길로 바뀌었는지 보려고).
    base_revision: i32,
    base_body: Value,
    /// 방 문서의 계보 표시. 본문에서 새로 만들 때마다 바뀐다. 예전 계보를 가진 클라이언트가 다시 붙어 옛 내용을
    /// 섞지 않게 그런 연결은 다시 열게 한다(4409).
    lineage: String,
    dirty_state: bool,
    dirty_since_snapshot: bool,
    last_change: Instant,
    last_editor: Option<UserId>,
    last_recheck: Instant,
    /// 연결의 세션·권한을 다시 확인하는 간격(`FVOCI_COLLAB_RECHECK_SECS`).
    recheck_every: Duration,
}

impl Room {
    fn new(
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
            rooms,
            doc: None,
            presence: HashMap::new(),
            conns: HashMap::new(),
            pending: Vec::new(),
            base_revision: 0,
            base_body: Value::Null,
            lineage: String::new(),
            dirty_state: false,
            dirty_since_snapshot: false,
            last_change: Instant::now(),
            last_editor: None,
            last_recheck: Instant::now(),
            recheck_every,
        }
    }

    async fn run(mut self, mut rx: mpsc::Receiver<Command>) {
        let mut flush = tokio::time::interval(FLUSH_EVERY);
        let mut persist = tokio::time::interval(PERSIST_EVERY);
        loop {
            tokio::select! {
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { break };
                    if !self.handle(cmd).await {
                        break;
                    }
                }
                _ = flush.tick() => self.flush(),
                _ = persist.tick() => {
                    self.persist().await;
                    if self.dirty_since_snapshot && self.last_change.elapsed() >= SNAPSHOT_IDLE {
                        self.snapshot().await;
                    }
                    if self.last_recheck.elapsed() >= self.recheck_every {
                        self.recheck().await;
                    }
                }
            }
            if self.doc.is_some() && self.conns.is_empty() {
                self.flush();
                self.snapshot().await;
                self.persist().await;
                break;
            }
        }
        self.rooms.remove(self.doc_id, self.generation);
        // 닫히는 동안 들어온 참여 요청은 실패시켜 새 방을 열게 한다.
        rx.close();
        while let Ok(cmd) = rx.try_recv() {
            if let Command::Join { reply, .. } = cmd {
                let _ignored = reply.send(Err(AppError::Conflict("room closed".into())));
            }
        }
    }

    /// 거짓이면 방을 닫는다.
    async fn handle(&mut self, cmd: Command) -> bool {
        match cmd {
            Command::Join { conn, peer, reply } => {
                if self.doc.is_none() {
                    match self.load(peer.user).await {
                        Ok(doc) => self.doc = Some(doc),
                        Err(err) => {
                            let _ignored = reply.send(Err(err));
                            return false;
                        }
                    }
                }
                let start = self.start_messages();
                let accepted = peer.out.try_send(Outbound::Binary(start)).is_ok();
                if accepted {
                    self.conns.insert(
                        conn,
                        Connection {
                            peer,
                            clients: HashSet::new(),
                        },
                    );
                }
                let _ignored = reply.send(Ok(()));
                true
            }
            Command::Incoming { conn, data } => {
                self.incoming(conn, &data);
                true
            }
            Command::Leave { conn } => {
                self.leave(conn);
                true
            }
            Command::Shutdown { reason, snapshot } => {
                if snapshot {
                    self.flush();
                    self.snapshot().await;
                    self.persist().await;
                }
                for (_, c) in self.conns.drain() {
                    let _ignored = c.peer.out.try_send(Outbound::Close(reason));
                }
                false
            }
        }
    }

    /// 저장된 Yjs 상태(리비전이 같을 때) 또는 본문에서 방 문서를 만든다. 들어오는 사람의 권한으로 읽는다.
    async fn load(&mut self, user: UserId) -> Result<Doc, AppError> {
        let mut tx = self.db.begin_as(user).await?;
        let (revision, _title, body, state): (i32, String, Value, Option<Vec<u8>>) =
            sqlx::query_as("SELECT * FROM app_collab_load($1)")
                .bind(self.doc_id)
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        tx.commit().await?;
        let doc = Doc::new();
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let restored = state
            .and_then(|bytes| Update::decode_v1(&bytes).ok())
            .is_some_and(|update| doc.transact_mut().apply_update(update).is_ok());
        if !restored {
            let map = meta(&doc);
            let mut txn = doc.transact_mut();
            convert::write_doc(&mut txn, &fragment, &body);
            map.insert(&mut txn, LINEAGE, Uuid::now_v7().to_string());
            drop(txn);
            self.dirty_state = true;
        }
        self.lineage = lineage_of(&doc).unwrap_or_default();
        self.base_revision = revision;
        self.base_body = body;
        Ok(doc)
    }

    fn start_messages(&self) -> Vec<u8> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };
        let sv = doc.transact().state_vector();
        let mut out = Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1();
        if let Some(update) = self.presence_update() {
            out.extend(Message::Awareness(update).encode_v1());
        }
        out
    }

    /// 지금 있는 참여자 표시 전부(없으면 `None`).
    fn presence_update(&self) -> Option<AwarenessUpdate> {
        let clients: HashMap<ClientID, AwarenessUpdateEntry> = self
            .presence
            .iter()
            .filter(|(_, e)| &*e.json != "null")
            .map(|(id, e)| (*id, e.clone()))
            .collect();
        (!clients.is_empty()).then_some(AwarenessUpdate { clients })
    }

    fn incoming(&mut self, conn: u64, data: &[u8]) {
        let Some(connection) = self.conns.get(&conn) else {
            return;
        };
        let (user, can_write, out) = (
            connection.peer.user,
            connection.peer.can_write,
            connection.peer.out.clone(),
        );
        let mut decoder = DecoderV1::from(data);
        let messages: Vec<Message> = MessageReader::new(&mut decoder)
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        if messages.is_empty() && !data.is_empty() {
            self.close(conn, CloseReason::Invalid);
            return;
        }
        for message in messages {
            match message {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let Some(doc) = &self.doc else {
                        continue;
                    };
                    let update = doc.transact().encode_state_as_update_v1(&sv);
                    self.send(
                        &out,
                        Message::Sync(SyncMessage::SyncStep2(update)).encode_v1(),
                        conn,
                    );
                }
                Message::Sync(SyncMessage::SyncStep2(update) | SyncMessage::Update(update)) => {
                    let empty = Update::decode_v1(&update).map_or(true, |u| u.is_empty());
                    if empty {
                        continue;
                    }
                    if !can_write {
                        self.close(conn, CloseReason::Invalid);
                        return;
                    }
                    self.pending.push(Pending { conn, user, update });
                }
                Message::Awareness(update) => self.awareness_update(conn, update),
                Message::AwarenessQuery => {
                    if let Some(update) = self.presence_update() {
                        self.send(&out, Message::Awareness(update).encode_v1(), conn);
                    }
                }
                Message::Auth(_) | Message::Custom(..) => {}
            }
        }
    }

    fn awareness_update(&mut self, conn: u64, update: AwarenessUpdate) {
        // 더 새로운(시계가 큰) 항목만 받는다. 표시 내용(JSON 글)은 서버가 해석하지 않고 그대로 전한다.
        let mut fresh = HashMap::new();
        for (id, entry) in update.clients {
            let newer = self
                .presence
                .get(&id)
                .is_none_or(|old| entry.clock > old.clock);
            if newer && entry.json.len() <= 16 * 1024 {
                self.presence.insert(id, entry.clone());
                fresh.insert(id, entry);
            }
        }
        if fresh.is_empty() {
            return;
        }
        let ids: Vec<ClientID> = fresh.keys().copied().collect();
        let encoded = Message::Awareness(AwarenessUpdate { clients: fresh }).encode_v1();
        if let Some(c) = self.conns.get_mut(&conn) {
            c.clients.extend(ids);
        }
        self.broadcast(&encoded, Some(conn));
    }

    /// 모인 변경을 하나씩 검사해 방 문서에 넣고 다른 연결에 보낸다.
    fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        for item in pending {
            if !self.conns.contains_key(&item.conn) {
                continue;
            }
            if self.foreign_lineage(&item.update) {
                self.close(item.conn, CloseReason::Reload);
                continue;
            }
            if !self.passes_checks(&item.update) {
                tracing::warn!(document = %self.doc_id.0, "rejected a live edit that fails document checks");
                self.close(item.conn, CloseReason::Invalid);
                continue;
            }
            let Some(doc) = &self.doc else {
                return;
            };
            let applied = Update::decode_v1(&item.update)
                .ok()
                .is_some_and(|u| doc.transact_mut().apply_update(u).is_ok());
            if !applied {
                self.close(item.conn, CloseReason::Invalid);
                continue;
            }
            self.dirty_state = true;
            self.dirty_since_snapshot = true;
            self.last_change = Instant::now();
            self.last_editor = Some(item.user);
            let message = Message::Sync(SyncMessage::Update(item.update)).encode_v1();
            self.broadcast(&message, Some(item.conn));
        }
    }

    /// 변경에 다른 계보 표시가 들어 있으면(방이 본문에서 새로 만들어진 뒤 옛 상태로 다시 붙은 클라이언트) 참.
    fn foreign_lineage(&self, update: &[u8]) -> bool {
        let probe = Doc::new();
        let applied = Update::decode_v1(update)
            .ok()
            .is_some_and(|u| probe.transact_mut().apply_update(u).is_ok());
        applied && lineage_of(&probe).is_some_and(|l| l != self.lineage)
    }

    /// 방 문서를 복사한 임시 문서에 변경을 넣어 본문 검사를 통과하는지 본다.
    fn passes_checks(&self, update: &[u8]) -> bool {
        let Some(doc) = &self.doc else {
            return false;
        };
        let state = doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let scratch = Doc::new();
        let fragment = scratch.get_or_insert_xml_fragment(FRAGMENT);
        let ok = [state.as_slice(), update].iter().all(|bytes| {
            Update::decode_v1(bytes)
                .ok()
                .is_some_and(|u| scratch.transact_mut().apply_update(u).is_ok())
        });
        if !ok {
            return false;
        }
        // 적용하지 못하고 남은 부분(아직 오지 않은 앞선 변경에 기대는 것)이 있으면 받지 않는다.
        if scratch.transact().has_missing_updates() {
            return false;
        }
        let body = convert::read_doc(&scratch.transact(), &fragment);
        documents::validate_body(&body, &self.limits).is_ok()
    }

    fn leave(&mut self, conn: u64) {
        let Some(connection) = self.conns.remove(&conn) else {
            return;
        };
        // 나간 연결의 참여자 표시를 지운다(시계를 올린 "null" 항목을 보내면 브라우저도 지운다).
        let mut gone = HashMap::new();
        for id in connection.clients {
            if let Some(old) = self.presence.remove(&id) {
                gone.insert(
                    id,
                    AwarenessUpdateEntry {
                        clock: old.clock.saturating_add(1),
                        json: Arc::from("null"),
                    },
                );
            }
        }
        if !gone.is_empty() {
            self.broadcast(
                &Message::Awareness(AwarenessUpdate { clients: gone }).encode_v1(),
                None,
            );
        }
    }

    fn close(&mut self, conn: u64, reason: CloseReason) {
        if let Some(c) = self.conns.get(&conn) {
            let _ignored = c.peer.out.try_send(Outbound::Close(reason));
        }
        self.leave(conn);
    }

    fn send(&mut self, out: &mpsc::Sender<Outbound>, bytes: Vec<u8>, conn: u64) {
        if out.try_send(Outbound::Binary(bytes)).is_err() {
            self.close(conn, CloseReason::Slow);
        }
    }

    fn broadcast(&mut self, bytes: &[u8], except: Option<u64>) {
        let slow: Vec<u64> = self
            .conns
            .iter()
            .filter(|(id, _)| Some(**id) != except)
            .filter(|(_, c)| {
                c.peer
                    .out
                    .try_send(Outbound::Binary(bytes.to_vec()))
                    .is_err()
            })
            .map(|(id, _)| *id)
            .collect();
        for id in slow {
            self.close(id, CloseReason::Slow);
        }
    }

    /// 편집 중 Yjs 상태 저장(서버 작업 문맥).
    async fn persist(&mut self) {
        if !self.dirty_state {
            return;
        }
        let Some(doc) = &self.doc else {
            return;
        };
        let state = doc
            .transact()
            .encode_state_as_update_v1(&StateVector::default());
        let result = async {
            let mut tx = self.db.begin_anonymous().await?;
            let stored: bool = sqlx::query_scalar("SELECT app_system_collab_store($1, $2, $3)")
                .bind(self.doc_id)
                .bind(self.base_revision)
                .bind(state)
                .fetch_one(tx.conn())
                .await?;
            tx.commit().await?;
            Ok::<bool, AppError>(stored)
        }
        .await;
        match result {
            Ok(_) => self.dirty_state = false,
            Err(err) => {
                tracing::warn!(document = %self.doc_id.0, error = %err, "could not store live editing state")
            }
        }
    }

    /// 방 문서를 저장 API와 같은 길로 리비전에 남긴다(마지막으로 고친 사람이 작성자).
    async fn snapshot(&mut self) {
        if !self.dirty_since_snapshot {
            return;
        }
        let (Some(doc), Some(editor)) = (&self.doc, self.last_editor) else {
            return;
        };
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let body = convert::read_doc(&doc.transact(), &fragment);
        if documents::validate_body(&body, &self.limits).is_err() {
            tracing::warn!(document = %self.doc_id.0, "live document failed checks; not saved as a revision");
            return;
        }
        match self.write_snapshot(editor, &body).await {
            Ok(Some(revision)) => {
                self.base_revision = revision;
                self.base_body = body;
                self.dirty_since_snapshot = false;
                self.dirty_state = true;
            }
            Ok(None) => {
                // 본문이 다른 길로 바뀌었다(복원). 덮어쓰지 않고 다시 열게 한다.
                for (_, c) in self.conns.drain() {
                    let _ignored = c.peer.out.try_send(Outbound::Close(CloseReason::Reload));
                }
                self.dirty_since_snapshot = false;
                self.dirty_state = false;
            }
            Err(err) => {
                tracing::warn!(document = %self.doc_id.0, error = %err, "could not save the live document as a revision");
            }
        }
    }

    /// 새 리비전 번호, 또는 본문이 다른 길로 바뀌었으면 `None`.
    async fn write_snapshot(&self, editor: UserId, body: &Value) -> Result<Option<i32>, AppError> {
        let mut tx = self.db.begin_as(editor).await?;
        sqlx::query("SELECT app_lock_item((SELECT item_id FROM documents WHERE id = $1))")
            .bind(self.doc_id)
            .execute(tx.conn())
            .await?;
        let access = documents::access(&mut tx, self.doc_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if !access.can_write() {
            return Err(AppError::Forbidden);
        }
        let current = documents::load_for_update(&mut tx, self.doc_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if current.body != self.base_body {
            tx.commit().await?;
            return Ok(None);
        }
        if current.body == *body {
            tx.commit().await?;
            return Ok(Some(current.revision));
        }
        let title = current.title.clone();
        let row =
            documents::write_revision(&mut tx, self.doc_id, &title, body, Uuid::now_v7()).await?;
        tx.commit().await?;
        Ok(Some(row.revision))
    }

    /// 연결마다 세션과 권한을 다시 본다. 끝났으면 닫고, 쓰기 권한이 바뀌었으면 따른다.
    async fn recheck(&mut self) {
        self.last_recheck = Instant::now();
        let peers: Vec<(u64, Peer)> = self
            .conns
            .iter()
            .map(|(id, c)| (*id, c.peer.clone()))
            .collect();
        for (conn, peer) in peers {
            match peer_access(&self.db, self.doc_id, &peer).await {
                Some(can_write) => {
                    if let Some(c) = self.conns.get_mut(&conn) {
                        c.peer.can_write = can_write;
                    }
                }
                None => self.close(conn, CloseReason::AccessLost),
            }
        }
    }
}

/// 세션이 살아 있고 문서를 실시간으로 열 수 있으면 쓰기 가능 여부.
pub async fn peer_access(db: &Db, doc: DocumentId, peer: &Peer) -> Option<bool> {
    let mut tx = db.begin_anonymous().await.ok()?;
    let user: Option<UserId> = sqlx::query_scalar("SELECT app_session_user($1)")
        .bind(&peer.session)
        .fetch_one(tx.conn())
        .await
        .ok()?;
    tx.commit().await.ok()?;
    if user != Some(peer.user) {
        return None;
    }
    let mut tx = db.begin_as(peer.user).await.ok()?;
    let access: String = sqlx::query_scalar("SELECT app_collab_access($1)")
        .bind(doc)
        .fetch_one(tx.conn())
        .await
        .ok()?;
    tx.commit().await.ok()?;
    Some(matches!(access.as_str(), "owner" | "write"))
}

/// 계보 표시를 두는 Yjs 맵. 편집기(조각 `default`)는 이 맵을 보지 않는다.
const META: &str = "fvoci";
const LINEAGE: &str = "lineage";

fn meta(doc: &Doc) -> yrs::MapRef {
    doc.get_or_insert_map(META)
}

fn lineage_of(doc: &Doc) -> Option<String> {
    let map = meta(doc);
    match map.get(&doc.transact(), LINEAGE) {
        Some(yrs::Out::Any(yrs::Any::String(s))) => Some(s.to_string()),
        _ => None,
    }
}
