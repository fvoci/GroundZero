//! 파일 행을 API 자원으로 읽는다.
use crate::model::FileView;
use fvoci_core::error::AppError;
use fvoci_core::ids::{DocumentId, FileId, ItemId};
use fvoci_db::UserTx;
use time::OffsetDateTime;
pub(crate) type FileRow = (
    FileId,
    String,
    Option<DocumentId>,
    Option<ItemId>,
    i32,
    Option<i64>,
    Option<String>,
    OffsetDateTime,
    String,
    bool,
);

// 보관을 포함한 접근 등급은 SQL 한 곳에서 계산한다.
pub(crate) const FILE_SELECT: &str = "SELECT f.id, f.name, f.document_id, f.item_id, f.current_version, v.size, v.content_type,
                                  f.updated_at,
                                  app_access('file', f.id),
                                  app_file_can_download(f.id)
                           FROM files f
                           LEFT JOIN file_versions v ON v.file_id = f.id AND v.version = f.current_version";

pub(crate) fn file_view(row: FileRow) -> FileView {
    let (
        id,
        name,
        document_id,
        item_id,
        current_version,
        size,
        content_type,
        updated_at,
        access,
        can_download,
    ) = row;
    FileView {
        id,
        name,
        document_id,
        item_id,
        current_version,
        size: size.unwrap_or_default(),
        content_type: content_type.unwrap_or_else(|| "application/octet-stream".into()),
        updated_at,
        can_write: matches!(access.as_str(), "owner" | "write"),
        can_download,
    }
}

pub(crate) async fn one_file(tx: &mut UserTx, id: FileId) -> Result<FileView, AppError> {
    let row: Option<FileRow> = sqlx::query_as(&format!("{FILE_SELECT} WHERE f.id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    row.filter(|r| r.8 != "none")
        .map(file_view)
        .ok_or(AppError::NotFound)
}
