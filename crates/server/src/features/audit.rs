//! 감사 기록 보기 (D-57). 워크스페이스 기록은 관리자에게, 개인 영역 기록은 소유자에게. 기록을 쓰는 것은 각
//! 기능의 정의자 함수이고 여기서는 읽기만 한다(행 보안 정책 `audit_log_select`가 범위를 정한다).

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::{UserId, WorkspaceId};

/// 한 번에 돌려주는 기록 수.
const PAGE: i64 = 100;

/// 감사 기록 한 건.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuditEntry {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// 예: `create`·`rename`·`delete`·`restore`·`purge`·`invite`·`change_role`·`export`.
    pub action: String,
    /// 예: `document`·`task`·`item`·`file`·`member`·`workspace`.
    pub target_type: String,
    #[schema(value_type = String, format = Uuid)]
    pub target_id: Uuid,
    /// 한 사람(없으면 백그라운드 작업).
    pub actor: Option<UserId>,
    pub actor_name: Option<String>,
    pub detail: Value,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// 쪽 넘기기: 이 기록보다 앞선 것.
#[derive(Debug, Deserialize, IntoParams)]
pub struct AuditQuery {
    /// 마지막으로 받은 기록 id(이것보다 오래된 것을 받는다).
    #[param(value_type = Option<String>, format = Uuid)]
    pub before: Option<Uuid>,
}

type Row = (
    Uuid,
    String,
    String,
    Uuid,
    Option<UserId>,
    Option<String>,
    Value,
    OffsetDateTime,
);

async fn page(
    tx: &mut UserTx,
    scope: &str,
    bind: Uuid,
    before: Option<Uuid>,
) -> Result<Vec<AuditEntry>, AppError> {
    // 기록 id는 시간순 UUIDv7이므로 id로 줄 세우고 넘긴다.
    let rows: Vec<Row> = sqlx::query_as(&format!(
        "SELECT a.id, a.action, a.target_type, a.target_id, a.actor, u.display_name, a.detail, a.created_at
         FROM audit_log a LEFT JOIN users u ON u.id = a.actor
         WHERE {scope} AND ($2::uuid IS NULL OR a.id < $2)
         ORDER BY a.id DESC LIMIT {PAGE}"
    ))
    .bind(bind)
    .bind(before)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(id, action, target_type, target_id, actor, actor_name, detail, created_at)| {
                AuditEntry {
                    id,
                    action,
                    target_type,
                    target_id,
                    actor,
                    actor_name,
                    detail,
                    created_at,
                }
            },
        )
        .collect())
}

/// 워크스페이스 감사 기록(관리자). 최신 먼저 100건씩.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/audit", operation_id = "workspace_audit",
    params(("workspace_id" = String, Path, format = Uuid), AuditQuery),
    responses((status = 200, body = [AuditEntry]), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "audit")]
pub async fn workspace(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_owner(&mut tx, crate::access::Kind::Workspace, ws).await?;
    let entries = page(&mut tx, "a.workspace_id = $1", ws.0, query.before).await?;
    tx.commit().await?;
    Ok(Json(entries))
}

/// 내 개인 영역 감사 기록(개인 노트·개인 파일·반출, 영구 삭제한 내 워크스페이스의 기록). 최신 먼저 100건씩.
#[utoipa::path(get, path = "/api/me/audit", operation_id = "my_audit", params(AuditQuery),
    responses((status = 200, body = [AuditEntry])), tag = "audit")]
pub async fn mine(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntry>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let entries = page(
        &mut tx,
        "a.workspace_id IS NULL AND a.owner_user_id = $1",
        user.0,
        query.before,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(entries))
}
