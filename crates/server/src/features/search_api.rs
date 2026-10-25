//! 통합 검색 API (D-39). 문서·업무·항목·일정을 한 번에 찾는다. 볼 수 있는 것만 나온다.
//!
//! Meilisearch가 있으면 "내 워크스페이스 또는 내 개인 노트 또는 나에게 공유된 문서"로 먼저 좁혀 찾은 뒤
//! PostgreSQL 접근 함수로 다시 거른다(비공개 항목·지운 것·권한이 바뀐 것 제외). 없으면 제목만 DB에서 찾는다.
//! 색인은 백그라운드 작업이 반영하므로 바로 직전의 변경은 조금 늦게 보일 수 있다.

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::workspaces::member_role;
use crate::http::AppState;
use crate::ids::{UserId, WorkspaceId};
use crate::search::{SearchClient, day_number};

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
    /// 업무 상태 이름(1..50자). 주면 업무만 나온다.
    pub status: Option<String>,
    /// 담당자(`me` 또는 사용자 id). 주면 업무만 나온다.
    pub assignee: Option<String>,
    /// 날짜 범위 시작(YYYY-MM-DD). 날짜만 있는 값(업무 기한(없으면 시작일), 종일 일정의 시작일, 기록의 발생일)은 그 날짜로,
    /// 실제 시각(문서·항목·파일의 고친 때, 발생일 없는 기록의 고친 때, 시각 일정의 시작)은 `tz`의 그날 0시부터 비교한다.
    pub from: Option<String>,
    /// 날짜 범위 끝(YYYY-MM-DD, 포함). 실제 시각은 `tz`의 다음 날 0시 전까지.
    pub to: Option<String>,
    /// 날짜 범위의 하루를 정하는 IANA 시간대(예: `Asia/Seoul`). 없으면 저장한 시간대, 그것도 없으면 UTC. 모르는 이름은 400.
    pub tz: Option<String>,
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
    /// 날짜만 있는 값과 비교하는 범위(끝 포함).
    from: Option<time::Date>,
    to: Option<time::Date>,
    /// 실제 시각과 비교하는 범위: `from`의 그날 시작부터 `to` 다음 날 시작 전까지(보는 사람 시간대, `day_bounds`).
    start: Option<OffsetDateTime>,
    end: Option<OffsetDateTime>,
    author: Option<Uuid>,
}

fn parse_day(raw: Option<&str>, name: &str) -> Result<Option<time::Date>, AppError> {
    let Some(raw) = raw.filter(|r| !r.trim().is_empty()) else {
        return Ok(None);
    };
    let format = time::macros::format_description!("[year]-[month]-[day]");
    let day = time::Date::parse(raw.trim(), &format)
        .map_err(|_| AppError::BadRequest(format!("{name} must be YYYY-MM-DD")))?;
    // 그날 시작과 다음 날 시작을 시각으로 만들 수 있는 날만(`day_bounds`): 9999-12-31은 다음 날이 `time` 범위 밖이고,
    // 1년 전은 시간대에 따라 PostgreSQL 시각 범위를 벗어날 수 있다.
    if day.year() < 1 || day.next_day().is_none() {
        return Err(AppError::BadRequest(format!("{name} is out of range")));
    }
    Ok(Some(day))
}

fn filters(query: &SearchQuery, me: UserId) -> Result<Filters, AppError> {
    let status = match query.status.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(s) => Some(crate::features::tasks::validate_status(s)?),
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
        start: None,
        end: None,
        author,
    })
}

/// 날짜 범위를 보는 사람 시간대의 시각 범위로 바꾼다(D-101). 시작은 `from`의 그날 시작, 끝은 `to` 다음 날의 시작(제외)이라
/// 여름시간으로 23·25시간인 날도 그대로 맞다. 색인 경로와 대체 경로가 이 값을 함께 쓴다. 시간대를 보내지 않으면 저장한
/// 시간대(D-81), 그것도 없으면 UTC. 모르는 시간대는 400.
async fn day_bounds(tx: &mut UserTx, f: &mut Filters, tz: Option<&str>) -> Result<(), AppError> {
    if f.from.is_none() && f.to.is_none() {
        return Ok(());
    }
    let tz: String = match tz.map(str::trim).filter(|t| !t.is_empty()) {
        Some(tz) => tz.to_owned(),
        None => sqlx::query_scalar(
            "SELECT coalesce(nullif((SELECT time_zone FROM user_settings WHERE user_id = app_user_id()), ''), 'UTC')",
        )
        .fetch_one(tx.conn())
        .await?,
    };
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(&tz)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 하루의 시작은 그날 0시가 처음 오는 시점이다(0시가 두 번 오는 날은 앞의 0시, 없는 날은 건너뛴 직후). 시간 기록의 날
    // 경계(`time_tracking.rs`, 감사 S21)와 같은 식.
    let (start, end): (Option<OffsetDateTime>, Option<OffsetDateTime>) = sqlx::query_as(
        "SELECT coalesce(app_local_instant($1::date::timestamp, $3), $1::date::timestamp AT TIME ZONE $3),
                coalesce(app_local_instant(($2::date + 1)::timestamp, $3), ($2::date + 1)::timestamp AT TIME ZONE $3)",
    )
    .bind(f.from)
    .bind(f.to)
    .bind(&tz)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    f.start = start;
    f.end = end;
    Ok(())
}

/// Meilisearch 필터에 더할 조건(값은 UUID·고정 낱말·숫자뿐이라 따옴표 밖으로 나갈 수 없다).
fn meili_filters(f: &Filters) -> String {
    let mut out = String::new();
    if let Some(m) = f.module {
        out.push_str(&format!(" AND module_id = \"{m}\""));
    }
    if let Some(s) = &f.status {
        // 사용자 정의 이름의 따옴표·역슬래시가 필터 식을 바꾸지 않게 문자열로 인코딩한다.
        out.push_str(&format!(
            " AND status = {}",
            serde_json::Value::String(s.clone())
        ));
    }
    if let Some(a) = f.assignee {
        out.push_str(&format!(" AND assignee_ids = \"{a}\""));
    }
    // 한 문서에는 `at`(실제 시각)·`date`(날짜만) 가운데 하나만 있다. 대체 경로와 같은 조건.
    let mut at = Vec::new();
    let mut date = Vec::new();
    if let (Some(start), Some(d)) = (f.start, f.from) {
        at.push(format!("at >= {}", start.unix_timestamp()));
        date.push(format!("date >= {}", day_number(d)));
    }
    if let (Some(end), Some(d)) = (f.end, f.to) {
        at.push(format!("at < {}", end.unix_timestamp()));
        date.push(format!("date <= {}", day_number(d)));
    }
    if !at.is_empty() {
        out.push_str(&format!(
            " AND (({}) OR ({}))",
            at.join(" AND "),
            date.join(" AND ")
        ));
    }
    if let Some(a) = f.author {
        out.push_str(&format!(" AND created_by = \"{a}\""));
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
    let workspaces: Vec<Uuid> = sqlx::query_scalar("SELECT app_my_workspace_ids(NULL::uuid)")
        .fetch_all(tx.conn())
        .await?;
    // 내 워크스페이스 밖에서 볼 수 있는 문서(받은 공유).
    let shared: Vec<Uuid> = sqlx::query_scalar(
        "SELECT d.id FROM documents d
         WHERE (d.workspace_id IS NULL AND d.owner_user_id IS DISTINCT FROM app_user_id())
            OR (d.workspace_id IS NOT NULL AND d.workspace_id NOT IN (SELECT app_my_workspace_ids(NULL::uuid)))
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
    let mut wanted = filters(&query, user)?;
    let mut tx = state.db.begin_as(user).await?;
    day_bounds(&mut tx, &mut wanted, query.tz.as_deref()).await?;
    let Some(client) = &state.search else {
        let results = fallback(&mut tx, q, query.workspace_id, &kinds, &wanted).await?;
        tx.commit().await?;
        return Ok(Json(results));
    };
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
/// 작성자·날짜 등 조건은 `filter`에 이미 들어 있고(D-102), 여기서는 호출자 권한으로 볼 수 있는지만 다시 본다(P-14).
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
    let module = wanted.module;
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
        // 날짜: 실제 시각(`at`)은 보는 사람 시간대의 범위($9·$10)로, 날짜만 있는 값(`on_day`)은 날짜($7·$8)로 본다. 한 행에는
        // 둘 중 하나만 있다(색인 `app_system_search_payload`와 같은 규칙, D-101).
        "SELECT kind, id, title, workspace_id,
                CASE WHEN r.item_id IS NOT NULL AND app_access('item', r.item_id) <> 'none' THEN r.item_id END
         FROM (
           SELECT 'document' AS kind, d.id, d.title, d.workspace_id, d.item_id, d.updated_at,
                  (SELECT module_id FROM items WHERE id = d.item_id) AS module_id, NULL::text AS status,
                  NULL::uuid[] AS assignees, d.updated_at AS at, NULL::date AS on_day, d.created_by
           FROM documents d WHERE d.title ILIKE $1 AND d.deleted_at IS NULL
           UNION ALL
           SELECT 'task', t.id, t.title, t.workspace_id, t.item_id, t.updated_at,
                  (SELECT module_id FROM items WHERE id = t.item_id), s.name,
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
           AND ($5::text IS NULL OR r.status = $5)
           AND ($6::uuid IS NULL OR $6 = ANY (r.assignees))
           AND (($7::date IS NULL AND $8::date IS NULL)
                OR (r.at >= coalesce($9::timestamptz, '-infinity') AND r.at < coalesce($10::timestamptz, 'infinity'))
                OR (r.on_day >= coalesce($7::date, '-infinity') AND r.on_day <= coalesce($8::date, 'infinity')))
           AND ($11::uuid IS NULL OR r.created_by = $11)
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
    .bind(wanted.start)
    .bind(wanted.end)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_status_is_a_quoted_filter_value() {
        let filter = Filters {
            status: Some("검토 \\\" OR kind = \\\"file".into()),
            ..Filters::default()
        };
        let expression = meili_filters(&filter);
        let encoded = expression.strip_prefix(" AND status = ").unwrap();
        let decoded: String = serde_json::from_str(encoded).unwrap();
        assert_eq!(Some(decoded), filter.status);
    }

    /// 날짜 범위는 그날 시작과 다음 날 시작을 시각으로 만든다. 다음 날이 없는 날(9999-12-31)이나 1년 전은 그 시각을
    /// 나타낼 수 없어(`time`·PostgreSQL 범위 밖) 500이 되던 것을 400으로 거절한다(CodeRabbit #99).
    #[test]
    fn days_without_a_representable_boundary_are_rejected() {
        for raw in ["9999-12-31", "0000-12-31", "-0001-01-01"] {
            assert!(
                matches!(parse_day(Some(raw), "to"), Err(AppError::BadRequest(_))),
                "{raw}"
            );
        }
        for raw in ["9999-12-30", "0001-01-01", "2026-03-08"] {
            assert!(matches!(parse_day(Some(raw), "to"), Ok(Some(_))), "{raw}");
        }
    }

    #[test]
    fn date_ranges_compare_instants_and_dates_separately() {
        let day = |s: &str| {
            time::Date::parse(s, time::macros::format_description!("[year]-[month]-[day]")).ok()
        };
        let at = |s: i64| OffsetDateTime::from_unix_timestamp(s).ok();
        let author = Uuid::from_u128(7);
        let both = Filters {
            from: day("2026-03-08"),
            to: day("2026-03-08"),
            start: at(1_772_946_000),
            end: at(1_773_028_800),
            author: Some(author),
            ..Filters::default()
        };
        assert_eq!(
            meili_filters(&both),
            format!(
                " AND ((at >= 1772946000 AND at < 1773028800) OR (date >= 20260308 AND date <= 20260308)) \
                 AND created_by = \"{author}\""
            )
        );
        let from_only = Filters {
            from: day("2026-03-09"),
            start: at(1_773_028_800),
            ..Filters::default()
        };
        assert_eq!(
            meili_filters(&from_only),
            " AND ((at >= 1773028800) OR (date >= 20260309))"
        );
        assert_eq!(meili_filters(&Filters::default()), "");
    }
}
