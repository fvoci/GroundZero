//! 트리·설정·공유·템플릿 명령.

use crate::state::DocumentLimits as Limits;
use crate::state::DocumentsState;
use axum::Json;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::http::StatusCode;
use fvoci_core::error::AppError;
use fvoci_core::error::ErrorBody;
use fvoci_core::ids::DocumentId;
use fvoci_db::UserTx;
use fvoci_db::new_ids;
use fvoci_http::session::CurrentUser;
use uuid::Uuid;

use super::{
    CreateShare, Document, DocumentSettings, DocumentTemplate, SetDocumentTemplate, SetParent,
    Share, TemplateQuery, load_with_access, require_manager, to_document,
};

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
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<SetParent>,
) -> Result<Json<Document>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    set_parent_in(&mut tx, &state.limits, id, input.parent_id, input.before_id).await?;
    let (row, access) = load_with_access(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(to_document(row, access)))
}

/// 비공개·복제 허용 설정(소유 권한자). 감사 기록을 남긴다.
#[utoipa::path(put, path = "/api/documents/{document_id}/settings",
    params(("document_id" = String, Path, format = Uuid)), request_body = DocumentSettings,
    responses((status = 200, body = Document), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
pub async fn set_settings(
    State(state): State<DocumentsState>,
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

/// 공유 목록(소유 권한자).
#[utoipa::path(get, path = "/api/documents/{document_id}/shares",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Share]), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn list_shares(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Vec<Share>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    require_manager(&mut tx, id).await?;
    let rows: Vec<Share> = sqlx::query_as(
        "SELECT s.id, s.organization_id, s.grantee_user_id, u.email, u.display_name, s.permission FROM document_shares s
         JOIN users u ON u.id = s.grantee_user_id WHERE s.document_id = $1 ORDER BY s.created_at",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(rows))
}

/// 공유 추가(소유 권한자). 받는 사람은 그 이메일을 제공자가 검증한 계정으로 로그인해야 한다.
#[utoipa::path(post, path = "/api/documents/{document_id}/shares",
    params(("document_id" = String, Path, format = Uuid)), request_body = CreateShare,
    responses((status = 201, body = Share), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn create_share(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<CreateShare>,
) -> Result<(StatusCode, Json<Share>), AppError> {
    let email = fvoci_core::validate::validate_email(&input.email)?;
    let mut tx = state.db.begin_as(user).await?;
    let share_id: Uuid = sqlx::query_scalar("SELECT app_share_document($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(&email)
        .bind(&input.permission)
        .bind(input.organization_id)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let share: Share = sqlx::query_as(
        "SELECT s.id, s.organization_id, s.grantee_user_id, u.email, u.display_name, s.permission
         FROM document_shares s JOIN users u ON u.id = s.grantee_user_id WHERE s.id = $1",
    )
    .bind(share_id)
    .fetch_one(tx.conn())
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(share)))
}

/// 공유 철회(소유 권한자). 다음 요청부터 접근이 막힌다.
#[utoipa::path(delete, path = "/api/document-shares/{share_id}",
    params(("share_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn delete_share(
    State(state): State<DocumentsState>,
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

/// 쓸 수 있는 문서 템플릿(읽을 수 있는 것만, 제목순).
#[utoipa::path(get, path = "/api/document-templates", operation_id = "document_templates", params(TemplateQuery),
    responses((status = 200, body = [DocumentTemplate]), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn templates(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<TemplateQuery>,
) -> Result<Json<Vec<DocumentTemplate>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    if let Some(ws) = query.workspace_id {
        fvoci_db::access::require_visible(&mut tx, fvoci_db::access::Kind::Workspace, ws).await?;
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

/// 템플릿 표시·해제(문서 소유 권한자).
#[utoipa::path(put, path = "/api/documents/{document_id}/template", operation_id = "set_document_template",
    params(("document_id" = String, Path, format = Uuid)), request_body = SetDocumentTemplate,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn set_template(
    State(state): State<DocumentsState>,
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
