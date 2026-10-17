//! 통합 검색 API (D-39). 문서·업무·항목·일정을 한 번에 찾는다. 볼 수 있는 것만 나온다.
//!
//! Meilisearch가 있으면 "내 워크스페이스 또는 내 개인 노트 또는 나에게 공유된 문서"로 먼저 좁혀 찾은 뒤
//! PostgreSQL 접근 함수로 다시 거른다(비공개 항목·지운 것·권한이 바뀐 것 제외). 없으면 제목만 DB에서 찾는다.
//! 색인은 백그라운드 작업이 반영하므로 바로 직전의 변경은 조금 늦게 보일 수 있다.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::workspaces::member_role;
use crate::http::AppState;
use crate::ids::WorkspaceId;

/// 한 번에 돌려주는 결과 수.
const MAX_RESULTS: usize = 20;
/// 거르기 전에 Meilisearch에서 가져오는 후보 수.
const CANDIDATES: usize = 60;
/// 검색어 길이 상한.
const MAX_QUERY_CHARS: usize = 200;

const KINDS: &[&str] = &["document", "task", "item", "event"];

/// 검색 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    pub q: String,
    /// 이 워크스페이스 안에서만(멤버).
    #[param(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<WorkspaceId>,
    /// 쉼표로 구분한 종류(`document,task,item,event`). 없으면 전부.
    pub kinds: Option<String>,
}

/// 검색 결과 한 건.
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResult {
    /// `document`·`task`·`item`·`event`.
    pub kind: String,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub title: String,
    pub reference: Option<String>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub item_id: Option<Uuid>,
    /// 본문에서 찾은 곳 근처 글자(일반 글자, 강조 표시 없음).
    pub snippet: Option<String>,
}

fn quote(values: impl IntoIterator<Item = String>) -> String {
    // 값은 모두 UUID 문자열이라 따옴표 밖으로 나갈 수 없다.
    let parts: Vec<String> = values.into_iter().map(|v| format!("\"{v}\"")).collect();
    format!("[{}]", parts.join(", "))
}

fn parse_kinds(raw: Option<&str>) -> Result<Vec<&'static str>, AppError> {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
        return Ok(KINDS.to_vec());
    };
    raw.split(',')
        .map(|k| {
            KINDS
                .iter()
                .copied()
                .find(|known| *known == k.trim())
                .ok_or_else(|| AppError::BadRequest(format!("unknown kind {k}")))
        })
        .collect()
}

async fn scope_filter(
    tx: &mut UserTx,
    workspace: Option<WorkspaceId>,
    kinds: &[&str],
) -> Result<String, AppError> {
    let kind_filter = format!("kind IN {}", quote(kinds.iter().map(|k| (*k).to_owned())));
    if let Some(ws) = workspace {
        member_role(tx, ws).await?;
        return Ok(format!("({kind_filter}) AND workspace_id = \"{}\"", ws.0));
    }
    let workspaces: Vec<Uuid> = sqlx::query_scalar("SELECT app_my_workspace_ids()")
        .fetch_all(tx.conn())
        .await?;
    // 내 워크스페이스 밖에서 볼 수 있는 문서(받은 공유).
    let shared: Vec<Uuid> = sqlx::query_scalar(
        "SELECT d.id FROM documents d
         WHERE (d.workspace_id IS NULL AND d.owner_user_id IS DISTINCT FROM app_user_id())
            OR (d.workspace_id IS NOT NULL AND d.workspace_id NOT IN (SELECT app_my_workspace_ids()))
         LIMIT 1000",
    )
    .fetch_all(tx.conn())
    .await?;
    let mut scope = vec![format!("owner_user_id = \"{}\"", tx.user().0)];
    if !workspaces.is_empty() {
        scope.push(format!(
            "workspace_id IN {}",
            quote(workspaces.iter().map(Uuid::to_string))
        ));
    }
    if !shared.is_empty() {
        scope.push(format!(
            "(kind = \"document\" AND id IN {})",
            quote(shared.iter().map(Uuid::to_string))
        ));
    }
    Ok(format!("({kind_filter}) AND ({})", scope.join(" OR ")))
}

async fn visible(
    tx: &mut UserTx,
    candidates: &[(String, Uuid)],
) -> Result<HashSet<(String, Uuid)>, AppError> {
    let kinds: Vec<&str> = candidates.iter().map(|(k, _)| k.as_str()).collect();
    let ids: Vec<Uuid> = candidates.iter().map(|(_, id)| *id).collect();
    let rows: Vec<(String, Uuid)> =
        sqlx::query_as("SELECT kind, id FROM app_search_visible($1, $2)")
            .bind(&kinds)
            .bind(&ids)
            .fetch_all(tx.conn())
            .await?;
    Ok(rows.into_iter().collect())
}

/// 번호 표시. 호출자 권한으로 읽어 키를 볼 수 없는 사람(멤버가 아닌 공유 대상)에게는 없다.
async fn reference(tx: &mut UserTx, kind: &str, id: Uuid) -> Result<Option<String>, AppError> {
    let sql = match kind {
        "document" => "SELECT app_document_reference($1)",
        "task" => "SELECT app_task_reference($1)",
        "item" => "SELECT app_item_reference($1)",
        _ => return Ok(None),
    };
    Ok(sqlx::query_scalar(sql)
        .bind(id)
        .fetch_optional(tx.conn())
        .await?
        .flatten())
}

fn parse_uuid(raw: Option<&str>) -> Option<Uuid> {
    raw.and_then(|r| Uuid::parse_str(r).ok())
}

/// 통합 검색(로그인한 사람). 볼 수 있는 것만, 최대 20건.
#[utoipa::path(get, path = "/api/search", operation_id = "search", params(SearchQuery),
    responses((status = 200, body = [SearchResult]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "search")]
pub async fn search(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, AppError> {
    let q = query.q.trim();
    if q.is_empty() {
        return Ok(Json(Vec::new()));
    }
    if q.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::BadRequest(format!(
            "the query must be at most {MAX_QUERY_CHARS} characters"
        )));
    }
    let kinds = parse_kinds(query.kinds.as_deref())?;
    let mut tx = state.db.begin_as(user).await?;
    let results = match &state.search {
        Some(client) => {
            let filter = scope_filter(&mut tx, query.workspace_id, &kinds).await?;
            let hits = client.query(q, &filter, CANDIDATES).await?;
            let candidates: Vec<(String, Uuid)> = hits
                .iter()
                .filter_map(|h| Some((h.kind.clone(), Uuid::parse_str(&h.id).ok()?)))
                .collect();
            let allowed = visible(&mut tx, &candidates).await?;
            let mut results = Vec::new();
            for h in hits {
                let Some(id) = Uuid::parse_str(&h.id).ok() else {
                    continue;
                };
                if results.len() == MAX_RESULTS || !allowed.contains(&(h.kind.clone(), id)) {
                    continue;
                }
                let snippet = h
                    .formatted
                    .as_ref()
                    .and_then(|f| f.get("text"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .filter(|s| !s.is_empty());
                let reference = reference(&mut tx, &h.kind, id).await?;
                results.push(SearchResult {
                    kind: h.kind,
                    id,
                    title: h.title,
                    reference,
                    workspace_id: parse_uuid(h.workspace_id.as_deref()),
                    item_id: parse_uuid(h.item_id.as_deref()),
                    snippet,
                });
            }
            results
        }
        None => fallback(&mut tx, q, query.workspace_id, &kinds).await?,
    };
    tx.commit().await?;
    Ok(Json(results))
}

/// Meilisearch가 없을 때: 제목만 DB에서 찾고 같은 접근 함수로 거른다.
async fn fallback(
    tx: &mut UserTx,
    q: &str,
    workspace: Option<WorkspaceId>,
    kinds: &[&str],
) -> Result<Vec<SearchResult>, AppError> {
    if let Some(ws) = workspace {
        member_role(tx, ws).await?;
    }
    let pattern = format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    type Row = (String, Uuid, String, Option<Uuid>, Option<Uuid>);
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT kind, id, title, workspace_id, item_id FROM (
           SELECT 'document' AS kind, id, title, workspace_id, item_id, updated_at
           FROM documents WHERE title ILIKE $1 AND deleted_at IS NULL
           UNION ALL
           SELECT 'task', id, title, workspace_id, item_id, updated_at
           FROM tasks WHERE title ILIKE $1 AND deleted_at IS NULL
           UNION ALL
           SELECT 'item', id, title, workspace_id, NULL, updated_at
           FROM items WHERE title ILIKE $1 AND deleted_at IS NULL
           UNION ALL
           SELECT 'event', id, title, workspace_id, item_id, updated_at
           FROM events WHERE title ILIKE $1 AND deleted_at IS NULL
         ) r
         WHERE r.kind = ANY($2) AND ($3::uuid IS NULL OR r.workspace_id = $3)
         ORDER BY r.updated_at DESC LIMIT 60",
    )
    .bind(&pattern)
    .bind(kinds)
    .bind(workspace)
    .fetch_all(tx.conn())
    .await?;
    let candidates: Vec<(String, Uuid)> = rows.iter().map(|r| (r.0.clone(), r.1)).collect();
    let allowed = visible(tx, &candidates).await?;
    let mut results = Vec::new();
    for (kind, id, title, workspace_id, item_id) in rows {
        if results.len() == MAX_RESULTS || !allowed.contains(&(kind.clone(), id)) {
            continue;
        }
        let reference = reference(tx, &kind, id).await?;
        results.push(SearchResult {
            kind,
            id,
            title,
            reference,
            workspace_id,
            item_id,
            snippet: None,
        });
    }
    Ok(results)
}
