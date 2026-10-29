//! 공통 접근 등급과 문서 관리 확인.

use fvoci_core::error::AppError;
use fvoci_core::ids::DocumentId;
use fvoci_db::UserTx;

use super::Access;

/// 현재 사용자의 권한. 볼 수 없으면 `None`.
pub async fn access(tx: &mut UserTx, id: DocumentId) -> Result<Option<Access>, AppError> {
    fvoci_db::access::grade(tx, fvoci_db::access::Kind::Document, id).await
}

/// 관리 목록은 SQL의 관리 판단(현재 등급 + 보관 중 관리자 예외)을 따른다.
/// 관리 허용이 내용 쓰기 등급을 높이지는 않는다.
pub async fn require_manager(tx: &mut UserTx, id: DocumentId) -> Result<(), AppError> {
    let can_manage: bool = sqlx::query_scalar("SELECT app_document_can_manage($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    if can_manage {
        return Ok(());
    }
    match access(tx, id).await? {
        None => Err(AppError::NotFound),
        Some(_) => Err(AppError::Forbidden),
    }
}
