//! @ 링크·사용자 언급·역방향 링크·알림 (D-18, D-27, D-39).
//!
//! 본문의 참조 노드는 종류와 내부 id만 가진다. 표시 이름은 여기서 권한을 확인해 만든다.
//! 볼 수 없는 대상과 없는 대상은 같은 모양(`visible: false`)으로 돌려준다(존재 여부를 드러내지 않는다).
//! 링크·언급 기록과 알림 생성은 저장 트랜잭션 안의 DB 함수 `app_record_document_references`가 한다
//! (`migrations/0008_references.sql`). 언급은 접근 권한을 주지 않는다.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::db::UserTx;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::documents::access;
use crate::http::AppState;
use crate::ids::{DocumentId, ItemId, WorkspaceId};

/// 종류별 후보 수 상한.
const CANDIDATES_PER_KIND: i64 = 8;
/// 알림 목록 상한.
const NOTIFICATION_PAGE: i32 = 100;

/// 참조 대상 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Item,
    Document,
    User,
}

impl ReferenceKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "user" => Some(Self::User),
            _ => None,
        }
    }
}

/// @ 입력 후보. 이름이 같은 후보는 참조 번호와 워크스페이스로 구분한다 (D-18).
#[derive(Debug, Serialize, ToSchema)]
pub struct ReferenceCandidate {
    pub kind: ReferenceKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// 항목·문서 제목 또는 사람 이름.
    pub title: String,
    /// 예: `DEV-3`, `WIKI-1`. 사람과 개인 노트는 없다.
    pub reference: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub workspace_name: Option<String>,
}

/// 후보 검색 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct CandidateQuery {
    /// 제목·이름 일부 또는 참조(`DEV-3`, 예전 키도 됨).
    #[serde(default)]
    pub q: String,
    /// 이 워크스페이스로 좁힌다. 없으면 내가 속한 모든 워크스페이스와 내 노트.
    pub workspace_id: Option<WorkspaceId>,
}

/// 해석할 참조.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReferenceInput {
    pub kind: ReferenceKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

/// 해석 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ResolveReferences {
    pub refs: Vec<ReferenceInput>,
}

/// 해석 결과. 볼 수 없거나 없는 대상은 `visible: false`이고 다른 값은 모두 비어 있다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ResolvedReference {
    pub kind: ReferenceKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub visible: bool,
    pub title: Option<String>,
    pub reference: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    /// 문서가 항목 아래 있으면 그 항목(볼 수 있을 때만).
    pub item_id: Option<ItemId>,
}

impl ResolvedReference {
    fn hidden(input: &ReferenceInput) -> Self {
        Self {
            kind: input.kind,
            id: input.id,
            visible: false,
            title: None,
            reference: None,
            workspace_id: None,
            item_id: None,
        }
    }
}

/// 이 대상을 가리키는 문서(내가 볼 수 있는 것만).
#[derive(Debug, Serialize, ToSchema)]
pub struct Backlink {
    pub id: DocumentId,
    pub reference: Option<String>,
    pub title: String,
    pub workspace_id: Option<WorkspaceId>,
    pub item_id: Option<ItemId>,
}

/// 알림. 지금 볼 수 있는 문서에 대한 것만 나온다.
#[derive(Debug, Serialize, ToSchema)]
pub struct Notification {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// 현재는 `mention`.
    pub kind: String,
    pub document_id: DocumentId,
    pub document_title: String,
    pub document_reference: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub item_id: Option<ItemId>,
    pub actor_name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub read: bool,
}

/// 읽음 표시 요청. `id`가 없으면 모두 읽음.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkRead {
    #[schema(value_type = Option<String>, format = Uuid)]
    pub id: Option<Uuid>,
}

/// LIKE 패턴 이스케이프(`\`, `%`, `_`).
fn like_pattern(q: &str) -> String {
    let mut out = String::with_capacity(q.len() + 2);
    out.push('%');
    for c in q.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

/// `DEV-3`·`wiki-12` 같은 참조 입력을 (대문자 키, 번호)로 나눈다.
fn parse_reference(q: &str) -> Option<(String, i64)> {
    let (key, number) = q.trim().rsplit_once('-')?;
    let key = key.to_ascii_uppercase();
    let mut chars = key.chars();
    let key_ok = chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        && (2..=10).contains(&key.len());
    let number: i64 = number.parse().ok().filter(|n| *n > 0)?;
    key_ok.then_some((key, number))
}

type CandidateRow = (
    String,
    Uuid,
    String,
    Option<String>,
    Option<WorkspaceId>,
    Option<String>,
);

fn to_candidates(rows: Vec<CandidateRow>) -> Vec<ReferenceCandidate> {
    rows.into_iter()
        .filter_map(
            |(kind, id, title, reference, workspace_id, workspace_name)| {
                Some(ReferenceCandidate {
                    kind: ReferenceKind::parse(&kind)?,
                    id,
                    title,
                    reference,
                    workspace_id,
                    workspace_name,
                })
            },
        )
        .collect()
}

/// 참조 번호로 찾기. 예전 키도 별칭으로 찾는다 (D-18). 볼 수 없는 대상은 RLS로 빠진다.
async fn by_reference(
    tx: &mut UserTx,
    ws: Option<WorkspaceId>,
    key: &str,
    number: i64,
) -> Result<Vec<CandidateRow>, AppError> {
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "WITH scopes AS (
           SELECT w.id AS workspace_id, w.id AS scope_id FROM workspaces w WHERE $2 = 'WIKI'
           UNION
           SELECT k.workspace_id, k.module_id FROM module_keys k WHERE k.key = $2
         )
         SELECT n.target_kind, n.target_id,
                coalesce(i.title, d.title),
                CASE n.target_kind WHEN 'item' THEN app_item_reference(n.target_id)
                                   ELSE app_document_reference(n.target_id) END,
                n.workspace_id, w.name
         FROM scopes s
         JOIN numbered_targets n ON n.scope_id = s.scope_id AND n.workspace_id = s.workspace_id
         JOIN workspaces w ON w.id = n.workspace_id
         LEFT JOIN items i ON n.target_kind = 'item' AND i.id = n.target_id
         LEFT JOIN documents d ON n.target_kind = 'document' AND d.id = n.target_id
         WHERE n.number = $3 AND ($1::uuid IS NULL OR n.workspace_id = $1)
           AND coalesce(i.title, d.title) IS NOT NULL",
    )
    .bind(ws)
    .bind(key)
    .bind(number)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows)
}

/// @ 입력 후보: 사람·항목·문서를 종류별로 찾는다. 볼 수 있는 것만 나온다.
#[utoipa::path(get, path = "/api/reference-candidates", params(CandidateQuery),
    responses((status = 200, body = [ReferenceCandidate]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn candidates(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<CandidateQuery>,
) -> Result<Json<Vec<ReferenceCandidate>>, AppError> {
    let q = query.q.trim();
    if q.chars().count() > state.config.limits.max_title_chars {
        return Err(AppError::BadRequest("query is too long".into()));
    }
    let mut tx = state.db.begin_as(user).await?;
    if let Some(ws) = query.workspace_id {
        crate::features::workspaces::member_role(&mut tx, ws).await?;
    }
    let ws = query.workspace_id;
    let pattern = like_pattern(q);
    let mut out = Vec::new();

    if let Some((key, number)) = parse_reference(q) {
        out.extend(to_candidates(
            by_reference(&mut tx, ws, &key, number).await?,
        ));
    }

    // 사람: 나와 워크스페이스를 함께 쓰는 사람(users RLS). 이메일은 내보내지 않는다.
    let people: Vec<CandidateRow> = sqlx::query_as(
        "SELECT DISTINCT 'user', u.id, u.display_name, NULL::text, NULL::uuid, NULL::text
         FROM workspace_members m JOIN users u ON u.id = m.user_id
         WHERE ($1::uuid IS NULL OR m.workspace_id = $1) AND u.display_name ILIKE $2
         ORDER BY 3 LIMIT $3",
    )
    .bind(ws)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(people));

    let items: Vec<CandidateRow> = sqlx::query_as(
        "SELECT 'item', i.id, i.title, app_item_reference(i.id), i.workspace_id, w.name
         FROM items i JOIN workspaces w ON w.id = i.workspace_id
         WHERE ($1::uuid IS NULL OR i.workspace_id = $1) AND i.title ILIKE $2
         ORDER BY i.updated_at DESC LIMIT $3",
    )
    .bind(ws)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(items));

    // 문서: 워크스페이스 문서와(범위를 정하지 않았으면) 내가 볼 수 있는 개인 노트.
    let docs: Vec<CandidateRow> = sqlx::query_as(
        "SELECT 'document', d.id, d.title, app_document_reference(d.id), d.workspace_id, w.name
         FROM documents d LEFT JOIN workspaces w ON w.id = d.workspace_id
         WHERE (CASE WHEN $1::uuid IS NULL THEN true ELSE d.workspace_id = $1 END)
           AND d.title ILIKE $2
         ORDER BY d.updated_at DESC LIMIT $3",
    )
    .bind(ws)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(docs));
    tx.commit().await?;

    // 참조 번호로 찾은 것이 제목 검색에도 걸리면 한 번만 보여 준다.
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert((c.kind, c.id)));
    Ok(Json(out))
}

/// 참조 해석. 볼 수 없는 대상과 없는 대상은 같은 모양으로 돌려준다.
#[utoipa::path(post, path = "/api/references/resolve", request_body = ResolveReferences,
    responses((status = 200, body = [ResolvedReference]), (status = 400, body = ErrorBody)), tag = "references")]
pub async fn resolve(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ResolveReferences>,
) -> Result<Json<Vec<ResolvedReference>>, AppError> {
    if input.refs.len() > state.config.limits.max_references_per_document {
        return Err(AppError::BadRequest(format!(
            "at most {} references per request",
            state.config.limits.max_references_per_document
        )));
    }
    let mut tx = state.db.begin_as(user).await?;
    let mut out = Vec::with_capacity(input.refs.len());
    for r in &input.refs {
        let resolved = match r.kind {
            ReferenceKind::Item => {
                let row: Option<(String, Option<String>, WorkspaceId)> = sqlx::query_as(
                    "SELECT title, app_item_reference(id), workspace_id FROM items WHERE id = $1",
                )
                .bind(r.id)
                .fetch_optional(tx.conn())
                .await?;
                row.map(|(title, reference, ws)| ResolvedReference {
                    kind: r.kind,
                    id: r.id,
                    visible: true,
                    title: Some(title),
                    reference,
                    workspace_id: Some(ws),
                    item_id: None,
                })
            }
            ReferenceKind::Document => {
                let row: Option<DocumentRefRow> = sqlx::query_as(
                    "SELECT title, app_document_reference(id), workspace_id,
                                CASE WHEN app_item_access(item_id) IS NOT NULL THEN item_id END
                         FROM documents WHERE id = $1",
                )
                .bind(r.id)
                .fetch_optional(tx.conn())
                .await?;
                row.map(|(title, reference, ws, item)| ResolvedReference {
                    kind: r.kind,
                    id: r.id,
                    visible: true,
                    title: Some(title),
                    reference,
                    workspace_id: ws,
                    item_id: item,
                })
            }
            ReferenceKind::User => {
                let name: Option<String> =
                    sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
                        .bind(r.id)
                        .fetch_optional(tx.conn())
                        .await?;
                name.map(|title| ResolvedReference {
                    kind: r.kind,
                    id: r.id,
                    visible: true,
                    title: Some(title),
                    reference: None,
                    workspace_id: None,
                    item_id: None,
                })
            }
        };
        out.push(resolved.unwrap_or_else(|| ResolvedReference::hidden(r)));
    }
    tx.commit().await?;
    Ok(Json(out))
}

type DocumentRefRow = (String, Option<String>, Option<WorkspaceId>, Option<ItemId>);

type BacklinkRow = (
    DocumentId,
    Option<String>,
    String,
    Option<WorkspaceId>,
    Option<ItemId>,
);

async fn backlinks_to(
    tx: &mut UserTx,
    kind: &str,
    target: Uuid,
) -> Result<Vec<Backlink>, AppError> {
    // document_links와 documents 모두 RLS로 출발 문서를 볼 수 있는 것만 남는다.
    let rows: Vec<BacklinkRow> = sqlx::query_as(
        "SELECT d.id, app_document_reference(d.id), d.title, d.workspace_id,
                CASE WHEN app_item_access(d.item_id) IS NOT NULL THEN d.item_id END
         FROM document_links l JOIN documents d ON d.id = l.document_id
         WHERE l.target_kind = $1 AND l.target_id = $2
         ORDER BY d.updated_at DESC",
    )
    .bind(kind)
    .bind(target)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, reference, title, workspace_id, item_id)| Backlink {
            id,
            reference,
            title,
            workspace_id,
            item_id,
        })
        .collect())
}

/// 이 문서를 가리키는 문서. 대상 문서를 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/documents/{document_id}/backlinks",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Backlink]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn document_backlinks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Vec<Backlink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let links = backlinks_to(&mut tx, "document", id.0).await?;
    tx.commit().await?;
    Ok(Json(links))
}

/// 이 항목을 가리키는 문서. 항목을 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/backlinks",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Backlink]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn item_backlinks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<ItemId>,
) -> Result<Json<Vec<Backlink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_item_access($1) IS NOT NULL")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let links = backlinks_to(&mut tx, "item", id.0).await?;
    tx.commit().await?;
    Ok(Json(links))
}

type NotificationRow = (
    Uuid,
    String,
    DocumentId,
    String,
    OffsetDateTime,
    Option<OffsetDateTime>,
    String,
    Option<String>,
    Option<WorkspaceId>,
    Option<ItemId>,
);

/// 내 알림(최근 순). 문서를 볼 수 없게 되면 나오지 않는다.
#[utoipa::path(get, path = "/api/me/notifications",
    responses((status = 200, body = [Notification]), (status = 401, body = ErrorBody)), tag = "references")]
pub async fn notifications(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<Notification>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT n.id, n.kind, n.document_id, n.actor_name, n.created_at, n.read_at,
                d.title, app_document_reference(d.id), d.workspace_id,
                CASE WHEN app_item_access(d.item_id) IS NOT NULL THEN d.item_id END
         FROM app_my_notifications($1) n JOIN documents d ON d.id = n.document_id
         ORDER BY n.created_at DESC, n.id DESC",
    )
    .bind(NOTIFICATION_PAGE)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(
                    id,
                    kind,
                    document_id,
                    actor_name,
                    created_at,
                    read_at,
                    document_title,
                    document_reference,
                    workspace_id,
                    item_id,
                )| Notification {
                    id,
                    kind,
                    document_id,
                    document_title,
                    document_reference,
                    workspace_id,
                    item_id,
                    actor_name,
                    created_at,
                    read: read_at.is_some(),
                },
            )
            .collect(),
    ))
}

/// 알림 읽음 표시. 내 알림만 바뀐다.
#[utoipa::path(post, path = "/api/me/notifications/read", request_body = MarkRead,
    responses((status = 204), (status = 401, body = ErrorBody)), tag = "references")]
pub async fn mark_read(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<MarkRead>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_mark_notifications_read($1)")
        .bind(input.id)
        .execute(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_queries_are_parsed_case_insensitively() {
        assert_eq!(parse_reference("dev-3"), Some(("DEV".into(), 3)));
        assert_eq!(parse_reference(" WIKI-12 "), Some(("WIKI".into(), 12)));
        for bad in ["DEV", "DEV-", "DEV-0", "D-1", "1A-2", "회의-1", "DEV-x"] {
            assert_eq!(parse_reference(bad), None, "{bad}");
        }
    }

    #[test]
    fn like_patterns_escape_wildcards() {
        assert_eq!(like_pattern("50%_a\\"), "%50\\%\\_a\\\\%");
    }
}
