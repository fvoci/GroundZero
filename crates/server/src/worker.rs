//! 백그라운드 작업 (O-19: 한 바이너리, 실행 역할로 나눔, 작업 기록은 PostgreSQL 테이블).
//!
//! 확인 간격(`FVOCI_WORKER_INTERVAL_SECS`)마다 저장소 정리와 검색 색인 반영을 하고, 간격이 정해진 일(휴지통
//! 만료와 이전 파일 버전 정리는 매시간, 반복 업무 회차는 15분마다, 업무 기한·항목 알림은 매시간)은 때가 되었을
//! 때만 하는 [`run_once`]와, 그것을 간격마다 부르는 [`spawn`]이 있다. 간격이 정해진 일은 DB의 `app_claim_schedule`로
//! 이름마다 한 번만 돌므로 여러 프로세스가 `all`·`worker` 역할로 떠 있어도 겹치지 않는다. 요청 처리 경로의
//! 동기화(`/recurrences/sync`)와 관리자 비우기는 그대로 두어, 작업 프로세스가 없을 때도 따라잡을 수 있다.
//! 알림·색인·저장소 정리는 요청 처리 경로가 대신하지 않으므로 `api` 역할만 띄우면 하지 않는다.
//! 작업은 사용자 문맥 없이(`app.user_id` 없음) 정책에 묶인 `app_system_*` 함수만 부른다.

use std::time::Duration;

use tokio::sync::watch;
use uuid::Uuid;

use crate::search::{SearchClient, TextExtractor};
use crate::storage::Storage;
use fvoci_core::error::AppError;
use fvoci_db::{Db, needed_ids};
use fvoci_search::indexing::{drain, prepare};

/// 한 번 돈 결과(로그·테스트용).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    /// 지운 휴지통 묶음·이전 파일 버전·만료된 감사와 활동 기록의 합계.
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

/// 작업 함수 하나. 행을 만드는 함수는 앱이 만든 id 목록을 `$1`로 받고(D-88), 필요한 개수의 상한은 같은
/// 트랜잭션에서 먼저 `ids`로 묻는다. 행을 만들지 않는 함수는 `ids`가 없다.
#[derive(Debug, Clone, Copy)]
pub struct Job {
    pub ids: Option<&'static str>,
    pub call: &'static str,
}

/// 주기 작업 이름과 간격.
const PURGE: (&str, &str) = ("trash_purge", "1 hour");
/// 휴지통 만료와 보존 기간이 지난 이전 파일 버전 정리(D-41, D-56).
const PURGE_JOBS: &[Job] = &[
    Job {
        ids: Some("SELECT app_system_purge_expired_ids()"),
        call: "SELECT app_system_purge_expired($1)",
    },
    Job {
        ids: None,
        call: "SELECT app_system_purge_old_versions()",
    },
    Job {
        ids: None,
        call: "SELECT app_system_purge_audit($1)",
    },
    Job {
        ids: None,
        call: "SELECT app_system_purge_activity($1)",
    },
];
const RECURRENCE: (&str, &str) = ("recurrence_tick", "15 minutes");
/// 업무 기한 알림(하루 전·당일, 같은 날 같은 업무는 한 번). `current_date`는 트랜잭션 시작 시각 기준이라 개수와
/// 실제 호출이 같은 날을 본다.
const DUE: (&str, &str) = ("due_notify", "1 hour");
const DUE_JOBS: &[Job] = &[
    Job {
        ids: Some("SELECT app_system_due_notifications_ids(current_date)"),
        call: "SELECT app_system_due_notifications(current_date, $1)",
    },
    Job {
        ids: Some("SELECT app_system_item_reminders_ids(current_date)"),
        call: "SELECT app_system_item_reminders(current_date, $1)",
    },
];

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

/// 새 id가 모자라 되돌린 작업을 다시 하는 횟수(첫 시도 포함).
const SHORT_ID_ATTEMPTS: u32 = 3;

/// 작업 함수들을 한 트랜잭션에서 차례로 부르고 결과를 더한다.
///
/// 상한(`ids`)을 본 뒤 다른 트랜잭션이 대상을 늘리면 함수가 새 id 부족(FV001)으로 멈춘다(D-88). 그때는 트랜잭션을
/// 통째로 되돌리고(이 묶음의 아무것도 남지 않음) 상한부터 다시 보고 새 id로 처음부터 다시 한다. 정해진 횟수만큼만
/// 하고, 그래도 모자라면 오류로 끝낸다(다음 주기에 다시 한다). 실패한 트랜잭션 안에서 본 함수만 다시 부르지 않는다.
async fn system_call(db: &Db, jobs: &[Job], retention: Retention) -> Result<i32, AppError> {
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
fn is_new_ids_short(err: &AppError) -> bool {
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
                    fvoci_activity::retention::purge_audit(&mut tx, retention.audit_days).await?
                }
                "SELECT app_system_purge_activity($1)" => {
                    fvoci_activity::retention::purge_activity(&mut tx, retention.activity_days)
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

async fn sync_series_once(db: &Db, series: Uuid) -> Result<i32, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_system_sync_task_series_ids($1)").bind(series),
    )
    .await?;
    let made = sqlx::query_scalar("SELECT app_system_sync_task_series($1, $2)")
        .bind(series)
        .bind(ids)
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(made)
}

async fn recurrence(db: &Db) -> Result<i32, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let series: Vec<Uuid> = sqlx::query_scalar("SELECT app_system_recurrence_due()")
        .fetch_all(tx.conn())
        .await?;
    tx.commit().await?;
    let mut total = 0;
    for id in series {
        for attempt in 1..=SHORT_ID_ATTEMPTS {
            match sync_series_once(db, id).await {
                Ok(made) => {
                    total += made;
                    break;
                }
                Err(err) if is_new_ids_short(&err) && attempt < SHORT_ID_ATTEMPTS => {
                    tracing::warn!(event = "job.new_ids_short", series = %id, attempt,
                        "retrying recurrence series with a fresh transaction");
                }
                Err(err) => {
                    tracing::warn!(event = "recurrence.sync_failed", series = %id,
                        error_code = err.code(), detail = fvoci_core::logging::safe_error(&err),
                        "recurrence series failed; continuing with the next series");
                    break;
                }
            }
        }
    }
    Ok(total)
}

/// 주기 작업을 간격과 상관없이 바로 한 번씩 돌린다(테스트·운영 점검용).
pub async fn run_all_now(
    db: &Db,
    search: Option<&SearchClient>,
    storage: Option<&Storage>,
) -> Result<TickReport, AppError> {
    run_all_now_with_retention(db, search, storage, Retention::default()).await
}

/// 서버 기본 보존 기간을 적용해 주기 작업을 바로 돌린다.
pub async fn run_all_now_with_retention(
    db: &Db,
    search: Option<&SearchClient>,
    storage: Option<&Storage>,
    retention: Retention,
) -> Result<TickReport, AppError> {
    let purged = system_call(db, PURGE_JOBS, retention).await?;
    let occurrences = recurrence(db).await?;
    // 기한 알림은 반복 회차를 만든 뒤에(오늘 만든 회차도 알린다).
    let notified = system_call(db, DUE_JOBS, Retention::default()).await?;
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
        let n = drain(db, client, &TextExtractor(storage)).await?;
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
                detail = fvoci_core::logging::safe_error(&err),
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

async fn when_due(db: &Db, schedule: (&str, &str), jobs: &[Job]) -> Result<i32, AppError> {
    if claim(db, schedule).await? {
        system_call(db, jobs, Retention::default()).await
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
    run_once_with_retention(db, search, storage, Retention::default()).await
}

/// 서버 기본 보존 기간을 적용해 때가 된 주기 작업을 처리한다.
pub async fn run_once_with_retention(
    db: &Db,
    search: Option<&SearchClient>,
    storage: Option<&Storage>,
    retention: Retention,
) -> TickReport {
    let purged = logged("trash_purge", async {
        if !claim(db, PURGE).await? {
            return Ok(0);
        }
        system_call(db, PURGE_JOBS, retention).await
    })
    .await;
    TickReport {
        cleaned: match storage {
            Some(storage) => logged("storage", clean_storage(db, storage)).await,
            None => 0,
        },
        indexed: match search {
            Some(client) => logged("search", index(db, client, storage)).await,
            None => 0,
        },
        purged,
        occurrences: logged("recurrence", async {
            if claim(db, RECURRENCE).await? {
                recurrence(db).await
            } else {
                Ok(0)
            }
        })
        .await,
        notified: logged("due_notify", when_due(db, DUE, DUE_JOBS)).await,
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
    retention: Retention,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            rooms.persist_all().await;
            let report =
                run_once_with_retention(&db, search.as_ref(), Some(&storage), retention).await;
            if report != TickReport::default() {
                tracing::info!(
                    event = "job.tick",
                    purged = report.purged,
                    occurrences = report.occurrences,
                    indexed = report.indexed,
                    cleaned = report.cleaned,
                    notified = report.notified,
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
