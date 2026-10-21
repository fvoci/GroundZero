//! 통합 검색 (D-39): Meilisearch REST API(별도 SDK 없이 기존 `reqwest`)와 PostgreSQL 대기열.
//!
//! 색인 문서는 종류·id·제목·번호·본문 글자와 거친 범위(워크스페이스, 개인 노트 소유자)만 갖는다. 검색은
//! Meilisearch에서 "내 워크스페이스 또는 내 개인 노트 또는 나에게 공유된 문서"로 먼저 좁히고, 결과를
//! PostgreSQL 접근 함수(`app_search_visible`)로 다시 걸러 볼 수 있는 것만 돌려준다(비공개 항목·지운 것 제외).
//! 색인 반영은 백그라운드 작업이 대기열(`search_queue`)을 꺼내 한다. Meilisearch가 없으면 검색 API는 제목만
//! PostgreSQL에서 찾는다.

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::config::SearchConfig;
use crate::db::Db;
use crate::error::AppError;
use crate::features::file_text;
use crate::features::search_text::plain_text;
use crate::storage::Storage;
use futures_util::StreamExt;

/// 색인 이름.
const INDEX: &str = "fvoci";
/// 검색 요청은 짧게 기다리고, 넘으면 API가 제목 검색으로 대신한다.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// 한 번에 꺼내는 대기열 수.
const BATCH: i32 = 200;
/// 보낸 작업을 그 자리에서 확인하는 횟수(50ms 간격, 최대 5초).
const WAIT_POLLS: u32 = 100;

/// 색인 작업의 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskOutcome {
    Succeeded,
    /// 실패·취소·없는 작업(까닭).
    Failed(String),
    /// 아직 대기·처리 중(작업 번호).
    Pending(u64),
}

/// Meilisearch 연결.
#[derive(Debug, Clone)]
pub struct SearchClient {
    base: String,
    key: String,
    http: reqwest::Client,
    /// 이 프로세스에서 색인 설정을 한 번 맞췄는지.
    configured: Arc<AtomicBool>,
}

/// 색인 문서.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct IndexDoc {
    /// `kind_id`(Meilisearch 기본 키는 영문·숫자·`-`·`_`만).
    pub uid: String,
    pub kind: String,
    pub id: String,
    pub workspace_id: Option<String>,
    pub owner_user_id: Option<String>,
    pub item_id: Option<String>,
    pub title: String,
    pub reference: Option<String>,
    pub text: String,
    /// 필터용: 모듈.
    pub module_id: Option<String>,
    /// 필터용: 업무 상태.
    pub status: Option<String>,
    /// 필터용: 업무 담당자.
    pub assignee_ids: Vec<String>,
    /// 필터용 날짜(YYYYMMDD 숫자): 업무는 기한(없으면 시작일), 일정은 시작일, 그 밖은 고친 날.
    pub day: Option<i32>,
}

/// 검색 결과 후보.
#[derive(Debug, Deserialize)]
pub struct Hit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub workspace_id: Option<String>,
    pub item_id: Option<String>,
    #[serde(rename = "_formatted", default)]
    pub formatted: Option<Value>,
}

fn uid(kind: &str, id: Uuid) -> String {
    format!("{kind}_{}", id.simple())
}

fn search_error(err: reqwest::Error) -> AppError {
    AppError::Config(format!(
        "search service {}",
        crate::logging::safe_reqwest(&err)
    ))
}

impl SearchClient {
    #[must_use]
    pub fn new(config: &SearchConfig) -> Self {
        Self {
            base: config.url.trim_end_matches('/').to_owned(),
            key: config.key.expose().to_owned(),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            configured: Arc::new(AtomicBool::new(false)),
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.key)
    }

    /// 작업 하나의 지금 상태. 없는 작업(Meilisearch 데이터가 지워짐 등)은 실패로 본다.
    pub async fn task_outcome(&self, task: u64) -> Result<TaskOutcome, AppError> {
        let response = self
            .request(reqwest::Method::GET, &format!("/tasks/{task}"))
            .send()
            .await
            .map_err(search_error)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(TaskOutcome::Failed(format!("search task {task} is gone")));
        }
        let status: Value = response
            .error_for_status()
            .map_err(search_error)?
            .json()
            .await
            .map_err(search_error)?;
        Ok(match status.get("status").and_then(Value::as_str) {
            Some("succeeded") => TaskOutcome::Succeeded,
            Some("failed" | "canceled") => TaskOutcome::Failed(format!(
                "search task {task} failed: {}",
                status.get("error").cloned().unwrap_or(Value::Null)
            )),
            _ => TaskOutcome::Pending(task),
        })
    }

    /// 비동기 작업을 `polls`번(50ms 간격) 확인하며 기다린다. 그 안에 끝나지 않으면 `Pending`(작업 번호)으로 돌려주고,
    /// 부르는 쪽이 그 번호를 남겨 다음에 확인한다(성공을 확인하기 전에는 끝난 것으로 보지 않는다).
    async fn wait(&self, response: reqwest::Response, polls: u32) -> Result<TaskOutcome, AppError> {
        let body: Value = response
            .error_for_status()
            .map_err(search_error)?
            .json()
            .await
            .map_err(search_error)?;
        let Some(task) = body.get("taskUid").and_then(Value::as_u64) else {
            return Ok(TaskOutcome::Succeeded);
        };
        for _ in 0..polls {
            match self.task_outcome(task).await? {
                TaskOutcome::Pending(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                done => return Ok(done),
            }
        }
        Ok(TaskOutcome::Pending(task))
    }

    /// 설정·색인 만들기처럼 이어지는 작업이 순서대로 처리되기만 하면 되는 것: 실패만 오류로.
    async fn wait_queued(&self, response: reqwest::Response) -> Result<(), AppError> {
        match self.wait(response, WAIT_POLLS).await? {
            TaskOutcome::Failed(why) => Err(AppError::Config(why)),
            TaskOutcome::Succeeded | TaskOutcome::Pending(_) => Ok(()),
        }
    }

    /// 색인이 없으면 만들고(처음 시작, 또는 Meilisearch 데이터가 지워짐) 설정을 맞춘다. 새로 만들었으면 `true`
    /// (그러면 부르는 쪽이 모든 것을 다시 대기열에 올린다). 프로세스마다 처음 한 번은 설정도 다시 맞춘다.
    pub async fn ensure_index(&self) -> Result<bool, AppError> {
        let exists = self
            .request(reqwest::Method::GET, &format!("/indexes/{INDEX}"))
            .send()
            .await
            .map_err(search_error)?;
        let created = if exists.status() == reqwest::StatusCode::NOT_FOUND {
            let response = self
                .request(reqwest::Method::POST, "/indexes")
                .json(&json!({ "uid": INDEX, "primaryKey": "uid" }))
                .send()
                .await
                .map_err(search_error)?;
            // 다른 프로세스가 먼저 만들었으면 작업이 실패로 끝난다(index_already_exists). 그쪽이 다시 올린다.
            self.wait_queued(response).await.is_ok()
        } else {
            exists.error_for_status().map_err(search_error)?;
            false
        };
        if created || !self.configured.load(Ordering::Relaxed) {
            let settings = self
                .request(reqwest::Method::PATCH, &format!("/indexes/{INDEX}/settings"))
                .json(&json!({
                    "searchableAttributes": ["title", "reference", "text"],
                    "filterableAttributes": ["kind", "workspace_id", "owner_user_id", "id", "module_id", "status", "assignee_ids", "day"],
                    "displayedAttributes": ["kind", "id", "title", "reference", "workspace_id", "item_id", "text"]
                }))
                .send()
                .await
                .map_err(search_error)?;
            self.wait_queued(settings).await?;
            self.configured.store(true, Ordering::Relaxed);
        }
        Ok(created)
    }

    async fn upsert(&self, docs: &[IndexDoc], polls: u32) -> Result<TaskOutcome, AppError> {
        if docs.is_empty() {
            return Ok(TaskOutcome::Succeeded);
        }
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/indexes/{INDEX}/documents?primaryKey=uid"),
            )
            .json(docs)
            .send()
            .await
            .map_err(search_error)?;
        self.wait(response, polls).await
    }

    async fn delete(&self, uids: &[String]) -> Result<TaskOutcome, AppError> {
        if uids.is_empty() {
            return Ok(TaskOutcome::Succeeded);
        }
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/indexes/{INDEX}/documents/delete-batch"),
            )
            .json(uids)
            .send()
            .await
            .map_err(search_error)?;
        self.wait(response, WAIT_POLLS).await
    }

    /// 거친 범위로 찾는다. 결과는 반드시 접근 함수로 다시 거른다.
    /// `title_only`면 제목에서만 찾는다(질의 단위 설정이라 색인 설정은 그대로).
    pub async fn query(
        &self,
        q: &str,
        filter: &str,
        offset: usize,
        limit: usize,
        title_only: bool,
    ) -> Result<Vec<Hit>, AppError> {
        let mut request = json!({
            "q": q,
            "filter": filter,
            "offset": offset,
            "limit": limit,
            "attributesToCrop": ["text"],
            "cropLength": 24,
            "cropMarker": "…",
            "attributesToRetrieve": ["kind", "id", "title", "workspace_id", "item_id"]
        });
        if title_only {
            request["attributesToSearchOn"] = json!(["title"]);
        }
        let body: Value = self
            .request(reqwest::Method::POST, &format!("/indexes/{INDEX}/search"))
            .timeout(QUERY_TIMEOUT)
            .json(&request)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(search_error)?
            .json()
            .await
            .map_err(search_error)?;
        let hits = body.get("hits").cloned().unwrap_or_else(|| json!([]));
        serde_json::from_value(hits).map_err(|e| AppError::Config(format!("search hits: {e}")))
    }
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
    assignee_ids: Option<Vec<Uuid>>,
    day: Option<time::Date>,
    file_key: Option<String>,
    file_type: Option<String>,
    file_size: Option<i64>,
}

/// 날짜를 필터용 숫자(YYYYMMDD)로.
fn day_number(day: time::Date) -> i32 {
    day.year() * 10_000 + i32::from(u8::from(day.month())) * 100 + i32::from(day.day())
}

/// 첨부 본문(지원 형식, 크기 제한 안). 읽지 못하면 이름으로만 색인한다.
async fn file_text(storage: &Storage, row: &PayloadRow) -> Option<String> {
    let key = row.file_key.as_deref()?;
    let name = row.title.as_deref().unwrap_or_default();
    let content_type = row.file_type.as_deref().unwrap_or_default();
    let size = u64::try_from(row.file_size?).ok()?;
    if size > file_text::MAX_EXTRACT_BYTES || !file_text::supported(content_type, name) {
        return None;
    }
    let mut stream = storage.get(key).await.ok()?;
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or_default());
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.ok()?);
    }
    let content_type = content_type.to_owned();
    let name = name.to_owned();
    // 형식 해석기가 멈추거나 패닉해도 색인 작업은 다음으로 넘어간다(이름으로만 찾음).
    let job = tokio::task::spawn_blocking(move || file_text::extract(&bytes, &content_type, &name));
    match tokio::time::timeout(file_text::EXTRACT_TIMEOUT, job).await {
        Ok(Ok(text)) => text,
        Ok(Err(_)) | Err(_) => {
            tracing::warn!(
                event = "search.extract_failed",
                file = key,
                "attachment text extraction failed or timed out"
            );
            None
        }
    }
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

/// 보낸 결과: 성공을 확인한 것과, 아직 끝나지 않은 작업(번호와 그 작업에 든 uid).
#[derive(Default)]
struct Submitted {
    done: Vec<String>,
    pending: Vec<(u64, Vec<String>)>,
}

/// 넣기를 한꺼번에 하고(`together`), 실패하면 하나씩 해서 결과를 모은다(문제 있는 한 건이 묶음을 막지 않게). 아직
/// 끝나지 않은 작업은 다시 보내지 않고 번호를 남긴다. `together`가 거짓이면 처음부터 하나씩 보낸다.
async fn upsert_each(client: &SearchClient, docs: &[IndexDoc], together: bool) -> Submitted {
    let uids = || docs.iter().map(|d| d.uid.clone()).collect::<Vec<_>>();
    let batch = if together {
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
                tracing::warn!(event = "search.upsert_failed", uid = %doc.uid, error_code = err.code(), detail = crate::logging::safe_error(&err), "search upsert failed");
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

/// 대기열을 한 번(최대 200개) 꺼내 색인에 반영한다. 꺼낸 수를 돌려준다. 반영하지 못한 것은 표시가 풀린 뒤
/// 다시 시도한다(대기열 행을 잠근 채 Meilisearch를 기다리지 않는다).
pub async fn drain(
    db: &Db,
    client: &SearchClient,
    storage: Option<&Storage>,
) -> Result<i32, AppError> {
    // 먼저 기다리던 작업의 결과를 반영한다(성공한 것만 지우고, 실패한 것은 아래에서 다시 꺼내 보낸다).
    let settled = settle_pending(db, client).await?;
    let mut tx = db.begin_anonymous().await?;
    let rows: Vec<(String, Uuid, OffsetDateTime, i32)> = sqlx::query_as(
        "SELECT kind, target_id, queued_at, attempts FROM app_system_search_claim($1)",
    )
    .bind(BATCH)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    if rows.is_empty() {
        return Ok(settled);
    }
    // 앞서 꺼냈는데 끝나지 않은 행(실패했거나 표시가 풀린 것)은 하나씩 보낸다. 묶음으로 보낸 작업이 기다림 뒤에 실패하면
    // 같은 묶음으로 다시 꺼내지므로, 그대로 묶어 보내면 문제 있는 한 건이 모두를 계속 막는다.
    let retried: HashSet<String> = rows
        .iter()
        .filter(|(_, _, _, attempts)| *attempts > 1)
        .map(|(kind, id, _, _)| uid(kind, *id))
        .collect();
    let claimed: Vec<(String, Uuid, OffsetDateTime)> = rows
        .into_iter()
        .map(|(kind, id, queued_at, _)| (kind, id, queued_at))
        .collect();
    let kinds: Vec<&str> = claimed.iter().map(|(k, _, _)| k.as_str()).collect();
    let ids: Vec<Uuid> = claimed.iter().map(|(_, id, _)| *id).collect();
    let mut tx = db.begin_anonymous().await?;
    let rows: Vec<PayloadRow> = sqlx::query_as(
        "SELECT kind, id, live, workspace_id, owner_user_id, item_id, title, reference, body, description,
                module_id, status, assignee_ids, day, file_key, file_type, file_size
         FROM app_system_search_payload($1, $2)",
    )
    .bind(&kinds)
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    let mut upserts = Vec::new();
    let mut deletes = Vec::new();
    for row in rows {
        if row.live != Some(true) {
            deletes.push(uid(&row.kind, row.id));
            continue;
        }
        let text = match (&row.body, row.kind.as_str()) {
            (Some(body), _) => plain_text(body),
            (None, "file") => match storage {
                Some(storage) => file_text(storage, &row).await.unwrap_or_default(),
                None => String::new(),
            },
            _ => row.description.clone().unwrap_or_default(),
        };
        upserts.push(IndexDoc {
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
            assignee_ids: row
                .assignee_ids
                .unwrap_or_default()
                .iter()
                .map(Uuid::to_string)
                .collect(),
            day: row.day.map(day_number),
            kind: row.kind,
        });
    }
    // 보내기 직전에 이 행들을 아직 쥐고 있고(점유가 남음) 세대가 그대로인지 확인하며 점유를 늘린다. 첨부를 읽는 등으로
    // 점유 시간을 넘긴 사이 더 새 변경이 오르거나 다른 작업 프로세스가 다시 꺼냈으면 옛 내용을 보내지 않는다(늦게 보낸
    // 옛 내용이 새 색인을 덮었다, 감사 S12). 그 행은 대기열에 남아 다음 차례에 새 내용으로 처리된다.
    let mut tx = db.begin_anonymous().await?;
    let held: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT kind, target_id FROM app_system_search_recheck($1, $2, $3)")
            .bind(&kinds)
            .bind(&ids)
            .bind(claimed.iter().map(|(_, _, at)| *at).collect::<Vec<_>>())
            .fetch_all(tx.conn())
            .await?;
    tx.commit().await?;
    let held: HashSet<String> = held.iter().map(|(kind, id)| uid(kind, *id)).collect();
    let stale = claimed.len() - held.len();
    upserts.retain(|d| held.contains(&d.uid));
    deletes.retain(|u| held.contains(u));
    let (again, fresh): (Vec<IndexDoc>, Vec<IndexDoc>) =
        upserts.into_iter().partition(|d| retried.contains(&d.uid));
    let mut submitted = upsert_each(client, &fresh, true).await;
    let alone = upsert_each(client, &again, false).await;
    submitted.done.extend(alone.done);
    submitted.pending.extend(alone.pending);
    let Submitted { done, mut pending } = submitted;
    let mut done: HashSet<String> = done.into_iter().collect();
    match client.delete(&deletes).await {
        Ok(TaskOutcome::Succeeded) => done.extend(deletes),
        Ok(TaskOutcome::Pending(task)) => pending.push((task, deletes)),
        Ok(TaskOutcome::Failed(_)) => {
            tracing::warn!(event = "search.delete_failed", "search delete task failed")
        }
        Err(err) => tracing::warn!(
            event = "search.delete_failed",
            error_code = err.code(),
            detail = crate::logging::safe_error(&err),
            "search delete failed"
        ),
    }
    // 아직 끝나지 않은 작업: 행마다 작업 번호와 이번에 보낸 세대를 남긴다(다시 보내지 않고 다음에 결과를 본다).
    for (task, uids) in &pending {
        let rows: Vec<&(String, Uuid, OffsetDateTime)> = claimed
            .iter()
            .filter(|(kind, id, _)| uids.contains(&uid(kind, *id)))
            .collect();
        let mut tx = db.begin_anonymous().await?;
        sqlx::query("SELECT app_system_search_submitted($1, $2, $3, $4)")
            .bind(rows.iter().map(|(k, _, _)| k.as_str()).collect::<Vec<_>>())
            .bind(rows.iter().map(|(_, id, _)| *id).collect::<Vec<_>>())
            .bind(rows.iter().map(|(_, _, at)| *at).collect::<Vec<_>>())
            .bind(i64::try_from(*task).unwrap_or(i64::MAX))
            .execute(tx.conn())
            .await?;
        tx.commit().await?;
    }
    let finished: Vec<&(String, Uuid, OffsetDateTime)> = claimed
        .iter()
        .filter(|(kind, id, _)| done.contains(&uid(kind, *id)))
        .collect();
    let mut tx = db.begin_anonymous().await?;
    sqlx::query("SELECT app_system_search_done($1, $2, $3)")
        .bind(
            finished
                .iter()
                .map(|(k, _, _)| k.as_str())
                .collect::<Vec<_>>(),
        )
        .bind(finished.iter().map(|(_, id, _)| *id).collect::<Vec<_>>())
        .bind(finished.iter().map(|(_, _, at)| *at).collect::<Vec<_>>())
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    if finished.is_empty() && pending.is_empty() && settled == 0 && stale == 0 {
        return Err(AppError::Config(
            "search indexing made no progress; will retry later".into(),
        ));
    }
    i32::try_from(claimed.len())
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
