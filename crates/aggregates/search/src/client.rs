//! 통합 검색 (D-39): Meilisearch REST API(별도 SDK 없이 기존 `reqwest`)와 PostgreSQL 대기열.
//!
//! 색인 문서는 종류·id·제목·번호·본문 글자와 거친 범위(워크스페이스, 개인 노트 소유자)만 갖는다. 검색은
//! Meilisearch에서 "내 워크스페이스 또는 내 개인 노트 또는 나에게 공유된 문서"로 먼저 좁히고, 결과를
//! PostgreSQL 접근 함수(`app_search_visible`)로 다시 걸러 볼 수 있는 것만 돌려준다(비공개 항목·지운 것 제외).
//! 색인 반영은 백그라운드 작업이 대기열(`search_queue`)을 꺼내 한다. Meilisearch가 없으면 검색 API는 제목만
//! PostgreSQL에서 찾는다.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use fvoci_core::error::AppError;

/// 색인 이름.
const INDEX: &str = "fvoci";
/// 검색 요청은 짧게 기다리고, 넘으면 API가 제목 검색으로 대신한다.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// 보낸 작업을 그 자리에서 확인하는 횟수(50ms 간격, 최대 5초).
pub(crate) const WAIT_POLLS: u32 = 100;

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
    pub status_category: Option<String>,
    /// 필터용: 업무 담당자.
    pub assignee_ids: Vec<String>,
    /// 필터용: 만든 사람.
    pub created_by: Option<String>,
    /// 필터용 실제 시각(UTC 유닉스 초): 문서·항목·파일의 고친 때, 발생일 없는 기록의 고친 때, 시각 일정의 시작.
    /// 검색 API가 보는 사람 시간대의 그날 시작·끝과 비교한다(D-101).
    pub at: Option<i64>,
    /// 필터용 날짜만 있는 값(YYYYMMDD 숫자): 업무 기한(없으면 시작일), 종일 일정의 시작일, 기록의 발생일. 시간대로 옮기지
    /// 않는다. 한 문서에는 `at`·`date` 가운데 하나만 있다.
    pub date: Option<i32>,
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

fn search_error(err: reqwest::Error) -> AppError {
    AppError::Config(format!("search service {}", safe_reqwest(&err)))
}

fn safe_reqwest(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timeout".into()
    } else if err.is_connect() {
        "connect".into()
    } else if let Some(status) = err.status() {
        format!("status {}", status.as_u16())
    } else if err.is_decode() {
        "decode".into()
    } else {
        "request".into()
    }
}

impl SearchClient {
    #[must_use]
    pub fn new(url: &str, key: &str) -> Self {
        Self {
            base: url.trim_end_matches('/').to_owned(),
            key: key.to_owned(),
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
                    "filterableAttributes": ["kind", "workspace_id", "owner_user_id", "id", "module_id", "status", "status_category", "assignee_ids", "created_by", "at", "date"],
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

    pub(crate) async fn upsert(
        &self,
        docs: &[IndexDoc],
        polls: u32,
    ) -> Result<TaskOutcome, AppError> {
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

    pub(crate) async fn delete(&self, uids: &[String]) -> Result<TaskOutcome, AppError> {
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
