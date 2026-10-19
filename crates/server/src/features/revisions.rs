//! 문서 리비전 기록 보기와 복원 (D-27, D-29, D-57, D-58).
//!
//! 기록은 문서를 볼 수 있는 사람이 본다(RLS: `document_revisions_select`). 복원은 쓰기 권한자가 하며
//! 옛 리비전의 내용으로 새 리비전을 만든다. 이력을 지우지 않고, 저장과 같은 충돌 규칙(기준 리비전,
//! 같은 `request_id` 재시도)을 따른다. 처리는 DB 함수 `app_restore_document_revision`이 한 트랜잭션에서 한다
//! (`migrations/0009_revision_restore.sql`).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::documents::{
    ConflictBody, Document, SCHEMA_VERSION, access, conflict, document_reply, load_with_access,
    to_document, validate_body,
};
use crate::features::workspaces::validate_title;
use crate::http::AppState;
use crate::ids::DocumentId;

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
}

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

type SummaryRow = (i32, String, Option<String>, OffsetDateTime, Option<i32>);

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
    let rows: Vec<SummaryRow> = sqlx::query_as(
        "SELECT r.revision, r.title, u.display_name, r.created_at, r.restored_from
         FROM document_revisions r LEFT JOIN users u ON u.id = r.author
         WHERE r.document_id = $1 AND ($2::integer IS NULL OR r.revision < $2)
         ORDER BY r.revision DESC LIMIT $3",
    )
    .bind(id)
    .bind(page.before)
    .bind(PAGE)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(
                |(revision, title, author_name, created_at, restored_from)| RevisionSummary {
                    revision,
                    title,
                    author_name,
                    created_at,
                    restored_from,
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
    tx: &mut crate::db::UserTx,
    id: DocumentId,
    revision: i32,
) -> Result<Option<Revision>, AppError> {
    let row: Option<RevisionRow> = sqlx::query_as(
        "SELECT r.revision, r.title, r.body, r.schema_version, u.display_name, r.created_at, r.restored_from
         FROM document_revisions r LEFT JOIN users u ON u.id = r.author
         WHERE r.document_id = $1 AND r.revision = $2",
    )
    .bind(id)
    .bind(revision)
    .fetch_optional(tx.conn())
    .await?;
    Ok(row.map(
        |(revision, title, body, schema_version, author_name, created_at, restored_from)| {
            Revision {
                revision,
                title,
                body,
                schema_version,
                author_name,
                created_at,
                restored_from,
            }
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

/// 저장된 리비전 하나를 내보낸다(D-75). 첨부 내려받기와 같은 권한(고칠 수 있거나, 볼 수 있고 소유자가 복제·내려받기를
/// 허용)이 있어야 하고 내보내기 기록을 남긴다. 돌려주는 내용은 그 리비전의 제목·본문이다. 그림·첨부는 브라우저가 파일
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
    sqlx::query("SELECT app_audit_document_export($1, $2, $3)")
        .bind(id)
        .bind(revision)
        .bind(format)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(found))
}

/// 옛 리비전의 내용으로 새 리비전을 만든다(쓰기 권한). 기준 리비전이 지났으면 409와 서버 최신본.
#[utoipa::path(post, path = "/api/documents/{document_id}/revisions/{revision}/restore", operation_id = "restore_revision",
    params(("document_id" = String, Path, format = Uuid), ("revision" = i32, Path)),
    request_body = RestoreRevision,
    responses((status = 200, body = Document), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ConflictBody)), tag = "documents")]
pub async fn restore(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((id, revision)): Path<(DocumentId, i32)>,
    Json(input): Json<RestoreRevision>,
) -> Result<Response, AppError> {
    // 실시간으로 편집 중이면 아직 리비전으로 남지 않은 편집을 먼저 리비전으로 남긴다. 복원이 그 편집을 이력 없이
    // 지우지 않게 한다(D-27). 고칠 수 있는 사람의 요청만. 방을 기다리는 동안 DB 연결을 쥐지 않는다.
    let mut tx = state.db.begin_as(user).await?;
    let can_write = access(&mut tx, id).await?.is_some_and(|a| a.can_write());
    let current: Option<i32> = sqlx::query_scalar("SELECT revision FROM documents WHERE id = $1")
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    tx.commit().await?;
    let mut base = input.base_revision;
    if can_write {
        // 화면이 보던 리비전 바로 다음이 방금 그 화면의 실시간 내용으로 남긴 리비전뿐이면 그것을 기준으로 삼는다(화면은
        // 그 리비전 번호를 모른다). 그 밖에 기준이 지났으면 아래 복원이 충돌로 알린다.
        let written = state.collab.snapshot_document(id, user).await;
        if current == Some(base) && written == Some(base + 1) {
            base += 1;
        }
    }
    let mut tx = state.db.begin_as(user).await?;
    // 복원할 내용도 지금의 한도(제목 길이·본문 크기·참조 수)를 지켜야 한다. 한도를 낮춘 뒤 옛 리비전으로
    // 한도를 넘는 문서가 다시 생기지 않게 한다. 볼 수 없거나 없는 리비전은 아래 함수와 같이 404.
    let source: Option<(String, Value)> = sqlx::query_as(
        "SELECT title, body FROM document_revisions WHERE document_id = $1 AND revision = $2",
    )
    .bind(id)
    .bind(revision)
    .fetch_optional(tx.conn())
    .await?;
    if let Some((title, body)) = &source {
        validate_title(title, state.config.limits.max_title_chars, "title")?;
        validate_body(body, &state.config.limits)?;
    }
    let outcome: String =
        sqlx::query_scalar("SELECT app_restore_document_revision($1, $2, $3, $4, $5)")
            .bind(id)
            .bind(revision)
            .bind(input.request_id)
            .bind(base)
            .bind(SCHEMA_VERSION)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    let doc = to_document(row, access);
    if outcome == "conflict" {
        return conflict(user, doc);
    }
    // 실시간 편집 방이 열려 있으면 저장 없이 닫고 복원된 본문으로 다시 열게 한다(D-70).
    state.collab.reset(id).await;
    document_reply(user, &doc)
}
