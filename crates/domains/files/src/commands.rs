//! 파일 이름·휴지통·버전 삭제 명령.
use crate::{
    FilesState,
    model::{FileTrashed, FileView, RenameFile},
    resource::one_file,
};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::FileId;
use fvoci_db::new_ids;
use fvoci_http::session::CurrentUser;
use uuid::Uuid;
/// 이름 바꾸기(쓸 수 있는 사람).
#[utoipa::path(put, path = "/api/files/{file_id}", operation_id = "rename_file",
    params(("file_id" = String, Path, format = Uuid)), request_body = RenameFile,
    responses((status = 200, body = FileView), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "files")]
pub async fn rename_file(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
    Json(input): Json<RenameFile>,
) -> Result<Json<FileView>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_rename_file($1, $2, $3)")
        .bind(id)
        .bind(&input.name)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let file = one_file(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(file))
}

/// 휴지통으로(쓸 수 있는 사람). 모든 버전이 함께 가고 함께 돌아온다.
#[utoipa::path(delete, path = "/api/files/{file_id}", operation_id = "trash_file",
    params(("file_id" = String, Path, format = Uuid)),
    responses((status = 200, body = FileTrashed), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "files")]
pub async fn trash_file(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
) -> Result<Json<FileTrashed>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let batch_id: Uuid = sqlx::query_scalar("SELECT app_trash_file($1, $2)")
        .bind(id)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(FileTrashed { batch_id }))
}

/// 현재가 아닌 버전 하나를 영구 삭제(쓸 수 있는 사람, D-56). 저장소의 내용은 백그라운드 작업이 지운다.
#[utoipa::path(delete, path = "/api/files/{file_id}/versions/{version}", operation_id = "delete_file_version",
    params(("file_id" = String, Path, format = Uuid), ("version" = i32, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "files")]
pub async fn delete_version(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path((id, version)): Path<(FileId, i32)>,
) -> Result<StatusCode, AppError> {
    // 그 파일의 워크스페이스에서 열린 실시간 방의 상태(본문의 파일 참조 포함)를 먼저 저장해, 방금 실시간 본문에
    // 넣은 이미지의 버전도 참조 확인(0059)이 보게 한다. 주기적 저장(3초)을 기다리면 그 사이에 지울 수 있었다.
    // 볼 수 없거나 고칠 수 없는 파일이면 아무 방도 건드리지 않고 아래 함수가 404·403을 낸다.
    let mut tx = state.db.begin_as(user).await?;
    let workspace: Option<Option<Uuid>> = sqlx::query_scalar(
        "SELECT workspace_id FROM files WHERE id = $1 AND app_access('file', id) IN ('owner', 'write')",
    )
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    if let Some(Some(workspace)) = workspace {
        state.collab.persist_workspace(workspace).await;
    }
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_file_version($1, $2, $3)")
        .bind(id)
        .bind(version)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
