//! 문서 리비전 기록 보기와 복원 (D-27, D-29, D-57, D-58).
//!
//! 기록은 문서를 볼 수 있는 사람이 본다(RLS: `document_revisions_select`). 복원은 쓰기 권한자가 하며
//! 옛 리비전의 내용으로 새 리비전을 만든다. 이력을 지우지 않고, 저장과 같은 충돌 규칙(기준 리비전,
//! 같은 `request_id` 재시도)을 따른다. 처리는 DB 함수 `app_restore_document_revision`이 한 트랜잭션에서 한다
//! (`crates/db/migrations/202610280003_documents.sql`). 새 UUIDv7은 앱이 공급한다(D-88).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::collab::room::RestoreRequest;
use crate::config::Limits;
use crate::features::documents::{
    ConflictBody, Document, SCHEMA_VERSION, access, conflict, document_reply, limit_lock_wait,
    load_with_access, lock_limit_error, to_document, validate_body,
};
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::DocumentId;
use fvoci_core::validate::validate_title;
use fvoci_db::Db;
use fvoci_http::session::CurrentUser;

/// 한 번에 돌려주는 리비전 수.
const PAGE: i64 = 100;

/// 리비전 목록 항목(본문 없음).
#[derive(Debug, Serialize, ToSchema)]
pub struct RevisionSummary {
    pub revision: i32,
    pub title: String,
    /// 저장한 사람. 워크스페이스를 함께 쓰지 않으면(공유받은 개인 노트 등) 없을 수 있다.
    pub author_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    /// 복원으로 만든 리비전이면 원본 리비전 번호.
    pub restored_from: Option<i32>,
    /// 저장한 사람이 고친 사람 대신 남긴 리비전(실시간 편집, D-95)이면 실제로 고친 사람(마지막이 앞). 보통 리비전은
    /// 없음, 대신 저장했으나 고친 사람 기록이 없으면 빈 목록. 이름을 볼 수 없는 사람은 `author_name`처럼 null이다.
    pub edited_by_names: Option<Vec<Option<String>>>,
}

/// 리비전 하나(본문 포함).
#[derive(Debug, Serialize, ToSchema)]
pub struct Revision {
    pub revision: i32,
    pub title: String,
    #[schema(value_type = Object)]
    pub body: Value,
    pub schema_version: i32,
    pub author_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub restored_from: Option<i32>,
    /// `RevisionSummary::edited_by_names`와 같다.
    pub edited_by_names: Option<Vec<Option<String>>>,
}

/// 대신 저장한 리비전의 실제 편집자 이름(`edited_by` 순서). 보통 리비전은 NULL, 기록이 없으면 빈 배열. 볼 수 없는
/// 사용자는 작성자 이름과 같이 NULL(사용자 행이 RLS로 걸러진다).
const EDITED_BY_NAMES: &str = "CASE WHEN r.edited_by IS NULL THEN NULL
         ELSE ARRAY(SELECT e.display_name FROM unnest(r.edited_by) WITH ORDINALITY AS x(id, n)
                    LEFT JOIN users e ON e.id = x.id ORDER BY x.n) END";

/// 목록 범위. `before`보다 작은 리비전을 최신순으로 돌려준다.
#[derive(Debug, Deserialize, IntoParams)]
pub struct RevisionPage {
    pub before: Option<i32>,
}

/// 복원 요청. `base_revision`은 사용자가 보고 있던 현재 리비전이다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RestoreRevision {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub base_revision: i32,
}

type SummaryRow = (
    i32,
    String,
    Option<String>,
    OffsetDateTime,
    Option<i32>,
    Option<Vec<Option<String>>>,
);

/// 리비전 목록(최신순, 한 번에 100개). 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/documents/{document_id}/revisions", operation_id = "list_revisions",
    params(("document_id" = String, Path, format = Uuid), RevisionPage),
    responses((status = 200, body = [RevisionSummary]), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Query(page): Query<RevisionPage>,
) -> Result<Json<Vec<RevisionSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let rows: Vec<SummaryRow> = sqlx::query_as(&format!(
        "SELECT r.revision, r.title, u.display_name, r.created_at, r.restored_from, {EDITED_BY_NAMES}
         FROM document_revisions r LEFT JOIN users u ON u.id = r.author
         WHERE r.document_id = $1 AND ($2::integer IS NULL OR r.revision < $2)
         ORDER BY r.revision DESC LIMIT $3"
    ))
    .bind(id)
    .bind(page.before)
    .bind(PAGE)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(revision, title, author_name, created_at, restored_from, edited_by_names)| {
                    RevisionSummary {
                        revision,
                        title,
                        author_name,
                        created_at,
                        restored_from,
                        edited_by_names,
                    }
                },
            )
            .collect(),
    ))
}

type RevisionRow = (
    i32,
    String,
    Value,
    i32,
    Option<String>,
    OffsetDateTime,
    Option<i32>,
    Option<Vec<Option<String>>>,
);

/// 리비전 하나. 문서나 리비전이 없거나 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/documents/{document_id}/revisions/{revision}", operation_id = "get_revision",
    params(("document_id" = String, Path, format = Uuid), ("revision" = i32, Path)),
    responses((status = 200, body = Revision), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, revision)): Path<(DocumentId, i32)>,
) -> Result<Json<Revision>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let found = fetch_revision(&mut tx, id, revision).await?;
    tx.commit().await?;
    Ok(Json(found.ok_or(AppError::NotFound)?))
}

async fn fetch_revision(
    tx: &mut fvoci_db::UserTx,
    id: DocumentId,
    revision: i32,
) -> Result<Option<Revision>, AppError> {
    let row: Option<RevisionRow> = sqlx::query_as(&format!(
        "SELECT r.revision, r.title, r.body, r.schema_version, u.display_name, r.created_at, r.restored_from,
                {EDITED_BY_NAMES}
         FROM document_revisions r LEFT JOIN users u ON u.id = r.author
         WHERE r.document_id = $1 AND r.revision = $2"
    ))
    .bind(id)
    .bind(revision)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row.map(
        |(
            revision,
            title,
            body,
            schema_version,
            author_name,
            created_at,
            restored_from,
            edited_by_names,
        )| Revision {
            revision,
            title,
            body,
            schema_version,
            author_name,
            created_at,
            restored_from,
            edited_by_names,
        },
    ))
}

/// 내보내기 형식(D-75). 변환은 브라우저가 편집기의 변환기로 한다. 서버는 권한을 확인하고 기록을 남긴다.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Markdown,
    Zip,
    Pdf,
}

/// 내보내기 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExportRequest {
    pub format: ExportFormat,
}

/// 저장된 리비전 하나를 내보낸다(D-75·D-115). 소유 권한이거나, 볼 수 있고 문서의 `allow_copy`가 켜져 있어야
/// 하며(수정 권한만으로는 안 된다) 내보내기 기록을 남긴다. 돌려주는 내용은 그 리비전의 제목·본문이다. 그림·첨부는 브라우저가 파일
/// 내용 API로 받으므로 파일마다 내려받기 권한을 다시 확인한다.
#[utoipa::path(post, path = "/api/documents/{document_id}/revisions/{revision}/export", operation_id = "export_revision",
    params(("document_id" = String, Path, format = Uuid), ("revision" = i32, Path)),
    request_body = ExportRequest,
    responses((status = 200, body = Revision), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn export(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, revision)): Path<(DocumentId, i32)>,
    Json(input): Json<ExportRequest>,
) -> Result<Json<Revision>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    let found = fetch_revision(&mut tx, id, revision)
        .await?
        .ok_or(AppError::NotFound)?;
    let format = match input.format {
        ExportFormat::Markdown => "markdown",
        ExportFormat::Zip => "zip",
        ExportFormat::Pdf => "pdf",
    };
    sqlx::query("SELECT app_audit_document_export($1, $2, $3, $4)")
        .bind(id)
        .bind(revision)
        .bind(format)
        .bind(fvoci_db::new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(found))
}

/// 복원의 409 본문(계약 문서용). 응답은 두 모양 가운데 하나를 그대로 보낸다(이 타입으로 감싸지 않는다).
/// - `ConflictBody`(`current` 포함): 기준 리비전이 지났다(먼저 남긴 실시간 편집의 리비전 때문일 수 있다).
/// - `ErrorBody`(`error: "invalid_state"`, `current` 없음): 복원하지 않았다. 실시간 편집을 먼저 리비전으로 남기지 못했거나
///   (편집은 남는다), 다른 변경이 문서를 잠금 한도보다 오래 쥐었거나, 방이 바쁘거나 닫혔다.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum RestoreConflict {
    Stale(Box<ConflictBody>),
    State(ErrorBody),
}

/// 옛 리비전의 내용으로 새 리비전을 만든다(쓰기 권한). 기준 리비전이 지났으면 409와 서버 최신본. 실시간 편집(열린 방의
/// 편집이나 방 없이 남은 저장 대기 상태)을 먼저 리비전으로 남기지 못하면 복원하지 않고 409(오류 본문, `invalid_state`)다.
/// 편집은 남는다. 남겼으면 기준이 지나 최신본과 함께 409다.
#[utoipa::path(post, path = "/api/documents/{document_id}/revisions/{revision}/restore", operation_id = "restore_revision",
    params(("document_id" = String, Path, format = Uuid), ("revision" = i32, Path)),
    request_body = RestoreRevision,
    responses((status = 200, body = Document), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = RestoreConflict,
               description = "기준이 지났으면 ConflictBody(최신본 포함), 실시간 편집을 먼저 남기지 못했거나 잠금 한도를 넘었으면 ErrorBody(`invalid_state`)")),
    tag = "documents")]
pub async fn restore(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, revision)): Path<(DocumentId, i32)>,
    Json(input): Json<RestoreRevision>,
) -> Result<Response, AppError> {
    let request = RestoreRequest {
        doc: id,
        user,
        revision,
        request_id: input.request_id,
        base_revision: input.base_revision,
    };
    // 실시간 방이 열려 있으면 그 방이 체크포인트(리비전으로 남기지 않은 편집 남기기) → 복원 → 방 초기화를 한 명령으로
    // 한다. 그 사이 편집이 끼어들지 않고, 남겼는지 확인되지 않으면 복원하지 않는다(D-27). 방이 없으면 방이 새로 열리지
    // 않게 막은 채 방 없이 남은 저장 대기 상태를 먼저 리비전으로 남긴다(D-95 조건 4, 상태의 편집자만 남기고 복원하는 사람은
    // 대신 저장하지 않는다. 남기지 못하면 409). 기준은 화면이 보낸 그대로다: 체크포인트가 리비전을 남겼으면 충돌로 알려 사람이 지금 내용을 보고
    // 다시 하게 한다. 고칠 수 없는 사람의 요청은 방과 저장 대기 상태를 건드리지 않고 여기서 거절한다(볼 수 없으면 404, 읽기만
    // 하면 403). 그 사람이 막 쓰기 권한을 얻었더라도 막음·체크포인트 없이 복원하지 않는다(그 사이 열린 방의 편집을 잃지
    // 않게, #64 반박 검증). 권한의 최종 확인은 복원 함수가 한다.
    let mut tx = state.db.begin_as(user).await?;
    let access_now = access(&mut tx, id).await?.ok_or(AppError::NotFound)?;
    if !access_now.can_write() {
        // 쓸 수 없는 이유(권한 부족 403, 보관된 항목 409)는 SQL이 정한다(D-113·D-106).
        sqlx::query("SELECT app_require_document($1, 'write')")
            .bind(id)
            .execute(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        return Err(AppError::Forbidden);
    }
    tx.commit().await?;
    let outcome = state
        .collab
        .restore(&state.db, &state.config.limits, request)
        .await?;
    let mut tx = state.db.begin_as(user).await?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    let doc = to_document(row, access);
    if outcome == "conflict" {
        return conflict(user, doc);
    }
    document_reply(user, &doc)
}

/// 복원 SQL: 한도 확인 뒤 `app_restore_document_revision`(권한·기준 확인 포함). `restored`·`unchanged`·`conflict`.
pub(crate) async fn restore_in_db(
    db: &Db,
    limits: &Limits,
    request: &RestoreRequest,
) -> Result<String, AppError> {
    let mut tx = db.begin_as(request.user).await?;
    // 열린 방은 이 복원을 자기 명령 안에서 한다: 문서·항목 잠금을 한도까지만 기다린다(이동과 같다, `documents::LOCK_LIMIT`).
    limit_lock_wait(&mut tx).await?;
    // 복원할 내용도 지금의 한도(제목 길이·본문 크기·참조 수)를 지켜야 한다. 한도를 낮춘 뒤 옛 리비전으로
    // 한도를 넘는 문서가 다시 생기지 않게 한다. 볼 수 없거나 없는 리비전은 아래 함수와 같이 404.
    let source: Option<(String, Value)> = sqlx::query_as(
        "SELECT title, body FROM document_revisions WHERE document_id = $1 AND revision = $2",
    )
    .bind(request.doc)
    .bind(request.revision)
    .fetch_optional(tx.conn())
    .await?;
    if let Some((title, body)) = &source {
        validate_title(title, limits.max_title_chars, "title")?;
        validate_body(body, limits)?;
    }
    let outcome: String =
        sqlx::query_scalar("SELECT app_restore_document_revision($1, $2, $3, $4, $5, $6)")
            .bind(request.doc)
            .bind(request.revision)
            .bind(request.request_id)
            .bind(request.base_revision)
            .bind(SCHEMA_VERSION)
            .bind(fvoci_db::new_ids(2))
            .fetch_one(tx.conn())
            .await
            .map_err(|err| {
                lock_limit_error(
                    err,
                    "another change is holding the document; nothing was restored, try again",
                )
            })?;
    tx.commit().await?;
    Ok(outcome)
}
