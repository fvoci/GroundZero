//! 공간 읽기의 등급 확인. app_access 조회만 하며 명시적 행 잠금을 잡지 않는다.

use fvoci_core::{access::Grade, error::AppError, ids::WorkspaceId};
use fvoci_db::UserTx;

pub(super) async fn require_visible(tx: &mut UserTx, ws: WorkspaceId) -> Result<Grade, AppError> {
    let raw: String = sqlx::query_scalar("SELECT app_access('workspace', $1)")
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    Grade::parse(&raw)?.ok_or(AppError::NotFound)
}

pub(super) async fn require_owner(tx: &mut UserTx, ws: WorkspaceId) -> Result<(), AppError> {
    match require_visible(tx, ws).await? {
        Grade::Owner => Ok(()),
        _ => Err(AppError::Forbidden),
    }
}
