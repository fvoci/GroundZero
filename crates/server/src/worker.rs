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
use crate::search::{SearchClient, drain};

/// 한 번 돈 결과(로그·테스트용).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TickReport {
    /// 영구 삭제한 휴지통 묶음 수(만료분).
    pub purged: i32,
    /// 만든 반복 업무 회차 수.
    pub occurrences: i32,
    /// 색인에 반영한 수(검색).
    pub indexed: i32,
}

/// 주기 작업 이름과 간격.
const PURGE: (&str, &str) = ("trash_purge", "1 hour");
const RECURRENCE: (&str, &str) = ("recurrence_tick", "15 minutes");
/// 처음 한 번 모든 것을 색인 대기열에 올린다(이름만 남아 다시 돌지 않는다).
const SEARCH_BOOTSTRAP: (&str, &str) = ("search_bootstrap", "100 years");

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
pub async fn run_all_now(db: &Db, search: Option<&SearchClient>) -> Result<TickReport, AppError> {
    Ok(TickReport {
        purged: system_call(db, "SELECT app_system_purge_expired()").await?,
        occurrences: system_call(db, "SELECT app_system_recurrence_tick()").await?,
        indexed: match search {
            Some(client) => drain_all(db, client).await?,
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

/// 때가 된 주기 작업을 한 번 처리한다.
pub async fn run_once(db: &Db, search: Option<&SearchClient>) -> Result<TickReport, AppError> {
    let mut report = TickReport::default();
    if let Some(client) = search {
        if claim(db, SEARCH_BOOTSTRAP).await? {
            client.ensure_index().await?;
            system_call(db, "SELECT app_system_search_enqueue_all()").await?;
        }
        report.indexed = drain_all(db, client).await?;
    }
    if claim(db, PURGE).await? {
        report.purged = system_call(db, "SELECT app_system_purge_expired()").await?;
    }
    if claim(db, RECURRENCE).await? {
        report.occurrences = system_call(db, "SELECT app_system_recurrence_tick()").await?;
    }
    Ok(report)
}

/// 간격마다 [`run_once`]를 부르는 작업을 띄운다. `shutdown`이 바뀌면 멈춘다.
pub fn spawn(
    db: Db,
    search: Option<SearchClient>,
    interval: Duration,
    mut shutdown: watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match run_once(&db, search.as_ref()).await {
                Ok(report) if report != TickReport::default() => {
                    tracing::info!(
                        purged = report.purged,
                        occurrences = report.occurrences,
                        indexed = report.indexed,
                        "background tick"
                    );
                }
                Ok(_) => {}
                Err(err) => tracing::warn!(error = %err, "background tick failed"),
            }
            tokio::select! {
                () = tokio::time::sleep(interval) => {}
                _ = shutdown.changed() => break,
            }
        }
    })
}
