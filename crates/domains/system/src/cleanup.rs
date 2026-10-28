//! 휴지통·이전 파일 버전 정리 본문. 트랜잭션 조립·재시도는 서버 worker가 맡는다.

use fvoci_core::error::AppError;
use fvoci_db::{AnonTx, needed_ids};

/// 만료된 휴지통을 정리한다. 트랜잭션의 커밋·전체 작업 재시도는 조립 crate가 맡는다.
pub async fn purge_expired(tx: &mut AnonTx) -> Result<i32, AppError> {
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_system_purge_expired_ids()"),
    )
    .await?;
    Ok(sqlx::query_scalar("SELECT app_system_purge_expired($1)")
        .bind(ids)
        .fetch_one(tx.conn())
        .await?)
}

/// 보존 기간이 지난 이전 파일 버전을 같은 트랜잭션에서 정리한다(D-56).
pub async fn purge_old_versions(tx: &mut AnonTx) -> Result<i32, AppError> {
    Ok(sqlx::query_scalar("SELECT app_system_purge_old_versions()")
        .fetch_one(tx.conn())
        .await?)
}
