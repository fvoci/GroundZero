//! 제목·실시간 체크포인트의 잠금은 공간 → 공간 멤버 → 모듈 → 항목 → 문서다(#186).

use super::documents::{self, Access, DocRow};
use fvoci_core::error::AppError;
use fvoci_core::ids::DocumentId;
use fvoci_db::UserTx;
use uuid::Uuid;

/// 저장 경로의 행을 순서대로 잠근 뒤 현재 자리와 쓰기 권한을 다시 확인한다.
pub async fn for_save(tx: &mut UserTx, id: DocumentId) -> Result<(Access, DocRow), AppError> {
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
    // 읽기만 허용된 문서는 갱신 RLS가 가리기 전에 기존 403/409를 돌려준다.
    require_write(tx, id).await?;
    let current = documents::load_for_update(tx, id).await?;
    let access = require_write(tx, id).await?;
    let current = current.ok_or(AppError::NotFound)?;
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

async fn require_write(tx: &mut UserTx, id: DocumentId) -> Result<Access, AppError> {
    let access = documents::access(tx, id).await?.ok_or(AppError::NotFound)?;
    if !access.can_write() {
        sqlx::query("SELECT app_require_document($1, 'write')")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        return Err(AppError::Forbidden);
    }
    Ok(access)
}
