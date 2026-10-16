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
    /// 제목·이름 일부 또는 참조(`DEV-3`, 예전 키도 됨). `워크스페이스이름/질의`로 쓰면 내가 속한 워크스페이스 중
    /// 이름이 그렇게 시작하는 곳(같은 이름이면 모두)에서 찾는다.
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
    /// 언급한 사람. 워크스페이스를 함께 쓰지 않으면(공유받은 개인 노트 등) 없을 수 있다.
    pub actor_name: Option<String>,
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

/// 검색 범위: 지정한 워크스페이스(멤버여야 함) 또는 내가 속한 모든 워크스페이스.
/// 이 목록으로 먼저 좁혀 RLS 판정 함수가 다른 테넌트의 행에 돌지 않게 한다.
async fn search_scope(
    tx: &mut UserTx,
    ws: Option<WorkspaceId>,
) -> Result<Vec<WorkspaceId>, AppError> {
    if let Some(ws) = ws {
        crate::features::workspaces::member_role(tx, ws).await?;
        return Ok(vec![ws]);
    }
    let ids: Vec<WorkspaceId> = sqlx::query_scalar("SELECT app_my_workspace_ids()")
        .fetch_all(tx.conn())
        .await?;
    Ok(ids)
}

/// 내가 속한 워크스페이스 중 이름이 `prefix`로 시작하는 곳(대소문자 무시). 같은 이름이면 모두 돌려준다.
async fn workspaces_named(tx: &mut UserTx, prefix: &str) -> Result<Vec<WorkspaceId>, AppError> {
    let mut pattern = like_pattern(prefix);
    pattern.remove(0); // 앞쪽 %를 빼 접두어 일치로 만든다.
    let ids: Vec<WorkspaceId> = sqlx::query_scalar(
        "SELECT w.id FROM workspaces w
         WHERE w.id IN (SELECT app_my_workspace_ids()) AND w.name ILIKE $1",
    )
    .bind(pattern)
    .fetch_all(tx.conn())
    .await?;
    Ok(ids)
}

/// 참조 번호로 찾기. 예전 키도 별칭으로 찾는다 (D-18). 볼 수 없는 대상은 RLS로 빠진다.
async fn by_reference(
    tx: &mut UserTx,
    scope: &[WorkspaceId],
    key: &str,
    number: i64,
) -> Result<Vec<CandidateRow>, AppError> {
    let rows: Vec<CandidateRow> = sqlx::query_as(
        "WITH scopes AS (
           SELECT w.id AS workspace_id, w.id AS scope_id FROM workspaces w
           WHERE $2 = 'WIKI' AND w.id = ANY($1)
           UNION
           SELECT k.workspace_id, k.module_id FROM module_keys k
           WHERE k.key = $2 AND k.workspace_id = ANY($1)
         )
         SELECT n.target_kind, n.target_id,
                coalesce(i.title, d.title),
                CASE n.target_kind WHEN 'item' THEN app_item_reference(n.target_id)
                                   ELSE app_document_reference(n.target_id) END,
                n.workspace_id, w.name
         FROM scopes s
         JOIN numbered_targets n ON n.scope_id = s.scope_id AND n.number = $3
         JOIN workspaces w ON w.id = n.workspace_id
         LEFT JOIN items i ON n.target_kind = 'item' AND i.id = n.target_id
         LEFT JOIN documents d ON n.target_kind = 'document' AND d.id = n.target_id
         WHERE coalesce(i.title, d.title) IS NOT NULL",
    )
    .bind(scope)
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
    // `이름/질의`: 내가 속한 워크스페이스 중 이름이 그렇게 시작하는 곳에서 찾는다(D-18의 `@A/DEV-1`).
    // 이름은 중복될 수 있으므로(D-17) 맞는 곳을 모두 범위에 넣고, 결과의 워크스페이스로 사용자가 구분해 고른다.
    let (scope, q, named) = match q.split_once('/') {
        Some((name, rest)) => (
            workspaces_named(&mut tx, name.trim()).await?,
            rest.trim(),
            true,
        ),
        None => (search_scope(&mut tx, query.workspace_id).await?, q, false),
    };
    let pattern = like_pattern(q);
    let mut out = Vec::new();

    if let Some((key, number)) = parse_reference(q) {
        out.extend(to_candidates(
            by_reference(&mut tx, &scope, &key, number).await?,
        ));
    }

    // 사람: 범위 안 워크스페이스의 멤버. 이메일은 내보내지 않는다.
    let people: Vec<CandidateRow> = sqlx::query_as(
        "SELECT DISTINCT 'user', u.id, u.display_name, NULL::text, NULL::uuid, NULL::text
         FROM workspace_members m JOIN users u ON u.id = m.user_id
         WHERE m.workspace_id = ANY($1) AND u.display_name ILIKE $2
         ORDER BY 3 LIMIT $3",
    )
    .bind(&scope)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(people));

    let items: Vec<CandidateRow> = sqlx::query_as(
        "SELECT 'item', i.id, i.title, app_item_reference(i.id), i.workspace_id, w.name
         FROM items i JOIN workspaces w ON w.id = i.workspace_id
         WHERE i.workspace_id = ANY($1) AND i.title ILIKE $2
         ORDER BY i.updated_at DESC LIMIT $3",
    )
    .bind(&scope)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(items));

    let docs: Vec<CandidateRow> = sqlx::query_as(
        "SELECT 'document', d.id, d.title, app_document_reference(d.id), d.workspace_id, w.name
         FROM documents d JOIN workspaces w ON w.id = d.workspace_id
         WHERE d.workspace_id = ANY($1) AND d.title ILIKE $2
         ORDER BY d.updated_at DESC LIMIT $3",
    )
    .bind(&scope)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(docs));

    // 범위를 정하지 않았으면(개인 노트에서 입력) 내 개인 노트도 찾는다.
    if query.workspace_id.is_none() && !named {
        let notes: Vec<CandidateRow> = sqlx::query_as(
            "SELECT 'document', d.id, d.title, NULL::text, NULL::uuid, NULL::text
             FROM documents d
             WHERE d.owner_user_id = $1 AND d.title ILIKE $2
             ORDER BY d.updated_at DESC LIMIT $3",
        )
        .bind(user)
        .bind(&pattern)
        .bind(CANDIDATES_PER_KIND)
        .fetch_all(tx.conn())
        .await?;
        out.extend(to_candidates(notes));
    }
    tx.commit().await?;

    // 참조 번호로 찾은 것이 제목 검색에도 걸리면 한 번만 보여 준다.
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert((c.kind, c.id)));
    Ok(Json(out))
}

/// 한 번에 해석할 수 있는 참조 수.
const MAX_RESOLVE: usize = 100;

type ResolveRow = (
    String,
    Uuid,
    String,
    Option<String>,
    Option<WorkspaceId>,
    Option<ItemId>,
);

/// 참조 해석. 볼 수 없는 대상과 없는 대상은 같은 모양으로 돌려준다. 한 번의 질의로 처리한다.
#[utoipa::path(post, path = "/api/references/resolve", request_body = ResolveReferences,
    responses((status = 200, body = [ResolvedReference]), (status = 400, body = ErrorBody)), tag = "references")]
pub async fn resolve(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ResolveReferences>,
) -> Result<Json<Vec<ResolvedReference>>, AppError> {
    if input.refs.len() > MAX_RESOLVE {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_RESOLVE} references per request"
        )));
    }
    let ids = |kind: ReferenceKind| -> Vec<Uuid> {
        input
            .refs
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.id)
            .collect()
    };
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<ResolveRow> = sqlx::query_as(
        "SELECT 'item', i.id, i.title, app_item_reference(i.id), i.workspace_id, NULL::uuid
         FROM items i WHERE i.id = ANY($1)
         UNION ALL
         SELECT 'document', d.id, d.title, app_document_reference(d.id), d.workspace_id,
                CASE WHEN app_item_access(d.item_id) IS NOT NULL THEN d.item_id END
         FROM documents d WHERE d.id = ANY($2)
         UNION ALL
         SELECT 'user', u.id, u.display_name, NULL::text, NULL::uuid, NULL::uuid
         FROM users u WHERE u.id = ANY($3)",
    )
    .bind(ids(ReferenceKind::Item))
    .bind(ids(ReferenceKind::Document))
    .bind(ids(ReferenceKind::User))
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;

    let mut found = std::collections::HashMap::new();
    for (kind, id, title, reference, workspace_id, item_id) in rows {
        if let Some(kind) = ReferenceKind::parse(&kind) {
            found.insert((kind, id), (title, reference, workspace_id, item_id));
        }
    }
    Ok(Json(
        input
            .refs
            .iter()
            .map(|r| match found.get(&(r.kind, r.id)) {
                Some((title, reference, workspace_id, item_id)) => ResolvedReference {
                    kind: r.kind,
                    id: r.id,
                    visible: true,
                    title: Some(title.clone()),
                    reference: reference.clone(),
                    workspace_id: *workspace_id,
                    item_id: *item_id,
                },
                None => ResolvedReference::hidden(r),
            })
            .collect(),
    ))
}

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
    Option<String>,
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

/// 공개 보기용: 참조 노드를 자리표시 글자 노드로 바꾼다(내부 id와 대상 존재를 익명 방문자에게 숨긴다).
/// 저장된 본문은 serde_json의 중첩 한도 안에서 읽혔으므로 재귀 깊이가 제한된다.
pub fn strip_references(node: &mut serde_json::Value) {
    use serde_json::Value;
    match node {
        Value::Array(items) => items.iter_mut().for_each(strip_references),
        Value::Object(map) => {
            if map.get("type").and_then(Value::as_str) == Some("reference") {
                *node = serde_json::json!({ "type": "text", "text": "[참조]" });
                return;
            }
            map.values_mut().for_each(strip_references);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_bodies_drop_reference_ids() {
        let id = "0198f1f2-0000-7000-8000-000000000001";
        let mut body = serde_json::json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
            { "type": "text", "text": "참고 " },
            { "type": "reference", "attrs": { "kind": "item", "id": id } }
        ] }] });
        strip_references(&mut body);
        assert!(!body.to_string().contains(id));
        assert_eq!(body["content"][0]["content"][1]["text"], "[참조]");
    }

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
