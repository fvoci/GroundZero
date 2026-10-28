//! 자동 정리 실행 본문. 예약·호출 순서는 조립 crate의 worker에 남는다.

use crate::state::ObjectStore;
use fvoci_core::error::AppError;
use fvoci_db::{AnonTx, Db, needed_ids};

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

async fn expire_uploads(db: &Db) -> Result<(), AppError> {
    let mut attempt = 1;
    loop {
        match expire_uploads_once(db).await {
            Err(err) if is_new_ids_short(&err) && attempt < 3 => {
                tracing::warn!(
                    event = "job.new_ids_short",
                    attempt,
                    "a background job needed more new ids than its bound; retrying the whole job"
                );
                attempt += 1;
            }
            other => return other,
        }
    }
}

async fn expire_uploads_once(db: &Db) -> Result<(), AppError> {
    let mut tx = db.begin_anonymous().await?;
    let _: i32 = sqlx::query_scalar("SELECT app_system_expire_uploads()")
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(())
}

/// 새 id 부족(FV001)인지. HTTP 경로는 같은 오류를 409로 돌려준다(`AppError::from_db_function`).
fn is_new_ids_short(err: &AppError) -> bool {
    matches!(err, AppError::Database(e)
        if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("FV001"))
}

/// 기한이 지난 업로드를 닫고, 저장소에서 지울 것(영구 삭제한 버전·그만둔 업로드)을 지운다. 실패한 것은
/// 다시 넣어 다음 주기에 한다(10번까지).
pub async fn clean_storage(
    db: &Db,
    storage: &dyn ObjectStore,
    storage_lock: i64,
) -> Result<i32, AppError> {
    expire_uploads(db).await?;
    let mut done = 0;
    // 하나씩 꺼내 지우고 바로 끝낸다(저장소 호출 동안 트랜잭션을 길게 열지 않는다). 한 주기에 200개까지.
    for _ in 0..200 {
        let mut tx = db.begin_anonymous().await?;
        // 백업·복구 중이면 쉰다(복사 중인 내용을 지우지 않게).
        let free: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock_shared($1)")
            .bind(storage_lock)
            .fetch_one(tx.conn())
            .await?;
        if !free {
            break;
        }
        let Some((key, upload, attempts)): Option<(String, String, i32)> = sqlx::query_as(
            "SELECT storage_key, storage_upload_id, attempts FROM app_system_storage_claim(1)",
        )
        .fetch_optional(tx.conn())
        .await?
        else {
            break;
        };
        let result = if upload.is_empty() {
            storage.delete(&key).await
        } else {
            storage.abort(&key, &upload).await
        };
        match result {
            Ok(()) => done += 1,
            Err(err) => {
                log_cleanup_failure(&key, attempts, &err);
                sqlx::query("SELECT app_system_storage_retry($1, $2, $3)")
                    .bind(&key)
                    .bind(&upload)
                    .bind(attempts)
                    .execute(tx.conn())
                    .await?;
            }
        }
        tx.commit().await?;
    }
    Ok(done)
}

fn log_cleanup_failure(key: &str, attempts: i32, err: &AppError) {
    if attempts >= 9 {
        tracing::error!(
            event = "storage.cleanup_gave_up",
            error_code = err.code(),
            detail = fvoci_core::logging::safe_error(err),
            key,
            attempts = attempts + 1,
            "storage cleanup gave up after 10 attempts"
        );
    } else {
        tracing::warn!(
            event = "storage.cleanup_retry",
            error_code = err.code(),
            detail = fvoci_core::logging::safe_error(err),
            key,
            attempts = attempts + 1,
            "storage cleanup failed; will retry"
        );
    }
}
