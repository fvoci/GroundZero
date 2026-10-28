use super::model::{Purged, SetRetention, SetVersionRetention, Trashed};
use crate::SystemState;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    ids::{DocumentId, ItemId, ModuleId, UserId, WorkspaceId},
};
use fvoci_db::{needed_ids, new_ids};
use fvoci_http::session::CurrentUser;
use uuid::Uuid;

async fn trash_call(
    state: &SystemState,
    user: UserId,
    sql: &str,
    id: Uuid,
) -> Result<Uuid, AppError> {
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
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Trashed>, AppError> {
    let batch_id = trash_call(&state, user, "SELECT app_trash_document($1, $2)", id.0).await?;
    Ok(Json(Trashed { batch_id }))
}

/// 항목 지우기(항목 소유 권한). 그 안의 업무·문서·일정은 항목과 함께 보이지 않는다.
#[utoipa::path(delete, path = "/api/items/{item_id}", operation_id = "trash_item",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Trashed), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "trash")]
pub async fn trash_item(
    State(state): State<SystemState>,
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
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ModuleId>,
) -> Result<Json<Trashed>, AppError> {
    let batch_id = trash_call(&state, user, "SELECT app_trash_module($1, $2)", id.0).await?;
    Ok(Json(Trashed { batch_id }))
}

/// 되살리기(관리자·지운 사람·개인 노트 소유자). 보존 기간이 지났으면 409.
#[utoipa::path(post, path = "/api/trash/{batch_id}/restore", operation_id = "restore_trash",
    params(("batch_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "trash")]
pub async fn restore(
    State(state): State<SystemState>,
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
    State(state): State<SystemState>,
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

/// 만료된 묶음 비우기(관리자).
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/trash/purge-expired", operation_id = "purge_expired_trash",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Purged), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "trash")]
pub async fn purge_expired(
    State(state): State<SystemState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Purged>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_core::access::Kind::Workspace, ws).await?;
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
    State(state): State<SystemState>,
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

/// 휴지통 보존 기간 바꾸기(관리자). 이미 지운 것에도 지금 기간이 적용된다.
#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/trash-retention", operation_id = "set_trash_retention",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = SetRetention,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn set_retention(
    State(state): State<SystemState>,
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

/// 이전 파일 버전 보존 기간 바꾸기(관리자, D-56). 기간이 지난 이전 버전은 작업 프로세스가 지우되, 기록이 연결하거나
/// 문서 리비전이 가리키는 버전은 지우지 않는다.
#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/version-retention", operation_id = "set_version_retention",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = SetVersionRetention,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "trash")]
pub async fn set_version_retention(
    State(state): State<SystemState>,
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
