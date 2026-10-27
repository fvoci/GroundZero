//! 문서 하나의 실시간 편집 방(D-70). 방마다 작업 하나가 Yjs 문서를 혼자 맡고(잠금 없음), 연결은 메시지만 주고받는다.
//!
//! - **경계**: 쓰기 권한이 있는 연결의 변경만 받는다. 연결마다 모인 변경을 합쳐, 방 문서를 복사한 임시 문서에 먼저
//!   넣고 편집기 스키마 모양(`convert::check_shape`)과 저장 API의 본문 검사(`validate_body`)를 통과해야 방 문서에 넣고
//!   다른 연결에 보낸다. 검사는 블로킹 작업으로 돌린다. 통과하지 못하면 버리고 그 연결을 닫는다(4400). 읽기 연결의
//!   변경은 받지 않고 버린다(편집기가 스스로 고친 것일 수 있어 닫지는 않는다).
//! - **한도**: 한 프레임의 메시지 수, 연결마다 초당 바이트·메시지 수, Yjs 상태 크기, 참여자 표시 수·크기.
//! - **저장**: 몇 초마다 Yjs 상태를 저장하고, 60초 조용하거나 마지막 사람이 나가면 저장 API와 같은 길
//!   (`documents::write_revision`)으로 리비전을 만든다. 마지막으로 고친 사람이 쓸 수 없으면 연결된 다른 작성자로 쓴다.
//!   작성자가 마지막 리비전 뒤로 고친 사람이 아니면(대신 저장) 고친 사람을 리비전에 따로 적는다(D-95, `edited_by`).
//!   고친 사람은 저장 대기 상태와 함께 저장해 방이 닫힌 뒤에도 잃지 않는다.
//!   그 사이 본문이 다른 길(복원)로 바뀌었으면 쓰지 않고 방을 닫는다. 저장 확인(`MSG_PERSISTED`)은 방이 계속 편집을
//!   지니고 있을 때나 리비전을 남긴 뒤에만 보낸다: 리비전을 남기지 못한 채 연결을 닫을 때는 닫은 뒤에 상태를 저장한다
//!   (확인받지 못한 편집은 브라우저가 초안으로 지닌다).
//! - **복원·이동**: 방이 자기 명령 안에서 한다. 그동안 들어온 메시지는 방의 대기열에서 기다린다(넣지도 확인하지도 않음).
//! - **한 서버**: 방은 메모리에 있다. 셀프호스트의 한 앱 프로세스(`role=All`)를 전제로 한다. 끝낼 때 모든 방의 리비전을
//!   남긴다.

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
use super::lease::{Lease, MovePermit, WritePermit};
use super::visibility::{CommittedMove, Snapshot};
use crate::config::Limits;
use crate::db::Db;
use crate::error::AppError;
use crate::features::documents::{self, MoveDocument, MoveReply};
use crate::ids::{DocumentId, UserId};

/// 방마다 들어오는 명령·메시지 대기열의 크기.
const ROOM_QUEUE: usize = 1024;
/// 작은 변경을 모아 검사하는 간격.
const FLUSH_EVERY: Duration = Duration::from_millis(100);
/// Yjs 상태를 저장하는 간격.
const PERSIST_EVERY: Duration = Duration::from_secs(3);
/// 복원 전 체크포인트(리비전으로 남기지 않은 편집 남기기)를 기다리는 최대 시간. 넘으면 복원하지 않는다.
const CHECKPOINT_LIMIT: Duration = Duration::from_secs(5);

/// 옛 리비전 복원 요청. 열린 방이 있으면 방이, 없으면 요청 처리기가 방이 새로 열리지 않게 막은 채 방 없이 남은 저장
/// 대기 상태를 먼저 리비전으로 남기고(남기지 못하면 복원하지 않고 409) `restore_in_db`로 한다(`Rooms::restore`).
#[derive(Debug, Clone)]
pub struct RestoreRequest {
    pub doc: DocumentId,
    pub user: UserId,
    pub revision: i32,
    pub request_id: Uuid,
    pub base_revision: i32,
}

/// 문서 옮기기 요청. 요청 처리기가 미리보기와 같은 권한 확인(문서 소유 권한·대상 쓰기 권한)을 거친 것만 만든다: 방은
/// 이 요청으로 체크포인트 리비전을 남기므로(옮기는 사람이 대신 저장할 수 있다, D-95) 거절될 요청이 방에 닿으면 안 된다.
/// 최종 확인은 이동 트랜잭션이 잠근 뒤 다시 한다. 열린 방이 있으면 방이, 없으면 요청 처리기가 방이 새로 열리지 않게 막은
/// 채 방 없이 남은 저장 대기 상태를 먼저 리비전으로 남기고(남기지 못하면 옮기지 않고 409) `documents::move_in_db`로
/// 한다(`Rooms::move_document`).
#[derive(Debug, Clone)]
pub struct MoveRequest {
    pub doc: DocumentId,
    pub user: UserId,
    pub input: MoveDocument,
}
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
    next: Arc<AtomicU64>,
}

/// 문서를 옮기거나 복원하는 동안 그 문서의 방이 새로 열리지 않게 막는다. 놓으면 풀린다.
#[derive(Debug)]
struct HoldGuard {
    rooms: Rooms,
    doc: DocumentId,
}

impl Drop for HoldGuard {
    fn drop(&mut self) {
        let _rooms = self.rooms.lock();
        let mut held = self.rooms.held_set();
        if let Some(count) = held.get_mut(&self.doc) {
            *count -= 1;
            if *count == 0 {
                held.remove(&self.doc);
            }
        }
    }
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
    access: Lease,
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

    /// 옮기거나 방 없이 복원하는 중인 문서 목록. 늘 방 목록 잠금(`lock`)을 먼저 잡은 뒤에 잡는다.
    fn held_set(&self) -> std::sync::MutexGuard<'_, HashMap<DocumentId, usize>> {
        self.held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 열린(닫히는 중이 아닌) 방.
    fn live_room(&self, doc: DocumentId) -> Option<Handle> {
        self.lock().get(&doc).filter(|h| !h.tx.is_closed()).cloned()
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
                // 옮기거나 방 없이 복원하는 중이면 들어가지 않는다. 클라이언트는 잠시 뒤 다시 붙고, 그때 새 자리의
                // 권한·설정(복원했으면 복원한 본문)으로 연다.
                if self.held_set().contains_key(&doc) {
                    return Err(AppError::Upstream(
                        "the document is being moved or restored".into(),
                    ));
                }
                if let Some(h) = rooms.get(&doc).filter(|h| !h.tx.is_closed()) {
                    h.clone()
                } else {
                    let generation = self.next.fetch_add(1, Ordering::Relaxed);
                    let (tx, rx) = mpsc::channel(ROOM_QUEUE);
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
                        access: peer.access.clone(),
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

    /// 방이 문서를 다른 워크스페이스로 옮겼다. 방이 닫히기까지 그 워크스페이스에 속하게 한다(실시간 끄기·파일 버전
    /// 지우기가 워크스페이스로 방을 찾는다).
    fn moved(&self, doc: DocumentId, workspace: Uuid) {
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
    async fn persist_where(&self, keep: impl Fn(&Handle) -> bool) {
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
    fn hold(&self, doc: DocumentId) -> (HoldGuard, Option<Handle>) {
        let rooms = self.lock();
        *self.held_set().entry(doc).or_default() += 1;
        let room = rooms.get(&doc).filter(|h| !h.tx.is_closed()).cloned();
        (
            HoldGuard {
                rooms: self.clone(),
                doc,
            },
            room,
        )
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
        let (_guard, mut room) = self.hold(request.doc);
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
        let (_guard, mut room) = self.hold(request.doc);
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
    /// 뒤(`begin_closing` 아래)에 쓴다. 작성자는 그 상태를 고친 사람(`load`가 이어받은 편집자, 마지막이 앞) 가운데 아직 쓸
    /// 수 있는 사람, 없으면 실시간을 끄는 관리자다(대신 저장, 실제 편집자를 리비전에 따로 적는다, D-95). 남겼거나 남길
    /// 것이 없으면 참.
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
            // 다른 OFF 요청이 같은 pending 목록을 먼저 저장하고 실시간을 껐을 수 있다.
            // load의 55000만 다시 확인하며, 실제 pending이나 권한/DB 실패는 계속 거부한다.
            Err(AppError::Conflict(_)) => {
                return pending_already_saved(db, doc, admin).await.unwrap_or(false);
            }
            Err(err) => {
                tracing::warn!(event = "collab.load_failed", document = %doc.0, error_code = err.code(), detail = crate::logging::safe_error(&err), "could not load a pending live state");
                return false;
            }
        }
        room.dirty_since_snapshot = true;
        room.snapshot_with(Some(admin)).await
    }

    /// 방 없이 남은 실시간 상태를 옮기거나 복원하기 전에 리비전으로 남긴다(#71 리뷰, D-95 조건 4). 앞선 방이 리비전을
    /// 남기지 못하고 닫혔거나(쓸 수 있는 사람이 없음) 서버가 주기 저장 뒤 갑자기 멈춘 경우로, 그 편집은 이미 저장 확인을
    /// 받았을 수 있다. 실시간이 꺼진 곳으로 옮기거나 다른 리비전으로 복원하면 그 상태는 이어받을 수 없게 되고(`app_collab_load`)
    /// 다음 일반 저장이 버린다. `hold` 아래에서 불러 그사이 방이 열려 그 상태를 이어받지 않는다.
    ///
    /// 새로 만든 방이라 연결이 없으므로 작성자 순서는 그 상태와 함께 저장된 편집자(마지막 리비전 뒤로 고친 사람, 마지막이
    /// 앞) 가운데 아직 쓸 수 있는 사람이다. 옮길 때는 그다음 옮기는 사람이 대신 남기고(`save_pending`의 관리자 자리, 방 안
    /// 체크포인트와 같다) 실제 편집자를 리비전에 따로 적는다(D-95, 편집자 기록이 없는 옛 상태면 빈 목록). 복원하는 사람은
    /// 대신 남기지 않는다(연결이 없으므로 연결된 작성자 자리도 없다, docs/development.md#live-document-preservation). 남길 것이 없으면(저장된 상태가
    /// 없음·본문과 같은 내용, 실시간이 꺼짐) 아무것도 하지 않는다. 남기지 못하면 방 안 체크포인트와 같은 409로 하지 않고
    /// 상태는 그대로 둔다.
    async fn save_pending_before(
        &self,
        db: &Db,
        limits: &Limits,
        doc: DocumentId,
        user: UserId,
        change: Change,
    ) -> Result<(), AppError> {
        let generation = self.next.fetch_add(1, Ordering::Relaxed);
        let mut room = Room::new(
            doc,
            generation,
            db.clone(),
            *limits,
            Duration::from_secs(30),
            self.clone(),
        );
        let loaded = match room.load(user).await {
            Ok(loaded) => loaded,
            // 실시간이 꺼진 문서(`app_collab_access`의 55000): 끌 때 남은 상태를 먼저 리비전으로 남겼다. 잠그지 않고 읽기만
            // 하는 함수라 다른 409(동시 변경 충돌 등)는 여기서 나지 않는다(`peer_access`와 같은 판단).
            Err(AppError::Conflict(_)) => return Ok(()),
            Err(err) => return Err(err),
        };
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

    fn remove(&self, doc: DocumentId, generation: u64) {
        let mut rooms = self.lock();
        if rooms.get(&doc).is_some_and(|h| h.generation == generation) {
            rooms.remove(&doc);
        }
    }
}

async fn pending_already_saved(db: &Db, doc: DocumentId, admin: UserId) -> Result<bool, AppError> {
    let mut tx = db.begin_as(admin).await?;
    let workspace: Option<Option<Uuid>> =
        sqlx::query_scalar("SELECT workspace_id FROM documents WHERE id=$1")
            .bind(doc)
            .fetch_optional(tx.conn())
            .await?;
    let Some(Some(workspace)) = workspace else {
        return Ok(false);
    };
    // 기존 관리자 전용 pending 판정을 그대로 쓴다. 문서 비가시성·관리자 철회는 성공으로 삼지 않는다.
    let pending: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app_collab_pending($1) pending WHERE pending=$2)",
    )
    .bind(workspace)
    .bind(doc)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(!pending)
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

/// `app_collab_load`의 한 줄: 문서 리비전, 제목, 본문, 저장된 상태와 그 편집자(상태를 쓸 수 없으면 둘 다 없음).
type LoadRow = (i32, String, Value, Option<Vec<u8>>, Option<Vec<Uuid>>);

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
    /// 마지막 리비전 남기기에서 DB 본문이 방이 아는 것과 달라(다른 길로 바뀜) 남기지 않고 닫았는지.
    changed_outside: bool,
    /// 마지막 리비전 남기기가 본문 검사를 통과하지 못해 남기지 않았는지(다시 해도 남길 수 없다).
    failed_checks: bool,
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
    async fn handle(&mut self, cmd: Command, rx: &mut mpsc::Receiver<Command>) -> bool {
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
            Command::Restore { request, done } => {
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
                    crate::features::revisions::restore_in_db(&self.db, &self.limits, &request)
                        .await;
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
            Command::Shutdown {
                reason,
                snapshot,
                saved,
                fallback,
            } => {
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
    }

    /// 문서를 옮긴다(`Command::Move`, 감사 S6·#71). 이 명령을 처리하는 동안 들어온 메시지는 대기열에서 기다린다(넣지도
    /// 확인하지도 않음). 옮겼으면 거짓(방을 닫는다).
    async fn move_document(
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
                    tracing::warn!(event = "collab.move_reload_failed", document = %self.doc_id.0, error_code = err.code(), detail = crate::logging::safe_error(&err), "could not reload the moved document");
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
    /// 이 명령 동안에는 저장 확인을 보내지 않는다). 실시간이 꺼진 것을 알아도 여기서는 리비전을 남기거나 연결을 닫지 않는다:
    /// 이동 트랜잭션이 문서를 잠그고 있고, 옮겼으면 새 자리의 설정으로 다시 열리며 옮기지 않았으면 다음 재확인이 따른다.
    async fn move_rechecking(&mut self, request: &MoveRequest) -> Result<MoveReply, AppError> {
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
    /// (`close_all`), 새 자리의 실시간 끄기가 이 방이 리비전이나 상태를 남기기 전에 남은 상태를 찾고 끝낼 수 있다.
    ///
    /// 이것은 그 틈을 좁힐 뿐 닫지 않는다. 커밋과 `Rooms::moved` 사이에 새 자리의 실시간 끄기가 방 목록을 모으면 이 방은
    /// 아직 옛 워크스페이스로 보여 닫기 요청을 받지 않는다. 커밋 전에 대기열에 들어온 닫기 요청(옛 워크스페이스의 실시간
    /// 끄기, 서버 종료)도 여기서 비워져 옮긴 뒤의 결과로 답을 받는다(남겼으면 참, 못 남겼으면 그 끄기는 409). `moved`를 커밋
    /// 전에 부를 수는 없다: 옮길지는 이동 트랜잭션이 잠근 뒤 다시 계산해 정하므로, 먼저 새 자리로 적으면 옮기지 않은 방을 새
    /// 자리의 끄기가 닫을 수 있고(4410, 문서는 옛 자리에서 실시간이 켜진 채), 방 목록 잠금(동기 잠금)은 커밋을 기다리는
    /// 동안 쥘 수 없다. 남는 틈에 걸린 것은 확인받지 않은 편집뿐이다: 체크포인트까지의 편집은 이미 리비전이고, 그 뒤의
    /// 편집은 커밋 뒤 리비전을 남겨야 처음 확인을 받는다. 그 리비전을 남기지 못하면 연결을 닫은 뒤 상태만 저장하므로
    /// 브라우저가 그 편집을 초안으로 지닌다.
    ///
    /// 커밋 전 편집은 마지막 재확인(이동 바로 앞, 잠금 대기 중에는 재확인 간격마다, `move_rechecking`)에서 본 쓰기
    /// 권한으로 받는다. 그 재확인이 닫은 연결의 편집은 넣지 않는다. 넣기 전에 세션만 한 번 더 보고 끝났으면 버린다(연결을
    /// 4403으로 닫는다, #64 리뷰). 세션은 자리와 상관없어 옮긴 뒤에도 볼 수 있다. 옛 자리 권한은 커밋 뒤에는 다시 볼 수
    /// 없어 마지막 재확인으로 갈음한다.
    async fn drain_after_move(
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
    async fn drop_ended_sessions(&mut self) {
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

    /// 방이 아직 문서를 불러오지 않았으면 불러온다(옮기기·복원하는 사람의 권한으로). 방을 연 연결의 참여 요청보다 이동·복원
    /// 명령이 먼저 닿으면(방 목록에 넣은 뒤 참여 요청을 보내기 전에 `Rooms::hold`가 그 방을 찾음) 저장 대기 상태를 이어받지
    /// 않은 채 복원·이동해 그 편집을 잃을 수 있다(#64 반박 검증). 실시간이 꺼졌으면(409) 이어받을 상태가 없다.
    async fn ensure_loaded(&mut self, user: UserId) -> Result<(), AppError> {
        if self.doc.is_some() {
            return Ok(());
        }
        match self.load(user).await {
            Ok(doc) => {
                self.doc = Some(doc);
                Ok(())
            }
            Err(AppError::Conflict(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }

    /// 저장된 Yjs 상태(그 뒤로 본문이 바뀌지 않았을 때) 또는 본문에서 방 문서를 만든다. 들어오는 사람의 권한으로 읽는다.
    /// 상태를 이어받으면 그 상태의 편집자(마지막 리비전 뒤로 고친 사람)도 이어받는다: 작성자를 고르는 순서의 앞에 두고
    /// 대신 저장할 때 실제 편집자로 적는다(D-95).
    async fn load(&mut self, user: UserId) -> Result<Doc, AppError> {
        let mut tx = self.db.begin_as(user).await?;
        let (revision, _title, body, state, editors): LoadRow =
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
        let editors: Vec<UserId> = editors
            .unwrap_or_default()
            .into_iter()
            .map(UserId)
            .collect();
        self.editors.clone_from(&editors);
        self.edited_since = editors;
        // 저장된 상태에서 불러왔으니 이 상태는 이미 DB에 있다.
        self.persisted = Some(doc.transact().snapshot().encode_v1());
        // 앞선 방이 리비전으로 남기지 못한 편집(이미 확인을 받았을 수 있다)을 이어받았으면 아직 리비전이 아니다: 이 방의
        // 리비전 남기기(복원·이동 앞의 체크포인트, 닫힐 때)가 그 편집도 남기게 한다(#64 반박 검증). 본문을 Yjs로 옮긴 모양과
        // 같으면(방을 열기만 함·이미 리비전으로 남김, 편집기가 저장한 본문의 null 속성은 Yjs에 남지 않음) 남길 것이 없다.
        let live = {
            let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
            convert::read_doc(&doc.transact(), &fragment)
        };
        self.dirty_since_snapshot = live != live_form(&self.base_body);
        Ok(doc)
    }

    /// 들어온 연결에 처음 보낼 메시지들. 메시지마다 프레임 하나다(y-websocket은 한 프레임에서 첫 메시지만 읽는다).
    fn start_messages(&self) -> Vec<Vec<u8>> {
        let Some(doc) = &self.doc else {
            return Vec::new();
        };
        let sv = doc.transact().state_vector();
        let mut out = vec![Message::Sync(SyncMessage::SyncStep1(sv)).encode_v1()];
        if let Some(update) = self.presence_update() {
            out.push(Message::Awareness(update).encode_v1());
        }
        // 이미 저장된 상태도 알린다: 다시 붙은 클라이언트의 편집이 그 안에 있으면 새 저장 없이 확인된다.
        if let Some(snapshot) = &self.persisted {
            out.push(Message::Custom(MSG_PERSISTED, snapshot.clone()).encode_v1());
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
        self.incoming_with_authorization(conn, data, EditAuthorization::Current);
    }

    fn incoming_with_authorization(
        &mut self,
        conn: u64,
        data: &[u8],
        authorization: EditAuthorization,
    ) {
        let Some(connection) = self.conns.get(&conn) else {
            return;
        };
        let permission = match &authorization {
            EditAuthorization::Current => connection.peer.access.permission(),
            EditAuthorization::BeforeMove { .. } => authorization
                .allows(&connection.peer.access)
                .then_some(true),
        };
        let Some(can_write) = permission else {
            if matches!(authorization, EditAuthorization::BeforeMove { .. }) {
                return;
            }
            let reason = connection.peer.access.reason().unwrap_or(CloseReason::Busy);
            self.close(conn, reason);
            return;
        };
        let (user, can_write, out) = (connection.peer.user, can_write, connection.peer.out.clone());
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
                    self.pending.push(Pending {
                        conn,
                        user,
                        update,
                        authorization: authorization.clone(),
                    });
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
        let mut by_conn: HashMap<u64, Vec<Pending>> = HashMap::new();
        for item in pending {
            if !order.contains(&item.conn) {
                order.push(item.conn);
            }
            by_conn.entry(item.conn).or_default().push(item);
        }
        for conn in order {
            let Some(edits) = by_conn.remove(&conn) else {
                continue;
            };
            let Some(first) = edits.first() else { continue };
            let user = first.user;
            if !self.conns.get(&conn).is_some_and(|c| {
                edits
                    .iter()
                    .all(|edit| edit.authorization.allows(&c.peer.access))
            }) {
                continue;
            }
            let Ok(merged) = yrs::merge_updates_v1(edits.iter().map(|edit| edit.update.as_slice()))
            else {
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
            // 새 위치 거부와 달리 검사 중 세션이 끝났거나 DB로 확인하지 못하면 대리 저장도 받지 않는다.
            if matches!(first.authorization, EditAuthorization::BeforeMove { .. }) {
                let Some(peer) = self
                    .conns
                    .get(&conn)
                    .map(|connection| connection.peer.clone())
                else {
                    continue;
                };
                if let Err(reason) = first.authorization.check_session(&self.db, &peer).await {
                    self.close(conn, reason);
                    continue;
                }
            }
            // 본문 검사 동안 기한이 지나거나 권한이 철회됐을 수 있다.
            if !self.conns.get(&conn).is_some_and(|c| {
                edits
                    .iter()
                    .all(|edit| edit.authorization.allows(&c.peer.access))
            }) {
                continue;
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
            for list in [&mut self.editors, &mut self.edited_since] {
                list.retain(|u| *u != user);
                list.insert(0, user);
            }
            let message = Message::Sync(SyncMessage::Update(merged)).encode_v1();
            self.broadcast(&message, Some(conn));
        }
    }

    /// 닫힌 연결(웹소켓이 끝났는데 나감 알림이 대기열에 못 들어간 것)을 걸러 낸다.
    fn prune(&mut self) {
        let closed: Vec<u64> = self
            .conns
            .iter()
            .filter(|(_, c)| c.peer.out.is_closed() || c.peer.access.permission().is_none())
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
            c.peer.access.close(reason);
            let _ignored = c.peer.out.try_send(Outbound::Close(reason));
        }
        self.leave(conn);
    }

    fn send(&mut self, out: &mpsc::Sender<Outbound>, bytes: Vec<u8>, conn: u64) {
        if !self
            .conns
            .get(&conn)
            .is_some_and(|c| c.peer.access.permission().is_some())
        {
            return;
        }
        if out.try_send(Outbound::Binary(bytes)).is_err() {
            self.close(conn, CloseReason::Busy);
        }
    }

    fn broadcast(&mut self, bytes: &[u8], except: Option<u64>) {
        let slow: Vec<u64> = self
            .conns
            .iter()
            .filter(|(id, c)| Some(**id) != except && c.peer.access.permission().is_some())
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
        let editors: Vec<Uuid> = self.edited_since.iter().map(|u| u.0).collect();
        let result = async {
            let mut tx = self.db.begin_anonymous().await?;
            // 리비전으로 아직 남지 않은 편집이 있는지와 그 편집자도 적는다(실시간을 끄기 전·옮기기 전에 남긴다, 마이그레이션
            // 0050·0094).
            let stored: bool =
                sqlx::query_scalar("SELECT app_system_collab_store($1, $2, $3, $4, $5, $6)")
                    .bind(self.doc_id)
                    .bind(self.base_revision)
                    .bind(state)
                    .bind(self.dirty_since_snapshot)
                    .bind(body)
                    .bind(editors)
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
    /// 최근에 고친 다른 사람, 그다음은 연결된 작성자 순. 작성자가 마지막 리비전 뒤로 고친 사람이 아니면(대신 저장) 고친
    /// 사람을 리비전에 따로 적는다(D-95, `write_snapshot`).
    async fn snapshot(&mut self) -> bool {
        self.snapshot_with(None).await
    }

    /// `fallback`: 편집한 사람·연결된 작성자 모두 남길 수 없을 때의 작성자(실시간을 끄는 관리자, 옮기는 사람).
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
            self.failed_checks = true;
            tracing::warn!(event = "collab.invalid_document", document = %self.doc_id.0, "live document failed checks; not saved as a revision");
            return false;
        }
        let mut authors = self.editors.clone();
        for c in self
            .conns
            .values()
            .filter(|c| c.peer.access.permission() == Some(true))
        {
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
                    self.edited_since.clear();
                    self.dirty_state = true;
                    // 리비전 번호와 상태가 어긋난 채로 멈추지 않도록 바로 저장한다.
                    self.persist().await;
                    return true;
                }
                Ok(None) => {
                    // 본문이 다른 길로 바뀌었다(복원). 덮어쓰지 않고 다시 열게 한다.
                    self.changed_outside = true;
                    tracing::warn!(event = "collab.changed_outside", document = %self.doc_id.0, "document changed outside live editing; reopening");
                    for (_, c) in self.conns.drain() {
                        let _ignored = c.peer.out.try_send(Outbound::Close(CloseReason::Reload));
                    }
                    self.dirty_since_snapshot = false;
                    self.edited_since.clear();
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

    /// 옮기거나 복원하기 전 체크포인트: 리비전으로 남기지 않은 편집을 남긴다. 작성자는 편집한 사람, 그다음 연결된 작성자
    /// 순이다(`snapshot_with`, 연결된 작성자에는 연결돼 있는 복원하는 사람도 든다. 대신 저장이면 실제 편집자를 따로 적는다,
    /// D-95). 옮길 때는 그 모두가 남길 수 없으면 옮기는 사람(`user`)이 대신 작성자가 된다. 복원하는 사람은 그 자리에 두지
    /// 않는다(방 없는 복원은 연결이 없어 저장 대기 상태의 편집자만 남길 수 있다). 남겼으면 `None`, 남겼는지 확인되지 않으면
    /// 하지 않는 이유(409). 다시 해도 남길 수 없는 원인(본문 검사 실패)에는 "다시 하라"고 하지 않는다.
    async fn checkpoint(&mut self, change: Change, user: UserId) -> Option<&'static str> {
        self.changed_outside = false;
        self.failed_checks = false;
        let moving = matches!(change, Change::Move);
        let stand_in = moving.then_some(user);
        let saved = tokio::time::timeout(CHECKPOINT_LIMIT, self.snapshot_with(stand_in)).await;
        match saved {
            Ok(true) if !self.changed_outside => None,
            Ok(true) if moving => Some(
                "the document changed outside live editing before its live edits were saved; nothing was moved",
            ),
            Ok(true) => Some(
                "the document changed outside live editing before its live edits were saved; nothing was restored",
            ),
            Ok(false) if self.failed_checks && moving => Some(
                "live edits do not pass the document checks, so they cannot be saved as a revision; nothing was moved",
            ),
            Ok(false) if self.failed_checks => Some(
                "live edits do not pass the document checks, so they cannot be saved as a revision; nothing was restored",
            ),
            Ok(false) if moving => Some(
                "live edits could not be saved as a revision first; nothing was moved, try again",
            ),
            Ok(false) => {
                Some("live edits could not be saved as a revision first; nothing was restored")
            }
            Err(_) if moving => {
                Some("saving live edits as a revision took too long; nothing was moved, try again")
            }
            Err(_) => Some(
                "saving live edits as a revision took too long; nothing was restored, try again",
            ),
        }
    }

    /// 새 리비전 번호, 또는 본문이 다른 길로 바뀌었으면 `None`. 작성자(`author`, 이 트랜잭션을 연 사람)가 마지막 리비전
    /// 뒤로 고친 사람에 들지 않으면 그 사람들을 리비전의 실제 편집자로 적는다(D-95: 옮긴 사람·관리자·연결된 다른 작성자가
    /// 대신 저장해도 내용을 쓴 것처럼 보이지 않게). 고친 사람을 모르면(이 기록 전에 저장된 대기 상태) 빈 목록이다.
    async fn write_snapshot(&self, author: UserId, body: &Value) -> Result<Option<i32>, AppError> {
        let edited_by = (!self.edited_since.contains(&author)).then_some(&self.edited_since[..]);
        let mut tx = self.db.begin_as(author).await?;
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
        let row = documents::write_revision(
            &mut tx,
            self.doc_id,
            &title,
            body,
            Uuid::now_v7(),
            edited_by,
        )
        .await?;
        tx.commit().await?;
        Ok(Some(row.revision))
    }

    /// 독립적인 연결 감시가 실시간 해제를 확인했으면 리비전을 남긴다. 연결 종료는 저장을 기다리지 않는다.
    async fn recheck(&mut self) {
        if self.recheck_peers().await {
            self.stop_realtime().await;
        }
    }

    async fn stop_realtime(&mut self) {
        self.flush().await;
        // 리비전을 남기지 못한 편집은 확인하지 않는다. 연결을 닫은 뒤 상태만 저장한다(#71).
        self.snapshot().await;
        for (_, c) in self.conns.drain() {
            let _ignored = c
                .peer
                .out
                .try_send(Outbound::Close(CloseReason::RealtimeOff));
        }
        self.persist().await;
    }

    /// 이동 직전 등 명시적인 재확인도 유지한다. peer별로 병렬 확인하며 기존 기한보다 오래 기다리지 않는다.
    async fn recheck_peers(&mut self) -> bool {
        self.refresh_peers().await;
        self.remove_closed_peers()
    }

    async fn refresh_peers(&self) {
        let peers: Vec<(u64, Peer)> = self
            .conns
            .iter()
            .map(|(id, c)| (*id, c.peer.clone()))
            .collect();
        let checks = peers.iter().map(|(_, peer)| async {
            let started = tokio::time::Instant::now();
            if let Ok(observed) = peer
                .access
                .until_closed(observe_access(&self.db, self.doc_id, peer))
                .await
            {
                peer.access.observe(started, observed);
            }
        });
        futures_util::future::join_all(checks).await;
    }

    /// 주기 처리는 독립 감시의 결과만 소비하고 DB 검사를 중복해서 시작하지 않는다.
    fn remove_closed_peers(&mut self) -> bool {
        let peers: Vec<(u64, Peer)> = self
            .conns
            .iter()
            .map(|(id, c)| (*id, c.peer.clone()))
            .collect();
        let mut off = false;
        for (conn, peer) in peers {
            if let Some(reason) = peer.access.reason() {
                off |= reason == CloseReason::RealtimeOff;
                self.close(conn, reason);
            }
        }
        off
    }
}

/// 저장 대기 편집을 먼저 리비전으로 남긴 뒤에 하는 일(남기지 못하면 하지 않는다). 대신 저장할 사람과 409 문구가 다르다.
#[derive(Debug, Clone, Copy)]
enum Change {
    Move,
    Restore,
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

/// 그 연결의 세션이 아직 그 사람의 것인지. DB 오류로 알 수 없으면 `None`.
async fn session_alive(db: &Db, peer: &Peer) -> Option<bool> {
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
        Ok(user) => Some(user == Some(peer.user)),
        Err(err) => {
            // R2 A: 알 수 없으면 연결을 닫는다. 닫힌 까닭(DB 오류)을 기한 초과와 구분해 로그에 남긴다.
            tracing::warn!(
                event = "collab.session_check_failed",
                error_code = err.code(),
                detail = crate::logging::safe_error(&err),
                "could not confirm the live editing session"
            );
            None
        }
    }
}

/// 세션이 살아 있고 문서를 실시간으로 열 수 있는지.
pub async fn peer_access(db: &Db, doc: DocumentId, peer: &Peer) -> PeerAccess {
    observe_access(db, doc, peer).await.access
}

pub(super) async fn observe_access(db: &Db, doc: DocumentId, peer: &Peer) -> AccessCheck {
    match session_alive(db, peer).await {
        Some(true) => {}
        Some(false) => return PeerAccess::Denied.into(),
        None => return PeerAccess::Unknown.into(),
    }
    let access = async {
        let mut tx = db.begin_anonymous().await?;
        // begin_as의 set_config SELECT보다 먼저 격리를 정한다. 예외로 끝나는 권한 판정도
        // 그 판정과 같은 snapshot을 보존하며 mover 신원이나 문서 위치를 추가로 읽지 않는다.
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(tx.conn())
            .await?;
        let snapshot = Arc::new(Snapshot::read(&mut tx).await?);
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(peer.user.0.to_string())
            .execute(tx.conn())
            .await?;
        let access: Result<String, AppError> = sqlx::query_scalar("SELECT app_collab_access($1)")
            .bind(doc)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function);
        if access.is_ok() {
            tx.commit().await?;
        }
        Ok::<_, AppError>((access, snapshot))
    }
    .await;
    let (access, snapshot) = match access {
        Ok(observed) => observed,
        Err(err) => {
            tracing::warn!(
                event = "collab.access_check_failed",
                document = %doc.0,
                error_code = err.code(),
                detail = crate::logging::safe_error(&err),
                "could not confirm live editing access"
            );
            return PeerAccess::Unknown.into();
        }
    };
    let access = match access {
        Ok(a) => PeerAccess::Allowed {
            can_write: matches!(a.as_str(), "owner" | "write"),
        },
        Err(AppError::NotFound | AppError::Forbidden) => PeerAccess::Denied,
        Err(AppError::Conflict(_)) => PeerAccess::RealtimeOff,
        Err(err) => {
            tracing::warn!(
                event = "collab.access_check_failed",
                document = %doc.0,
                error_code = err.code(),
                detail = crate::logging::safe_error(&err),
                "could not confirm live editing access"
            );
            PeerAccess::Unknown
        }
    };
    AccessCheck {
        access,
        snapshot: Some(snapshot),
    }
}

/// 계보 표시를 두는 Yjs 맵. 편집기(조각 `default`)는 이 맵을 보지 않는다.
const META: &str = "fvoci";
const LINEAGE: &str = "lineage";

fn meta(doc: &Doc) -> yrs::MapRef {
    doc.get_or_insert_map(META)
}

/// 본문을 Yjs 문서에 썼다가 다시 읽은 모양(null 속성이 빠지고 표시가 이름순이다). 저장된 상태가 본문과 같은 내용인지 볼 때
/// 쓴다.
fn live_form(body: &Value) -> Value {
    let doc = Doc::new();
    let fragment = doc.get_or_insert_xml_fragment(FRAGMENT);
    convert::write_doc(&mut doc.transact_mut(), &fragment, body);
    convert::read_doc(&doc.transact(), &fragment)
}

fn lineage_of(doc: &Doc) -> Option<String> {
    let map = meta(doc);
    match map.get(&doc.transact(), LINEAGE) {
        Some(yrs::Out::Any(yrs::Any::String(s))) => Some(s.to_string()),
        _ => None,
    }
}
