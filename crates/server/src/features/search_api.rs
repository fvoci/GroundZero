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
use crate::ids::{UserId, WorkspaceId};
use crate::search::SearchClient;

/// 한 번에 돌려주는 결과 수.
const MAX_RESULTS: usize = 20;
/// 거르기 전에 Meilisearch에서 가져오는 후보 수.
const CANDIDATES: usize = 60;
/// 거르고 나서 모자라면 더 받아 보는 쪽 수(후보 최대 300개).
const MAX_PAGES: usize = 5;
/// 검색어 길이 상한.
const MAX_QUERY_CHARS: usize = 200;

const KINDS: &[&str] = &["document", "task", "item", "event", "file", "record"];

/// 검색 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SearchQuery {
    pub q: String,
    /// 이 워크스페이스 안에서만(멤버).
    #[param(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<WorkspaceId>,
    /// 쉼표로 구분한 종류(`document,task,item,event,file,record`). 없으면 전부.
    pub kinds: Option<String>,
    /// 이 모듈 안의 것만.
    #[param(value_type = Option<String>, format = Uuid)]
    pub module_id: Option<Uuid>,
    /// 업무 상태(`todo`·`in_progress`·`done`). 주면 업무만 나온다.
    pub status: Option<String>,
    /// 담당자(`me` 또는 사용자 id). 주면 업무만 나온다.
    pub assignee: Option<String>,
    /// 날짜 범위 시작(YYYY-MM-DD): 업무는 기한(없으면 시작일), 일정은 시작일, 기록은 발생일(없으면 고친 날), 그 밖은 고친 날.
    pub from: Option<String>,
    /// 날짜 범위 끝(YYYY-MM-DD, 포함).
    pub to: Option<String>,
    /// 작성자(`me` 또는 사용자 id): 그 사람이 만든 것만(각 대상의 `created_by`). 볼 수 있는 범위는 넓히지 않는다.
    pub author: Option<String>,
    /// `true`면 제목에서만 찾는다(본문·첨부 글자·번호는 보지 않는다).
    pub title_only: Option<bool>,
}

/// 검색 조건 중 필터.
#[derive(Debug, Default)]
struct Filters {
    module: Option<Uuid>,
    status: Option<String>,
    assignee: Option<Uuid>,
    from: Option<time::Date>,
    to: Option<time::Date>,
    author: Option<Uuid>,
}

fn parse_day(raw: Option<&str>, name: &str) -> Result<Option<time::Date>, AppError> {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
        return Ok(None);
    };
    let format = time::macros::format_description!("[year]-[month]-[day]");
    time::Date::parse(raw.trim(), &format)
        .map(Some)
        .map_err(|_| AppError::BadRequest(format!("{name} must be YYYY-MM-DD")))
}

fn filters(query: &SearchQuery, me: UserId) -> Result<Filters, AppError> {
    let status = match query.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s @ ("todo" | "in_progress" | "done")) => Some(s.to_owned()),
        Some(other) => return Err(AppError::BadRequest(format!("unknown status {other}"))),
    };
    let person = |raw: Option<&str>, name: &str| -> Result<Option<Uuid>, AppError> {
        match raw.map(str::trim) {
            None | Some("") => Ok(None),
            Some("me") => Ok(Some(me.0)),
            Some(raw) => Uuid::parse_str(raw)
                .map(Some)
                .map_err(|_| AppError::BadRequest(format!("{name} must be me or a user id"))),
        }
    };
    let assignee = person(query.assignee.as_deref(), "assignee")?;
    let author = person(query.author.as_deref(), "author")?;
    Ok(Filters {
        module: query.module_id,
        status,
        assignee,
        from: parse_day(query.from.as_deref(), "from")?,
        to: parse_day(query.to.as_deref(), "to")?,
        author,
    })
}

fn day_number(day: time::Date) -> i32 {
    day.year() * 10_000 + i32::from(u8::from(day.month())) * 100 + i32::from(day.day())
}

/// Meilisearch 필터에 더할 조건(값은 UUID·고정 낱말·숫자뿐이라 따옴표 밖으로 나갈 수 없다).
fn meili_filters(f: &Filters) -> String {
    let mut out = String::new();
    if let Some(m) = f.module {
        out.push_str(&format!(" AND module_id = \"{m}\""));
    }
    if let Some(s) = &f.status {
        out.push_str(&format!(" AND status = \"{s}\""));
    }
    if let Some(a) = f.assignee {
        out.push_str(&format!(" AND assignee_ids = \"{a}\""));
    }
    if let Some(d) = f.from {
        out.push_str(&format!(" AND day >= {}", day_number(d)));
    }
    if let Some(d) = f.to {
        out.push_str(&format!(" AND day <= {}", day_number(d)));
    }
    out
}

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

/// 이 항목들 가운데 그 사람이 볼 수 있는 것(호출자 권한, 항목 RLS).
async fn visible_items(tx: &mut UserTx, ids: &[Uuid]) -> Result<HashSet<Uuid>, AppError> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let rows: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM items WHERE id = ANY($1)")
        .bind(ids)
        .fetch_all(tx.conn())
        .await?;
    Ok(rows.into_iter().collect())
}

/// 후보 가운데 그 사람이 만든 것(각 대상의 `created_by`). 호출자 권한(RLS)으로 읽으므로 볼 수 없는 행은 나오지 않는다.
async fn authored(
    tx: &mut UserTx,
    author: Uuid,
    candidates: &[(String, Uuid)],
) -> Result<HashSet<(String, Uuid)>, AppError> {
    let ids: Vec<Uuid> = candidates.iter().map(|(_, id)| *id).collect();
    let rows: Vec<(String, Uuid)> = sqlx::query_as(
        "SELECT 'document', id FROM documents WHERE id = ANY($2) AND created_by = $1
         UNION ALL SELECT 'task', id FROM tasks WHERE id = ANY($2) AND created_by = $1
         UNION ALL SELECT 'item', id FROM items WHERE id = ANY($2) AND created_by = $1
         UNION ALL SELECT 'event', id FROM events WHERE id = ANY($2) AND created_by = $1
         UNION ALL SELECT 'file', id FROM files WHERE id = ANY($2) AND created_by = $1
         UNION ALL SELECT 'record', id FROM item_records WHERE id = ANY($2) AND created_by = $1",
    )
    .bind(author)
    .bind(&ids)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows.into_iter().collect())
}

/// 파일이 붙은 문서(파일 결과에서 문서로 열 때).
async fn file_document(tx: &mut UserTx, kind: &str, id: Uuid) -> Result<Option<Uuid>, AppError> {
    if kind != "file" {
        return Ok(None);
    }
    Ok(
        sqlx::query_scalar("SELECT document_id FROM files WHERE id = $1")
            .bind(id)
            .fetch_optional(tx.conn())
            .await?
            .flatten(),
    )
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
    let wanted = filters(&query, user)?;
    let Some(client) = &state.search else {
        let mut tx = state.db.begin_as(user).await?;
        let results = fallback(&mut tx, q, query.workspace_id, &kinds, &wanted).await?;
        tx.commit().await?;
        return Ok(Json(results));
    };
    let mut tx = state.db.begin_as(user).await?;
    let filter = format!(
        "({}){}",
        scope_filter(&mut tx, query.workspace_id, &kinds).await?,
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
/// Meilisearch를 기다리는 동안 DB 트랜잭션을 열어 두지 않는다.
async fn indexed(
    state: &AppState,
    client: &SearchClient,
    user: UserId,
    q: &str,
    filter: &str,
    title_only: bool,
    wanted: &Filters,
) -> Result<Option<Vec<SearchResult>>, AppError> {
    let (author, module) = (wanted.author, wanted.module);
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
                    detail = crate::logging::safe_error(&err),
                    "search service unavailable; using title search"
                );
                return Ok(None);
            }
            Err(err) => {
                tracing::warn!(
                    event = "search.paging_failed",
                    error_code = err.code(),
                    detail = crate::logging::safe_error(&err),
                    "search service failed while paging"
                );
                break;
            }
        };
        let last_page = hits.len() < CANDIDATES;
        let candidates: Vec<(String, Uuid)> = hits
            .iter()
            .filter_map(|h| Some((h.kind.clone(), Uuid::parse_str(&h.id).ok()?)))
            .collect();
        let mut tx = state.db.begin_as(user).await?;
        let allowed = visible(&mut tx, &candidates).await?;
        let made = match author {
            Some(author) => Some(authored(&mut tx, author, &candidates).await?),
            None => None,
        };
        // 항목 id·모듈은 그 항목을 볼 수 있는 사람에게만 쓴다(감사 델타 SD4). 공유로만 문서·첨부를 보는 사람에게는 항목
        // id를 주지 않고, 모듈로 거를 때는 대체(DB) 경로처럼 그 결과를 내지 않는다.
        let item_ids: Vec<Uuid> = hits
            .iter()
            .filter_map(|h| parse_uuid(h.item_id.as_deref()))
            .collect();
        let seen_items = visible_items(&mut tx, &item_ids).await?;
        for h in hits {
            let Some(id) = Uuid::parse_str(&h.id).ok() else {
                continue;
            };
            let key = (h.kind.clone(), id);
            let item = parse_uuid(h.item_id.as_deref()).filter(|i| seen_items.contains(i));
            if results.len() == MAX_RESULTS
                || !allowed.contains(&key)
                || made.as_ref().is_some_and(|m| !m.contains(&key))
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
            let reference = reference(&mut tx, &h.kind, id).await?;
            let document_id = file_document(&mut tx, &h.kind, id).await?;
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
        tx.commit().await?;
        if results.len() == MAX_RESULTS || last_page {
            break;
        }
    }
    Ok(Some(results))
}

/// Meilisearch가 없을 때: 제목만 DB에서 찾고 같은 접근 함수로 거른다.
async fn fallback(
    tx: &mut UserTx,
    q: &str,
    workspace: Option<WorkspaceId>,
    kinds: &[&str],
    wanted: &Filters,
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
        // 항목 id는 그 항목을 볼 수 있는 사람에게만 준다(공유로만 문서·첨부를 보는 사람에게 비공개 항목을 드러내지 않는다,
        // 문서 조회와 같은 규칙). 감사 델타 SD4.
        "SELECT kind, id, title, workspace_id,
                CASE WHEN r.item_id IS NOT NULL AND app_item_access(r.item_id) IS NOT NULL THEN r.item_id END
         FROM (
           SELECT 'document' AS kind, d.id, d.title, d.workspace_id, d.item_id, d.updated_at,
                  (SELECT module_id FROM items WHERE id = d.item_id) AS module_id, NULL::text AS status,
                  NULL::uuid[] AS assignees, d.updated_at::date AS day, d.created_by
           FROM documents d WHERE d.title ILIKE $1 AND d.deleted_at IS NULL
           UNION ALL
           SELECT 'task', t.id, t.title, t.workspace_id, t.item_id, t.updated_at,
                  (SELECT module_id FROM items WHERE id = t.item_id), t.status,
                  ARRAY(SELECT user_id FROM task_assignees WHERE task_id = t.id), coalesce(t.due_date, t.start_date),
                  t.created_by
           FROM tasks t WHERE t.title ILIKE $1 AND t.deleted_at IS NULL
           UNION ALL
           SELECT 'item', i.id, i.title, i.workspace_id, NULL, i.updated_at, i.module_id, NULL, NULL,
                  i.updated_at::date, i.created_by
           FROM items i WHERE i.title ILIKE $1 AND i.deleted_at IS NULL
           UNION ALL
           SELECT 'event', e.id, e.title, e.workspace_id, e.item_id, e.updated_at,
                  (SELECT module_id FROM items WHERE id = e.item_id), NULL, NULL,
                  coalesce(e.start_date, (e.starts_at AT TIME ZONE e.timezone)::date), e.created_by
           FROM events e WHERE e.title ILIKE $1 AND e.deleted_at IS NULL
           UNION ALL
           SELECT 'file', f.id, f.name, f.workspace_id, coalesce(f.item_id, d.item_id), f.updated_at,
                  (SELECT module_id FROM items WHERE id = coalesce(f.item_id, d.item_id)), NULL, NULL,
                  f.updated_at::date, f.created_by
           FROM files f LEFT JOIN documents d ON d.id = f.document_id
           WHERE f.name ILIKE $1 AND f.deleted_at IS NULL
           UNION ALL
           SELECT 'record', rc.id, rc.title, rc.workspace_id, rc.item_id, rc.updated_at,
                  (SELECT module_id FROM items WHERE id = rc.item_id), NULL, NULL,
                  coalesce(rc.occurred_on, rc.updated_at::date), rc.created_by
           FROM item_records rc WHERE rc.title ILIKE $1
         ) r
         WHERE r.kind = ANY($2) AND ($3::uuid IS NULL OR r.workspace_id = $3)
           AND ($4::uuid IS NULL OR r.module_id = $4)
           AND ($5::text IS NULL OR r.status = $5)
           AND ($6::uuid IS NULL OR $6 = ANY (r.assignees))
           AND ($7::date IS NULL OR r.day >= $7)
           AND ($8::date IS NULL OR r.day <= $8)
           AND ($9::uuid IS NULL OR r.created_by = $9)
         ORDER BY r.updated_at DESC LIMIT 60",
    )
    .bind(&pattern)
    .bind(kinds)
    .bind(workspace)
    .bind(wanted.module)
    .bind(wanted.status.as_deref())
    .bind(wanted.assignee)
    .bind(wanted.from)
    .bind(wanted.to)
    .bind(wanted.author)
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
