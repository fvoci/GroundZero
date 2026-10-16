//! 위키 문서: 목록·생성·조회·저장.
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
use crate::ids::{DocumentId, Role, WorkspaceId};

/// 현재 본문 스키마 버전.
pub const SCHEMA_VERSION: i32 = 1;
/// 위키의 번호열 범위 키 (D-18, D-48).
pub const WIKI_SCOPE: &str = "WIKI";

/// 문서 목록 항목.
#[derive(Debug, Serialize, ToSchema)]
pub struct DocumentSummary {
    pub id: DocumentId,
    /// 사람이 보는 참조. 예: `WIKI-3`.
    pub reference: String,
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
    pub workspace_id: WorkspaceId,
    pub reference: String,
    pub title: String,
    /// Tiptap JSON. `type`이 `doc`인 객체.
    #[schema(value_type = Object)]
    pub body: Value,
    pub schema_version: i32,
    pub revision: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    /// 현재 사용자가 고칠 수 있는지.
    pub can_edit: bool,
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

fn empty_body() -> Value {
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

type DocRow = (
    DocumentId,
    WorkspaceId,
    String,
    i64,
    String,
    Value,
    i32,
    i32,
    OffsetDateTime,
);

const DOC_COLUMNS: &str =
    "id, workspace_id, scope, number, title, body, schema_version, revision, updated_at";

fn to_document(row: DocRow, role: Role) -> Document {
    let (id, workspace_id, scope, number, title, body, schema_version, revision, updated_at) = row;
    Document {
        id,
        workspace_id,
        reference: format!("{scope}-{number}"),
        title,
        body,
        schema_version,
        revision,
        updated_at,
        can_edit: role.can_write(),
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

/// 위키 문서 목록.
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
    let rows: Vec<(DocumentId, String, i64, String, i32, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, scope, number, title, revision, updated_at FROM documents
         WHERE workspace_id = $1 AND placement = 'wiki' ORDER BY number",
    )
    .bind(ws)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(id, scope, number, title, revision, updated_at)| DocumentSummary {
                    id,
                    reference: format!("{scope}-{number}"),
                    title,
                    revision,
                    updated_at,
                },
            )
            .collect(),
    ))
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
    let attempt = async {
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
        let row = load(&mut tx, id).await?.ok_or(AppError::NotFound)?;
        tx.commit().await?;
        Ok(to_document(row, role))
    };
    match attempt.await {
        Ok(doc) => Ok((StatusCode::CREATED, Json(doc))),
        // 같은 request_id가 동시에 들어와 다른 쪽이 먼저 만들었다. 다시 부르면 그 문서를 돌려받는다.
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            let mut tx = state.db.begin_as(user).await?;
            let role = member_role(&mut tx, ws).await?;
            let id: DocumentId =
                sqlx::query_scalar("SELECT app_create_wiki_document($1, $2, $3, $4, $5)")
                    .bind(ws)
                    .bind(input.request_id)
                    .bind(&title)
                    .bind(SCHEMA_VERSION)
                    .bind(empty_body())
                    .fetch_one(tx.conn())
                    .await?;
            let row = load(&mut tx, id).await?.ok_or(AppError::NotFound)?;
            tx.commit().await?;
            Ok((StatusCode::CREATED, Json(to_document(row, role))))
        }
        Err(e) => Err(e),
    }
}

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
    let row = load(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let role = member_role(&mut tx, row.1).await?;
    tx.commit().await?;
    Ok(Json(to_document(row, role)))
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
/// 순서: 권한 확인 → 행 잠금 → 재시도 판단 → 내용 동일 판단 → 리비전 비교 → 갱신·리비전 기록.
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
    // 잠그기 전에 읽어 멤버·역할을 판정한다(독자는 잠글 수 없고, 403과 404를 구분해야 한다).
    let visible = load(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let role = member_role(&mut tx, visible.1).await?;
    if !role.can_write() {
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
        let doc = to_document(current, role);
        return Ok(if doc.revision == revision {
            Json(doc).into_response()
        } else {
            conflict(doc)
        });
    }

    if current.7 != input.base_revision {
        tx.commit().await?;
        return Ok(conflict(to_document(current, role)));
    }
    // 내용이 같으면 새 리비전을 만들지 않는다. JSON 객체 비교는 키 순서와 무관하다.
    if current.4 == title && current.5 == input.body {
        tx.commit().await?;
        return Ok(Json(to_document(current, role)).into_response());
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
    .bind(row.7)
    .bind(&title)
    .bind(SCHEMA_VERSION)
    .bind(&input.body)
    .bind(input.request_id)
    .execute(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(to_document(row, role)).into_response())
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
}
