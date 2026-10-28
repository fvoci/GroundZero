//! 활동·감사 보존 실행 본문. 일정과 트랜잭션 조립은 서버 작업자가 맡는다.

use fvoci_core::error::AppError;
use fvoci_db::AnonTx;

/// 조직·공간 한도와 서버 기본값에 따라 만료 감사를 정리한다.
pub async fn purge_audit(tx: &mut AnonTx, days: Option<i32>) -> Result<i32, AppError> {
    Ok(sqlx::query_scalar("SELECT app_system_purge_audit($1)")
        .bind(days)
        .fetch_one(tx.conn())
        .await?)
}

/// 조직·공간 한도와 서버 기본값에 따라 만료 활동을 정리한다.
pub async fn purge_activity(tx: &mut AnonTx, days: Option<i32>) -> Result<i32, AppError> {
    Ok(sqlx::query_scalar("SELECT app_system_purge_activity($1)")
        .bind(days)
        .fetch_one(tx.conn())
        .await?)
}
