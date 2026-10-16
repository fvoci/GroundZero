//! 문서: 위키 문서와 개인 노트, 저장, 공유, 복제, 이동.
//!
//! 한 문서 엔진을 쓴다. 문서는 워크스페이스 소유(위키) 또는 사용자 소유(개인 노트) 중 하나다 (D-59).
//! 접근 판정은 DB 함수 `app_document_access`(owner·write·read·없음) 하나로 하고, RLS와 이 모듈이 같은 함수를
//! 쓴다(`migrations/0004_personal_notes_sharing.sql`).
//!
//! 저장 경계 (D-62): 본문은 Tiptap JSON(`schema_version` 1)이며 저장할 때마다 리비전이 하나 늘어난다.
//! 이것은 JSON 리비전 저장이고 공동 편집이 아니다. CRDT 도입 시 상태 열을 `body` 옆에 추가하며
//! 문서 id·번호·리비전 번호는 그대로 이어진다.
//!
//! 충돌 (D-53): 저장 요청의 `base_revision`이 현재 리비전과 다르면 409와 함께 서버 최신본을 돌려준다.
//! 클라이언트는 초안을 버리지 않는다. 같은 `request_id`의 재시도는 이미 만든 리비전을 돌려준다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::config::Limits;
use crate::db::{UserTx, is_unique_violation};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::workspaces::{member_role, validate_title};
use crate::http::AppState;
use crate::ids::{DocumentId, ItemId, WorkspaceId};

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

    fn can_write(self) -> bool {
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
}

/// 문서 전체.
#[derive(Debug, Serialize, ToSchema)]
pub struct Document {
    pub id: DocumentId,
    /// 워크스페이스 문서면 소유 워크스페이스. 개인 노트는 없다.
    pub workspace_id: Option<WorkspaceId>,
    /// 모듈 항목 아래 문서면 그 항목 (D-59).
    pub item_id: Option<ItemId>,
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
}

/// 문서 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDocument {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub title: String,
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
    pub error: &'static str,
    pub current: Document,
}

pub(crate) fn empty_body() -> Value {
    serde_json::json!({ "type": "doc", "content": [{ "type": "paragraph" }] })
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
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub(crate) struct DocRow {
    id: DocumentId,
    workspace_id: Option<WorkspaceId>,
    item_id: Option<ItemId>,
    placement: String,
    reference: Option<String>,
    title: String,
    body: Value,
    schema_version: i32,
    revision: i32,
    updated_at: OffsetDateTime,
    visibility: String,
    allow_copy: bool,
}

/// 참조 문자열은 저장하지 않고 현재 키로 만든다(키가 바뀌어도 번호·id는 그대로, D-18).
const DOC_COLUMNS: &str = "id, workspace_id, item_id, placement, app_document_reference(id) AS reference, \
                           title, body, schema_version, revision, updated_at, visibility, allow_copy";

pub(crate) fn to_document(row: DocRow, access: Access) -> Document {
    Document {
        id: row.id,
        workspace_id: row.workspace_id,
        item_id: row.item_id,
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
async fn load_for_update(tx: &mut UserTx, id: DocumentId) -> Result<Option<DocRow>, AppError> {
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
pub(crate) const SUMMARY_COLUMNS: &str =
    "id, app_document_reference(id) AS reference, title, revision, updated_at";

pub(crate) type SummaryRow = (DocumentId, Option<String>, String, i32, OffsetDateTime);

pub(crate) fn to_summaries(rows: Vec<SummaryRow>) -> Vec<DocumentSummary> {
    rows.into_iter()
        .map(
            |(id, reference, title, revision, updated_at)| DocumentSummary {
                id,
                reference,
                title,
                revision,
                updated_at,
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
         WHERE workspace_id = $1 AND placement = 'wiki' ORDER BY number"
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
        let id: DocumentId =
            sqlx::query_scalar("SELECT app_create_wiki_document($1, $2, $3, $4, $5)")
                .bind(ws)
                .bind(input.request_id)
                .bind(&title)
                .bind(SCHEMA_VERSION)
                .bind(empty_body())
                .fetch_one(tx.conn())
                .await?;
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
         WHERE owner_user_id = app_user_id() ORDER BY updated_at DESC"
    ))
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_summaries(rows)))
}

/// 개인 노트 생성.
#[utoipa::path(post, path = "/api/me/notes", request_body = CreateDocument,
    responses((status = 201, body = Document), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody)), tag = "documents")]
pub async fn create_note(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateDocument>,
) -> Result<(StatusCode, Json<Document>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: DocumentId = sqlx::query_scalar("SELECT app_create_personal_note($1, $2, $3, $4)")
            .bind(input.request_id)
            .bind(&title)
            .bind(SCHEMA_VERSION)
            .bind(empty_body())
            .fetch_one(tx.conn())
            .await?;
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

/// 문서 조회. 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/documents/{document_id}",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Document), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Document>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(to_document(row, access)))
}

fn conflict(current: Document) -> Response {
    (
        StatusCode::CONFLICT,
        Json(ConflictBody {
            error: "conflict",
            current,
        }),
    )
        .into_response()
}

/// 문서 저장. 충돌 시 409 + 서버 최신본.
///
/// 순서: 권한 확인 → 행 잠금 → 재시도 판단 → 리비전 비교 → 내용 동일 판단 → 갱신·리비전 기록.
/// 행을 잠근 뒤 판단하므로 동시에 들어온 저장도 최신 행 기준으로 결정된다.
#[utoipa::path(put, path = "/api/documents/{document_id}",
    params(("document_id" = String, Path, format = Uuid)), request_body = SaveDocument,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ConflictBody)), tag = "documents")]
pub async fn save(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<SaveDocument>,
) -> Result<Response, AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    validate_body(&input.body, &state.config.limits)?;

    let mut tx = state.db.begin_as(user).await?;
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
    let replay: Option<i32> = sqlx::query_scalar(
        "SELECT revision FROM document_revisions
         WHERE document_id = $1 AND request_id = $2 AND author = app_user_id()",
    )
    .bind(id)
    .bind(input.request_id)
    .fetch_optional(tx.conn())
    .await?;
    if let Some(revision) = replay {
        tx.commit().await?;
        let doc = to_document(current, access);
        return Ok(if doc.revision == revision {
            Json(doc).into_response()
        } else {
            conflict(doc)
        });
    }

    if current.revision != input.base_revision {
        tx.commit().await?;
        return Ok(conflict(to_document(current, access)));
    }
    // 내용이 같으면 새 리비전을 만들지 않는다. JSON 객체 비교는 키 순서와 무관하다.
    if current.title == title && current.body == input.body {
        tx.commit().await?;
        return Ok(Json(to_document(current, access)).into_response());
    }

    // 제목 변경 감사 기록은 트리거가 남긴다(본문은 남기지 않는다).
    let row: DocRow = sqlx::query_as(&format!(
        "UPDATE documents
         SET title = $2, body = $3, schema_version = $4, revision = revision + 1,
             updated_by = app_user_id(), updated_at = now()
         WHERE id = $1
         RETURNING {DOC_COLUMNS}"
    ))
    .bind(id)
    .bind(&title)
    .bind(&input.body)
    .bind(SCHEMA_VERSION)
    .fetch_one(tx.conn())
    .await?;
    sqlx::query(
        "INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
         VALUES ($1, $2, $3, $4, $5, app_user_id(), $6)",
    )
    .bind(id)
    .bind(row.revision)
    .bind(&title)
    .bind(SCHEMA_VERSION)
    .bind(&input.body)
    .bind(input.request_id)
    .execute(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_document(row, access)).into_response())
}

// ---------------------------------------------------------------------------
// 설정·공유 (소유 권한자)
// ---------------------------------------------------------------------------

/// 문서 설정 변경 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct DocumentSettings {
    /// `workspace` 또는 `private`.
    pub visibility: String,
    pub allow_copy: bool,
}

/// 비공개·복제 허용 설정(소유 권한자). 감사 기록을 남긴다.
#[utoipa::path(put, path = "/api/documents/{document_id}/settings",
    params(("document_id" = String, Path, format = Uuid)), request_body = DocumentSettings,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn set_settings(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<DocumentSettings>,
) -> Result<Json<Document>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_document_settings($1, $2, $3)")
        .bind(id)
        .bind(&input.visibility)
        .bind(input.allow_copy)
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
    match access(&mut tx, id).await? {
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
    let share_id: Uuid = sqlx::query_scalar("SELECT app_share_document($1, $2, $3)")
        .bind(id)
        .bind(&email)
        .bind(&input.permission)
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
    sqlx::query("SELECT app_unshare_document($1)")
        .bind(share)
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
    let new_id: DocumentId = sqlx::query_scalar("SELECT app_copy_document($1, $2, $3, $4)")
        .bind(id)
        .bind(input.request_id)
        .bind(input.target_workspace_id)
        .bind(&title)
        .fetch_one(tx.conn())
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
    /// 이동하면 참조 번호가 대상 위치(위키 또는 모듈)에서 새로 매겨진다. 링크는 id 기준이라 유지된다 (D-18).
    pub reference_changes: bool,
    /// 이전 리비전 기록도 함께 옮겨져 새로 볼 수 있게 되는 사람에게 보인다.
    pub history_moves_with_document: bool,
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
    Ok(MovePreview {
        gains: gains.into_iter().map(|(_, n)| n).collect(),
        loses: loses.into_iter().map(|(_, n)| n).collect(),
        reference_changes: true,
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
    // 소유 권한자만 잠글 수 있다(RLS 갱신 정책). 다른 사람에게는 행이 없고, 아래 함수가 권한 오류를 낸다.
    sqlx::query("SELECT 1 FROM documents WHERE id = $1 FOR UPDATE")
        .bind(id)
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
    let (workspace_id, reference): (WorkspaceId, String) =
        sqlx::query_as("SELECT workspace_id, reference FROM app_move_document($1, $2, $3)")
            .bind(id)
            .bind(input.target_workspace_id)
            .bind(input.target_item_id)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let document = match access(&mut tx, id).await? {
        Some(a) => load(&mut tx, id).await?.map(|row| to_document(row, a)),
        None => None,
    };
    tx.commit().await?;
    Ok(Json(MoveOutcome {
        workspace_id,
        reference,
        document,
    })
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(max: usize) -> Limits {
        Limits {
            max_document_bytes: max,
            max_title_chars: 10,
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
    fn access_parsing() {
        assert_eq!(Access::parse("owner"), Some(Access::Owner));
        assert!(Access::Write.can_write());
        assert!(!Access::Read.can_write());
        assert_eq!(Access::parse("other"), None);
    }
}
