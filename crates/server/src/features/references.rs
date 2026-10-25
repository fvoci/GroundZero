//! @ 링크·사용자 언급·역방향 링크·알림 (D-18, D-27, D-39).
//!
//! 본문의 참조 노드는 종류와 내부 id만 가진다. 표시 이름은 여기서 권한을 확인해 만든다.
//! 볼 수 없는 대상과 없는 대상은 같은 모양(`visible: false`)으로 돌려준다(존재 여부를 드러내지 않는다).
//! 링크·언급 기록과 알림 생성은 저장 트랜잭션 안의 DB 함수 `app_record_document_references`가 한다
//! (`migrations/202610250001_initial_state.sql`). 새 UUIDv7은 앱이 공급하며(D-88), 언급은 접근 권한을 주지 않는다.

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
use crate::ids::{DocumentId, ItemId, OrganizationId, WorkspaceId};

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

/// 알림. invite는 초대 ID만 있고 대상 제목·참조는 없다.
#[derive(Debug, Serialize, ToSchema)]
pub struct Notification {
    pub organization_id: Option<OrganizationId>,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub invitation_id: Option<Uuid>,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// `mention`(문서에서 언급)·`comment`(내 문서·업무에 댓글)·`reply`(참여한 스레드에 답글)·`due`(업무 기한: 내일·오늘)·
    /// `expiry`(강좌 수강권 만료·목표 완료일)·`review`(복습일)·`deadline`(투고 제출·수정 마감, 산출물 마감).
    pub kind: String,
    /// 대상 문서(문서 알림).
    pub document_id: Option<DocumentId>,
    /// 대상 업무(업무 알림).
    #[schema(value_type = Option<String>, format = Uuid)]
    pub task_id: Option<Uuid>,
    /// 대상 제목(문서 또는 업무).
    pub document_title: Option<String>,
    /// 대상 번호(문서 또는 업무).
    pub document_reference: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
    pub item_id: Option<ItemId>,
    /// 댓글·답글이면 그 댓글.
    #[schema(value_type = Option<String>, format = Uuid)]
    pub comment_id: Option<Uuid>,
    /// 댓글 첫 부분(최대 140자).
    pub excerpt: Option<String>,
    /// 기한·만료·복습·마감 알림의 날짜(YYYY-MM-DD).
    pub due_date: Option<String>,
    /// 항목 알림의 대상 이름(기록 제목이나 강좌 이름).
    pub subject: Option<String>,
    /// 한 사람. 워크스페이스를 함께 쓰지 않거나(공유받은 개인 노트 등) 자동 알림(기한)이면 없다.
    pub actor_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub read: bool,
}

/// 읽음 표시 요청. `id`가 없으면 모두 읽음.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkRead {
    pub organization_id: Option<OrganizationId>,
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
                CASE WHEN app_access('item', d.item_id) <> 'none' THEN d.item_id END
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

/// 이 업무를 가리키는 문서. 업무를 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/tasks/{task_id}/backlinks",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Backlink]), (status = 404, body = ErrorBody)), tag = "references")]
pub async fn task_backlinks(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Backlink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_access('task', $1) <> 'none'")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let links = backlinks_to(&mut tx, "task", id).await?;
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
    let visible: bool = sqlx::query_scalar("SELECT app_access('item', $1) <> 'none'")
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
    Option<OrganizationId>,
    Option<Uuid>,
    String,
    Option<DocumentId>,
    Option<Uuid>,
    Option<Uuid>,
    Option<String>,
    serde_json::Value,
    OffsetDateTime,
    Option<OffsetDateTime>,
    Option<String>,
    Option<String>,
    Option<WorkspaceId>,
    Option<ItemId>,
);

/// 알림 범위와 페이지 커서.
#[derive(Debug, Deserialize, IntoParams)]
pub struct NotificationQuery {
    pub organization: Option<OrganizationId>,
    pub limit: Option<i32>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[param(value_type = Option<String>, format = DateTime)]
    pub before: Option<OffsetDateTime>,
    pub before_id: Option<Uuid>,
}

/// 내 알림(최근 순). 대상을 볼 수 없게 되면 나오지 않는다.
#[utoipa::path(get, path = "/api/me/notifications", params(NotificationQuery),
    responses((status = 200, body = [Notification]), (status = 401, body = ErrorBody)), tag = "references")]
pub async fn notifications(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<NotificationQuery>,
) -> Result<Json<Vec<Notification>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<NotificationRow> = sqlx::query_as(
        "SELECT n.id, n.organization_id, n.invitation_id, n.kind, n.document_id, n.task_id, n.comment_id, n.actor_name, n.detail, n.created_at, n.read_at,
                coalesce(d.title, t.title, i.title),
                CASE WHEN d.id IS NOT NULL THEN app_document_reference(d.id)
                     WHEN t.id IS NOT NULL THEN app_task_reference(t.id)
                     ELSE app_item_reference(i.id) END,
                coalesce(d.workspace_id, t.workspace_id, i.workspace_id),
                CASE WHEN app_access('item', coalesce(d.item_id, t.item_id, i.id)) <> 'none'
                     THEN coalesce(d.item_id, t.item_id, i.id) END
         FROM app_my_notifications($1, $2, $3, $4) n
         LEFT JOIN documents d ON d.id = n.document_id
         LEFT JOIN tasks t ON t.id = n.task_id
         LEFT JOIN items i ON i.id = n.item_id
         ORDER BY n.created_at DESC, n.id DESC",
    )
    .bind(query.organization)
    .bind(query.limit.unwrap_or(NOTIFICATION_PAGE).clamp(1, NOTIFICATION_PAGE))
    .bind(query.before)
    .bind(query.before_id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(
                    id,
                    organization_id,
                    invitation_id,
                    kind,
                    document_id,
                    task_id,
                    comment_id,
                    actor_name,
                    detail,
                    created_at,
                    read_at,
                    title,
                    reference,
                    workspace_id,
                    item_id,
                )| Notification {
                    id,
                    organization_id,
                    invitation_id,
                    kind,
                    document_id,
                    task_id,
                    document_title: title,
                    document_reference: reference,
                    workspace_id,
                    item_id,
                    comment_id,
                    excerpt: detail
                        .get("excerpt")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    due_date: detail
                        .get("due_date")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    subject: detail
                        .get("subject")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
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
    sqlx::query("SELECT app_mark_notifications_read($1, $2)")
        .bind(input.id)
        .bind(input.organization_id)
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
