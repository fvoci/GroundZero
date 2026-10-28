//! 휴지통·복원·영구 삭제 (D-39, D-41, D-57).
//!
//! 지우면 휴지통 묶음 하나가 생긴다(문서는 하위 문서까지, 업무는 하위 업무까지, 항목은 항목 하나, 일정은 하나).
//! 지운 것은 접근 함수가 없는 것으로 보므로 어디에도 나오지 않는다. 보존 기간(워크스페이스마다, 기본 30일)이
//! 지나면 되살릴 수 없고 "만료"로 남으며, 실제 삭제는 관리자(개인 노트는 소유자)가 직접 한다(자동 삭제는 O-19
//! 뒤). DB 정의는 `crates/db/migrations/202610280009_system.sql`의 `app_restore_batch`·`app_purge_batch` 등에 있다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::features::tasks::EXPECTED_TASK_USER_DOC;
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, ItemId, ModuleId, UserId, WorkspaceId};
use fvoci_db::{UserTx, needed_ids, new_ids};
use fvoci_http::session::{CurrentUser, ExpectedUser};

/// 지운 결과(되살릴 때 쓰는 휴지통 묶음).
#[derive(Debug, Serialize, ToSchema)]
pub struct Trashed {
    #[schema(value_type = String, format = Uuid)]
    pub batch_id: Uuid,
}

/// 휴지통 묶음의 맨 위 대상 종류.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    Item,
    Document,
    Task,
    Event,
    /// 첨부 파일(모든 버전).
    File,
    /// 모듈(그 아래 항목은 모듈과 함께 보이지 않음, D-78).
    Module,
}

impl TrashKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "task" => Some(Self::Task),
            "event" => Some(Self::Event),
            "file" => Some(Self::File),
            "module" => Some(Self::Module),
            _ => None,
        }
    }
}

/// 휴지통 묶음.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrashEntry {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub kind: TrashKind,
    #[schema(value_type = String, format = Uuid)]
    pub root_id: Uuid,
    pub title: String,
    /// 함께 지워진 수(하위 문서·하위 업무 포함).
    pub member_count: i32,
    pub deleted_by: UserId,
    pub deleted_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub deleted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    /// 보존 기간이 지나 되살릴 수 없다(영구 삭제 대기).
    pub expired: bool,
    /// 만료됐지만 살아 있는 기록·문서가 참조하는 파일 버전이 있어 자동으로 영구 삭제하지 않는다(D-56).
    pub kept_for_references: bool,
    /// 영구 삭제할 수 있는지(관리자·개인 노트 소유자).
    pub can_purge: bool,
}

/// 휴지통.
#[derive(Debug, Serialize, ToSchema)]
pub struct Trash {
    pub retention_days: i32,
    /// 이전 파일 버전 보존 기간(일, D-56). 없으면 영구 보존. 개인 휴지통에서는 없다.
    pub version_retention_days: Option<i32>,
    pub entries: Vec<TrashEntry>,
}

type EntryRow = (
    Uuid,
    String,
    Uuid,
    String,
    i32,
    UserId,
    Option<String>,
    OffsetDateTime,
    OffsetDateTime,
    bool,
);

async fn entries(
    tx: &mut UserTx,
    workspace: Option<WorkspaceId>,
) -> Result<Vec<TrashEntry>, AppError> {
    let rows: Vec<EntryRow> = sqlx::query_as("SELECT * FROM app_trash_list($1)")
        .bind(workspace)
        .fetch_all(tx.conn())
        .await?;
    let now = OffsetDateTime::now_utc();
    let expired: Vec<Uuid> = rows.iter().filter(|r| r.8 <= now).map(|r| r.0).collect();
    let kept: Vec<Uuid> = if expired.is_empty() {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT * FROM app_trash_kept_for_references($1)")
            .bind(&expired)
            .fetch_all(tx.conn())
            .await?
    };
    rows.into_iter()
        .map(
            |(
                id,
                kind,
                root_id,
                title,
                member_count,
                deleted_by,
                name,
                deleted_at,
                expires_at,
                can_purge,
            )| {
                Ok(TrashEntry {
                    id,
                    kind: TrashKind::parse(&kind)
                        .ok_or_else(|| AppError::Config(format!("unknown trash kind {kind}")))?,
                    root_id,
                    title,
                    member_count,
                    deleted_by,
                    deleted_by_name: name,
                    deleted_at,
                    expires_at,
                    expired: expires_at <= now,
                    kept_for_references: kept.contains(&id),
                    can_purge,
                })
            },
        )
        .collect()
}

async fn trash_call(state: &AppState, user: UserId, sql: &str, id: Uuid) -> Result<Uuid, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 휴지통 묶음과 감사 기록의 id 2개.
    let batch: Uuid = sqlx::query_scalar(sql)
        .bind(id)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(batch)
}

/// 문서 지우기(하위 문서까지). 문서 소유 권한이나, 쓰기 권한이 있고 내가 만든 문서.
#[utoipa::path(delete, path = "/api/documents/{document_id}", operation_id = "trash_document",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Trashed), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "trash")]
pub async fn trash_document(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Trashed>, AppError> {
    let batch_id = trash_call(&state, user, "SELECT app_trash_document($1, $2)", id.0).await?;
    Ok(Json(Trashed { batch_id }))
}

/// 업무 지우기(하위 업무까지, 쓰기 권한).
#[utoipa::path(delete, path = "/api/tasks/{task_id}", operation_id = "trash_task",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)),
    responses((status = 200, body = Trashed), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "account_changed: 기대 사용자 불일치; invalid_state: 보관·동시 변경")), tag = "trash")]
pub async fn trash_task(
    State(state): State<AppState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Trashed>, AppError> {
    let batch_id = trash_call(&state, user, "SELECT app_trash_task($1, $2)", id).await?;
    Ok(Json(Trashed { batch_id }))
}

/// 항목 지우기(항목 소유 권한). 그 안의 업무·문서·일정은 항목과 함께 보이지 않는다.
#[utoipa::path(delete, path = "/api/items/{item_id}", operation_id = "trash_item",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Trashed), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "trash")]
pub async fn trash_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Trashed>, AppError> {
    let batch_id = trash_call(&state, user, "SELECT app_trash_item($1, $2)", id.0).await?;
    Ok(Json(Trashed { batch_id }))
}

/// 모듈 지우기(관리자, D-78). 모듈 행만 휴지통에 들고 그 아래 항목은 모듈과 함께 보이지 않는다. 되살리면 모듈만
/// 돌아오고, 그 전에 따로 지운 항목은 자기 휴지통 묶음에 그대로 남는다.
#[utoipa::path(delete, path = "/api/modules/{module_id}", operation_id = "trash_module",
    params(("module_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Trashed), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "trash")]
pub async fn trash_module(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
) -> Result<Json<Trashed>, AppError> {
    let batch_id = trash_call(&state, user, "SELECT app_trash_module($1, $2)", id.0).await?;
    Ok(Json(Trashed { batch_id }))
}

/// 워크스페이스 휴지통(관리자는 전부, 작성자는 자기가 지운 것만).
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/trash", operation_id = "workspace_trash",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Trash), (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn workspace_trash(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Trash>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_visible(&mut tx, crate::access::Kind::Workspace, ws).await?;
    let (retention_days, version_retention_days): (i32, Option<i32>) = sqlx::query_as(
        "SELECT trash_retention_days, version_retention_days FROM workspaces WHERE id = $1",
    )
    .bind(ws)
    .fetch_one(tx.conn())
    .await?;
    let entries = entries(&mut tx, Some(ws)).await?;
    tx.commit().await?;
    Ok(Json(Trash {
        retention_days,
        version_retention_days,
        entries,
    }))
}

/// 내 개인 노트 휴지통(보존 30일).
#[utoipa::path(get, path = "/api/me/trash", operation_id = "my_trash",
    responses((status = 200, body = Trash)), tag = "trash")]
pub async fn my_trash(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Trash>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let entries = entries(&mut tx, None).await?;
    tx.commit().await?;
    Ok(Json(Trash {
        retention_days: 30,
        version_retention_days: None,
        entries,
    }))
}

/// 되살리기(관리자·지운 사람·개인 노트 소유자). 보존 기간이 지났으면 409.
#[utoipa::path(post, path = "/api/trash/{batch_id}/restore", operation_id = "restore_trash",
    params(("batch_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "trash")]
pub async fn restore(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(batch): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_restore_batch($1, $2)")
        .bind(batch)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 영구 삭제(관리자, 개인 노트는 소유자). 되돌릴 수 없다. 번호는 다시 쓰지 않는다.
#[utoipa::path(delete, path = "/api/trash/{batch_id}", operation_id = "purge_trash",
    params(("batch_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn purge(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(batch): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_purge_batch($1, $2)")
        .bind(batch)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 비운 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct Purged {
    pub purged: i32,
}

/// 만료된 묶음 비우기(관리자).
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/trash/purge-expired", operation_id = "purge_expired_trash",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Purged), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "trash")]
pub async fn purge_expired(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Purged>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    crate::access::require_visible(&mut tx, crate::access::Kind::Workspace, ws).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_purge_expired_ids($1)").bind(ws),
    )
    .await?;
    let purged: i32 = sqlx::query_scalar("SELECT app_purge_expired($1, $2)")
        .bind(ws)
        .bind(ids)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(Purged { purged }))
}

/// 내 개인 노트 휴지통의 만료된 묶음 비우기.
#[utoipa::path(post, path = "/api/me/trash/purge-expired", operation_id = "purge_my_expired_trash",
    responses((status = 200, body = Purged)), tag = "trash")]
pub async fn purge_my_expired(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Purged>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_purge_expired_ids(NULL)"),
    )
    .await?;
    let purged: i32 = sqlx::query_scalar("SELECT app_purge_expired(NULL, $1)")
        .bind(ids)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(Purged { purged }))
}

/// 휴지통 보존 기간.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetRetention {
    /// 1~3650일.
    pub days: i32,
}

/// 휴지통 보존 기간 바꾸기(관리자). 이미 지운 것에도 지금 기간이 적용된다.
#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/trash-retention", operation_id = "set_trash_retention",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = SetRetention,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn set_retention(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<SetRetention>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_trash_retention($1, $2, $3)")
        .bind(ws)
        .bind(input.days)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 이전 파일 버전 보존 기간.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetVersionRetention {
    /// 1~3650일. 없으면 영구 보존.
    pub days: Option<i32>,
}

/// 이전 파일 버전 보존 기간 바꾸기(관리자, D-56). 기간이 지난 이전 버전은 작업 프로세스가 지우되, 기록이 연결하거나
/// 문서 리비전이 가리키는 버전은 지우지 않는다.
#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/version-retention", operation_id = "set_version_retention",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = SetVersionRetention,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn set_version_retention(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<SetVersionRetention>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_version_retention($1, $2, $3)")
        .bind(ws)
        .bind(input.days)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
