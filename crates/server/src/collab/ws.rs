//! 실시간 편집 웹소켓(`GET /api/documents/{id}/collab`). y-websocket 클라이언트와 같은 y-sync 메시지를 주고받는다.
//! 연결 전에 공개 원점(Origin), 세션, 문서 권한, 실시간 켜짐을 확인한다(웹소켓 열기는 GET이라 공통 원점 검사를 거치지
//! 않으므로 여기서 본다).

use axum::extract::ws::{CloseFrame, Message as WsMessage, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::room::{CloseReason, Outbound, Peer};
use crate::error::AppError;
use crate::features::accounts::{self, CurrentUser};
use crate::http::AppState;
use crate::ids::DocumentId;

/// 연결로 보낼 것이 이만큼 쌓이면 느린 연결로 보고 닫는다.
const OUTBOX: usize = 512;

/// 연결 조건. `write=1`: 편집기로 연다(쓸 수 없게 됐으면 붙지 않고 4403으로 닫아, 그동안의 입력이 조용히 버려지지
/// 않고 화면이 지금 권한으로 다시 열리게 한다).
#[derive(Debug, Default, Deserialize)]
pub struct ConnectQuery {
    #[serde(default)]
    write: Option<String>,
}

pub async fn connect(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Query(query): Query<ConnectQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, AppError> {
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    if origin != Some(state.config.public_origin().as_str()) {
        return Err(AppError::Forbidden);
    }
    let session =
        accounts::session_token_hash(&headers, &state.config).ok_or(AppError::Unauthorized)?;
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
    let request_id = crate::logging::current_request_id().unwrap_or_default();
    Ok(ws
        .max_message_size(max)
        .max_frame_size(max)
        .on_upgrade(move |socket| async move {
            let (out, outbox) = mpsc::channel(OUTBOX);
            let peer = Peer {
                user,
                session,
                can_write,
                out,
            };
            crate::logging::REQUEST_ID
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
        let _ignored = sink.send(close(CloseReason::AccessLost)).await;
        return;
    }
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
                AppError::Conflict(_) => CloseReason::RealtimeOff,
                AppError::NotFound | AppError::Forbidden | AppError::Unauthorized => {
                    CloseReason::AccessLost
                }
                // 방을 열지 못한 일시적인 문제(DB 등)는 다시 붙게 한다.
                other => {
                    tracing::warn!(
                        event = "collab.join_failed",
                        document = %id.0,
                        error_code = other.code(),
                        detail = crate::logging::safe_error(&other),
                        "could not open the live editing room"
                    );
                    CloseReason::Busy
                }
            };
            let _ignored = sink.send(close(reason)).await;
            return;
        }
    };
    let writer = tokio::spawn(async move {
        while let Some(item) = outbox.recv().await {
            match item {
                Outbound::Binary(bytes) => {
                    if sink.send(WsMessage::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                Outbound::Close(reason) => {
                    let _ignored = sink.send(close(reason)).await;
                    break;
                }
            }
        }
    });
    while let Some(Ok(message)) = stream.next().await {
        match message {
            WsMessage::Binary(bytes) => {
                if !membership.send(bytes.to_vec()).await {
                    break;
                }
            }
            WsMessage::Close(_) => break,
            WsMessage::Text(_) | WsMessage::Ping(_) | WsMessage::Pong(_) => {}
        }
        if writer.is_finished() {
            break;
        }
    }
    drop(membership);
    writer.abort();
}

fn close(reason: CloseReason) -> WsMessage {
    WsMessage::Close(Some(CloseFrame {
        code: reason.code(),
        reason: reason.text().into(),
    }))
}
