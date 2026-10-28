//! 워크스페이스 목록·생성. 생성은 정의자 함수 `app_create_workspace`가 워크스페이스·관리자 멤버십·감사 기록을
//! 한 트랜잭션으로 만든다. 같은 `request_id`로 재시도하면 같은 워크스페이스를 돌려준다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::OrganizationsState;
use fvoci_core::access::Grade;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{OrganizationId, Role, UserId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_db::{UserTx, new_ids};
use fvoci_http::session::CurrentUser;

/// 워크스페이스 요약.
#[derive(Debug, Serialize, ToSchema)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub organization_id: OrganizationId,
    pub name: String,
    /// 현재 사용자의 역할.
    pub role: Role,
    /// 현재 사용자가 소유자인지 (D-51).
    pub is_owner: bool,
    /// 워크스페이스 문서를 실시간 공동 편집으로 연다(D-27, D-70). 관리자가 켜고 끈다.
    pub realtime_enabled: bool,
    /// 현재 사용자의 접근 등급(D-113, `app_access('workspace', id)`): read·write·owner. 웹의 쓰기·관리 표시는 역할이
    /// 아니라 이 칸을 본다.
    pub access: Grade,
}

type WorkspaceRow = (
    WorkspaceId,
    OrganizationId,
    String,
    String,
    bool,
    bool,
    String,
);

/// 워크스페이스 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWorkspace {
    pub organization_id: OrganizationId,
    /// 클라이언트가 만든 요청 ID. 재시도 중복 방지.
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub name: String,
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
    (id, organization_id, name, role, is_owner, realtime_enabled, access): WorkspaceRow,
) -> Result<Workspace, AppError> {
    let role = Role::parse(&role)
        .ok_or_else(|| AppError::Config(format!("unknown role in database: {role}")))?;
    Ok(Workspace {
        id,
        organization_id,
        name,
        role,
        is_owner,
        realtime_enabled,
        access: Grade::parse_present(&access)?,
    })
}

/// 내가 속한 워크스페이스.
#[utoipa::path(get, path = "/api/workspaces",
    responses((status = 200, body = [Workspace]), (status = 401, body = ErrorBody)), tag = "workspaces")]
pub async fn list(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Workspace>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<WorkspaceRow> = sqlx::query_as(
        "SELECT w.id, w.organization_id, w.name, m.role, w.owner_id = m.user_id, w.realtime_enabled,
                app_access('workspace', w.id)
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
#[utoipa::path(post, params(("x-fvoci-expected-user" = Option<UserId>, Header)), path = "/api/workspaces", request_body = CreateWorkspace,
    responses((status = 409, body = ErrorBody), (status = 201, body = Workspace), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "workspaces")]
pub async fn create(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateWorkspace>,
) -> Result<(StatusCode, Json<Workspace>), AppError> {
    let name = validate_title(&input.name, state.max_title_chars, "name")?;
    let mut tx = state.db.begin_as(user).await?;
    let id: WorkspaceId = sqlx::query_scalar("SELECT app_create_workspace($1, $2, $3, $4)")
        .bind(input.request_id)
        .bind(input.organization_id)
        .bind(&name)
        .bind(new_ids(3))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let row: WorkspaceRow = sqlx::query_as(
        "SELECT w.id, w.organization_id, w.name, m.role, w.owner_id = m.user_id, w.realtime_enabled,
                app_access('workspace', w.id)
         FROM workspaces w
         JOIN workspace_members m ON m.workspace_id = w.id AND m.user_id = app_user_id()
         WHERE w.id = $1",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(row_to_workspace(row)?)))
}

/// 워크스페이스 지우기 확인. 이름을 그대로 다시 입력한다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DeleteWorkspace {
    pub name: String,
}

/// 지운 워크스페이스가 영구 삭제되는 때.
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceDeleted {
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

/// 내가 소유한 지운 워크스페이스.
#[derive(Debug, Serialize, ToSchema)]
pub struct DeletedWorkspace {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub deleted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    /// 보존 기간이 지나 되살릴 수 없다(곧 영구 삭제).
    pub expired: bool,
}

/// 워크스페이스 지우기(소유자, 이름 확인, D-51). 곧바로 모든 멤버에게 보이지 않고, 보존 기간 안에 소유자가
/// 되살릴 수 있다. 소유권 인계가 진행 중이면 409.
#[utoipa::path(delete, path = "/api/workspaces/{workspace_id}", operation_id = "delete_workspace",
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("workspace_id" = String, Path, format = Uuid)), request_body = DeleteWorkspace,
    responses((status = 200, body = WorkspaceDeleted), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "workspaces")]
pub async fn delete(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<DeleteWorkspace>,
) -> Result<Json<WorkspaceDeleted>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let expires_at: OffsetDateTime = sqlx::query_scalar("SELECT app_delete_workspace($1, $2, $3)")
        .bind(ws)
        .bind(&input.name)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(WorkspaceDeleted { expires_at }))
}

/// 내가 소유한 지운 워크스페이스 목록.
#[utoipa::path(get, path = "/api/me/deleted-workspaces", operation_id = "deleted_workspaces",
    responses((status = 200, body = [DeletedWorkspace])), tag = "workspaces")]
pub async fn deleted(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<DeletedWorkspace>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(Uuid, String, OffsetDateTime, OffsetDateTime)> =
        sqlx::query_as("SELECT id, name, deleted_at, expires_at FROM app_my_deleted_workspaces()")
            .fetch_all(tx.conn())
            .await?;
    tx.commit().await?;
    let now = OffsetDateTime::now_utc();
    Ok(Json(
        rows.into_iter()
            .map(|(id, name, deleted_at, expires_at)| DeletedWorkspace {
                id,
                name,
                deleted_at,
                expires_at,
                expired: expires_at <= now,
            })
            .collect(),
    ))
}

/// 지운 워크스페이스 되살리기(소유자, 보존 기간 안). 지났으면 409.
#[utoipa::path(post, path = "/api/me/deleted-workspaces/{workspace_id}/restore", operation_id = "restore_workspace",
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("workspace_id" = String, Path, format = Uuid)),
    responses((status = 400, body = ErrorBody), (status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "workspaces")]
pub async fn restore(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_restore_workspace($1, $2)")
        .bind(ws)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 지운 워크스페이스 영구 삭제(소유자). 모든 내용이 지워지고 되돌릴 수 없다. 개인 시간 기록은 남는다.
#[utoipa::path(delete, path = "/api/me/deleted-workspaces/{workspace_id}", operation_id = "purge_workspace",
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("workspace_id" = String, Path, format = Uuid)),
    responses((status = 409, body = ErrorBody), (status = 400, body = ErrorBody), (status = 204), (status = 404, body = ErrorBody)), tag = "workspaces")]
pub async fn purge(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_purge_workspace($1, $2)")
        .bind(ws)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
