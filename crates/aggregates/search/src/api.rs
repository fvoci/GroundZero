//! 검색 집계 읽기: 색인 후보 또는 SQL 제목 검색을 최종 권한으로 거른다.

use crate::SearchState;
use crate::filters::{Filters, SearchQuery, day_bounds, filters, meili_filters, parse_kinds};
use crate::indexing::SearchClient;
use crate::visibility::{
    file_document, parse_uuid, reference, scope_filter, visible, visible_items,
};
use axum::Json;
use axum::extract::{Query, State};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{OrganizationId, UserId, WorkspaceId};
use fvoci_db::UserTx;
use fvoci_http::organization::OrgScope;
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

/// 한 번에 돌려주는 결과 수.
const MAX_RESULTS: usize = 20;
/// 거르기 전에 Meilisearch에서 가져오는 후보 수.
const CANDIDATES: usize = 60;
/// 거르고 나서 모자라면 더 받아 보는 쪽 수(후보 최대 300개).
const MAX_PAGES: usize = 5;
/// 검색어 길이 상한.
const MAX_QUERY_CHARS: usize = 200;

#[cfg(test)]
#[path = "validation_tests.rs"]
mod validation_tests;

/// 검색 결과 한 건.
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResult {
    /// `document`·`task`·`item`·`event`·`file`.
    pub kind: String,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub title: String,
    pub reference: Option<String>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<Uuid>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub item_id: Option<Uuid>,
    /// 첨부 파일이 붙은 문서(파일만).
    #[schema(value_type = Option<String>, format = Uuid)]
    pub document_id: Option<Uuid>,
    /// 본문에서 찾은 곳 근처 글자(일반 글자, 강조 표시 없음).
    pub snippet: Option<String>,
}

/// 통합 검색(로그인한 사람). 볼 수 있는 것만, 최대 20건.
#[utoipa::path(get, path = "/api/search", operation_id = "search", params(SearchQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = [SearchResult]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "search")]
pub async fn search(
    State(state): State<SearchState>,
    OrgScope { user, organization }: OrgScope,
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
    let mut wanted = filters(&query, user)?;
    wanted.organization = Some(organization);
    let mut tx = state.db.begin_as(user).await?;
    day_bounds(&mut tx, &mut wanted, query.tz.as_deref()).await?;
    if let Some(workspace) = query.workspace_id {
        fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Workspace, workspace)
            .await?;
        let in_scope: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = $1 AND organization_id = $2)",
        )
        .bind(workspace)
        .bind(organization)
        .fetch_one(tx.conn())
        .await?;
        if !in_scope {
            return Err(AppError::NotFound);
        }
    }
    let Some(client) = &state.search else {
        let results = fallback(&mut tx, q, query.workspace_id, &kinds, &wanted).await?;
        tx.commit().await?;
        return Ok(Json(results));
    };
    let filter = format!(
        "({}){}",
        scope_filter(&mut tx, query.workspace_id, &kinds, organization).await?,
        meili_filters(&wanted)
    );
    tx.commit().await?;
    let title_only = query.title_only.unwrap_or(false);
    if let Some(results) = indexed(&state, client, user, q, &filter, title_only, &wanted).await? {
        return Ok(Json(results));
    }
    // 검색 서비스가 응답하지 않으면 제목 검색으로 대신한다.
    let mut tx = state.db.begin_as(user).await?;
    let results = fallback(&mut tx, q, query.workspace_id, &kinds, &wanted).await?;
    tx.commit().await?;
    Ok(Json(results))
}

/// Meilisearch 후보를 쪽마다 받아 볼 수 있는 것만 20개까지 모은다. 검색 서비스가 처음부터 응답하지 않으면 `None`.
/// 작성자·날짜 등 조건은 `filter`에 이미 들어 있고(D-102), 여기서는 호출자 권한으로 볼 수 있는지만 다시 본다(P-14).
/// Meilisearch를 기다리는 동안 DB 트랜잭션을 열어 두지 않는다.
async fn indexed(
    state: &SearchState,
    client: &SearchClient,
    user: UserId,
    q: &str,
    filter: &str,
    title_only: bool,
    wanted: &Filters,
) -> Result<Option<Vec<SearchResult>>, AppError> {
    let mut results = Vec::new();
    for page in 0..MAX_PAGES {
        let hits = match client
            .query(q, filter, page * CANDIDATES, CANDIDATES, title_only)
            .await
        {
            Ok(hits) => hits,
            Err(err) if page == 0 => {
                tracing::warn!(
                    event = "search.unavailable",
                    error_code = err.code(),
                    detail = fvoci_core::logging::safe_error(&err),
                    "search service unavailable; using title search"
                );
                return Ok(None);
            }
            Err(err) => {
                tracing::warn!(
                    event = "search.paging_failed",
                    error_code = err.code(),
                    detail = fvoci_core::logging::safe_error(&err),
                    "search service failed while paging"
                );
                break;
            }
        };
        let last_page = hits.len() < CANDIDATES;
        let mut tx = state.db.begin_as(user).await?;
        append_hits(&mut tx, &mut results, hits, wanted).await?;
        tx.commit().await?;
        if results.len() == MAX_RESULTS || last_page {
            break;
        }
    }
    Ok(Some(results))
}

/// 한 쪽의 후보를 호출자 권한으로 확인한 뒤 응답에 넣는다.
async fn append_hits(
    tx: &mut UserTx,
    results: &mut Vec<SearchResult>,
    hits: Vec<crate::indexing::Hit>,
    wanted: &Filters,
) -> Result<(), AppError> {
    let module = wanted.module;
    let candidates: Vec<(String, Uuid)> = hits
        .iter()
        .filter_map(|h| Some((h.kind.clone(), Uuid::parse_str(&h.id).ok()?)))
        .collect();
    let allowed = visible(tx, &candidates, wanted.organization).await?;
    // 항목 id·모듈은 그 항목을 볼 수 있는 사람에게만 쓴다(감사 델타 SD4). 공유로만 문서·첨부를 보는 사람에게는 항목
    // id를 주지 않고, 모듈로 거를 때는 대체(DB) 경로처럼 그 결과를 내지 않는다.
    let item_ids: Vec<Uuid> = hits
        .iter()
        .filter_map(|h| parse_uuid(h.item_id.as_deref()))
        .collect();
    let seen_items = visible_items(tx, &item_ids).await?;
    for h in hits {
        let Some(id) = Uuid::parse_str(&h.id).ok() else {
            continue;
        };
        let key = (h.kind.clone(), id);
        let item = parse_uuid(h.item_id.as_deref()).filter(|i| seen_items.contains(i));
        if results.len() == MAX_RESULTS
            || !allowed.contains(&key)
            || (module.is_some() && h.item_id.is_some() && item.is_none())
        {
            continue;
        }
        let snippet = h
            .formatted
            .as_ref()
            .and_then(|f| f.get("text"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .filter(|s| !s.is_empty());
        let reference = reference(tx, &h.kind, id).await?;
        let document_id = file_document(tx, &h.kind, id).await?;
        results.push(SearchResult {
            kind: h.kind,
            id,
            title: h.title,
            reference,
            workspace_id: parse_uuid(h.workspace_id.as_deref()),
            item_id: item,
            snippet,
            document_id,
        });
    }
    Ok(())
}

/// Meilisearch가 없을 때: 제목만 DB에서 찾고 같은 접근 함수로 거른다.
async fn fallback(
    tx: &mut UserTx,
    q: &str,
    workspace: Option<WorkspaceId>,
    kinds: &[&str],
    wanted: &Filters,
) -> Result<Vec<SearchResult>, AppError> {
    let rows = fallback_rows(tx, q, workspace, kinds, wanted).await?;
    let candidates: Vec<(String, Uuid)> = rows.iter().map(|r| (r.0.clone(), r.1)).collect();
    let allowed = visible(tx, &candidates, wanted.organization).await?;
    let mut results = Vec::new();
    for (kind, id, title, workspace_id, item_id) in rows {
        if results.len() == MAX_RESULTS || !allowed.contains(&(kind.clone(), id)) {
            continue;
        }
        let reference = reference(tx, &kind, id).await?;
        let document_id = file_document(tx, &kind, id).await?;
        results.push(SearchResult {
            kind,
            id,
            title,
            reference,
            workspace_id,
            item_id,
            snippet: None,
            document_id,
        });
    }
    Ok(results)
}

type FallbackRow = (String, Uuid, String, Option<Uuid>, Option<Uuid>);

async fn fallback_rows(
    tx: &mut UserTx,
    q: &str,
    workspace: Option<WorkspaceId>,
    kinds: &[&str],
    wanted: &Filters,
) -> Result<Vec<FallbackRow>, AppError> {
    if let Some(ws) = workspace {
        fvoci_db::access::require_visible(tx, fvoci_db::access::Kind::Workspace, ws).await?;
    }
    let pattern = format!(
        "%{}%",
        q.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let rows: Vec<FallbackRow> = sqlx::query_as(
        // 항목 id는 그 항목을 볼 수 있는 사람에게만 준다(공유로만 문서·첨부를 보는 사람에게 비공개 항목을 드러내지 않는다,
        // 문서 조회와 같은 규칙). 감사 델타 SD4.
        // 날짜: 실제 시각(`at`)은 보는 사람 시간대의 범위($9·$10)로, 날짜만 있는 값(`on_day`)은 날짜($7·$8)로 본다. 한 행에는
        // 둘 중 하나만 있다(색인 `app_system_search_payload`와 같은 규칙, D-101).
        "SELECT kind, id, title, workspace_id,
                CASE WHEN r.item_id IS NOT NULL AND app_access('item', r.item_id) <> 'none' THEN r.item_id END
         FROM (
           SELECT 'document' AS kind, d.id, d.title, d.workspace_id, d.item_id, d.updated_at,
                  (SELECT module_id FROM items WHERE id = d.item_id) AS module_id, NULL::text AS status_category,
                  NULL::uuid[] AS assignees, d.updated_at AS at, NULL::date AS on_day, d.created_by
           FROM documents d WHERE d.title ILIKE $1 AND d.deleted_at IS NULL
           UNION ALL
           SELECT 'task', t.id, t.title, t.workspace_id, t.item_id, t.updated_at,
                  (SELECT module_id FROM items WHERE id = t.item_id), s.category,
                  ARRAY(SELECT user_id FROM task_assignees WHERE task_id = t.id), NULL,
                  coalesce(t.due_date, t.start_date), t.created_by
           FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.title ILIKE $1 AND t.deleted_at IS NULL
           UNION ALL
           SELECT 'item', i.id, i.title, i.workspace_id, NULL, i.updated_at, i.module_id, NULL, NULL,
                  i.updated_at, NULL, i.created_by
           FROM items i WHERE i.title ILIKE $1 AND i.deleted_at IS NULL
           UNION ALL
           SELECT 'event', e.id, e.title, e.workspace_id, e.item_id, e.updated_at,
                  (SELECT module_id FROM items WHERE id = e.item_id), NULL, NULL,
                  e.starts_at, e.start_date, e.created_by
           FROM events e WHERE e.title ILIKE $1 AND e.deleted_at IS NULL
           UNION ALL
           SELECT 'file', f.id, f.name, f.workspace_id, coalesce(f.item_id, d.item_id), f.updated_at,
                  (SELECT module_id FROM items WHERE id = coalesce(f.item_id, d.item_id)), NULL, NULL,
                  f.updated_at, NULL, f.created_by
           FROM files f LEFT JOIN documents d ON d.id = f.document_id
           WHERE f.name ILIKE $1 AND f.deleted_at IS NULL
           UNION ALL
           SELECT 'record', rc.id, rc.title, rc.workspace_id, rc.item_id, rc.updated_at,
                  (SELECT module_id FROM items WHERE id = rc.item_id), NULL, NULL,
                  CASE WHEN rc.occurred_on IS NULL THEN rc.updated_at END, rc.occurred_on, rc.created_by
           FROM item_records rc WHERE rc.title ILIKE $1
         ) r
         WHERE r.kind = ANY($2) AND ($3::uuid IS NULL OR r.workspace_id = $3)
           AND ($4::uuid IS NULL OR r.module_id = $4)
           AND ($5::text IS NULL OR r.status_category = $5)
           AND ($6::uuid IS NULL OR $6 = ANY (r.assignees))
           AND (($7::date IS NULL AND $8::date IS NULL)
                OR (r.at >= coalesce($9::timestamptz, '-infinity') AND r.at < coalesce($10::timestamptz, 'infinity'))
                OR (r.on_day >= coalesce($7::date, '-infinity') AND r.on_day <= coalesce($8::date, 'infinity')))
           AND ($11::uuid IS NULL OR r.created_by = $11)
           AND (r.workspace_id IN (SELECT app_my_workspace_ids($12))
                OR (r.workspace_id IS NULL AND (
                    (r.kind = 'document' AND EXISTS (SELECT 1 FROM documents d WHERE d.id = r.id AND d.owner_user_id = app_user_id()))
                    OR (r.kind = 'file' AND EXISTS (SELECT 1 FROM files f JOIN documents d ON d.id = f.document_id WHERE f.id = r.id AND d.owner_user_id = app_user_id())))))
         ORDER BY r.updated_at DESC LIMIT 60",
    )
    .bind(&pattern)
    .bind(kinds)
    .bind(workspace)
    .bind(wanted.module)
    .bind(wanted.status_category)
    .bind(wanted.assignee)
    .bind(wanted.from)
    .bind(wanted.to)
    .bind(wanted.start)
    .bind(wanted.end)
    .bind(wanted.author)
    .bind(wanted.organization)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}
