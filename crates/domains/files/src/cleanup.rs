//! 저장소 정리 본문. 일정·만료 호출·백업 잠금 값은 조립 crate가 맡는다.

use fvoci_core::error::AppError;
use fvoci_db::Db;
use fvoci_storage::Storage;

/// 기한이 지난 업로드를 닫고, 저장소에서 지울 것(영구 삭제한 버전·그만둔 업로드)을 지운다. 실패한 것은
/// 다시 넣어 다음 주기에 한다(10번까지).
pub async fn clean_storage(
    db: &Db,
    storage: &Storage,
    storage_lock: i64,
    expire_uploads: impl std::future::Future<Output = Result<i32, AppError>>,
) -> Result<i32, AppError> {
    expire_uploads.await?;
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
                log_failure(&err, &key, attempts);
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

fn log_failure(err: &AppError, key: &str, attempts: i32) {
    if attempts >= 9 {
        tracing::error!(
            event = "storage.cleanup_gave_up",
            error_code = err.code(),
            detail = fvoci_core::logging::safe_error(err),
            key = key,
            attempts = attempts + 1,
            "storage cleanup gave up after 10 attempts"
        );
    } else {
        tracing::warn!(
            event = "storage.cleanup_retry",
            error_code = err.code(),
            detail = fvoci_core::logging::safe_error(err),
            key = key,
            attempts = attempts + 1,
            "storage cleanup failed; will retry"
        );
    }
}
