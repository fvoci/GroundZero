//! 자동 정리 실행 본문. 예약·호출 순서는 조립 crate의 worker에 남는다.

use crate::state::ObjectStore;
use fvoci_core::error::AppError;
use fvoci_db::{Db, needed_ids};

/// 작업 함수 하나. 행을 만드는 함수는 앱이 만든 id 목록을 `$1`로 받고(D-88), 필요한 개수의 상한은 같은
/// 트랜잭션에서 먼저 `ids`로 묻는다. 행을 만들지 않는 함수는 `ids`가 없다.
#[derive(Debug, Clone, Copy)]
pub struct Job {
    pub ids: Option<&'static str>,
    pub call: &'static str,
}

/// 새 id가 모자라 되돌린 작업을 다시 하는 횟수(첫 시도 포함).
pub const SHORT_ID_ATTEMPTS: u32 = 3;

/// 작업 함수들을 한 트랜잭션에서 차례로 부르고 결과를 더한다.
///
/// 상한(`ids`)을 본 뒤 다른 트랜잭션이 대상을 늘리면 함수가 새 id 부족(FV001)으로 멈춘다(D-88). 그때는 트랜잭션을
/// 통째로 되돌리고(이 묶음의 아무것도 남지 않음) 상한부터 다시 보고 새 id로 처음부터 다시 한다. 정해진 횟수만큼만
/// 하고, 그래도 모자라면 오류로 끝낸다(다음 주기에 다시 한다). 실패한 트랜잭션 안에서 본 함수만 다시 부르지 않는다.
pub async fn system_call(db: &Db, jobs: &[Job], retention: Retention) -> Result<i32, AppError> {
    let mut attempt = 1;
    loop {
        match system_call_once(db, jobs, retention).await {
            Err(err) if is_new_ids_short(&err) && attempt < SHORT_ID_ATTEMPTS => {
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

/// 작업 함수 묶음을 바로 한 번 돌린다(새 id 부족이면 정해진 횟수만큼 처음부터 다시, 테스트·운영 점검용).
pub async fn run_jobs(db: &Db, jobs: &[Job]) -> Result<i32, AppError> {
    system_call(db, jobs, Retention::default()).await
}

/// 새 id 부족(FV001)인지. HTTP 경로는 같은 오류를 409로 돌려준다(`AppError::from_db_function`).
pub fn is_new_ids_short(err: &AppError) -> bool {
    matches!(err, AppError::Database(e)
        if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("FV001"))
}

async fn system_call_once(db: &Db, jobs: &[Job], retention: Retention) -> Result<i32, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let mut total = 0;
    for job in jobs {
        let n: i32 = match job.ids {
            Some(bound) => {
                let ids = needed_ids(tx.conn(), sqlx::query_scalar(bound)).await?;
                sqlx::query_scalar(job.call)
                    .bind(ids)
                    .fetch_one(tx.conn())
                    .await?
            }
            None => match job.call {
                "SELECT app_system_purge_audit($1)" => {
                    sqlx::query_scalar(job.call)
                        .bind(retention.audit_days)
                        .fetch_one(tx.conn())
                        .await?
                }
                "SELECT app_system_purge_activity($1)" => {
                    sqlx::query_scalar(job.call)
                        .bind(retention.activity_days)
                        .fetch_one(tx.conn())
                        .await?
                }
                _ => sqlx::query_scalar(job.call).fetch_one(tx.conn()).await?,
            },
        };
        total += n;
    }
    tx.commit().await?;
    Ok(total)
}

/// 서버 기본 보존 기간. None이면 조직·공간 설정만 따르고 서버 기본값으로는 지우지 않는다.
#[derive(Debug, Default, Clone, Copy)]
pub struct Retention {
    pub audit_days: Option<i32>,
    pub activity_days: Option<i32>,
}

/// 기한이 지난 업로드를 닫고, 저장소에서 지울 것(영구 삭제한 버전·그만둔 업로드)을 지운다. 실패한 것은
/// 다시 넣어 다음 주기에 한다(10번까지).
pub async fn clean_storage(
    db: &Db,
    storage: &dyn ObjectStore,
    storage_lock: i64,
) -> Result<i32, AppError> {
    system_call(
        db,
        &[Job {
            ids: None,
            call: "SELECT app_system_expire_uploads()",
        }],
        Retention::default(),
    )
    .await?;
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
                if attempts >= 9 {
                    tracing::error!(
                        event = "storage.cleanup_gave_up",
                        error_code = err.code(),
                        detail = fvoci_core::logging::safe_error(&err),
                        key = key.as_str(),
                        attempts = attempts + 1,
                        "storage cleanup gave up after 10 attempts"
                    );
                } else {
                    tracing::warn!(
                        event = "storage.cleanup_retry",
                        error_code = err.code(),
                        detail = fvoci_core::logging::safe_error(&err),
                        key = key.as_str(),
                        attempts = attempts + 1,
                        "storage cleanup failed; will retry"
                    );
                }
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
