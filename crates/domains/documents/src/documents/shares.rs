//! 공유 자원 읽기와 명령(D-20·D-113·D-114). 역할·상한은 SQL이 계산한다.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{DocumentId, OrganizationId, UserId};
use fvoci_db::new_ids;
use fvoci_http::session::{CurrentUser, EXPECTED_USER_IF_SENT_DOC};
use serde::Deserialize;
use utoipa::IntoParams;
use uuid::Uuid;

use super::{CreateShare, Share, ShareRecipient, require_manager};
use crate::state::DocumentsState;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ShareRecipientQuery {
    pub organization_id: OrganizationId,
    pub email: String,
}

const SHARE_COLUMNS: &str = "SELECT s.id, s.organization_id, s.grantee_user_id,
    r.email, r.display_name, s.permission, r.recipient_role, r.max_permission
    FROM document_shares s CROSS JOIN LATERAL
    app_document_share_recipient_for_share(s.id) r";

fn share_error(error: sqlx::Error) -> AppError {
    if error.as_database_error().is_some_and(|db| {
        db.code().as_deref() == Some("22023")
            && db.message() == "the recipient role only allows read access"
    }) {
        return AppError::invalid(
            "permission",
            InvalidReason::OutOfRange,
            None,
            "the recipient role only allows read access",
        );
    }
    AppError::from_db_function(error)
}

#[utoipa::path(get, path = "/api/documents/{document_id}/share-recipient",
    operation_id = "document_share_recipient",
    params(("document_id" = DocumentId, Path), ShareRecipientQuery),
    responses((status = 200, body = ShareRecipient), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn recipient(
    State(state): State<DocumentsState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Query(input): Query<ShareRecipientQuery>,
) -> Result<Json<ShareRecipient>, AppError> {
    let email = fvoci_core::validate::validate_email(&input.email)?;
    let mut tx = state.db.begin_as(user).await?;
    let recipient = sqlx::query_as("SELECT * FROM app_document_share_recipient($1, $2::text, $3)")
        .bind(id)
        .bind(email)
        .bind(input.organization_id)
        .fetch_optional(tx.conn())
        .await
        .map_err(AppError::from_db_function)?
        .ok_or(AppError::NotFound)?;
    tx.commit().await?;
    Ok(Json(recipient))
}

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
    let shares = sqlx::query_as(&format!(
        "{SHARE_COLUMNS} WHERE s.document_id = $1 ORDER BY s.created_at"
    ))
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(shares))
}

#[utoipa::path(post, path = "/api/documents/{document_id}/shares",
    params(("document_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = Option<UserId>, Header, description = EXPECTED_USER_IF_SENT_DOC)), request_body = CreateShare,
    responses((status = 201, body = Share), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
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
        .bind(email)
        .bind(input.permission)
        .bind(input.organization_id)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(share_error)?;
    let share = sqlx::query_as(&format!("{SHARE_COLUMNS} WHERE s.id = $1"))
        .bind(share_id)
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(share)))
}

#[utoipa::path(delete, path = "/api/document-shares/{share_id}",
    params(("share_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = Option<UserId>, Header, description = EXPECTED_USER_IF_SENT_DOC)),
    responses((status = 204), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
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
