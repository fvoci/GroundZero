//! 백그라운드 작업 (O-19: 한 바이너리, 실행 역할로 나눔, 작업 기록은 PostgreSQL 테이블).
//!
//! 정해진 주기의 일(만료된 휴지통 비우기, 반복 업무 회차 만들기)을 한 번씩
//! 처리하는 [`run_once`]와, 그것을 간격마다 부르는 [`spawn`]이 있다. 주기 작업은 DB의 `app_claim_schedule`로
//! 이름마다 한 번만 돌므로 여러 프로세스가 `all`·`worker` 역할로 떠 있어도 겹치지 않는다. 요청 처리 경로의
//! 동기화(`/recurrences/sync`)와 관리자 비우기는 그대로 두어, 작업 프로세스가 없을 때도 따라잡을 수 있다.
//! 작업은 사용자 문맥 없이(`app.user_id` 없음) 정책에 묶인 `app_system_*` 함수만 부른다.

use std::time::Duration;

use tokio::sync::watch;

use crate::db::Db;
use crate::error::AppError;
use crate::search::{SearchClient, drain, prepare};
use crate::storage::Storage;

/// 한 번 돈 결과(로그·테스트용).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    /// 영구 삭제한 휴지통 묶음 수(만료분).
    pub purged: i32,
    /// 만든 반복 업무 회차 수.
    pub occurrences: i32,
    /// 색인에 반영한 수(검색).
    pub indexed: i32,
    /// 저장소에서 지운 수(영구 삭제한 파일 내용, 그만두거나 기한이 지난 업로드).
    pub cleaned: i32,
    /// 만든 기한 알림 수.
    pub notified: i32,
}

/// 주기 작업 이름과 간격.
const PURGE: (&str, &str) = ("trash_purge", "1 hour");
/// 휴지통 만료와 보존 기간이 지난 이전 파일 버전 정리(D-41, D-56).
const PURGE_SQL: &str = "SELECT app_system_purge_expired() + app_system_purge_old_versions()";
const RECURRENCE: (&str, &str) = ("recurrence_tick", "15 minutes");
/// 업무 기한 알림(하루 전·당일, 같은 날 같은 업무는 한 번).
const DUE: (&str, &str) = ("due_notify", "1 hour");
const DUE_SQL: &str =
    "SELECT app_system_due_notifications(current_date) + app_system_item_reminders(current_date)";

async fn claim(db: &Db, (name, every): (&str, &str)) -> Result<bool, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let due: bool = sqlx::query_scalar("SELECT app_claim_schedule($1, $2::interval)")
        .bind(name)
        .bind(every)
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(due)
}

async fn system_call(db: &Db, sql: &str) -> Result<i32, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let n: i32 = sqlx::query_scalar(sql).fetch_one(tx.conn()).await?;
    tx.commit().await?;
    Ok(n)
}

/// 주기 작업을 간격과 상관없이 바로 한 번씩 돌린다(테스트·운영 점검용).
pub async fn run_all_now(
    db: &Db,
    search: Option<&SearchClient>,
    storage: Option<&Storage>,
) -> Result<TickReport, AppError> {
    let purged = system_call(db, PURGE_SQL).await?;
    let occurrences = system_call(db, "SELECT app_system_recurrence_tick()").await?;
    // 기한 알림은 반복 회차를 만든 뒤에(오늘 만든 회차도 알린다).
    let notified = system_call(db, DUE_SQL).await?;
    Ok(TickReport {
        purged,
        notified,
        cleaned: match storage {
            Some(storage) => clean_storage(db, storage).await?,
            None => 0,
        },
        occurrences,
        indexed: match search {
            Some(client) => {
                prepare(db, client).await?;
                drain_all(db, client, storage).await?
            }
            None => 0,
        },
    })
}

/// 검색 대기열을 빌 때까지 반영한다(한 번에 최대 20회).
async fn drain_all(
    db: &Db,
    client: &SearchClient,
    storage: Option<&Storage>,
) -> Result<i32, AppError> {
    let mut total = 0;
    for _ in 0..20 {
        let n = drain(db, client, storage).await?;
        total += n;
        if n == 0 {
            break;
        }
    }
    Ok(total)
}

/// 기한이 지난 업로드를 닫고, 저장소에서 지울 것(영구 삭제한 버전·그만둔 업로드)을 지운다. 실패한 것은
/// 다시 넣어 다음 주기에 한다(10번까지).
async fn clean_storage(db: &Db, storage: &Storage) -> Result<i32, AppError> {
    system_call(db, "SELECT app_system_expire_uploads()").await?;
    let mut done = 0;
    // 하나씩 꺼내 지우고 바로 끝낸다(저장소 호출 동안 트랜잭션을 길게 열지 않는다). 한 주기에 200개까지.
    for _ in 0..200 {
        let mut tx = db.begin_anonymous().await?;
        // 백업·복구 중이면 쉰다(복사 중인 내용을 지우지 않게).
        let free: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock_shared($1)")
            .bind(crate::backup::STORAGE_LOCK)
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
                        detail = crate::logging::safe_error(&err),
                        key = key.as_str(),
                        attempts = attempts + 1,
                        "storage cleanup gave up after 10 attempts"
                    );
                } else {
                    tracing::warn!(
                        event = "storage.cleanup_retry",
                        error_code = err.code(),
                        detail = crate::logging::safe_error(&err),
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

/// 일 하나를 작업 id 안에서 하고(그 안의 로그 줄에 `job`·`job_id`, D-83), 실패를 기록하고 0으로 본다. 한 일이
/// 실패해도(예: 검색 서비스 중단) 다른 일은 계속한다.
async fn logged(
    job: &'static str,
    work: impl std::future::Future<Output = Result<i32, AppError>>,
) -> i32 {
    crate::logging::in_job(job, async {
        work.await.unwrap_or_else(|err| {
            tracing::warn!(
                event = "job.failed",
                error_code = err.code(),
                detail = crate::logging::safe_error(&err),
                "background job failed"
            );
            0
        })
    })
    .await
}

async fn index(db: &Db, client: &SearchClient, storage: Option<&Storage>) -> Result<i32, AppError> {
    prepare(db, client).await?;
    drain_all(db, client, storage).await
}

async fn when_due(db: &Db, schedule: (&str, &str), sql: &str) -> Result<i32, AppError> {
    if claim(db, schedule).await? {
        system_call(db, sql).await
    } else {
        Ok(0)
    }
}

/// 때가 된 주기 작업을 한 번 처리한다. 일마다 따로 실패를 기록하고 계속한다.
pub async fn run_once(
    db: &Db,
    search: Option<&SearchClient>,
    storage: Option<&Storage>,
) -> TickReport {
    TickReport {
        cleaned: match storage {
            Some(storage) => logged("storage", clean_storage(db, storage)).await,
            None => 0,
        },
        indexed: match search {
            Some(client) => logged("search", index(db, client, storage)).await,
            None => 0,
        },
        purged: logged("trash_purge", when_due(db, PURGE, PURGE_SQL)).await,
        occurrences: logged(
            "recurrence",
            when_due(db, RECURRENCE, "SELECT app_system_recurrence_tick()"),
        )
        .await,
        notified: logged("due_notify", when_due(db, DUE, DUE_SQL)).await,
    }
}

/// 간격마다 [`run_once`]를 부르는 작업을 띄운다. `shutdown`이 바뀌면 멈춘다.
/// 같은 프로세스의 열린 실시간 방은 매번 먼저 상태를 저장하게 해, 방금 실시간 본문에 넣은 이미지의 파일 참조가
/// 이전 버전 정리·휴지통 비우기의 참조 확인에 보이게 한다(작업만 따로 도는 프로세스에는 방이 없다).
pub fn spawn(
    db: Db,
    search: Option<SearchClient>,
    storage: Storage,
    rooms: crate::collab::Rooms,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            rooms.persist_all().await;
            let report = run_once(&db, search.as_ref(), Some(&storage)).await;
            if report != TickReport::default() {
                tracing::info!(
                    event = "job.tick",
                    purged = report.purged,
                    occurrences = report.occurrences,
                    indexed = report.indexed,
                    "background tick"
                );
            }
            tokio::select! {
                () = tokio::time::sleep(interval) => {}
                _ = shutdown.changed() => break,
            }
        }
    })
}
