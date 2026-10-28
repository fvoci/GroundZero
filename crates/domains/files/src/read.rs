//! 첨부 목록과 파일·버전 읽기.
use crate::{
    FilesState,
    model::{FileDetail, FileView, VersionView},
    resource::{FILE_SELECT, FileRow, file_view, one_file},
};
use axum::Json;
use axum::extract::{Path, State};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, FileId, ItemId, UserId};
use fvoci_http::session::CurrentUser;
use time::OffsetDateTime;
/// 문서에 붙은 파일(문서를 볼 수 있는 사람).
#[utoipa::path(get, path = "/api/documents/{document_id}/files", operation_id = "document_files",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [FileView]), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn document_files(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(doc): Path<DocumentId>,
) -> Result<Json<Vec<FileView>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Document, doc).await?;
    let rows: Vec<FileRow> = sqlx::query_as(&format!(
        "{FILE_SELECT} WHERE f.document_id = $1 AND f.deleted_at IS NULL ORDER BY f.name, f.id"
    ))
    .bind(doc)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows.into_iter().map(file_view).collect()))
}

/// 항목에 붙은 파일(항목을 볼 수 있는 사람).
#[utoipa::path(get, path = "/api/items/{item_id}/files", operation_id = "item_files",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [FileView]), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn item_files(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<FileView>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Item, item).await?;
    let rows: Vec<FileRow> = sqlx::query_as(&format!(
        "{FILE_SELECT} WHERE f.item_id = $1 AND f.deleted_at IS NULL ORDER BY f.name, f.id"
    ))
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows.into_iter().map(file_view).collect()))
}

/// 파일과 모든 버전.
#[utoipa::path(get, path = "/api/files/{file_id}", operation_id = "get_file",
    params(("file_id" = String, Path, format = Uuid)),
    responses((status = 200, body = FileDetail), (status = 404, body = ErrorBody)), tag = "files")]
pub async fn get_file(
    State(state): State<FilesState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<FileId>,
) -> Result<Json<FileDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let file = one_file(&mut tx, id).await?;
    type Row = (
        i32,
        i64,
        String,
        String,
        String,
        UserId,
        Option<String>,
        OffsetDateTime,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT v.version, v.size, v.sha256, v.content_type, v.note, v.uploaded_by, u.display_name, v.created_at
         FROM file_versions v LEFT JOIN users u ON u.id = v.uploaded_by
         WHERE v.file_id = $1 ORDER BY v.version DESC",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let versions = rows
        .into_iter()
        .map(
            |(
                version,
                size,
                sha256,
                content_type,
                note,
                uploaded_by,
                uploaded_by_name,
                created_at,
            )| {
                VersionView {
                    version,
                    size,
                    sha256,
                    content_type,
                    note,
                    uploaded_by,
                    uploaded_by_name,
                    created_at,
                }
            },
        )
        .collect();
    Ok(Json(FileDetail { file, versions }))
}
