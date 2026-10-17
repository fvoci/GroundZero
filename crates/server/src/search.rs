//! 통합 검색 (D-39): Meilisearch REST API(별도 SDK 없이 기존 `reqwest`)와 PostgreSQL 대기열.
//!
//! 색인 문서는 종류·id·제목·번호·본문 글자와 거친 범위(워크스페이스, 개인 노트 소유자)만 갖는다. 검색은
//! Meilisearch에서 "내 워크스페이스 또는 내 개인 노트 또는 나에게 공유된 문서"로 먼저 좁히고, 결과를
//! PostgreSQL 접근 함수(`app_search_visible`)로 다시 걸러 볼 수 있는 것만 돌려준다(비공개 항목·지운 것 제외).
//! 색인 반영은 백그라운드 작업이 대기열(`search_queue`)을 꺼내 한다. Meilisearch가 없으면 검색 API는 제목만
//! PostgreSQL에서 찾는다.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::config::SearchConfig;
use crate::db::Db;
use crate::error::AppError;
use crate::features::search_text::plain_text;

/// 색인 이름.
const INDEX: &str = "fvoci";

/// Meilisearch 연결.
#[derive(Debug, Clone)]
pub struct SearchClient {
    base: String,
    key: String,
    http: reqwest::Client,
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
}

/// 검색 결과 후보.
#[derive(Debug, Deserialize)]
pub struct Hit {
    pub kind: String,
    pub id: String,
    pub title: String,
    pub reference: Option<String>,
    pub workspace_id: Option<String>,
    pub item_id: Option<String>,
    #[serde(rename = "_formatted", default)]
    pub formatted: Option<Value>,
}

fn uid(kind: &str, id: Uuid) -> String {
    format!("{kind}_{}", id.simple())
}

fn search_error(err: reqwest::Error) -> AppError {
    AppError::Config(format!("search service: {err}"))
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
        }
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        self.http
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.key)
    }

    /// 비동기 작업이 끝날 때까지 기다린다(실패하면 오류).
    async fn wait(&self, response: reqwest::Response) -> Result<(), AppError> {
        let body: Value = response
            .error_for_status()
            .map_err(search_error)?
            .json()
            .await
            .map_err(search_error)?;
        let Some(task) = body.get("taskUid").and_then(Value::as_u64) else {
            return Ok(());
        };
        for _ in 0..100 {
            let status: Value = self
                .request(reqwest::Method::GET, &format!("/tasks/{task}"))
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(search_error)?
                .json()
                .await
                .map_err(search_error)?;
            match status.get("status").and_then(Value::as_str) {
                Some("succeeded") => return Ok(()),
                Some("failed" | "canceled") => {
                    return Err(AppError::Config(format!(
                        "search task {task} failed: {}",
                        status.get("error").cloned().unwrap_or(Value::Null)
                    )));
                }
                _ => tokio::time::sleep(Duration::from_millis(50)).await,
            }
        }
        Err(AppError::Config(format!("search task {task} timed out")))
    }

    /// 색인과 설정을 만든다(여러 번 불러도 같다).
    pub async fn ensure_index(&self) -> Result<(), AppError> {
        let created = self
            .request(reqwest::Method::POST, "/indexes")
            .json(&json!({ "uid": INDEX, "primaryKey": "uid" }))
            .send()
            .await
            .map_err(search_error)?;
        // 이미 있으면 작업이 실패로 끝나므로(index_already_exists) 결과는 보지 않는다.
        let _ignored = self.wait(created).await;
        let settings = self
            .request(reqwest::Method::PATCH, &format!("/indexes/{INDEX}/settings"))
            .json(&json!({
                "searchableAttributes": ["title", "reference", "text"],
                "filterableAttributes": ["kind", "workspace_id", "owner_user_id", "id"],
                "displayedAttributes": ["kind", "id", "title", "reference", "workspace_id", "item_id", "text"]
            }))
            .send()
            .await
            .map_err(search_error)?;
        self.wait(settings).await
    }

    async fn upsert(&self, docs: &[IndexDoc]) -> Result<(), AppError> {
        if docs.is_empty() {
            return Ok(());
        }
        let response = self
            .request(
                reqwest::Method::POST,
                &format!("/indexes/{INDEX}/documents"),
            )
            .json(docs)
            .send()
            .await
            .map_err(search_error)?;
        self.wait(response).await
    }

    async fn delete(&self, uids: &[String]) -> Result<(), AppError> {
        if uids.is_empty() {
            return Ok(());
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
        self.wait(response).await
    }

    /// 거친 범위로 찾는다. 결과는 반드시 접근 함수로 다시 거른다.
    pub async fn query(&self, q: &str, filter: &str, limit: usize) -> Result<Vec<Hit>, AppError> {
        let body: Value = self
            .request(reqwest::Method::POST, &format!("/indexes/{INDEX}/search"))
            .json(&json!({
                "q": q,
                "filter": filter,
                "limit": limit,
                "attributesToCrop": ["text"],
                "cropLength": 24,
                "cropMarker": "…",
                "attributesToRetrieve": ["kind", "id", "title", "reference", "workspace_id", "item_id"]
            }))
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

type PayloadRow = (
    String,
    Uuid,
    Option<bool>,
    Option<Uuid>,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    Option<String>,
    Option<Value>,
    Option<String>,
);

/// 대기열을 한 번(최대 200개) 비워 색인에 반영한다. 반영에 실패하면 꺼낸 것을 되돌린다.
pub async fn drain(db: &Db, client: &SearchClient) -> Result<i32, AppError> {
    let mut tx = db.begin_anonymous().await?;
    let claimed: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT kind, target_id FROM app_system_search_claim(200)")
            .fetch_all(tx.conn())
            .await?;
    if claimed.is_empty() {
        tx.commit().await?;
        return Ok(0);
    }
    let kinds: Vec<&str> = claimed.iter().map(|(k, _)| k.as_str()).collect();
    let ids: Vec<Uuid> = claimed.iter().map(|(_, id)| *id).collect();
    let rows: Vec<PayloadRow> = sqlx::query_as(
        "SELECT kind, id, live, workspace_id, owner_user_id, item_id, title, reference, body, description
         FROM app_system_search_payload($1, $2)",
    )
    .bind(&kinds)
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    let mut upserts = Vec::new();
    let mut deletes = Vec::new();
    for (kind, id, live, workspace_id, owner, item_id, title, reference, body, description) in rows
    {
        if live != Some(true) {
            deletes.push(uid(&kind, id));
            continue;
        }
        let text = match body {
            Some(body) => plain_text(&body),
            None => description.unwrap_or_default(),
        };
        upserts.push(IndexDoc {
            uid: uid(&kind, id),
            kind,
            id: id.to_string(),
            workspace_id: workspace_id.map(|w| w.to_string()),
            owner_user_id: owner.map(|o| o.to_string()),
            item_id: item_id.map(|i| i.to_string()),
            title: title.unwrap_or_default(),
            reference,
            text,
        });
    }
    client.upsert(&upserts).await?;
    client.delete(&deletes).await?;
    tx.commit().await?;
    i32::try_from(claimed.len()).map_err(|_| AppError::Config("too many search jobs".into()))
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
