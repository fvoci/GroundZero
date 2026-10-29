//! 일반 저장의 잠금은 공간 → 모듈 → 항목 → 문서다(#186). 기다리는 동안 이동했으면 새 자리에서 다시 요청한다.

use super::documents::{self, Access, DocRow};
use fvoci_core::error::AppError;
use fvoci_core::ids::DocumentId;
use fvoci_db::UserTx;
use uuid::Uuid;

pub(super) async fn for_save(
    tx: &mut UserTx,
    id: DocumentId,
) -> Result<(Access, DocRow), AppError> {
    let placement: (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT workspace_id, item_id FROM documents WHERE id=$1")
            .bind(id)
            .fetch_optional(tx.conn())
            .await?
            .ok_or(AppError::NotFound)?;
    sqlx::query("SELECT app_lock_document_workspace($1)")
        .bind(id)
        .execute(tx.conn())
        .await?;
    sqlx::query("SELECT app_lock_item($1)")
        .bind(placement.1)
        .execute(tx.conn())
        .await?;
    let access = documents::access(tx, id).await?.ok_or(AppError::NotFound)?;
    if !access.can_write() {
        sqlx::query("SELECT app_require_document($1, 'write')")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        return Err(AppError::Forbidden);
    }
    let current = documents::load_for_update(tx, id)
        .await?
        .ok_or(AppError::NotFound)?;
    let locked_placement: (Option<Uuid>, Option<Uuid>) =
        sqlx::query_as("SELECT workspace_id, item_id FROM documents WHERE id=$1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    if locked_placement != placement {
        return Err(AppError::Conflict(
            "the document moved while saving; reopen it and try again".into(),
        ));
    }
    Ok((access, current))
}
