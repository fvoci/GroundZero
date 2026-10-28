//! 문서 공유 처리기. 역할·권한·감사는 SQL이 검사한다(D-20·D-113·D-114).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{DocumentId, OrganizationId, UserId};
use fvoci_db::new_ids;
use fvoci_http::session::CurrentUser;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::require_manager;
use crate::http::AppState;

/// 공유 항목.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Share {
    pub organization_id: OrganizationId,
    pub grantee_user_id: UserId,
    pub display_name: String,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub email: Option<String>,
    /// `read` 또는 `write`.
    pub permission: String,
}

/// 공유 추가 요청. 같은 이메일이면 권한을 바꾼다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShare {
    pub organization_id: Option<OrganizationId>,
    pub email: String,
    /// `read` 또는 `write`.
    pub permission: String,
}

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
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<CreateShare>,
) -> Result<(StatusCode, Json<Share>), AppError> {
    let email = crate::features::members::validate_email(&input.email)?;
    let mut tx = state.db.begin_as(user).await?;
    let share_id: Uuid = sqlx::query_scalar("SELECT app_share_document($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(&email)
        .bind(&input.permission)
        .bind(input.organization_id)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(share_error)?;
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
