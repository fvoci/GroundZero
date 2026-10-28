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

use crate::search::{SearchClient, drain, prepare};
use crate::storage::Storage;
use fvoci_core::error::AppError;
use fvoci_db::{Db, needed_ids};
pub use fvoci_system::cleanup::{Job, Retention, run_jobs};
use fvoci_system::cleanup::{SHORT_ID_ATTEMPTS, clean_storage, is_new_ids_short, system_call};

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
            Some(storage) => clean_storage(db, storage, crate::backup::STORAGE_LOCK).await?,
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
            Some(storage) => {
                logged(
                    "storage",
                    clean_storage(db, storage, crate::backup::STORAGE_LOCK),
                )
                .await
            }
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
