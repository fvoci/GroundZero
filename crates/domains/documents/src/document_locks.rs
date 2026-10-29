//! 제목·실시간 체크포인트·복원의 공통 SQL 잠금. 개인 노트는 공유 조직 → 조직 멤버 → 문서다(#233).

use super::documents::{self, Access, DocRow};
use fvoci_core::error::AppError;
use fvoci_core::ids::DocumentId;
use fvoci_db::UserTx;

/// 저장 경로의 행을 순서대로 잠근 뒤 현재 자리와 쓰기 권한을 다시 확인한다.
pub async fn for_save(tx: &mut UserTx, id: DocumentId) -> Result<(Access, DocRow), AppError> {
    // SQL이 상위 행과 문서를 순서대로 잠그고, 자리·공유 조직·쓰기 권한을 다시 확인한다.
    sqlx::query("SELECT app_lock_document_workspace($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let current = documents::load_for_update(tx, id).await?;
    let access = require_write(tx, id).await?;
    let current = current.ok_or(AppError::NotFound)?;
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
