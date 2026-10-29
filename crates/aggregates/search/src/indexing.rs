//! 검색 큐 처리. 점유·payload·세대 재확인·전송·결과 기록은 이 순서로 한다.

use crate::TextExtractor;
use crate::client::WAIT_POLLS;
pub use crate::client::{Hit, IndexDoc, SearchClient, TaskOutcome};
use fvoci_core::error::AppError;
use fvoci_db::Db;
use serde_json::Value;
use std::collections::HashSet;
use time::OffsetDateTime;
use uuid::Uuid;

/// 한 번에 꺼내는 대기열 수.
const BATCH: i32 = 200;

fn uid(kind: &str, id: Uuid) -> String {
    format!("{kind}_{}", id.simple())
}

#[derive(Debug, sqlx::FromRow)]
struct PayloadRow {
    kind: String,
    id: Uuid,
    live: Option<bool>,
    workspace_id: Option<Uuid>,
    owner_user_id: Option<Uuid>,
    item_id: Option<Uuid>,
    title: Option<String>,
    reference: Option<String>,
    body: Option<Value>,
    description: Option<String>,
    module_id: Option<Uuid>,
    status: Option<String>,
    status_category: Option<String>,
    assignee_ids: Option<Vec<Uuid>>,
    created_by: Option<Uuid>,
    at: Option<OffsetDateTime>,
    on_day: Option<time::Date>,
    file_key: Option<String>,
    file_type: Option<String>,
    file_size: Option<i64>,
}

/// 날짜를 필터용 숫자(YYYYMMDD)로. 검색 API가 같은 함수로 범위를 만든다.
pub(crate) fn day_number(day: time::Date) -> i32 {
    day.year() * 10_000 + i32::from(u8::from(day.month())) * 100 + i32::from(day.day())
}

/// 색인 준비: 색인이 없으면 만들고 모든 것을 대기열에 올린다.
pub async fn prepare(db: &Db, client: &SearchClient) -> Result<(), AppError> {
    if client.ensure_index().await? {
        let mut tx = db.begin_anonymous().await?;
        let queued: i32 = sqlx::query_scalar("SELECT app_system_search_enqueue_all()")
            .fetch_one(tx.conn())
            .await?;
        tx.commit().await?;
        tracing::info!(
            event = "search.index_created",
            queued,
            "search index created; queued everything for indexing"
        );
    }
    Ok(())
}

/// 새 문서는 묶고 재시도 문서는 따로 보낸다.
#[derive(Debug, Clone, Copy)]
enum BatchMode {
    Together,
    Separate,
}

/// 보낸 결과: 성공을 확인한 것과, 아직 끝나지 않은 작업(번호와 그 작업에 든 uid).
#[derive(Default)]
struct Submitted {
    done: Vec<String>,
    pending: Vec<(u64, Vec<String>)>,
}

/// 넣기를 한꺼번에 하고(`together`), 실패하면 하나씩 해서 결과를 모은다(문제 있는 한 건이 묶음을 막지 않게). 아직
/// 끝나지 않은 작업은 다시 보내지 않고 번호를 남긴다. `together`가 거짓이면 처음부터 하나씩 보낸다.
async fn upsert_each(client: &SearchClient, docs: &[IndexDoc], mode: BatchMode) -> Submitted {
    let uids = || docs.iter().map(|d| d.uid.clone()).collect::<Vec<_>>();
    let batch = if matches!(mode, BatchMode::Together) {
        client.upsert(docs, WAIT_POLLS).await
    } else {
        Ok(TaskOutcome::Failed("sent one by one".into()))
    };
    match batch {
        Ok(TaskOutcome::Succeeded) => {
            return Submitted {
                done: uids(),
                pending: Vec::new(),
            };
        }
        Ok(TaskOutcome::Pending(task)) => {
            return Submitted {
                done: Vec::new(),
                pending: vec![(task, uids())],
            };
        }
        Ok(TaskOutcome::Failed(_)) | Err(_) => {}
    }
    let mut out = Submitted::default();
    for doc in docs {
        // 하나씩 보낸 것은 기다리지 않는다(묶음 크기만큼 길어지면 꺼낸 행의 표시 시간을 넘긴다). 작업 번호를 남겨 다음에 확인한다.
        match client.upsert(std::slice::from_ref(doc), 0).await {
            Ok(TaskOutcome::Succeeded) => out.done.push(doc.uid.clone()),
            Ok(TaskOutcome::Pending(task)) => out.pending.push((task, vec![doc.uid.clone()])),
            Ok(TaskOutcome::Failed(_)) => {
                tracing::warn!(event = "search.upsert_failed", uid = %doc.uid, "search upsert task failed");
            }
            Err(err) => {
                tracing::warn!(event = "search.upsert_failed", uid = %doc.uid, error_code = err.code(), detail = fvoci_core::logging::safe_error(&err), "search upsert failed");
            }
        }
    }
    out
}

/// 기다리던 작업들의 결과를 확인해 대기열에 반영한다. 끝난(성공·실패) 작업 수. 확인하지 못한 것은 다음에 다시 본다
/// (실패 횟수를 쓰지 않는다).
async fn settle_pending(db: &Db, client: &SearchClient) -> Result<i32, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let tasks: Vec<i64> = sqlx::query_scalar("SELECT task_uid FROM app_system_search_pending($1)")
        .bind(BATCH)
        .fetch_all(tx.conn())
        .await?;
    tx.commit().await?;
    let mut finished = 0;
    for task in tasks {
        let outcome = match u64::try_from(task) {
            Ok(uid) => client.task_outcome(uid).await,
            Err(_) => Ok(TaskOutcome::Failed(format!("invalid search task {task}"))),
        };
        let label = match outcome {
            Ok(TaskOutcome::Succeeded) => "succeeded",
            Ok(TaskOutcome::Failed(_)) => "failed",
            Ok(TaskOutcome::Pending(_)) | Err(_) => "pending",
        };
        if label != "pending" {
            finished += 1;
        }
        let mut tx = db.begin_anonymous().await?;
        sqlx::query("SELECT app_system_search_settled($1, $2)")
            .bind(task)
            .bind(label)
            .execute(tx.conn())
            .await?;
        tx.commit().await?;
    }
    Ok(finished)
}

/// 이번 꺼냄의 세대와 시도 번호. 재확인·결과 기록은 같은 행 순서를 쓴다.
#[derive(Debug)]
struct Claimed {
    rows: Vec<(String, Uuid, OffsetDateTime)>,
    attempts: Vec<i32>,
    retried: HashSet<String>,
}

impl Claimed {
    fn kinds(&self) -> Vec<&str> {
        self.rows.iter().map(|(kind, _, _)| kind.as_str()).collect()
    }

    fn ids(&self) -> Vec<Uuid> {
        self.rows.iter().map(|(_, id, _)| *id).collect()
    }

    fn generations(&self) -> Vec<OffsetDateTime> {
        self.rows.iter().map(|(_, _, at)| *at).collect()
    }
}

async fn claim(db: &Db) -> Result<Claimed, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let rows: Vec<(String, Uuid, OffsetDateTime, i32)> = sqlx::query_as(
        "SELECT kind, target_id, queued_at, attempts FROM app_system_search_claim($1)",
    )
    .bind(BATCH)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    // 실패하거나 점유가 풀린 행은 하나씩 보내 문제 있는 한 건이 묶음을 막지 않게 한다.
    let retried = rows
        .iter()
        .filter(|(_, _, _, attempts)| *attempts > 1)
        .map(|(kind, id, _, _)| uid(kind, *id))
        .collect();
    let attempts = rows.iter().map(|(_, _, _, attempts)| *attempts).collect();
    let rows = rows
        .into_iter()
        .map(|(kind, id, at, _)| (kind, id, at))
        .collect();
    Ok(Claimed {
        rows,
        attempts,
        retried,
    })
}

async fn payload(db: &Db, claimed: &Claimed) -> Result<Vec<PayloadRow>, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let rows = sqlx::query_as(
        "SELECT kind, id, live, workspace_id, owner_user_id, item_id, title, reference, body, description,
                module_id, status, status_category, assignee_ids, created_by, at, on_day, file_key, file_type, file_size
         FROM app_system_search_payload($1, $2)",
    )
    .bind(claimed.kinds())
    .bind(claimed.ids())
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(rows)
}

async fn document(row: PayloadRow, extractor: &impl TextExtractor) -> IndexDoc {
    let text = match (&row.body, row.kind.as_str()) {
        (Some(body), _) => extractor.plain_text(body),
        (None, "file") => extractor
            .file_text(
                row.file_key.as_deref(),
                row.title.as_deref(),
                row.file_type.as_deref(),
                row.file_size,
            )
            .await
            .unwrap_or_default(),
        _ => row.description.clone().unwrap_or_default(),
    };
    IndexDoc {
        uid: uid(&row.kind, row.id),
        id: row.id.to_string(),
        workspace_id: row.workspace_id.map(|w| w.to_string()),
        owner_user_id: row.owner_user_id.map(|o| o.to_string()),
        item_id: row.item_id.map(|i| i.to_string()),
        title: row.title.unwrap_or_default(),
        reference: row.reference,
        text,
        module_id: row.module_id.map(|m| m.to_string()),
        status: row.status,
        status_category: row.status_category,
        assignee_ids: row
            .assignee_ids
            .unwrap_or_default()
            .iter()
            .map(Uuid::to_string)
            .collect(),
        created_by: row.created_by.map(|c| c.to_string()),
        at: row.at.map(OffsetDateTime::unix_timestamp),
        date: row.on_day.map(day_number),
        kind: row.kind,
    }
}

async fn documents(
    rows: Vec<PayloadRow>,
    extractor: &impl TextExtractor,
) -> (Vec<IndexDoc>, Vec<String>) {
    let mut upserts = Vec::new();
    let mut deletes = Vec::new();
    for row in rows {
        if row.live != Some(true) {
            deletes.push(uid(&row.kind, row.id));
        } else {
            upserts.push(document(row, extractor).await);
        }
    }
    (upserts, deletes)
}

/// 보내기 직전에 세대·시도 번호를 다시 보고 점유를 늘린다. 첨부 추출 중 새 변경이나 다른 꺼냄이 있었다면 보내지 않는다.
/// 시간만 지나고 아무도 꺼내지 않았으면 보낸다(오래 걸린 묶음이 매번 버려져 갇히지 않게).
async fn recheck(db: &Db, claimed: &Claimed) -> Result<HashSet<String>, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let held: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT kind, target_id FROM app_system_search_recheck($1, $2, $3, $4)")
            .bind(claimed.kinds())
            .bind(claimed.ids())
            .bind(claimed.generations())
            .bind(&claimed.attempts)
            .fetch_all(tx.conn())
            .await?;
    tx.commit().await?;
    Ok(held.iter().map(|(kind, id)| uid(kind, *id)).collect())
}

/// 재확인과 다른 문장으로 옛 점유를 푼다. 같은 문장은 시작 뒤 커밋된 다시 올리기를 놓칠 수 있다.
async fn release_stale(db: &Db, claimed: &Claimed, held: &HashSet<String>) -> Result<(), AppError> {
    let left: Vec<usize> = (0..claimed.rows.len())
        .filter(|&i| !held.contains(&uid(&claimed.rows[i].0, claimed.rows[i].1)))
        .collect();
    let mut tx = db.begin_anonymous().await?;
    sqlx::query("SELECT app_system_search_release_stale($1, $2, $3, $4)")
        .bind(
            left.iter()
                .map(|&i| claimed.rows[i].0.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(left.iter().map(|&i| claimed.rows[i].1).collect::<Vec<_>>())
        .bind(left.iter().map(|&i| claimed.rows[i].2).collect::<Vec<_>>())
        .bind(
            left.iter()
                .map(|&i| claimed.attempts[i])
                .collect::<Vec<_>>(),
        )
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn submit(
    client: &SearchClient,
    upserts: Vec<IndexDoc>,
    deletes: Vec<String>,
    retried: &HashSet<String>,
) -> Submitted {
    let (again, fresh): (Vec<IndexDoc>, Vec<IndexDoc>) = upserts
        .into_iter()
        .partition(|doc| retried.contains(&doc.uid));
    let mut submitted = upsert_each(client, &fresh, BatchMode::Together).await;
    let alone = upsert_each(client, &again, BatchMode::Separate).await;
    submitted.done.extend(alone.done);
    submitted.pending.extend(alone.pending);
    match client.delete(&deletes).await {
        Ok(TaskOutcome::Succeeded) => submitted.done.extend(deletes),
        Ok(TaskOutcome::Pending(task)) => submitted.pending.push((task, deletes)),
        Ok(TaskOutcome::Failed(_)) => {
            tracing::warn!(event = "search.delete_failed", "search delete task failed");
        }
        Err(err) => tracing::warn!(
            event = "search.delete_failed",
            error_code = err.code(),
            detail = fvoci_core::logging::safe_error(&err),
            "search delete failed"
        ),
    }
    submitted
}

/// 끝나지 않은 작업은 보낸 세대와 번호를 기록해 다음에 결과를 본다.
async fn record_pending(
    db: &Db,
    claimed: &Claimed,
    pending: &[(u64, Vec<String>)],
) -> Result<(), AppError> {
    for (task, uids) in pending {
        let rows: Vec<&(String, Uuid, OffsetDateTime)> = claimed
            .rows
            .iter()
            .filter(|(kind, id, _)| uids.contains(&uid(kind, *id)))
            .collect();
        let mut tx = db.begin_anonymous().await?;
        sqlx::query("SELECT app_system_search_submitted($1, $2, $3, $4)")
            .bind(
                rows.iter()
                    .map(|(kind, _, _)| kind.as_str())
                    .collect::<Vec<_>>(),
            )
            .bind(rows.iter().map(|(_, id, _)| *id).collect::<Vec<_>>())
            .bind(rows.iter().map(|(_, _, at)| *at).collect::<Vec<_>>())
            .bind(i64::try_from(*task).unwrap_or(i64::MAX))
            .execute(tx.conn())
            .await?;
        tx.commit().await?;
    }
    Ok(())
}

async fn record_done(db: &Db, claimed: &Claimed, done: &[String]) -> Result<usize, AppError> {
    let done: HashSet<&String> = done.iter().collect();
    let finished: Vec<&(String, Uuid, OffsetDateTime)> = claimed
        .rows
        .iter()
        .filter(|(kind, id, _)| done.contains(&uid(kind, *id)))
        .collect();
    let mut tx = db.begin_anonymous().await?;
    sqlx::query("SELECT app_system_search_done($1, $2, $3)")
        .bind(
            finished
                .iter()
                .map(|(kind, _, _)| kind.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(finished.iter().map(|(_, id, _)| *id).collect::<Vec<_>>())
        .bind(finished.iter().map(|(_, _, at)| *at).collect::<Vec<_>>())
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(finished.len())
}

/// 대기열을 한 번(최대 200개) 꺼내 반영한다. DB 트랜잭션을 잡은 채 검색 서비스를 기다리지 않는다.
pub async fn drain(
    db: &Db,
    client: &SearchClient,
    extractor: &impl TextExtractor,
) -> Result<i32, AppError> {
    let settled = settle_pending(db, client).await?;
    let claimed = claim(db).await?;
    if claimed.rows.is_empty() {
        return Ok(settled);
    }
    let (mut upserts, mut deletes) = documents(payload(db, &claimed).await?, extractor).await;
    let held = recheck(db, &claimed).await?;
    let stale = claimed.rows.len() - held.len();
    if stale > 0 {
        release_stale(db, &claimed, &held).await?;
    }
    upserts.retain(|doc| held.contains(&doc.uid));
    deletes.retain(|id| held.contains(id));
    let submitted = submit(client, upserts, deletes, &claimed.retried).await;
    record_pending(db, &claimed, &submitted.pending).await?;
    let finished = record_done(db, &claimed, &submitted.done).await?;
    if finished == 0 && submitted.pending.is_empty() && settled == 0 && stale == 0 {
        return Err(AppError::Config(
            "search indexing made no progress; will retry later".into(),
        ));
    }
    i32::try_from(claimed.rows.len())
        .map(|n| n + settled)
        .map_err(|_| AppError::Config("too many search jobs".into()))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uids_use_only_allowed_characters() {
        let id = Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef);
        let made = uid("document", id);
        assert_eq!(made, "document_0123456789abcdef0123456789abcdef");
        assert!(
            made.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        );
    }
}
