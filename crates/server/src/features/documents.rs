//! 문서: 위키 문서와 개인 노트, 저장, 공유, 복제, 이동.
//!
//! 한 문서 엔진을 쓴다. 문서는 워크스페이스 소유(위키) 또는 사용자 소유(개인 노트) 중 하나다 (D-59).
//! 접근 판정은 DB 함수 `app_document_access`(owner·write·read·없음) 하나로 하고, RLS와 이 모듈이 같은 함수를
//! 쓴다(`migrations/0004_personal_notes_sharing.sql`).
//!
//! 저장 경계 (D-62): 본문은 Tiptap JSON(`schema_version` 1)이며 저장할 때마다 리비전이 하나 늘어난다.
//! 이것은 JSON 리비전 저장이고 공동 편집이 아니다. CRDT 도입 시 상태 열을 `body` 옆에 추가하며
//! 문서 id·번호·리비전 번호는 그대로 이어진다.
//! 충돌 (D-53): 저장 요청의 `base_revision`이 현재 리비전과 다르면 409와 함께 서버 최신본을 돌려준다.
//! 클라이언트는 초안을 버리지 않는다. 같은 `request_id`의 재시도는 이미 만든 리비전을 돌려준다.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::config::Limits;
use crate::db::{UserTx, is_unique_violation, needed_ids, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::{CurrentUser, ExpectedUser};
use crate::features::workspaces::{member_role, validate_title};
use crate::http::AppState;
use crate::http::cache;
use crate::ids::{DocumentId, ItemId, UserId, WorkspaceId};

/// 현재 본문 스키마 버전.
pub const SCHEMA_VERSION: i32 = 1;

/// 문서에 대한 현재 사용자의 권한.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// 개인 노트 소유자 또는 워크스페이스 관리자. 설정·공유·이동을 할 수 있다.
    Owner,
    Write,
    Read,
}

impl Access {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(Self::Owner),
            "write" => Some(Self::Write),
            "read" => Some(Self::Read),
            _ => None,
        }
    }

    pub(crate) fn can_write(self) -> bool {
        matches!(self, Self::Owner | Self::Write)
    }
}

/// 현재 사용자의 권한. 볼 수 없으면 `None`.
pub async fn access(tx: &mut UserTx, id: DocumentId) -> Result<Option<Access>, AppError> {
    let raw: Option<String> = sqlx::query_scalar("SELECT app_document_access($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    Ok(raw.as_deref().and_then(Access::parse))
}

/// 보관 상한을 뺀 권한. 노출을 줄이는 관리 화면(공유·공개 링크 목록)에만 쓴다.
pub async fn manager_access(tx: &mut UserTx, id: DocumentId) -> Result<Option<Access>, AppError> {
    let raw: Option<String> = sqlx::query_scalar("SELECT app_document_base_access($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    Ok(raw.as_deref().and_then(Access::parse))
}

/// 문서 목록 항목.
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentSummary {
    pub id: DocumentId,
    /// 사람이 보는 참조. 예: `WIKI-3`. 개인 노트는 없다.
    pub reference: Option<String>,
    pub title: String,
    pub revision: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    /// 트리의 부모. 없거나 볼 수 없으면 없다(최상위로 보인다).
    pub parent_id: Option<DocumentId>,
}

/// 문서 전체.
#[derive(Debug, Serialize, ToSchema)]
pub struct Document {
    pub id: DocumentId,
    /// 워크스페이스 문서면 소유 워크스페이스. 개인 노트는 없다.
    pub workspace_id: Option<WorkspaceId>,
    /// 모듈 항목 아래 문서면 그 항목 (D-59).
    pub item_id: Option<ItemId>,
    /// 트리의 부모. 없거나 볼 수 없으면 없다.
    pub parent_id: Option<DocumentId>,
    /// `wiki`, `module` 또는 `personal`.
    pub placement: String,
    pub reference: Option<String>,
    pub title: String,
    /// Tiptap JSON. `type`이 `doc`인 객체.
    #[schema(value_type = Object)]
    pub body: Value,
    pub schema_version: i32,
    pub revision: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    pub access: Access,
    /// 현재 사용자가 고칠 수 있는지.
    pub can_edit: bool,
    /// `workspace` 또는 `private` (D-20, D-40). 개인 노트는 항상 `private`.
    pub visibility: String,
    /// 소유 권한자 외의 사람이 복제할 수 있는지 (D-20).
    pub allow_copy: bool,
    /// 공유 철회·공개 링크 철회·비공개 전환처럼 노출을 줄이는 관리를 할 수 있는지.
    /// 보관된 항목 아래 문서는 `access`가 `read`여도 소유 권한자는 이 값이 참이다.
    pub can_manage: bool,
    /// 휴지통으로 옮길 수 있는지(문서 소유 권한, 또는 쓰기 권한이 있고 내가 만든 문서).
    pub can_delete: bool,
    /// 템플릿으로 표시됨(D-27). 읽을 수 있는 사람이 새 문서의 본문으로 쓴다.
    pub is_template: bool,
    /// 실시간 공동 편집 중인 문서(워크스페이스가 켬, D-70). 본문은 실시간 연결로 고치고 저장 API는 제목만 바꾼다.
    pub realtime: bool,
    /// 이 사람이 내보낼 수 있는지(D-75). 첨부 내려받기와 같은 규칙: 고칠 수 있거나, 볼 수 있고 복제·내려받기 허용.
    pub can_export: bool,
}

/// 문서 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub title: String,
    /// 이 문서 아래에 만든다(같은 자리의 문서). 없으면 최상위 맨 끝.
    #[serde(default)]
    pub parent_id: Option<DocumentId>,
    /// 이 템플릿의 본문으로 시작한다(D-27). 읽을 수 있는 템플릿이어야 한다.
    #[serde(default)]
    #[schema(value_type = Option<String>, format = Uuid)]
    pub template_id: Option<DocumentId>,
}

/// 문서 저장 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    /// 편집을 시작한 리비전.
    pub base_revision: i32,
    pub title: String,
    #[schema(value_type = Object)]
    pub body: Value,
}

/// 저장 충돌 응답. `current`가 서버 최신본이다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ConflictBody {
    /// `conflict`(409, `base_revision`이 지남) 또는 `precondition_failed`(412, `If-Match`가 맞지 않음).
    pub error: &'static str,
    pub current: Document,
    /// `current`를 이 사람이 단건 조회했을 때의 ETag(D-77). 서버 최신본을 받아들인 뒤 `If-Match`로 쓴다.
    pub etag: String,
}

/// 문서 저장의 409 본문(계약 문서용). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `ConflictBody`(`error: "conflict"`, `current`): `base_revision`이 지남.
/// - `ErrorBody`(`current` 없음): `account_changed`(기대 사용자가 세션의 사용자와 다름), `invalid_state`(실시간 편집
///   중 본문 변경, 동시 변경 재시도 등).
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum SaveConflict {
    Stale(Box<ConflictBody>),
    State(ErrorBody),
}

/// 기대 사용자 머리의 계약 설명(OpenAPI).
pub(crate) const EXPECTED_USER_DOC: &str = "이 편집·초안을 시작한 사용자 id(필수). 서버는 세션의 사용자와 비교만 한다: 다르면 \
    아무것도 바꾸지 않고 409 `account_changed`, 없거나 형식이 틀리면 400. 신원·권한은 세션 쿠키로만 정하며 이 값으로 \
    아무것도 허용하지 않는다. 적용 범위는 사용자 초안을 들고 자동 저장·재시도하거나 그 초안을 보존하는 쓰기(`PUT /api/documents/{document_id}`, \
    `POST /api/me/notes`), 업로드 시작(`POST /api/uploads`, 만료된 업로드를 스스로 다시 시작함)과 실시간 편집 연결(같은 값을 질의 `expected_user`로, 다르면 닫기 코드 4412)이고 다른 경로는 \
    이 값을 읽지 않는다";

pub(crate) fn empty_body() -> Value {
    serde_json::json!({ "type": "doc", "content": [{ "type": "paragraph" }] })
}

/// 새 문서의 본문: 템플릿을 주면 그 본문(읽을 수 있는 템플릿이어야 함, 아니면 404), 아니면 빈 문서.
pub(crate) async fn initial_body(
    tx: &mut UserTx,
    template: Option<DocumentId>,
) -> Result<Value, AppError> {
    let Some(template) = template else {
        return Ok(empty_body());
    };
    // 문서 RLS가 읽을 수 없는 템플릿을 거른다.
    let body: Option<Value> = sqlx::query_scalar(
        "SELECT body FROM documents WHERE id = $1 AND is_template AND deleted_at IS NULL",
    )
    .bind(template)
    .fetch_optional(tx.conn())
    .await?;
    body.ok_or(AppError::NotFound)
}

/// 본문 검증: `type: "doc"` 객체이고 직렬화 크기가 상한 이하.
pub fn validate_body(body: &Value, limits: &Limits) -> Result<(), AppError> {
    let is_doc = body
        .as_object()
        .and_then(|o| o.get("type"))
        .and_then(Value::as_str)
        == Some("doc");
    if !is_doc {
        return Err(AppError::BadRequest(
            "body must be a document object with type \"doc\"".into(),
        ));
    }
    let size = serde_json::to_vec(body)
        .map_err(|e| AppError::BadRequest(format!("body is not serializable: {e}")))?
        .len();
    if size > limits.max_document_bytes {
        return Err(AppError::BadRequest(format!(
            "body is {size} bytes; the limit is {}",
            limits.max_document_bytes
        )));
    }
    validate_references(body, limits.max_references_per_document)
}

/// 참조 노드(`type: "reference"`) 검사: `attrs.kind`는 item·document·user, `attrs.id`는 UUID,
/// 개수는 상한 이하. 표시 이름 같은 다른 속성은 받지 않는다(제목이 본문에 남지 않게, D-18).
/// 이미지 주소와 링크 주소도 같은 순회에서 확인한다(D-27). 깊은 중첩에서도 스택이 넘치지 않도록 반복으로 순회한다.
fn validate_references(body: &Value, max: usize) -> Result<(), AppError> {
    let mut stack = vec![body];
    let mut count = 0usize;
    while let Some(node) = stack.pop() {
        match node {
            Value::Array(items) => stack.extend(items),
            Value::Object(map) => {
                match map.get("type").and_then(Value::as_str) {
                    Some("image") => check_image_node(map)?,
                    Some("link") => check_link_mark(map)?,
                    _ => {}
                }
                if map.get("type").and_then(Value::as_str) == Some("reference") {
                    count += 1;
                    if count > max {
                        return Err(AppError::BadRequest(format!(
                            "a document can hold at most {max} references"
                        )));
                    }
                    check_reference_node(map)?;
                }
                stack.extend(map.values());
            }
            _ => {}
        }
    }
    Ok(())
}

/// 이미지 노드의 주소는 이 서비스의 첨부 파일 내용 주소만 받는다(D-27). 바깥 주소는 읽는 사람의 접속 정보를
/// 남에게 보내므로(추적 이미지) 받지 않는다.
fn check_image_node(node: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let src = node
        .get("attrs")
        .and_then(|a| a.get("src"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if is_file_url(src) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "images must point to an attachment of this service".into(),
        ))
    }
}

/// 첨부 주소의 표준형: `/api/files/{소문자 하이픈 id}/content` 뒤에 없음·`?inline=true`·`?version=N`·
/// `?version=N&inline=true`(N은 0으로 시작하지 않는 양의 32비트 정수). 화면(`fileContentUrl`)이 만드는 꼴이고,
/// DB가 리비전 본문에서 버전 참조를 뽑는 규칙(D-56)이 이 꼴을 빠짐없이 알아본다.
fn is_file_url(url: &str) -> bool {
    let Some((id, query)) = url
        .strip_prefix("/api/files/")
        .and_then(|rest| rest.split_once("/content"))
    else {
        return false;
    };
    let version_ok = |v: &str| {
        !v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit()) && v.parse::<i32>().is_ok()
    };
    Uuid::parse_str(id).is_ok_and(|u| u.hyphenated().to_string() == id)
        && match query {
            "" | "?inline=true" => true,
            q => q
                .strip_prefix("?version=")
                .is_some_and(|v| version_ok(v.strip_suffix("&inline=true").unwrap_or(v))),
        }
}

/// 링크 표시의 주소: 스킴이 있으면 http·https·mailto만(`javascript:`·`data:` 등 거부), 없으면 상대 경로·앵커.
fn check_link_mark(mark: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let href = mark
        .get("attrs")
        .and_then(|a| a.get("href"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let raw = href;
    let href = href.trim();
    let lower = href.to_ascii_lowercase();
    // 스킴이 있으면 http·https·mailto만, 없으면(상대 경로·#앵커) 다른 호스트로 가는 `//`만 막는다.
    let scheme = lower.split_once(':').map(|(s, _)| s).filter(|s| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    let ok = match scheme {
        Some(s) => matches!(s, "http" | "https" | "mailto"),
        None => !href.starts_with("//") && (!lower.starts_with("/api/files/") || is_file_url(raw)),
    };
    if ok && !href.chars().any(char::is_control) {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "links must be http, https, mailto or a relative path".into(),
        ))
    }
}

/// 참조 노드는 `type`·`attrs`(와 편집기가 붙이는 `marks`)만 가진다. 제목 같은 글자를 노드에 두지 않는다.
/// id는 하이픈이 있는 36자 표준 형식만 받는다(DB 추출 함수와 같은 규칙).
fn check_reference_node(node: &serde_json::Map<String, Value>) -> Result<(), AppError> {
    let bad = || AppError::BadRequest("reference nodes need attrs {kind, id} only".into());
    if node
        .keys()
        .any(|k| !matches!(k.as_str(), "type" | "attrs" | "marks"))
    {
        return Err(bad());
    }
    let attrs = node
        .get("attrs")
        .and_then(Value::as_object)
        .ok_or_else(bad)?;
    if attrs.len() != 2 {
        return Err(bad());
    }
    let kind = attrs.get("kind").and_then(Value::as_str);
    if !matches!(kind, Some("item" | "document" | "task" | "user")) {
        return Err(bad());
    }
    let id = attrs.get("id").and_then(Value::as_str).ok_or_else(bad)?;
    let canonical = id.len() == 36
        && id.char_indices().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
    if !canonical {
        return Err(bad());
    }
    Ok(())
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct DocRow {
    id: DocumentId,
    workspace_id: Option<WorkspaceId>,
    item_id: Option<ItemId>,
    parent_id: Option<DocumentId>,
    placement: String,
    reference: Option<String>,
    pub(crate) title: String,
    pub(crate) body: Value,
    schema_version: i32,
    pub(crate) revision: i32,
    updated_at: OffsetDateTime,
    visibility: String,
    allow_copy: bool,
    can_manage: bool,
    can_delete: bool,
    is_template: bool,
    pub(crate) realtime: bool,
    can_export: bool,
}

/// 참조 문자열은 저장하지 않고 현재 키로 만든다(키가 바뀌어도 번호·id는 그대로, D-18).
/// 항목 id는 그 항목을 볼 수 있는 사람에게만 준다(공유로만 문서를 보는 사람에게 비공개 항목을 드러내지 않는다).
const DOC_COLUMNS: &str = "id, workspace_id, \
    CASE WHEN app_item_access(item_id) IS NOT NULL THEN item_id END AS item_id, \
    CASE WHEN app_document_access(parent_id) IS NOT NULL THEN parent_id END AS parent_id, placement, \
    app_document_reference(id) AS reference, title, body, schema_version, revision, updated_at, \
    visibility, allow_copy, coalesce(app_document_base_access(id) = 'owner', false) AS can_manage, \
    coalesce(app_document_access(id) = 'owner' \
      OR (app_document_access(id) = 'write' AND created_by = app_user_id()), false) AS can_delete, \
    is_template, app_document_realtime(id) AS realtime, app_document_can_export(id) AS can_export";

pub(crate) fn to_document(row: DocRow, access: Access) -> Document {
    Document {
        id: row.id,
        workspace_id: row.workspace_id,
        item_id: row.item_id,
        parent_id: row.parent_id,
        reference: row.reference,
        placement: row.placement,
        title: row.title,
        body: row.body,
        schema_version: row.schema_version,
        revision: row.revision,
        updated_at: row.updated_at,
        access,
        can_edit: access.can_write(),
        visibility: row.visibility,
        allow_copy: row.allow_copy,
        can_manage: row.can_manage,
        can_delete: row.can_delete,
        is_template: row.is_template,
        realtime: row.realtime,
        can_export: row.can_export,
    }
}

async fn load(tx: &mut UserTx, id: DocumentId) -> Result<Option<DocRow>, AppError> {
    let row = sqlx::query_as(&format!(
        "SELECT {DOC_COLUMNS} FROM documents WHERE id = $1"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row)
}

/// 행을 잠그고 읽는다. 같은 문서의 저장은 직렬화되어 재시도 판단과 리비전 비교가 최신 행 기준이 된다.
pub(crate) async fn load_for_update(
    tx: &mut UserTx,
    id: DocumentId,
) -> Result<Option<DocRow>, AppError> {
    let row = sqlx::query_as(&format!(
        "SELECT {DOC_COLUMNS} FROM documents WHERE id = $1 FOR UPDATE"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row)
}

/// 문서와 권한을 함께 읽는다. 볼 수 없으면 404.
pub(crate) async fn load_with_access(
    tx: &mut UserTx,
    id: DocumentId,
) -> Result<(DocRow, Access), AppError> {
    let access = access(tx, id).await?.ok_or(AppError::NotFound)?;
    let row = load(tx, id).await?.ok_or(AppError::NotFound)?;
    Ok((row, access))
}

/// 목록 조회 열. `SummaryRow`와 순서가 같다.
/// 목록 조회 열. `SummaryRow`와 순서가 같다. 부모는 볼 수 있을 때만 준다(아니면 그 문서는 최상위로 보인다).
pub(crate) const SUMMARY_COLUMNS: &str = "id, app_document_reference(id) AS reference, title, revision, \
    updated_at, CASE WHEN app_document_access(parent_id) IS NOT NULL THEN parent_id END AS parent_id";

/// 트리 순서. 같은 부모 아래에서 위치, 같으면 번호·만든 순서.
pub(crate) const TREE_ORDER: &str = "position, number NULLS LAST, created_at, id";

pub(crate) type SummaryRow = (
    DocumentId,
    Option<String>,
    String,
    i32,
    OffsetDateTime,
    Option<DocumentId>,
);

pub(crate) fn to_summaries(rows: Vec<SummaryRow>) -> Vec<DocumentSummary> {
    rows.into_iter()
        .map(
            |(id, reference, title, revision, updated_at, parent_id)| DocumentSummary {
                id,
                reference,
                title,
                revision,
                updated_at,
                parent_id,
            },
        )
        .collect()
}

// ---------------------------------------------------------------------------
// 위키
// ---------------------------------------------------------------------------

/// 위키 문서 목록. 볼 수 있는 문서만(비공개 문서는 작성자·관리자·공유 대상에게만) 나온다.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/wiki/documents",
    params(("workspace_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [DocumentSummary]), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn list_wiki(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<Vec<DocumentSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    member_role(&mut tx, ws).await?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM documents
         WHERE workspace_id = $1 AND placement = 'wiki' ORDER BY {TREE_ORDER}"
    ))
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 위키 문서 생성. 번호 할당·문서·리비전 1·감사 기록은 정의자 함수 `app_create_wiki_document`가
/// 한 번에 만든다. 같은 사용자의 같은 `request_id`는 같은 문서를 돌려준다.
#[utoipa::path(post, path = "/api/workspaces/{workspace_id}/wiki/documents",
    params(("workspace_id" = String, Path, format = Uuid)), request_body = CreateDocument,
    responses((status = 201, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn create_wiki(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let role = member_role(&mut tx, ws).await?;
        if !role.can_write() {
            return Err(AppError::Forbidden);
        }
        let body = initial_body(&mut tx, input.template_id).await?;
        let id: DocumentId =
            sqlx::query_scalar("SELECT app_create_wiki_document($1, $2, $3, $4, $5, $6, $7, $8)")
                .bind(ws)
                .bind(input.request_id)
                .bind(&title)
                .bind(SCHEMA_VERSION)
                .bind(body)
                .bind(input.parent_id)
                .bind(max_depth(&state.config.limits))
                .bind(new_ids(3))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        let (row, access) = load_with_access(&mut tx, id).await?;
        tx.commit().await?;
        Ok(to_document(row, access))
    };
    match run().await {
        Ok(doc) => Ok((StatusCode::CREATED, Json(doc))),
        // 같은 request_id가 동시에 들어와 다른 쪽이 먼저 만들었다. 다시 부르면 그 문서를 돌려받는다.
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

// ---------------------------------------------------------------------------
// 개인 노트·공유받은 문서
// ---------------------------------------------------------------------------

/// 내 개인 노트 목록. 워크스페이스 관리자에게도 보이지 않는다 (D-22, D-49).
#[utoipa::path(get, path = "/api/me/notes",
    responses((status = 200, body = [DocumentSummary]), (status = 401, body = ErrorBody)), tag = "documents")]
pub async fn my_notes(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<DocumentSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT {SUMMARY_COLUMNS} FROM documents
         WHERE owner_user_id = app_user_id() ORDER BY {TREE_ORDER}"
    ))
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 개인 노트 생성.
///
/// 기대 사용자 머리가 필요하다(초안을 내 노트로 보존하는 길이 이 요청으로 시작한다, `ExpectedUser`).
#[utoipa::path(post, path = "/api/me/notes", request_body = CreateDocument,
    params(("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_USER_DOC)),
    responses((status = 201, body = Document), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody),
              (status = 409, body = ErrorBody, description = "`account_changed`: 기대 사용자가 세션의 사용자와 다름(아무것도 만들지 않음)")),
    tag = "documents")]
pub async fn create_note(
    State(state): State<AppState>,
    ExpectedUser(user): ExpectedUser,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let body = initial_body(&mut tx, input.template_id).await?;
        let id: DocumentId =
            sqlx::query_scalar("SELECT app_create_personal_note($1, $2, $3, $4, $5, $6, $7)")
                .bind(input.request_id)
                .bind(&title)
                .bind(SCHEMA_VERSION)
                .bind(body)
                .bind(input.parent_id)
                .bind(max_depth(&state.config.limits))
                .bind(new_ids(3))
                .fetch_one(tx.conn())
                .await
                .map_err(AppError::from_db_function)?;
        let (row, access) = load_with_access(&mut tx, id).await?;
        tx.commit().await?;
        Ok(to_document(row, access))
    };
    match run().await {
        Ok(doc) => Ok((StatusCode::CREATED, Json(doc))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 공유받은 문서.
#[derive(Debug, Serialize, ToSchema)]
pub struct SharedDocument {
    pub id: DocumentId,
    pub title: String,
    /// `read` 또는 `write`.
    pub permission: String,
    /// 공유한 사람의 이름. 워크스페이스 이름은 멤버가 아닌 사람에게 알리지 않는다.
    pub shared_by: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// 나에게 공유된 문서(검증된 이메일 기준).
#[utoipa::path(get, path = "/api/me/shared",
    responses((status = 200, body = [SharedDocument]), (status = 401, body = ErrorBody)), tag = "documents")]
pub async fn shared_with_me(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<SharedDocument>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(DocumentId, String, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, title, permission, shared_by, updated_at FROM app_shared_with_me()",
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, title, permission, shared_by, updated_at)| SharedDocument {
                    id,
                    title,
                    permission,
                    shared_by,
                    updated_at,
                },
            )
            .collect(),
    ))
}

// ---------------------------------------------------------------------------
// 조회·저장
// ---------------------------------------------------------------------------

/// 문서 표현(D-77): 단건 조회가 내보내는 바이트와 그 강한 ETag. 조회·저장·충돌이 같은 계산을 쓴다.
pub(crate) fn representation(user: UserId, doc: &Document) -> Result<(Vec<u8>, String), AppError> {
    let bytes = serde_json::to_vec(doc).map_err(|err| AppError::Io(err.into()))?;
    let etag = cache::strong_etag(user.0.as_bytes(), &bytes);
    Ok((bytes, etag))
}

fn with_etag(mut response: Response, etag: &str) -> Response {
    if let Ok(value) = HeaderValue::from_str(etag) {
        response.headers_mut().insert(header::ETAG, value);
    }
    response
}

fn json_bytes(bytes: Vec<u8>) -> Response {
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        )],
        bytes,
    )
        .into_response()
}

/// 문서 한 건 응답. 본문은 단건 조회와 같은 바이트이고 `ETag`를 붙인다(저장·복원 결과).
pub(crate) fn document_reply(user: UserId, doc: &Document) -> Result<Response, AppError> {
    let (bytes, etag) = representation(user, doc)?;
    Ok(with_etag(json_bytes(bytes), &etag))
}

/// 문서 조회. 볼 수 없으면 404.
///
/// 강한 ETag를 준다(D-77). `If-None-Match`는 세션과 권한을 확인한 뒤에만 비교하고 같으면 304다. 권한이 철회되면
/// 404, 로그아웃이면 401이라 이전 ETag로 304를 받지 못한다. 브라우저만 저장하고 매번 재검증한다.
#[utoipa::path(get, path = "/api/documents/{document_id}",
    params(("document_id" = String, Path, format = Uuid),
           ("If-None-Match" = Option<String>, Header, description = "앞서 받은 ETag. 같으면 304")),
    responses((status = 200, body = Document, headers(("ETag" = String, description = "강한 ETag"))),
              (status = 304, description = "바뀌지 않음", headers(("ETag" = String))),
              (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    let (bytes, etag) = representation(user, &to_document(row, access))?;
    let response = if cache::none_match(&headers, &etag) {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        json_bytes(bytes)
    };
    let mut response = with_etag(response, &etag);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static(cache::PRIVATE_REVALIDATE),
    );
    Ok(response)
}

fn conflict_reply(
    status: StatusCode,
    error: &'static str,
    user: UserId,
    current: Document,
) -> Result<Response, AppError> {
    let (_, etag) = representation(user, &current)?;
    Ok((
        status,
        Json(ConflictBody {
            error,
            current,
            etag,
        }),
    )
        .into_response())
}

/// 409: `base_revision`이 지났다. 서버 최신본과 그 ETag를 준다.
pub(crate) fn conflict(user: UserId, current: Document) -> Result<Response, AppError> {
    conflict_reply(StatusCode::CONFLICT, "conflict", user, current)
}

/// 문서 저장. 충돌 시 409(또는 `If-Match`가 맞지 않으면 412) + 서버 최신본.
///
/// 순서: 권한 확인 → 행 잠금 → 재시도 판단 → `If-Match`(있으면) → 리비전 비교 → 내용 동일 판단 → 갱신·리비전 기록.
/// 행을 잠근 뒤 판단하므로 동시에 들어온 저장도 최신 행 기준으로 결정된다. `If-Match`는 잠근 행으로 만든 지금
/// 표현의 ETag와 강하게 비교한다(D-77). 같은 요청의 재시도는 `If-Match`보다 먼저 판단해 멱등 응답을 지킨다.
///
/// 권한 확인보다 먼저 기대 사용자를 비교한다(`ExpectedUser`): 다르면 문서를 읽거나 잠그지 않고 409 `account_changed`.
#[utoipa::path(put, path = "/api/documents/{document_id}",
    params(("document_id" = String, Path, format = Uuid),
           ("If-Match" = Option<String>, Header, description = "단건 조회의 강한 ETag. 다르면 412"),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_USER_DOC)),
    request_body = SaveDocument,
    responses((status = 200, body = Document, headers(("ETag" = String, description = "저장 뒤 표현의 ETag"))),
              (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = SaveConflict,
               description = "`conflict`면 ConflictBody(서버 최신본 포함). `account_changed`(기대 사용자가 세션의 사용자와 다름)·`invalid_state`(실시간 편집 중 본문 변경 등)면 ErrorBody. 어느 쪽도 아무것도 바꾸지 않았다"),
              (status = 412, body = ConflictBody)), tag = "documents")]
pub async fn save(
    State(state): State<AppState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<DocumentId>,
    headers: HeaderMap,
    Json(input): Json<SaveDocument>,
) -> Result<Response, AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    validate_body(&input.body, &state.config.limits)?;

    let mut tx = state.db.begin_as(user).await?;
    // 항목 아래 문서면 항목·모듈을 먼저 공유 잠금한다(판정 뒤 보관·비공개 전환이 끼어들지 않게).
    sqlx::query("SELECT app_lock_item((SELECT item_id FROM documents WHERE id = $1))")
        .bind(id)
        .execute(tx.conn())
        .await?;
    // 잠그기 전에 권한을 판정한다(읽기 권한만 있으면 잠글 수 없고, 403과 404를 구분해야 한다).
    let access = access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    if !access.can_write() {
        return Err(AppError::Forbidden);
    }
    let current = load_for_update(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;

    // 같은 사용자의 같은 요청 재시도. 그 요청이 만든 리비전이 아직 최신이면 성공으로 돌려주고,
    // 그 뒤에 다른 저장이 있었으면 충돌로 알린다(재시도한 클라이언트가 남의 최신본을 덮지 않게).
    let replay: Option<(i32, Option<i32>)> = sqlx::query_as(
        "SELECT revision, restored_from FROM document_revisions
         WHERE document_id = $1 AND request_id = $2 AND author = app_user_id()",
    )
    .bind(id)
    .bind(input.request_id)
    .fetch_optional(tx.conn())
    .await?;
    if let Some((_, Some(_))) = replay {
        // 복원에 쓴 request_id를 저장에 다시 쓰면 재시도로 보지 않는다(엉뚱한 성공 응답 방지).
        return Err(AppError::BadRequest(
            "request_id was already used for another change".into(),
        ));
    }
    if let Some((revision, None)) = replay {
        tx.commit().await?;
        let doc = to_document(current, access);
        return if doc.revision == revision {
            document_reply(user, &doc)
        } else {
            conflict(user, doc)
        };
    }

    if headers.contains_key(header::IF_MATCH) {
        let now = to_document(current.clone(), access);
        let (_, etag) = representation(user, &now)?;
        if cache::if_match(&headers, &etag) == Some(false) {
            tx.commit().await?;
            return conflict_reply(
                StatusCode::PRECONDITION_FAILED,
                "precondition_failed",
                user,
                now,
            );
        }
    }
    if current.revision != input.base_revision {
        tx.commit().await?;
        return conflict(user, to_document(current, access));
    }
    // 내용이 같으면 새 리비전을 만들지 않는다. JSON 객체 비교는 키 순서와 무관하다.
    if current.title == title && current.body == input.body {
        tx.commit().await?;
        return document_reply(user, &to_document(current, access));
    }
    // 실시간 편집 중이면 본문은 실시간 연결만 고친다(두 길로 본문이 갈라지지 않게). 제목은 여기서 바꾼다.
    // 워크스페이스 행을 공유 잠금하고 다시 읽어, 켜기·끄기와 엇갈려 꺼진 줄 알고 쓴 본문이 켜진 뒤 들어오지 않게 한다.
    let realtime: bool = sqlx::query_scalar("SELECT app_document_realtime_locked($1)")
        .bind(id)
        .fetch_one(tx.conn())
        .await?;
    if realtime && current.body != input.body {
        return Err(AppError::Conflict(
            "realtime editing is on for this document; edit the body in the live editor".into(),
        ));
    }

    let row = write_revision(&mut tx, id, &title, &input.body, input.request_id).await?;
    tx.commit().await?;
    document_reply(user, &to_document(row, access))
}

/// 저장의 끝부분: 문서 행을 새 리비전으로 바꾸고 리비전·링크·언급을 기록한다. 저장 API와 실시간 편집의
/// 리비전(collab)이 같은 길을 쓴다. 호출하는 쪽이 문서 행을 잠그고 쓰기 권한을 확인한다.
pub(crate) async fn write_revision(
    tx: &mut UserTx,
    id: DocumentId,
    title: &str,
    body: &Value,
    request_id: Uuid,
) -> Result<DocRow, AppError> {
    let row: DocRow = sqlx::query_as(&format!(
        "UPDATE documents
         SET title = $2, body = $3, schema_version = $4, revision = revision + 1,
             updated_by = app_user_id(), updated_at = now()
         WHERE id = $1
         RETURNING {DOC_COLUMNS}"
    ))
    .bind(id)
    .bind(title)
    .bind(body)
    .bind(SCHEMA_VERSION)
    .fetch_optional(tx.conn())
    .await?
    // 잠금 뒤 권한이 바뀌어 갱신 정책이 행을 거른 경우(예: 그사이 항목 보관): 500이 아니라 403.
    .ok_or(AppError::Forbidden)?;
    sqlx::query(
        "INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
         VALUES ($1, $2, $3, $4, $5, app_user_id(), $6)",
    )
    .bind(id)
    .bind(row.revision)
    .bind(title)
    .bind(SCHEMA_VERSION)
    .bind(body)
    .bind(request_id)
    .execute(tx.conn())
    .await?;
    // 제목이 바뀌었으면 감사 기록을 남긴다(이전·새 제목은 함수가 두 리비전에서 읽는다, 본문은 남기지 않는다).
    sqlx::query("SELECT app_audit_document_rename($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(row.revision)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 링크·언급은 방금 저장한 본문에서 뽑아 같은 트랜잭션에 기록한다.
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_record_document_references_ids($1, true)").bind(id),
    )
    .await?;
    sqlx::query("SELECT app_record_document_references($1, true, $2)")
        .bind(id)
        .bind(ids)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// 트리 (D-27)
// ---------------------------------------------------------------------------

/// 부모·순서 바꾸기 요청. `parent_id`가 없으면 최상위, `before_id`가 있으면 그 형제 바로 앞, 없으면 맨 끝.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetParent {
    pub parent_id: Option<DocumentId>,
    pub before_id: Option<DocumentId>,
}

/// DB 함수에 넘길 트리 깊이 한도.
pub(crate) fn max_depth(limits: &Limits) -> i32 {
    i32::try_from(limits.max_document_depth).unwrap_or(i32::MAX)
}

async fn set_parent_in(
    tx: &mut UserTx,
    limits: &Limits,
    id: DocumentId,
    parent: Option<DocumentId>,
    before: Option<DocumentId>,
) -> Result<(), AppError> {
    sqlx::query("SELECT app_set_document_parent($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(parent)
        .bind(before)
        .bind(max_depth(limits))
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    Ok(())
}

/// 트리에서 부모·순서 바꾸기(쓰기 권한). 권한은 바뀌지 않는다. 부모는 같은 자리에 있어야 하고
/// 자기 하위로는 옮길 수 없으며 깊이 한도(`FVOCI_MAX_DOCUMENT_DEPTH`)를 넘을 수 없다.
#[utoipa::path(put, path = "/api/documents/{document_id}/parent",
    params(("document_id" = String, Path, format = Uuid)), request_body = SetParent,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn set_parent(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<SetParent>,
) -> Result<Json<Document>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    set_parent_in(
        &mut tx,
        &state.config.limits,
        id,
        input.parent_id,
        input.before_id,
    )
    .await?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(to_document(row, access)))
}

// ---------------------------------------------------------------------------
// 설정·공유 (소유 권한자)
// ---------------------------------------------------------------------------

/// 문서 설정 변경 요청. 바꾸는 칸만 보낸다. 없거나 null인 칸은 지금 값 그대로다(연 뒤 다른 소유 권한자가 바꾼
/// 비공개·복제 금지를 다른 칸만 바꾼 요청이 되돌리지 않게, 감사 W6).
#[derive(Debug, Deserialize, ToSchema)]
pub struct DocumentSettings {
    /// `workspace` 또는 `private`.
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub allow_copy: Option<bool>,
}

/// 비공개·복제 허용 설정(소유 권한자). 감사 기록을 남긴다.
#[utoipa::path(put, path = "/api/documents/{document_id}/settings",
    params(("document_id" = String, Path, format = Uuid)), request_body = DocumentSettings,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
pub async fn set_settings(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<DocumentSettings>,
) -> Result<Json<Document>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_document_settings($1, $2, $3, $4)")
        .bind(id)
        .bind(&input.visibility)
        .bind(input.allow_copy)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(to_document(row, access)))
}

/// 공유 항목.
#[derive(Debug, Serialize, ToSchema)]
pub struct Share {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub email: String,
    /// `read` 또는 `write`.
    pub permission: String,
}

/// 공유 추가 요청. 같은 이메일이면 권한을 바꾼다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShare {
    pub email: String,
    /// `read` 또는 `write`.
    pub permission: String,
}

/// 공유 목록(소유 권한자).
#[utoipa::path(get, path = "/api/documents/{document_id}/shares",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Share]), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn list_shares(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Vec<Share>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    match manager_access(&mut tx, id).await? {
        None => return Err(AppError::NotFound),
        Some(Access::Owner) => {}
        Some(_) => return Err(AppError::Forbidden),
    }
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT id, grantee_email, permission FROM document_shares
         WHERE document_id = $1 ORDER BY created_at",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, email, permission)| Share {
                id,
                email,
                permission,
            })
            .collect(),
    ))
}

/// 공유 추가(소유 권한자). 받는 사람은 그 이메일을 제공자가 검증한 계정으로 로그인해야 한다.
#[utoipa::path(post, path = "/api/documents/{document_id}/shares",
    params(("document_id" = String, Path, format = Uuid)), request_body = CreateShare,
    responses((status = 201, body = Share), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn create_share(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<CreateShare>,
) -> Result<(StatusCode, Json<Share>), AppError> {
    let email = crate::features::members::validate_email(&input.email)?;
    let mut tx = state.db.begin_as(user).await?;
    let share_id: Uuid = sqlx::query_scalar("SELECT app_share_document($1, $2, $3, $4)")
        .bind(id)
        .bind(&email)
        .bind(&input.permission)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(Share {
            id: share_id,
            email,
            permission: input.permission,
        }),
    ))
}

/// 공유 철회(소유 권한자). 다음 요청부터 접근이 막힌다.
#[utoipa::path(delete, path = "/api/document-shares/{share_id}",
    params(("share_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn delete_share(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(share): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_unshare_document($1, $2)")
        .bind(share)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 복제·이동
// ---------------------------------------------------------------------------

/// 복제 요청. `target_workspace_id`가 없으면 내 개인 노트로 만든다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CopyDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub target_workspace_id: Option<WorkspaceId>,
    pub title: String,
}

/// 복제. 새 문서(리비전 1)이며 이후 원본과 동기화하지 않는다 (D-23).
/// 소유 권한자는 항상, 그 밖의 사람은 원본이 복제를 허용할 때만 할 수 있다.
#[utoipa::path(post, path = "/api/documents/{document_id}/copy",
    params(("document_id" = String, Path, format = Uuid)), request_body = CopyDocument,
    responses((status = 201, body = Document), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn copy(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<CopyDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let mut tx = state.db.begin_as(user).await?;
    let new_id: DocumentId = sqlx::query_scalar("SELECT app_copy_document($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(input.request_id)
        .bind(input.target_workspace_id)
        .bind(&title)
        .bind(new_ids(3))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 사본의 링크도 기록한다. 언급 알림은 원작성자가 한 것이 아니므로 만들지 않는다(새 행 없음).
    sqlx::query("SELECT app_record_document_references($1, false, '{}')")
        .bind(new_id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let (row, access) = load_with_access(&mut tx, new_id).await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(to_document(row, access))))
}

/// 이동 대상.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveTarget {
    pub target_workspace_id: Option<WorkspaceId>,
    /// 대상 워크스페이스의 모듈 항목. 없으면 그 워크스페이스의 위키 (D-59).
    pub target_item_id: Option<ItemId>,
}

/// 이동 요청. 미리보기에서 접근 변화를 확인했다는 표시(`acknowledge`)가 있어야 이동한다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveDocument {
    pub target_workspace_id: Option<WorkspaceId>,
    pub target_item_id: Option<ItemId>,
    /// 사용자가 확인한 미리보기. 없거나 지금 계산한 것과 다르면 이동하지 않고 409와 새 미리보기를 돌려준다.
    pub acknowledged: Option<AcknowledgedPreview>,
}

/// 사용자가 확인한 접근 변화.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AcknowledgedPreview {
    pub gains: Vec<String>,
    pub loses: Vec<String>,
}

/// 이동 미리보기: 접근이 생기는 사람과 사라지는 사람.
#[derive(Debug, Serialize, ToSchema)]
pub struct MovePreview {
    pub gains: Vec<String>,
    pub loses: Vec<String>,
    /// 참조 번호가 새로 매겨지는지. 같은 모듈의 다른 항목으로 옮기면 번호를 유지한다. 링크는 id 기준이라 유지된다 (D-18).
    pub reference_changes: bool,
    /// 이전 리비전 기록도 함께 옮겨져 새로 볼 수 있게 되는 사람에게 보인다.
    pub history_moves_with_document: bool,
    /// 이동해도 유지되는 활성 공개 링크 수. 로그인 없이 계속 볼 수 있다.
    pub active_public_links: i64,
    /// 원래 자리에 남는 하위 문서 수(내가 볼 수 있는 것). 이동하는 문서의 원래 부모에 붙는다.
    pub children_left_behind: i64,
}

/// 이동 결과. 이동한 사람이 새 위치에서 문서를 볼 수 없으면 `document`가 없다
/// (예: 비공개 문서를 작성 권한만 있는 워크스페이스로 옮긴 경우).
#[derive(Debug, Serialize, ToSchema)]
pub struct MoveOutcome {
    pub workspace_id: WorkspaceId,
    pub reference: String,
    pub document: Option<Document>,
}

async fn preview(
    tx: &mut UserTx,
    id: DocumentId,
    target: Option<WorkspaceId>,
    item: Option<ItemId>,
) -> Result<MovePreview, AppError> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT change, display_name FROM app_move_preview($1, $2, $3)")
            .bind(id)
            .bind(target)
            .bind(item)
            .fetch_all(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let (gains, loses): (Vec<_>, Vec<_>) = rows.into_iter().partition(|(c, _)| c == "gain");
    let keeps_number: bool = sqlx::query_scalar("SELECT app_move_keeps_number($1, $2, $3)")
        .bind(id)
        .bind(target)
        .bind(item)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 공개 링크는 이동해도 유지된다. 비공개 항목으로 옮길 때 알 수 있게 개수를 보여 준다.
    // 하위 문서는 원래 자리에 남는다(이동하는 문서의 원래 부모에 붙음). 볼 수 있는 것만 센다.
    let children_left_behind: i64 =
        sqlx::query_scalar("SELECT count(*) FROM documents WHERE parent_id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    let active_public_links: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM public_links
         WHERE document_id = $1 AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await?;
    Ok(MovePreview {
        gains: gains.into_iter().map(|(_, n)| n).collect(),
        loses: loses.into_iter().map(|(_, n)| n).collect(),
        reference_changes: !keeps_number,
        active_public_links,
        children_left_behind,
        history_moves_with_document: true,
    })
}

/// 이동 미리보기(소유 권한자).
#[utoipa::path(post, path = "/api/documents/{document_id}/move/preview",
    params(("document_id" = String, Path, format = Uuid)), request_body = MoveTarget,
    responses((status = 200, body = MovePreview), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "documents")]
pub async fn move_preview(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<MoveTarget>,
) -> Result<Json<MovePreview>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let p = preview(&mut tx, id, input.target_workspace_id, input.target_item_id).await?;
    tx.commit().await?;
    Ok(Json(p))
}

/// 이동(소유 권한자). 확인한 미리보기가 없거나 지금 계산한 것과 다르면 409와 새 미리보기를 돌려준다.
#[utoipa::path(post, path = "/api/documents/{document_id}/move",
    params(("document_id" = String, Path, format = Uuid)), request_body = MoveDocument,
    responses((status = 200, body = MoveOutcome), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = MovePreview)), tag = "documents")]
pub async fn move_document(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<MoveDocument>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 잠금 순서(자리 → 문서 행 → 항목 → 번호): 원래 자리와 새 자리를 먼저 잠근다.
    sqlx::query("SELECT app_lock_move($1, $2, $3)")
        .bind(id)
        .bind(input.target_workspace_id)
        .bind(input.target_item_id)
        .execute(tx.conn())
        .await?;
    // 소유 권한자만 잠글 수 있다(RLS 갱신 정책). 다른 사람에게는 행이 없고, 아래 함수가 권한 오류를 낸다.
    sqlx::query("SELECT 1 FROM documents WHERE id = $1 FOR UPDATE")
        .bind(id)
        .execute(tx.conn())
        .await?;
    // 원래 항목과 대상 항목(과 그 모듈)도 잠가, 확인한 접근 변화가 이동 시점과 같게 한다.
    sqlx::query(
        "SELECT app_lock_item((SELECT item_id FROM documents WHERE id = $1)), app_lock_item($2)",
    )
    .bind(id)
    .bind(input.target_item_id)
    .execute(tx.conn())
    .await?;
    // 잠근 뒤 미리보기를 다시 계산해 사용자가 확인한 것과 같을 때만 옮긴다.
    let current = preview(&mut tx, id, input.target_workspace_id, input.target_item_id).await?;
    let confirmed = input.acknowledged.as_ref().is_some_and(|ack| {
        let mut a = (ack.gains.clone(), ack.loses.clone());
        let mut b = (current.gains.clone(), current.loses.clone());
        a.0.sort();
        a.1.sort();
        b.0.sort();
        b.1.sort();
        a == b
    });
    if !confirmed {
        tx.commit().await?;
        return Ok((StatusCode::CONFLICT, Json(current)).into_response());
    }
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_move_document_ids($1)").bind(id),
    )
    .await?;
    let (workspace_id, reference): (WorkspaceId, String) =
        sqlx::query_as("SELECT workspace_id, reference FROM app_move_document($1, $2, $3, $4)")
            .bind(id)
            .bind(input.target_workspace_id)
            .bind(input.target_item_id)
            .bind(ids)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let document = match access(&mut tx, id).await? {
        Some(a) => load(&mut tx, id).await?.map(|row| to_document(row, a)),
        None => None,
    };
    tx.commit().await?;
    state.collab.moved(id, workspace_id.0);
    Ok(Json(MoveOutcome {
        workspace_id,
        reference,
        document,
    })
    .into_response())
}

// ---------------------------------------------------------------------------
// 문서 템플릿 (D-27)
// ---------------------------------------------------------------------------

/// 템플릿 목록 조건.
#[derive(Debug, Deserialize, IntoParams)]
pub struct TemplateQuery {
    /// 이 워크스페이스의 템플릿과 내 개인 노트 템플릿. 없으면 내 개인 노트 템플릿만.
    #[param(value_type = Option<String>, format = Uuid)]
    pub workspace_id: Option<WorkspaceId>,
}

/// 템플릿 하나.
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentTemplate {
    pub id: DocumentId,
    pub title: String,
    /// 예: `WIKI-3`. 개인 노트면 없다.
    pub reference: Option<String>,
    /// 개인 노트 템플릿인지.
    pub personal: bool,
}

/// 쓸 수 있는 문서 템플릿(읽을 수 있는 것만, 제목순).
#[utoipa::path(get, path = "/api/document-templates", operation_id = "document_templates", params(TemplateQuery),
    responses((status = 200, body = [DocumentTemplate]), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn templates(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<TemplateQuery>,
) -> Result<Json<Vec<DocumentTemplate>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    if let Some(ws) = query.workspace_id {
        member_role(&mut tx, ws).await?;
    }
    let rows: Vec<(DocumentId, String, Option<String>, bool)> = sqlx::query_as(
        "SELECT id, title, app_document_reference(id), owner_user_id IS NOT NULL FROM documents
         WHERE is_template AND deleted_at IS NULL
           AND (owner_user_id = app_user_id() OR ($1::uuid IS NOT NULL AND workspace_id = $1))
         ORDER BY title, id LIMIT 200",
    )
    .bind(query.workspace_id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, title, reference, personal)| DocumentTemplate {
                id,
                title,
                reference,
                personal,
            })
            .collect(),
    ))
}

/// 템플릿 표시.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetDocumentTemplate {
    pub template: bool,
}

/// 템플릿 표시·해제(문서 소유 권한자).
#[utoipa::path(put, path = "/api/documents/{document_id}/template", operation_id = "set_document_template",
    params(("document_id" = String, Path, format = Uuid)), request_body = SetDocumentTemplate,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn set_template(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<SetDocumentTemplate>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_document_template($1, $2, $3)")
        .bind(id)
        .bind(input.template)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn limits(max: usize) -> Limits {
        Limits {
            max_document_bytes: max,
            max_title_chars: 10,
            max_references_per_document: 2,
            max_document_depth: 3,
        }
    }

    #[test]
    fn body_must_be_doc_object() {
        assert!(validate_body(&serde_json::json!({ "type": "doc" }), &limits(100)).is_ok());
        assert!(validate_body(&serde_json::json!({ "type": "paragraph" }), &limits(100)).is_err());
        assert!(validate_body(&serde_json::json!([1, 2]), &limits(100)).is_err());
    }

    #[test]
    fn body_size_is_limited() {
        let big = serde_json::json!({ "type": "doc", "content": "x".repeat(200) });
        assert!(matches!(
            validate_body(&big, &limits(100)),
            Err(AppError::BadRequest(_))
        ));
    }

    #[test]
    fn title_validation() {
        assert_eq!(validate_title("  회의록  ", 10, "title").unwrap(), "회의록");
        assert!(validate_title("   ", 10, "title").is_err());
        assert!(validate_title("가나다라마바사아자차카", 10, "title").is_err());
        assert!(validate_title("a\u{0007}b", 10, "title").is_err());
    }

    #[test]
    fn reference_nodes_are_checked_and_limited() {
        let id = "0198f1f2-0000-7000-8000-000000000001";
        let reference = |kind: &str, id: &str| json!({ "type": "reference", "attrs": { "kind": kind, "id": id } });
        let doc = |nodes: Vec<Value>| json!({ "type": "doc", "content": [{ "type": "paragraph", "content": nodes }] });
        let l = limits(10_000);
        assert!(
            validate_body(&doc(vec![reference("item", id), reference("user", id)]), &l).is_ok()
        );
        assert!(validate_body(&doc(vec![reference("workspace", id)]), &l).is_err());
        assert!(validate_body(&doc(vec![reference("item", "DEV-1")]), &l).is_err());
        // DB 추출과 같은 형식만: 하이픈 없는 형식·중괄호·urn은 거부한다.
        for odd in [
            "0198f1f20000700080000000000000001",
            "{0198f1f2-0000-7000-8000-000000000001}",
            "urn:uuid:0198f1f2-0000-7000-8000-000000000001",
        ] {
            assert!(
                validate_body(&doc(vec![reference("item", odd)]), &l).is_err(),
                "{odd}"
            );
        }
        let with_text =
            json!({ "type": "reference", "text": "비밀", "attrs": { "kind": "item", "id": id } });
        assert!(
            validate_body(&doc(vec![with_text]), &l).is_err(),
            "no title outside attrs"
        );
        let labelled =
            json!({ "type": "reference", "attrs": { "kind": "item", "id": id, "label": "비밀" } });
        assert!(
            validate_body(&doc(vec![labelled]), &l).is_err(),
            "labels are not stored"
        );
        let three = doc(vec![
            reference("item", id),
            reference("item", id),
            reference("user", id),
        ]);
        assert!(
            validate_body(&three, &l).is_err(),
            "limit is 2 in this test"
        );
    }

    #[test]
    fn access_parsing() {
        assert_eq!(Access::parse("owner"), Some(Access::Owner));
        assert!(Access::Write.can_write());
        assert!(!Access::Read.can_write());
        assert_eq!(Access::parse("other"), None);
    }

    #[test]
    fn images_and_links_must_stay_inside_the_service() {
        let doc = |node: serde_json::Value| json!({ "type": "doc", "content": [node] });
        let image =
            |src: &str| doc(json!({ "type": "image", "attrs": { "src": src, "alt": "x" } }));
        let id = "01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf";
        for ok in [
            format!("/api/files/{id}/content"),
            format!("/api/files/{id}/content?version=2&inline=true"),
            format!("/api/files/{id}/content?inline=true"),
            format!("/api/files/{id}/content?version=2147483647"),
        ] {
            assert!(validate_body(&image(&ok), &limits(10_000)).is_ok(), "{ok}");
        }
        for bad in [
            "https://tracker.example/pixel.png".to_owned(),
            format!("/api/files/{id}/content?version=1&x=1"),
            "/api/files/not-an-id/content".to_owned(),
            "javascript:alert(1)".to_owned(),
            // 표준형이 아니면 버전 참조(D-56)를 놓치므로 받지 않는다.
            format!("/api/files/{}/content?version=1", id.to_uppercase()),
            format!("/api/files/{}/content?version=1", id.replace('-', "")),
            format!("/api/files/{id}/content?inline=true&version=1"),
            format!("/api/files/{id}/content?version=01"),
            format!("/api/files/{id}/content?version=+1"),
            format!("/api/files/{id}/content?version=0"),
            format!("/api/files/{id}/content?version=99999999999"),
        ] {
            assert!(
                validate_body(&image(&bad), &limits(10_000)).is_err(),
                "{bad}"
            );
        }
        let link = |href: &str| {
            doc(
                json!({ "type": "paragraph", "content": [{ "type": "text", "text": "a",
                "marks": [{ "type": "link", "attrs": { "href": href } }] }] }),
            )
        };
        for ok in [
            "https://example.test",
            "mailto:a@example.test",
            "/docs/somewhere",
            "/api/files/01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf/content?version=3",
        ] {
            assert!(validate_body(&link(ok), &limits(10_000)).is_ok(), "{ok}");
        }
        for bad in [
            "javascript:alert(1)",
            "//evil.example",
            "data:text/html,x",
            "/api/files/x/content",
            "/API/files/01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf/content?version=03",
            " /api/files/01a1481e-6cb0-73cb-a92b-c7cf6cbc3ebf/content?version=3 ",
        ] {
            assert!(validate_body(&link(bad), &limits(10_000)).is_err(), "{bad}");
        }
    }
}
