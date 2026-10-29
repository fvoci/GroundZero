//! 다영역 참조 후보 검색과 일괄 해석(D-18).

use axum::Json;
use axum::extract::{Query, State};
use fvoci_http::session::{CurrentUser, EXPECTED_USER_IF_SENT_DOC};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::state::ReferencesState;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_db::UserTx;

/// 종류별 후보 수 상한.
const CANDIDATES_PER_KIND: i64 = 8;

/// 참조 대상 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    Item,
    Document,
    Task,
    User,
}

impl ReferenceKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "task" => Some(Self::Task),
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
    #[schema(max_items = 100)]
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
        fvoci_db::access::require_visible(tx, fvoci_db::access::Kind::Workspace, ws).await?;
        return Ok(vec![ws]);
    }
    let ids: Vec<WorkspaceId> = sqlx::query_scalar("SELECT app_my_workspace_ids(NULL::uuid)")
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
         WHERE w.id IN (SELECT app_my_workspace_ids(NULL::uuid)) AND w.name ILIKE $1",
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
                coalesce(i.title, d.title, tk.title),
                CASE n.target_kind WHEN 'item' THEN app_item_reference(n.target_id)
                                   WHEN 'task' THEN app_task_reference(n.target_id)
                                   ELSE app_document_reference(n.target_id) END,
                n.workspace_id, w.name
         FROM scopes s
         JOIN numbered_targets n ON n.scope_id = s.scope_id AND n.number = $3
         JOIN workspaces w ON w.id = n.workspace_id
         LEFT JOIN items i ON n.target_kind = 'item' AND i.id = n.target_id
         LEFT JOIN documents d ON n.target_kind = 'document' AND d.id = n.target_id
         LEFT JOIN tasks tk ON n.target_kind = 'task' AND tk.id = n.target_id
         WHERE coalesce(i.title, d.title, tk.title) IS NOT NULL",
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
    responses((status = 200, body = [ReferenceCandidate]), (status = 404, body = ErrorBody),
              (status = 400, body = ErrorBody)), tag = "references")]
pub async fn candidates(
    State(state): State<ReferencesState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<CandidateQuery>,
) -> Result<Json<Vec<ReferenceCandidate>>, AppError> {
    let q = query.q.trim();
    if q.chars().count() > state.max_title_chars {
        return Err(AppError::invalid(
            "q",
            InvalidReason::TooLong,
            u64::try_from(state.max_title_chars).ok(),
            "query is too long",
        ));
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

    let tasks: Vec<CandidateRow> = sqlx::query_as(
        "SELECT 'task', t.id, t.title, app_task_reference(t.id), t.workspace_id, w.name
         FROM tasks t JOIN workspaces w ON w.id = t.workspace_id
         WHERE t.workspace_id = ANY($1) AND t.title ILIKE $2
         ORDER BY t.updated_at DESC LIMIT $3",
    )
    .bind(&scope)
    .bind(&pattern)
    .bind(CANDIDATES_PER_KIND)
    .fetch_all(tx.conn())
    .await?;
    out.extend(to_candidates(tasks));

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
#[utoipa::path(post,
    params(("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = EXPECTED_USER_IF_SENT_DOC)), path = "/api/references/resolve", request_body = ResolveReferences,
    responses((status = 200, body = [ResolvedReference]), (status = 400, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "references")]
pub async fn resolve(
    State(state): State<ReferencesState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<ResolveReferences>,
) -> Result<Json<Vec<ResolvedReference>>, AppError> {
    if input.refs.len() > MAX_RESOLVE {
        return Err(AppError::invalid(
            "refs",
            InvalidReason::TooMany,
            u64::try_from(MAX_RESOLVE).ok(),
            format!("at most {MAX_RESOLVE} references per request"),
        ));
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
                CASE WHEN app_access('item', d.item_id) <> 'none' THEN d.item_id END
         FROM documents d WHERE d.id = ANY($2)
         UNION ALL
         SELECT 'user', u.id, u.display_name, NULL::text, NULL::uuid, NULL::uuid
         FROM users u WHERE u.id = ANY($3)
         UNION ALL
         SELECT 'task', t.id, t.title, app_task_reference(t.id), t.workspace_id, t.item_id
         FROM tasks t WHERE t.id = ANY($4)",
    )
    .bind(ids(ReferenceKind::Item))
    .bind(ids(ReferenceKind::Document))
    .bind(ids(ReferenceKind::User))
    .bind(ids(ReferenceKind::Task))
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
