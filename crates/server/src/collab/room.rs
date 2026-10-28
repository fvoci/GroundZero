//! 문서 하나의 실시간 편집 방(D-70). 한 작업이 Yjs 문서를 맡으며, 연결의 변경은 임시 문서에서 모양·본문·한도를
//! 검사한 뒤 반영한다. 읽기 연결의 변경은 버리고 잘못된 쓰기는 4400으로 닫는다.
//! 상태와 편집자·첨부 귀속을 주기적으로 저장하며, 60초 조용하거나 마지막 사람이 나가면 리비전을 남긴다.
//! 저장자는 최근 편집자 → 연결된 작성자 순이며 대리 저장의 실제 편집자는 edited_by로 구분한다(D-95).
//! 저장 확인은 DB에 들어간 상태의 스냅숏이다. 확인받지 못한 입력은 브라우저가 보존한다(D-99).
//! 복원·이동은 방 명령 안에서 처리한다. 공유 목록 오염은 방을 종료하고 저장 상태와 클라이언트 변경으로 재개한다(#172).
//! 셀프호스트 한 앱 프로세스(role=All)를 전제로 하며 정상 종료에는 모든 방의 리비전을 남긴다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;
use yrs::sync::awareness::AwarenessUpdateEntry;
use yrs::sync::{AwarenessUpdate, Message, MessageReader, SyncMessage};
use yrs::updates::decoder::{Decode, DecoderV1};
use yrs::updates::encoder::Encode;
use yrs::{ClientID, Doc, Map, ReadTxn, StateVector, Transact, Update};

use super::convert::{self, FRAGMENT};
use super::file_refs::{Attribution, LoadRow, new_document};
use super::lease::{Lease, MovePermit, WritePermit};
use super::visibility::{CommittedMove, Snapshot};
use crate::config::Limits;
use crate::features::documents::{self, MoveReply};
use fvoci_core::error::AppError;
use fvoci_core::ids::{DocumentId, UserId};
use fvoci_db::Db;

mod actor;
mod movement;
mod peers;
mod persistence;
#[doc(hidden)]
pub mod recovery;
mod registry;
mod updates;

pub(super) use peers::observe_access;
pub use peers::peer_access;
use peers::session_alive;
use persistence::lineage_of;

/// 방마다 들어오는 명령·메시지 대기열의 크기.
const ROOM_QUEUE: usize = 1024;
/// 작은 변경을 모아 검사하는 간격.
const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// Yjs 상태를 저장하는 간격.
const PERSIST_EVERY: Duration = Duration::from_secs(3);
/// 복원 전 체크포인트(리비전으로 남기지 않은 편집 남기기)를 기다리는 최대 시간. 넘으면 복원하지 않는다.
const CHECKPOINT_LIMIT: Duration = Duration::from_secs(5);

pub use fvoci_documents::state::{MoveRequest, RestoreRequest};

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
    /// 연결한 세션의 사용자가 연결 요청의 기대 사용자(`expected_user`, 편집기를 연 사용자)와 다름. 다른 탭에서 계정을
    /// 바꾼 뒤 앞 계정의 화면이 다시 붙었다. 방에 들어가기 전에 닫고 다시 붙지 않는다.
    AccountChanged,
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
            Self::AccountChanged => 4412,
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
            Self::AccountChanged => "signed-in account changed",
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
    pub out: mpsc::Sender<Outbound>,
    pub(crate) access: Lease,
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
        /// 웹소켓에서 받은 때. 문서를 옮긴 커밋 뒤에 받은 것은 넣지 않는다(새 자리에서 권한을 잃은 사람의 쓰기).
        received: Instant,
        permit: Option<WritePermit>,
    },
    Leave {
        conn: u64,
    },
    /// 모아 둔 변경을 넣고 상태(파일 참조 포함)를 지금 저장한다. 파일 버전을 지우기 직전에 쓴다.
    Persist {
        done: oneshot::Sender<()>,
    },
    /// 모아 둔 변경을 넣고 아직 리비전으로 남지 않은 편집을 지금 리비전으로 남긴다. 반출 직전에 쓴다.
    Snapshot {
        done: oneshot::Sender<()>,
    },
    /// 문서를 옮긴다(감사 S6, #71). 체크포인트 → 이동 트랜잭션 → (옮겼으면) 커밋 전에 받은 편집 넣기 → 리비전 남기기 →
    /// 연결 닫기(4409)를 이 명령 하나 안에서 한다. 그동안 들어온 메시지는 대기열에서 기다린다. 체크포인트가 실패하면
    /// 옮기지 않는다. 옮기지 않았으면 방은 그대로 이어 간다.
    Move {
        request: MoveRequest,
        done: oneshot::Sender<Result<MoveReply, AppError>>,
    },
    /// 옛 리비전으로 복원한다. 체크포인트(리비전으로 남기지 않은 편집 남기기) → 복원 → 방 초기화를 이 명령 하나 안에서
    /// 해 그 사이 편집·참여가 끼어들지 않게 한다. 체크포인트가 실패하거나 시간 안에 끝나지 않으면 복원하지 않는다.
    Restore {
        request: RestoreRequest,
        done: oneshot::Sender<Result<String, AppError>>,
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
    /// 옮기거나 방 없이 복원하는 중인 문서. 이 동안은 그 문서의 방을 새로 열지 않는다(방 목록과 같은 잠금 아래에서 본다).
    held: Arc<Mutex<HashMap<DocumentId, usize>>>,
    recovery: Arc<recovery::Epoch>,
    next: Arc<AtomicU64>,
}

/// 문서를 옮기거나 복원하는 동안 그 문서의 방이 새로 열리지 않게 막는다. 놓으면 풀린다.
#[derive(Debug)]
struct HoldGuard {
    rooms: Rooms,
    doc: DocumentId,
    epoch: u64,
}

/// 실시간을 끄는 동안 그 워크스페이스에 방이 새로 열리지 않게 막는다. 놓으면 풀린다.
#[derive(Debug)]
pub struct ClosingGuard {
    rooms: Rooms,
    workspace: Uuid,
    epoch: u64,
}

/// 방에 들어간 연결. 떨어지면 방에서 나간다.
#[derive(Debug)]
pub struct Membership {
    conn: u64,
    tx: mpsc::Sender<Command>,
    access: Lease,
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
    authorization: EditAuthorization,
}

#[derive(Debug, Clone)]
enum EditAuthorization {
    Current,
    BeforeMove { permit: MovePermit },
}

impl EditAuthorization {
    fn allows(&self, access: &Lease) -> bool {
        match self {
            Self::Current => access.permission() == Some(true),
            Self::BeforeMove { permit } => permit.deadline().is_some(),
        }
    }

    async fn check_session(&self, db: &Db, peer: &Peer) -> Result<(), CloseReason> {
        match self {
            Self::BeforeMove { permit } => permit.check_session(session_alive(db, peer)).await,
            Self::Current => match peer.access.until_closed(session_alive(db, peer)).await {
                Ok(Some(true)) => Ok(()),
                Ok(Some(false)) => Err(CloseReason::AccessLost),
                Ok(None) => Err(CloseReason::Busy),
                Err(reason) => Err(reason),
            },
        }
    }
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
    file_refs: Attribution,
    /// 방 문서의 계보 표시. 본문에서 새로 만들 때마다 바뀐다. 예전 계보를 가진 클라이언트가 다시 붙어 옛 내용을
    /// 섞지 않게 그런 연결은 다시 열게 한다(4409).
    lineage: String,
    dirty_state: bool,
    dirty_since_snapshot: bool,
    /// 마지막 리비전 남기기를 막은 본문 충돌 또는 검사 실패. 체크포인트 시작 때 비운다.
    snapshot_failure: Option<SnapshotFailure>,
    last_change: Instant,
    /// 마지막으로 DB에 저장한(또는 저장된 것을 불러온) 상태의 스냅숏(v1). 들어오는 연결에 처음 메시지와 함께 보낸다.
    persisted: Option<Vec<u8>>,
    /// 고친 사람(마지막이 앞, 방이 열린 동안 모두). 리비전 작성자를 고를 때 쓴다.
    editors: Vec<UserId>,
    /// 마지막 리비전 뒤로 고친 사람(마지막이 앞). 리비전을 남기면 비운다. 상태와 함께 저장하고(D-95), 작성자가 이 안에
    /// 없으면(대신 저장) 리비전에 실제 편집자로 적는다(`edited_by`).
    edited_since: Vec<UserId>,
    /// 연결의 세션·권한을 다시 확인하는 간격(`FVOCI_COLLAB_RECHECK_SECS`).
    recheck_every: Duration,
    registry_epoch: tokio::sync::watch::Receiver<u64>,
}
/// 저장 대기 편집을 먼저 리비전으로 남긴 뒤에 하는 일(남기지 못하면 하지 않는다). 대신 저장할 사람과 409 문구가 다르다.
#[derive(Debug, Clone, Copy)]
enum Change {
    Move,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SnapshotFailure {
    ChangedOutside,
    InvalidBody,
}

enum Verdict {
    Ok,
    /// 거절 이유(로그용 고정 분류).
    Invalid(String),
    Reload,
}

/// 다시 확인한 연결의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerAccess {
    Allowed {
        can_write: bool,
    },
    Denied,
    RealtimeOff,
    /// DB 오류 등으로 알 수 없음. 연결은 재접속 가능한 오류로 종료한다.
    Unknown,
}

#[derive(Debug, Clone)]
pub(super) struct AccessCheck {
    pub(super) access: PeerAccess,
    pub(super) snapshot: Option<Arc<Snapshot>>,
}

impl From<PeerAccess> for AccessCheck {
    fn from(access: PeerAccess) -> Self {
        Self {
            access,
            snapshot: None,
        }
    }
}
