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
}

/// 주기 작업 이름과 간격.
const PURGE: (&str, &str) = ("trash_purge", "1 hour");
const RECURRENCE: (&str, &str) = ("recurrence_tick", "15 minutes");

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
    let purged = system_call(db, "SELECT app_system_purge_expired()").await?;
    Ok(TickReport {
        purged,
        cleaned: match storage {
            Some(storage) => clean_storage(db, storage).await?,
            None => 0,
        },
        occurrences: system_call(db, "SELECT app_system_recurrence_tick()").await?,
        indexed: match search {
            Some(client) => {
                prepare(db, client).await?;
                drain_all(db, client).await?
            }
            None => 0,
        },
    })
}

/// 검색 대기열을 빌 때까지 반영한다(한 번에 최대 20회).
async fn drain_all(db: &Db, client: &SearchClient) -> Result<i32, AppError> {
    let mut total = 0;
    for _ in 0..20 {
        let n = drain(db, client).await?;
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
                    tracing::error!(key, error = %err, "storage cleanup gave up after 10 attempts");
                } else {
                    tracing::warn!(key, error = %err, "storage cleanup failed; will retry");
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

/// 일 하나의 실패를 기록하고 0으로 본다. 한 일이 실패해도(예: 검색 서비스 중단) 다른 일은 계속한다.
fn logged(job: &str, result: Result<i32, AppError>) -> i32 {
    result.unwrap_or_else(|err| {
        tracing::warn!(job, error = %err, "background job failed");
        0
    })
}

async fn index(db: &Db, client: &SearchClient) -> Result<i32, AppError> {
    prepare(db, client).await?;
    drain_all(db, client).await
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
            Some(storage) => logged("storage", clean_storage(db, storage).await),
            None => 0,
        },
        indexed: match search {
            Some(client) => logged("search", index(db, client).await),
            None => 0,
        },
        purged: logged(
            "trash_purge",
            when_due(db, PURGE, "SELECT app_system_purge_expired()").await,
        ),
        occurrences: logged(
            "recurrence",
            when_due(db, RECURRENCE, "SELECT app_system_recurrence_tick()").await,
        ),
    }
}

/// 간격마다 [`run_once`]를 부르는 작업을 띄운다. `shutdown`이 바뀌면 멈춘다.
pub fn spawn(
    db: Db,
    search: Option<SearchClient>,
    storage: Storage,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let report = run_once(&db, search.as_ref(), Some(&storage)).await;
            if report != TickReport::default() {
                tracing::info!(
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
