//! 문서 하나의 실시간 편집 방(D-70). 방마다 작업 하나가 Yjs 문서를 혼자 맡고(잠금 없음), 연결은 메시지만 주고받는다.
//!
//! - **경계**: 쓰기 권한이 있는 연결의 변경만 받는다. 연결마다 모인 변경을 합쳐, 방 문서를 복사한 임시 문서에 먼저
//!   넣고 편집기 스키마 모양(`convert::check_shape`)과 저장 API의 본문 검사(`validate_body`)를 통과해야 방 문서에 넣고
//!   다른 연결에 보낸다. 검사는 블로킹 작업으로 돌린다. 통과하지 못하면 버리고 그 연결을 닫는다(4400). 읽기 연결의
//!   변경은 받지 않고 버린다(편집기가 스스로 고친 것일 수 있어 닫지는 않는다).
//! - **한도**: 한 프레임의 메시지 수, 연결마다 초당 바이트·메시지 수, Yjs 상태 크기, 참여자 표시 수·크기.
//! - **저장**: 몇 초마다 Yjs 상태를 저장하고, 60초 조용하거나 마지막 사람이 나가면 저장 API와 같은 길
//!   (`documents::write_revision`)으로 리비전을 만든다. 마지막으로 고친 사람이 쓸 수 없으면 연결된 다른 작성자로 쓴다.
//!   그 사이 본문이 다른 길(복원)로 바뀌었으면 쓰지 않고 방을 닫는다.
//! - **한 서버**: 방은 메모리에 있다. 셀프호스트의 한 앱 프로세스(`role=All`)를 전제로 한다. 끝낼 때 모든 방의 리비전을
//!   남긴다.

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
/// 방이 상태를 DB에 저장한 뒤 보내는 확인(y-sync 사용자 정의 메시지). 저장한 상태의 스냅숏(지움 기록 + 상태 벡터,
/// v1)을 담는다. 클라이언트는 자기 편집이 그 안에 들었을 때만 복구용 초안을 지운다(연결 상태를 저장 확인으로 보지 않음).
pub const MSG_PERSISTED: u8 = 100;
/// 한 프레임에 담을 수 있는 메시지 수(y-websocket은 보통 하나).
const MAX_MESSAGES_PER_FRAME: usize = 16;
/// 연결마다 1초에 받을 수 있는 메시지 수와 바이트(넘치면 닫고 다시 붙게 한다).
const MAX_MESSAGES_PER_SECOND: u32 = 120;
const MAX_BYTES_PER_SECOND: usize = 4 * 1024 * 1024;
/// 한 연결이 알릴 수 있는 참여자 id 수와 항목 크기.
const MAX_PRESENCE_IDS: usize = 4;
const MAX_PRESENCE_BYTES: usize = 4 * 1024;

/// 닫는 이유(웹소켓 닫기 코드). 4400~4499는 y-websocket이 다시 붙지 않는 코드다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// 내용 검사 실패, 잘못된 메시지. 다시 붙지 않는다.
    Invalid,
    /// 권한·세션이 끝남. 다시 붙지 않는다.
    AccessLost,
    /// 문서가 다른 길로 바뀌었거나 클라이언트 상태가 맞지 않아 새 문서로 다시 열어야 함.
    Reload,
    /// 워크스페이스가 실시간을 끔.
    RealtimeOff,
    /// 너무 느리거나 너무 많이 보냄. 잠시 뒤 다시 붙는다.
    Busy,
    /// 서버를 다시 시작함. 다시 붙는다.
    Restart,
}

impl CloseReason {
    #[must_use]
    pub const fn code(self) -> u16 {
        match self {
            Self::Invalid => 4400,
            Self::AccessLost => 4403,
            Self::Reload => 4409,
            Self::RealtimeOff => 4410,
            Self::Busy => 4503,
            Self::Restart => 1012,
        }
    }

    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Invalid => "invalid message",
            Self::AccessLost => "access ended",
            Self::Reload => "document changed; reload",
            Self::RealtimeOff => "realtime editing turned off",
            Self::Busy => "too much traffic; reconnect later",
            Self::Restart => "server restarting",
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
    /// 모아 둔 변경을 넣고 상태(파일 참조 포함)를 지금 저장한다. 파일 버전을 지우기 직전에 쓴다.
    Persist {
        done: oneshot::Sender<()>,
    },
    Shutdown {
        reason: CloseReason,
        snapshot: bool,
        /// 닫기 전에 방의 편집이 모두 리비전으로 남았는지(남길 것이 없었으면 참).
        saved: Option<oneshot::Sender<bool>>,
        /// 편집한 사람들이 리비전을 남길 수 없을 때 대신 작성자가 될 사람(실시간을 끄는 관리자).
        fallback: Option<UserId>,
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
    /// 실시간을 끄는 중인 워크스페이스. 이 동안은 방을 새로 열지 않는다(방 목록과 같은 잠금 아래에서 본다).
    closing: Arc<Mutex<HashMap<Uuid, usize>>>,
    next: Arc<AtomicU64>,
}

/// 실시간을 끄는 동안 그 워크스페이스에 방이 새로 열리지 않게 막는다. 놓으면 풀린다.
#[derive(Debug)]
pub struct ClosingGuard {
    rooms: Rooms,
    workspace: Uuid,
}

impl Drop for ClosingGuard {
    fn drop(&mut self) {
        let _rooms = self.rooms.lock();
        let mut closing = self.rooms.closing_set();
        // 같은 워크스페이스를 동시에 끄는 다른 요청이 있으면 그 막음은 남긴다.
        if let Some(count) = closing.get_mut(&self.workspace) {
            *count -= 1;
            if *count == 0 {
                closing.remove(&self.workspace);
            }
        }
    }
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
        // 대기열이 가득 차 못 보내도 방은 닫힌 연결을 주기적으로 걸러 낸다.
        let _ignored = self.tx.try_send(Command::Leave { conn: self.conn });
    }
}

impl Rooms {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<DocumentId, Handle>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 끄는 중인 워크스페이스 목록. 늘 방 목록 잠금(`lock`)을 먼저 잡은 뒤에 잡는다.
    fn closing_set(&self) -> std::sync::MutexGuard<'_, HashMap<Uuid, usize>> {
        self.closing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 이 워크스페이스에 방이 새로 열리지 않게 한다(실시간 끄기 동안). 이미 열린 방은 `close_workspace`로 닫는다.
    pub fn begin_closing(&self, workspace: Uuid) -> ClosingGuard {
        let _rooms = self.lock();
        *self.closing_set().entry(workspace).or_default() += 1;
        ClosingGuard {
            rooms: self.clone(),
            workspace,
        }
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
        for _ in 0..5 {
            let handle = {
                let mut rooms = self.lock();
                // 실시간을 끄는 중이면 열지 않는다. 클라이언트는 잠시 뒤 다시 붙고, 그때 꺼졌으면 4410을 받는다.
                if workspace.is_some_and(|w| self.closing_set().contains_key(&w)) {
                    return Err(AppError::Upstream(
                        "live editing is being turned off".into(),
                    ));
                }
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
                // 방이 닫히는 중이었다(답 없이 버려짐). 새로 연다.
                Err(_) => tokio::task::yield_now().await,
            }
        }
        Err(AppError::Upstream(
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
                    saved: None,
                    fallback: None,
                })
                .await;
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
    async fn persist_where(&self, keep: impl Fn(&Handle) -> bool) {
        let handles: Vec<Handle> = self.lock().values().filter(|h| keep(h)).cloned().collect();
        let waits = handles.into_iter().map(|handle| async move {
            let (done, answer) = oneshot::channel();
            if handle.tx.try_send(Command::Persist { done }).is_ok() {
                let _ignored = answer.await;
            }
        });
        let _ignored = tokio::time::timeout(
            Duration::from_secs(2),
            futures_util::future::join_all(waits),
        )
        .await;
    }

    /// 워크스페이스가 실시간을 끈다(또는 껐다). 그 워크스페이스의 방은 리비전을 남기고 닫는다. 끝날 때까지 기다린다.
    /// 모든 방의 편집이 리비전으로 남았으면 참(`begin_closing`과 함께 써서 그동안 새 방이 열리지 않게 한다).
    pub async fn close_workspace(&self, workspace: Uuid, admin: UserId) -> bool {
        let handles: Vec<Handle> = self
            .lock()
            .values()
            .filter(|h| h.workspace == Some(workspace))
            .cloned()
            .collect();
        close_all(handles, CloseReason::RealtimeOff, Some(admin)).await
    }

    /// 방 없이 남은 실시간 상태(리비전으로 남지 않은 편집)를 리비전으로 남긴다. 실시간을 끄기 전, 방을 모두 닫은
    /// 뒤(`begin_closing` 아래)에 쓴다. 작성자는 실시간을 끄는 관리자. 남겼거나 남길 것이 없으면 참.
    pub async fn save_pending(
        &self,
        db: &Db,
        limits: &Limits,
        doc: DocumentId,
        admin: UserId,
    ) -> bool {
        let generation = self.next.fetch_add(1, Ordering::Relaxed);
        let mut room = Room::new(
            doc,
            generation,
            db.clone(),
            *limits,
            Duration::from_secs(30),
            self.clone(),
        );
        match room.load(admin).await {
            Ok(loaded) => room.doc = Some(loaded),
            Err(err) => {
                tracing::warn!(event = "collab.load_failed", document = %doc.0, error_code = err.code(), detail = crate::logging::safe_error(&err), "could not load a pending live state");
                return false;
            }
        }
        room.dirty_since_snapshot = true;
        room.snapshot_with(Some(admin)).await
    }

    /// 서버를 끝낸다. 모든 방의 리비전을 남기고 닫는다(클라이언트는 다시 붙는다).
    pub async fn shutdown(&self) {
        let handles: Vec<Handle> = self.lock().values().cloned().collect();
        let _saved = close_all(handles, CloseReason::Restart, None).await;
    }

    fn remove(&self, doc: DocumentId, generation: u64) {
        let mut rooms = self.lock();
        if rooms.get(&doc).is_some_and(|h| h.generation == generation) {
            rooms.remove(&doc);
        }
    }
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

struct Connection {
    peer: Peer,
    /// 이 연결이 알린 참여자(awareness) id. 다른 연결은 이 id를 바꿀 수 없고, 나가면 지운다.
    clients: HashSet<ClientID>,
    /// 1초 창의 받은 메시지·바이트.
    window: Instant,
    messages: u32,
    bytes: usize,
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
    /// 참여자 id를 처음 알린 연결.
    presence_owner: HashMap<ClientID, u64>,
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
    /// 마지막으로 DB에 저장한(또는 저장된 것을 불러온) 상태의 스냅숏(v1). 들어오는 연결에 처음 메시지와 함께 보낸다.
    persisted: Option<Vec<u8>>,
    /// 고친 사람(마지막이 앞). 리비전 작성자를 고를 때 쓴다.
    editors: Vec<UserId>,
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
            presence_owner: HashMap::new(),
            conns: HashMap::new(),
            pending: Vec::new(),
            base_revision: 0,
            base_body: Value::Null,
            lineage: String::new(),
            dirty_state: false,
            dirty_since_snapshot: false,
            last_change: Instant::now(),
            persisted: None,
            editors: Vec::new(),
            last_recheck: Instant::now(),
            recheck_every,
        }
    }

    /// Yjs 상태 크기 한도: 본문 한도의 네 배(지운 흔적 등 Yjs의 덧붙는 몫을 둔다).
    fn max_state_bytes(&self) -> usize {
        self.limits
            .max_document_bytes
            .saturating_mul(4)
            .max(1 << 20)
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
                _ = flush.tick() => self.flush().await,
                _ = persist.tick() => {
                    self.flush().await;
                    self.prune();
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
                if peer.out.try_send(Outbound::Binary(start)).is_ok() {
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
            Command::Incoming { conn, data } => {
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
            Command::Shutdown {
                reason,
                snapshot,
                saved,
                fallback,
            } => {
                let kept = if snapshot {
                    self.flush().await;
                    let kept = self.snapshot_with(fallback).await;
                    self.persist().await;
                    kept
                } else {
                    true
                };
                // 실시간을 끄려는데 리비전을 남기지 못했으면 방을 닫지 않는다. 끄기는 거절되고(켜진 채), 편집은 방에
                // 그대로 남아 다음 시도에서 다시 리비전으로 남긴다.
                if !kept && reason == CloseReason::RealtimeOff {
                    if let Some(saved) = saved {
                        let _ignored = saved.send(false);
                    }
                    return true;
                }
                for (_, c) in self.conns.drain() {
                    let _ignored = c.peer.out.try_send(Outbound::Close(reason));
                }
                if let Some(saved) = saved {
                    let _ignored = saved.send(kept);
                }
                false
            }
        }
    }

    /// 저장된 Yjs 상태(그 뒤로 본문이 바뀌지 않았을 때) 또는 본문에서 방 문서를 만든다. 들어오는 사람의 권한으로 읽는다.
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
        let restored = state
            .and_then(|bytes| Update::decode_v1(&bytes).ok())
            .is_some_and(|update| doc.transact_mut().apply_update(update).is_ok());
        if !restored || lineage_of(&doc).is_none() {
            let doc = Doc::new();
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            let map = meta(&doc);
            let mut txn = doc.transact_mut();
            convert::write_doc(&mut txn, &fragment, &body);
            map.insert(&mut txn, LINEAGE, Uuid::now_v7().to_string());
            drop(txn);
            self.dirty_state = true;
            self.lineage = lineage_of(&doc).unwrap_or_default();
            self.base_revision = revision;
            self.base_body = body;
            return Ok(doc);
        }
        self.lineage = lineage_of(&doc).unwrap_or_default();
        self.base_revision = revision;
        self.base_body = body;
        // 저장된 상태에서 불러왔으니 이 상태는 이미 DB에 있다.
        self.persisted = Some(doc.transact().snapshot().encode_v1());
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
        // 이미 저장된 상태도 알린다: 다시 붙은 클라이언트의 편집이 그 안에 있으면 새 저장 없이 확인된다.
        if let Some(snapshot) = &self.persisted {
            out.extend(Message::Custom(MSG_PERSISTED, snapshot.clone()).encode_v1());
        }
        out
    }

    /// 지금 있는 참여자 표시 전부(없으면 `None`).
    fn presence_update(&self) -> Option<AwarenessUpdate> {
        let clients: HashMap<ClientID, AwarenessUpdateEntry> = self
            .presence
            .iter()
            .map(|(id, e)| (*id, e.clone()))
            .collect();
        (!clients.is_empty()).then_some(AwarenessUpdate { clients })
    }

    /// 1초 창 안의 메시지·바이트가 한도 안이면 참.
    fn within_rate(&mut self, conn: u64, messages: u32, bytes: usize) -> bool {
        let Some(c) = self.conns.get_mut(&conn) else {
            return false;
        };
        if c.window.elapsed() >= Duration::from_secs(1) {
            c.window = Instant::now();
            c.messages = 0;
            c.bytes = 0;
        }
        c.messages = c.messages.saturating_add(messages);
        c.bytes = c.bytes.saturating_add(bytes);
        c.messages <= MAX_MESSAGES_PER_SECOND && c.bytes <= MAX_BYTES_PER_SECOND
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
            .take(MAX_MESSAGES_PER_FRAME + 1)
            .collect::<Result<_, _>>()
            .unwrap_or_default();
        if (messages.is_empty() && !data.is_empty()) || messages.len() > MAX_MESSAGES_PER_FRAME {
            self.close(conn, CloseReason::Invalid);
            return;
        }
        let count = u32::try_from(messages.len()).unwrap_or(u32::MAX);
        if !self.within_rate(conn, count, data.len()) {
            self.close(conn, CloseReason::Busy);
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
                        // 읽기 연결의 변경은 받지 않는다(편집기가 스스로 맞춘 것일 수 있어 닫지는 않는다).
                        tracing::debug!(event = "collab.read_only_change", document = %self.doc_id.0, "ignored a change from a read-only connection");
                        continue;
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
        // 이 연결이 처음 알린 id만, 더 새로운(시계가 큰) 항목만 받는다. 표시 내용(JSON 글)은 해석하지 않고 그대로 전한다.
        let mut fresh = HashMap::new();
        for (id, entry) in update.clients {
            let owner = self.presence_owner.get(&id).copied();
            if owner.is_some_and(|o| o != conn) {
                continue;
            }
            let owned_count = self.conns.get(&conn).map_or(0, |c| c.clients.len());
            if owner.is_none() && owned_count >= MAX_PRESENCE_IDS {
                continue;
            }
            let newer = self
                .presence
                .get(&id)
                .is_none_or(|old| entry.clock > old.clock);
            if !newer || entry.json.len() > MAX_PRESENCE_BYTES {
                continue;
            }
            if &*entry.json == "null" {
                // 그 참여자가 떠났다.
                self.presence.remove(&id);
            } else {
                self.presence.insert(id, entry.clone());
            }
            self.presence_owner.insert(id, conn);
            if let Some(c) = self.conns.get_mut(&conn) {
                c.clients.insert(id);
            }
            fresh.insert(id, entry);
        }
        if fresh.is_empty() {
            return;
        }
        let encoded = Message::Awareness(AwarenessUpdate { clients: fresh }).encode_v1();
        // 보낸 연결에도 돌려준다. y-websocket은 30초 동안 아무것도 받지 못하면 끊고 다시 붙으므로, 혼자 편집할 때는
        // 15초마다 오는 자기 표시 갱신이 연결을 살려 둔다(참조 서버와 같다).
        self.broadcast(&encoded, None);
    }

    /// 모인 변경을 연결마다 합쳐 검사(블로킹 작업)한 뒤 방 문서에 넣고 다른 연결에 보낸다.
    async fn flush(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let pending = std::mem::take(&mut self.pending);
        let mut order: Vec<u64> = Vec::new();
        let mut by_conn: HashMap<u64, (UserId, Vec<Vec<u8>>)> = HashMap::new();
        for item in pending {
            if !order.contains(&item.conn) {
                order.push(item.conn);
            }
            by_conn
                .entry(item.conn)
                .or_insert_with(|| (item.user, Vec::new()))
                .1
                .push(item.update);
        }
        for conn in order {
            let Some((user, updates)) = by_conn.remove(&conn) else {
                continue;
            };
            if !self.conns.contains_key(&conn) {
                continue;
            }
            let Ok(merged) = yrs::merge_updates_v1(&updates) else {
                self.close(conn, CloseReason::Invalid);
                continue;
            };
            let Some(doc) = &self.doc else {
                return;
            };
            let state = doc
                .transact()
                .encode_state_as_update_v1(&StateVector::default());
            let limits = self.limits;
            let lineage = self.lineage.clone();
            let max_state = self.max_state_bytes();
            let candidate = merged.clone();
            let verdict = tokio::task::spawn_blocking(move || {
                check_update(&state, &candidate, &lineage, &limits, max_state)
            })
            .await
            .unwrap_or(Verdict::Invalid("check task failed".into()));
            match verdict {
                Verdict::Ok => {}
                Verdict::Reload => {
                    self.close(conn, CloseReason::Reload);
                    continue;
                }
                Verdict::Invalid(reason) => {
                    // 이유는 우리가 붙인 고정 분류다(문서 내용은 담지 않는다, D-83).
                    tracing::warn!(
                        event = "collab.edit_rejected",
                        document = %self.doc_id.0,
                        reason,
                        "rejected a live edit that fails document checks"
                    );
                    self.close(conn, CloseReason::Invalid);
                    continue;
                }
            }
            let Some(doc) = &self.doc else {
                return;
            };
            // 다시 붙은 연결의 동기화 답에는 이미 반영된 지움 기록이 늘 함께 오므로(Yjs), 실제로 바뀐 것만 편집으로
            // 친다. 그렇지 않으면 다시 붙을 때마다 조용한 시간이 처음부터 시작돼 리비전이 남지 않는다.
            let before = doc.transact().state_vector();
            let applied = Update::decode_v1(&merged).ok().and_then(|u| {
                let mut txn = doc.transact_mut();
                txn.apply_update(u).ok()?;
                Some(!txn.delete_set().is_empty())
            });
            let Some(deleted) = applied else {
                self.close(conn, CloseReason::Invalid);
                continue;
            };
            if !deleted && doc.transact().state_vector() == before {
                continue;
            }
            self.dirty_state = true;
            self.dirty_since_snapshot = true;
            self.last_change = Instant::now();
            self.editors.retain(|u| *u != user);
            self.editors.insert(0, user);
            let message = Message::Sync(SyncMessage::Update(merged)).encode_v1();
            self.broadcast(&message, Some(conn));
        }
    }

    /// 닫힌 연결(웹소켓이 끝났는데 나감 알림이 대기열에 못 들어간 것)을 걸러 낸다.
    fn prune(&mut self) {
        let closed: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, c)| c.peer.out.is_closed())
            .map(|(id, _)| *id)
            .collect();
        for id in closed {
            self.leave(id);
        }
    }

    fn leave(&mut self, conn: u64) {
        let Some(connection) = self.conns.remove(&conn) else {
            return;
        };
        // 나간 연결의 참여자 표시를 지운다(시계를 올린 "null" 항목을 보내면 브라우저도 지운다).
        let mut gone = HashMap::new();
        for id in connection.clients {
            self.presence_owner.remove(&id);
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
            self.close(conn, CloseReason::Busy);
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
            self.close(id, CloseReason::Busy);
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
        // 저장하는 상태와 그 스냅숏(저장 확인에 담는다)을 같은 순간에서 만든다.
        let (state, snapshot) = {
            let txn = doc.transact();
            (
                txn.encode_state_as_update_v1(&StateVector::default()),
                txn.snapshot().encode_v1(),
            )
        };
        // 리비전이 되지 않은 편집이 있으면 본문도 넘겨, 그 본문이 가리키는 파일 버전을 붙잡게 한다(마이그레이션 0059).
        let body = self.dirty_since_snapshot.then(|| {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            convert::read_doc(&doc.transact(), &fragment)
        });
        let result = async {
            let mut tx = self.db.begin_anonymous().await?;
            // 리비전으로 아직 남지 않은 편집이 있는지도 적는다(실시간을 끄기 전에 남긴다, 마이그레이션 0050).
            let stored: bool =
                sqlx::query_scalar("SELECT app_system_collab_store($1, $2, $3, $4, $5)")
                    .bind(self.doc_id)
                    .bind(self.base_revision)
                    .bind(state)
                    .bind(self.dirty_since_snapshot)
                    .bind(body)
                    .fetch_one(tx.conn())
                    .await?;
            tx.commit().await?;
            Ok::<bool, AppError>(stored)
        }
        .await;
        match result {
            Ok(stored) => {
                self.dirty_state = false;
                // DB에 들어간 것만 확인으로 알린다(낡은 상태라 거절됐으면 알리지 않는다).
                if stored {
                    let message = Message::Custom(MSG_PERSISTED, snapshot.clone()).encode_v1();
                    self.persisted = Some(snapshot);
                    self.broadcast(&message, None);
                }
            }
            Err(err) => {
                tracing::warn!(event = "collab.store_failed", document = %self.doc_id.0, error_code = err.code(), detail = crate::logging::safe_error(&err), "could not store live editing state");
            }
        }
    }

    /// 방 문서를 저장 API와 같은 길로 리비전에 남긴다. 작성자는 마지막으로 고친 사람, 그 사람이 쓸 수 없으면
    /// 최근에 고친 다른 사람, 그다음은 연결된 작성자 순.
    async fn snapshot(&mut self) -> bool {
        self.snapshot_with(None).await
    }

    /// `fallback`: 편집한 사람·연결된 작성자 모두 남길 수 없을 때의 작성자(실시간을 끄는 관리자).
    async fn snapshot_with(&mut self, fallback: Option<UserId>) -> bool {
        if !self.dirty_since_snapshot {
            return true;
        }
        let Some(doc) = &self.doc else {
            return true;
        };
        let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
        let body = convert::read_doc(&doc.transact(), &fragment);
        if documents::validate_body(&body, &self.limits).is_err() {
            tracing::warn!(event = "collab.invalid_document", document = %self.doc_id.0, "live document failed checks; not saved as a revision");
            return false;
        }
        let mut authors = self.editors.clone();
        for c in self.conns.values().filter(|c| c.peer.can_write) {
            if !authors.contains(&c.peer.user) {
                authors.push(c.peer.user);
            }
        }
        if let Some(admin) = fallback.filter(|a| !authors.contains(a)) {
            authors.push(admin);
        }
        for author in authors {
            match self.write_snapshot(author, &body).await {
                Ok(Some(revision)) => {
                    self.base_revision = revision;
                    self.base_body = body;
                    self.dirty_since_snapshot = false;
                    self.dirty_state = true;
                    // 리비전 번호와 상태가 어긋난 채로 멈추지 않도록 바로 저장한다.
                    self.persist().await;
                    return true;
                }
                Ok(None) => {
                    // 본문이 다른 길로 바뀌었다(복원). 덮어쓰지 않고 다시 열게 한다.
                    tracing::warn!(event = "collab.changed_outside", document = %self.doc_id.0, "document changed outside live editing; reopening");
                    for (_, c) in self.conns.drain() {
                        let _ignored = c.peer.out.try_send(Outbound::Close(CloseReason::Reload));
                    }
                    self.dirty_since_snapshot = false;
                    self.dirty_state = false;
                    return true;
                }
                Err(AppError::Forbidden | AppError::NotFound) => {}
                Err(err) => {
                    tracing::warn!(event = "collab.save_failed", document = %self.doc_id.0, error_code = err.code(), detail = crate::logging::safe_error(&err), "could not save the live document as a revision");
                    return false;
                }
            }
        }
        tracing::warn!(event = "collab.no_editor", document = %self.doc_id.0, "no editor can save the live document; keeping the live state");
        false
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

    /// 연결마다 세션과 권한을 다시 본다. 끝났거나 쓰기 권한을 잃었으면 닫고(4403, 다시 붙으면 지금 권한으로),
    /// 실시간이 꺼졌으면 리비전을 남기고 닫으며, 읽기에서 쓰기로 바뀌었으면 따른다. 일시적인 DB 오류로는 닫지 않는다.
    async fn recheck(&mut self) {
        self.last_recheck = Instant::now();
        let peers: Vec<(u64, Peer)> = self
            .conns
            .iter()
            .map(|(id, c)| (*id, c.peer.clone()))
            .collect();
        let mut off = false;
        for (conn, peer) in peers {
            match peer_access(&self.db, self.doc_id, &peer).await {
                PeerAccess::Allowed { can_write } => {
                    // 쓰기 권한을 잃은 연결은 닫는다. 열어 두면 화면은 "연결됨"인 채로 그 사람의 입력을 조용히 버린다.
                    if !can_write && peer.can_write {
                        self.close(conn, CloseReason::AccessLost);
                    } else if let Some(c) = self.conns.get_mut(&conn) {
                        c.peer.can_write = can_write;
                    }
                }
                PeerAccess::Denied => self.close(conn, CloseReason::AccessLost),
                PeerAccess::RealtimeOff => off = true,
                PeerAccess::Unknown => {}
            }
        }
        if off {
            self.flush().await;
            self.snapshot().await;
            self.persist().await;
            for (_, c) in self.conns.drain() {
                let _ignored = c
                    .peer
                    .out
                    .try_send(Outbound::Close(CloseReason::RealtimeOff));
            }
        }
    }
}

enum Verdict {
    Ok,
    /// 거절 이유(로그용 고정 분류).
    Invalid(String),
    Reload,
}

/// 방 상태에 변경을 넣은 임시 문서가 계보·모양·크기·본문 검사를 통과하는지(블로킹 작업에서 돈다).
fn check_update(
    state: &[u8],
    update: &[u8],
    lineage: &str,
    limits: &Limits,
    max_state: usize,
) -> Verdict {
    // 변경 혼자에 다른 계보 표시가 있으면 옛 상태로 다시 붙은 클라이언트다.
    let probe = Doc::new();
    let Ok(alone) = Update::decode_v1(update) else {
        return Verdict::Invalid("update does not decode".into());
    };
    if probe.transact_mut().apply_update(alone).is_err() {
        return Verdict::Invalid("update does not apply".into());
    }
    if lineage_of(&probe).is_some_and(|l| l != lineage) {
        return Verdict::Reload;
    }
    let scratch = Doc::new();
    let fragment = scratch.get_or_insert_xml_fragment(FRAGMENT);
    let applied = [state, update].iter().all(|bytes| {
        Update::decode_v1(bytes)
            .ok()
            .is_some_and(|u| scratch.transact_mut().apply_update(u).is_ok())
    });
    if !applied {
        return Verdict::Invalid("update does not apply to the room state".into());
    }
    let txn = scratch.transact();
    // 앞선 변경이 빠져 적용하지 못하고 남은 것이 있으면 이 클라이언트 상태는 서버와 어긋났다. 새로 열게 한다.
    if txn.has_missing_updates() {
        return Verdict::Reload;
    }
    if txn.encode_state_as_update_v1(&StateVector::default()).len() > max_state {
        return Verdict::Invalid("state too large".into());
    }
    if let Err(reason) = convert::check_shape(&txn) {
        return Verdict::Invalid(reason.into());
    }
    let body = convert::read_doc(&txn, &fragment);
    match documents::validate_body(&body, limits) {
        Ok(()) => Verdict::Ok,
        // 본문 검사의 설명은 우리가 쓴 고정 문구와 개수뿐이다.
        Err(AppError::BadRequest(reason)) => Verdict::Invalid(reason),
        Err(other) => Verdict::Invalid(other.code().into()),
    }
}

/// 다시 확인한 연결의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerAccess {
    Allowed {
        can_write: bool,
    },
    Denied,
    RealtimeOff,
    /// DB 오류 등으로 알 수 없음. 다음에 다시 본다.
    Unknown,
}

/// 세션이 살아 있고 문서를 실시간으로 열 수 있는지.
pub async fn peer_access(db: &Db, doc: DocumentId, peer: &Peer) -> PeerAccess {
    let session = async {
        let mut tx = db.begin_anonymous().await?;
        let user: Option<UserId> = sqlx::query_scalar("SELECT app_session_user($1)")
            .bind(&peer.session)
            .fetch_one(tx.conn())
            .await?;
        tx.commit().await?;
        Ok::<Option<UserId>, AppError>(user)
    }
    .await;
    match session {
        Ok(user) if user == Some(peer.user) => {}
        Ok(_) => return PeerAccess::Denied,
        Err(_) => return PeerAccess::Unknown,
    }
    let access = async {
        let mut tx = db.begin_as(peer.user).await?;
        let access: String = sqlx::query_scalar("SELECT app_collab_access($1)")
            .bind(doc)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        tx.commit().await?;
        Ok::<String, AppError>(access)
    }
    .await;
    match access {
        Ok(a) => PeerAccess::Allowed {
            can_write: matches!(a.as_str(), "owner" | "write"),
        },
        Err(AppError::NotFound | AppError::Forbidden) => PeerAccess::Denied,
        Err(AppError::Conflict(_)) => PeerAccess::RealtimeOff,
        Err(_) => PeerAccess::Unknown,
    }
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
