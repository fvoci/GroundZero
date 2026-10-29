//! 실시간 편집 웹소켓(`GET /api/documents/{id}/collab`). y-websocket 클라이언트와 같은 y-sync 메시지를 주고받는다.
//! 연결 전에 공개 원점(Origin), 세션, 문서 권한, 실시간 켜짐을 확인한다(웹소켓 열기는 GET이라 공통 원점 검사를 거치지
//! 않으므로 여기서 본다).

use axum::extract::ws::{CloseFrame, Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use futures_util::{FutureExt, SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::lease::Lease;
use super::room::{self, CloseReason, Outbound, Peer};
use crate::features::accounts;
use crate::http::AppState;
use fvoci_core::error::AppError;
use fvoci_core::ids::DocumentId;
use fvoci_http::origin::PublicOrigin;
use fvoci_http::session::CurrentUser;

/// 연결로 보낼 것이 이만큼 쌓이면 느린 연결로 보고 닫는다.
const OUTBOX: usize = 512;

/// 연결 조건. `write=1`: 편집기로 연다(쓸 수 없게 됐으면 붙지 않고 4403으로 닫아, 그동안의 입력이 조용히 버려지지
/// 않고 화면이 지금 권한으로 다시 열리게 한다). `expected_user`(필수): 편집기를 연 사용자 id. 브라우저 WebSocket은
/// 머리를 붙이지 못해 질의로 받는다(저장 API의 `x-fvoci-expected-user`와 같은 계약, #60).
#[derive(Debug, Default, Deserialize)]
pub struct ConnectQuery {
    #[serde(default)]
    write: Option<String>,
    #[serde(default)]
    expected_user: Option<String>,
}

pub async fn connect(
    State(state): State<AppState>,
    State(public): State<PublicOrigin>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Query(query): Query<ConnectQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    if !public.matches(&headers) {
        return Err(AppError::Forbidden);
    }
    // 기대 사용자: 없거나 형식이 틀리면 업그레이드 전에 400(값 없는 옛 연결은 받지 않는다, D-92). 세션의 사용자와
    // 다르면(다른 탭에서 계정을 바꾼 뒤 앞 계정의 화면이 새 쿠키로 다시 붙음) 문서를 보거나 방에 들어가기 전에 4412로
    // 닫는다. 브라우저는 업그레이드 거절의 HTTP 상태를 읽지 못하고 y-websocket은 끝없이 다시 붙으므로, 열고 닫기 코드로
    // 알린다. 값은 비교에만 쓰고 권한은 세션의 사용자로만 본다.
    match fvoci_http::session::check_expected_user(
        user,
        query.expected_user.as_deref(),
        "expected_user",
    ) {
        Ok(()) => {}
        Err(AppError::AccountChanged) => {
            return Ok(ws.on_upgrade(|mut socket| async move {
                let _ignored = socket.send(close(CloseReason::AccountChanged)).await;
            }));
        }
        Err(err) => return Err(err),
    }
    let session =
        accounts::session_token_hash(&headers, &state.config).ok_or(AppError::Unauthorized)?;
    let checked_at = tokio::time::Instant::now();
    let mut tx = state.db.begin_as(user).await?;
    let access: String = sqlx::query_scalar("SELECT app_collab_access($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let workspace: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT workspace_id FROM documents WHERE id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    tx.commit().await?;
    let can_write = matches!(access.as_str(), "owner" | "write");
    let lost_write = query.write.as_deref() == Some("1") && !can_write;
    let max = state
        .config
        .limits
        .max_document_bytes
        .saturating_mul(2)
        .max(1 << 20);
    // 연결은 업그레이드 뒤 따로 도는 작업이라 요청 id를 직접 넘긴다(연결 동안의 로그 줄에 붙는다, D-83).
    let request_id = fvoci_core::logging::current_request_id().unwrap_or_default();
    Ok(ws
        .max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| async move {
            let (out, outbox) = mpsc::channel(OUTBOX);
            let peer = Peer {
                user,
                session,
                out,
                access: Lease::new(can_write, checked_at, state.config.collab_recheck),
            };
            fvoci_core::logging::REQUEST_ID
                .scope(
                    request_id,
                    run(socket, state, id, workspace, peer, outbox, lost_write),
                )
                .await;
        }))
}

async fn run(
    socket: WebSocket,
    state: AppState,
    id: DocumentId,
    workspace: Option<uuid::Uuid>,
    peer: Peer,
    mut outbox: mpsc::Receiver<Outbound>,
    lost_write: bool,
) {
    let (mut sink, mut stream) = socket.split();
    if lost_write {
        let _ignored = sink.send(close(CloseReason::AccessLost)).now_or_never();
        return;
    }
    let access = peer.access.clone();
    let _recheck = Recheck::spawn(state.db.clone(), id, peer.clone());
    let session = async {
        access.ready().await;
        let membership = match state
            .collab
            .join(
                &state.db,
                &state.config.limits,
                state.config.collab_recheck,
                id,
                workspace,
                peer,
            )
            .await
        {
            Ok(m) => m,
            Err(err) => {
                let reason = match err {
                    AppError::NotFound | AppError::Forbidden | AppError::Unauthorized => {
                        CloseReason::AccessLost
                    }
                    // 방을 열지 못한 일시적인 문제(DB 등)는 다시 붙게 한다.
                    other => {
                        tracing::warn!(
                            event = "collab.join_failed",
                            document = %id.0,
                            error_code = other.code(),
                            detail = fvoci_core::logging::safe_error(&other),
                            "could not open the live editing room"
                        );
                        CloseReason::Busy
                    }
                };
                return Some(reason);
            }
        };
        loop {
            tokio::select! {
                biased;
                item = outbox.recv() => {
                    match item {
                        Some(Outbound::Binary(bytes)) => {
                            if sink.send(WsMessage::Binary(bytes.into())).await.is_err() {
                                break;
                            }
                        }
                        Some(Outbound::Close(reason)) => return Some(reason),
                        None => break,
                    }
                }
                _ = membership.closed() => break,
                message = stream.next() => {
                    match message {
                        Some(Ok(WsMessage::Binary(bytes))) => {
                            if !membership.send(bytes.to_vec()).await {
                                break;
                            }
                        }
                        Some(Ok(WsMessage::Text(_) | WsMessage::Ping(_) | WsMessage::Pong(_))) => {},
                        _ => break,
                    }
                }
            }
        }
        None
    };
    let reason = match access.until_closed(session).await {
        Ok(reason) => reason,
        Err(reason) => Some(reason),
    };
    if let Some(reason) = reason {
        // 막힌 송신 큐 뒤에서 종료를 기다리지 않는다. 소켓이 바로 쓸 수 없으면 드롭해 연결을 끝낸다.
        let _ignored = sink.send(close(reason)).now_or_never();
    }
}

/// 연결마다 따로 확인하므로 한 peer의 DB 대기나 방의 긴 저장 작업이 다른 peer를 막지 않는다.
struct Recheck {
    task: tokio::task::JoinHandle<()>,
}

impl Recheck {
    fn spawn(db: fvoci_db::Db, id: DocumentId, peer: Peer) -> Self {
        let task = tokio::spawn(async move {
            peer.access
                .monitor(|| room::observe_access(&db, id, &peer))
                .await;
        });
        Self { task }
    }
}

impl Drop for Recheck {
    fn drop(&mut self) {
        // 정상 종료 전에 받은 편집은 기존 기한 안에서 방이 처리한다. 새 검증은 멈추되 기한을 연장하지 않는다.
        self.task.abort();
    }
}

fn close(reason: CloseReason) -> WsMessage {
    WsMessage::Close(Some(CloseFrame {
        code: reason.code(),
        reason: reason.text().into(),
    }))
}
