//! 워크스페이스 목록·생성. 생성은 정의자 함수 `app_create_workspace`가 워크스페이스·관리자 멤버십·감사 기록을
//! 한 트랜잭션으로 만든다. 같은 `request_id`로 재시도하면 같은 워크스페이스를 돌려준다.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::{Role, WorkspaceId};

/// 워크스페이스 요약.
#[derive(Debug, Serialize, ToSchema)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub name: String,
    /// 현재 사용자의 역할.
    pub role: Role,
    /// 현재 사용자가 소유자인지 (D-51).
    pub is_owner: bool,
}

/// 워크스페이스 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkspace {
    /// 클라이언트가 만든 요청 ID. 재시도 중복 방지.
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub name: String,
}

/// 이름·제목 검증. 앞뒤 공백을 지우고 비어 있거나 상한을 넘으면 거부한다.
pub fn validate_title(raw: &str, max_chars: usize, field: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(format!("{field} must not be empty")));
    }
    if trimmed.chars().count() > max_chars {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {max_chars} characters"
        )));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(AppError::BadRequest(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(trimmed.to_owned())
}

/// 현재 사용자의 워크스페이스 역할. 멤버가 아니면 존재를 숨기고 404.
pub async fn member_role(tx: &mut UserTx, ws: WorkspaceId) -> Result<Role, AppError> {
    let raw: Option<String> = sqlx::query_scalar("SELECT app_member_role($1)")
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    raw.as_deref()
        .and_then(Role::parse)
        .ok_or(AppError::NotFound)
}

fn row_to_workspace(
    (id, name, role, is_owner): (WorkspaceId, String, String, bool),
) -> Result<Workspace, AppError> {
    let role = Role::parse(&role)
        .ok_or_else(|| AppError::Config(format!("unknown role in database: {role}")))?;
    Ok(Workspace {
        id,
        name,
        role,
        is_owner,
    })
}

/// 내가 속한 워크스페이스.
#[utoipa::path(get, path = "/api/workspaces",
    responses((status = 200, body = [Workspace]), (status = 401, body = ErrorBody)), tag = "workspaces")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Workspace>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(WorkspaceId, String, String, bool)> = sqlx::query_as(
        "SELECT w.id, w.name, m.role, w.owner_id = m.user_id
         FROM workspaces w JOIN workspace_members m ON m.workspace_id = w.id
         WHERE m.user_id = app_user_id()
         ORDER BY w.created_at, w.id",
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(row_to_workspace)
        .collect::<Result<_, _>>()
        .map(Json)
}

/// 워크스페이스 생성. 생성자는 관리자가 된다.
#[utoipa::path(post, path = "/api/workspaces", request_body = CreateWorkspace,
    responses((status = 201, body = Workspace), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody)), tag = "workspaces")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<Workspace>), AppError> {
    let name = validate_title(&input.name, state.config.limits.max_title_chars, "name")?;
    let mut tx = state.db.begin_as(user).await?;
    let id: WorkspaceId = sqlx::query_scalar("SELECT app_create_workspace($1, $2)")
        .bind(input.request_id)
        .bind(&name)
        .fetch_one(tx.conn())
        .await?;
    let row: (WorkspaceId, String, String, bool) = sqlx::query_as(
        "SELECT w.id, w.name, m.role, w.owner_id = m.user_id FROM workspaces w
         JOIN workspace_members m ON m.workspace_id = w.id AND m.user_id = app_user_id()
         WHERE w.id = $1",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row_to_workspace(row)?)))
}
