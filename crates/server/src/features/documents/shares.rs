//! 공유 자원 읽기와 명령(D-20·D-113·D-114). 역할·상한은 SQL이 계산한다.

use axum::extract::{FromRef, Path, Query, State};
use axum::http::StatusCode;
use axum::middleware::from_extractor_with_state;
use axum::routing::{delete, get};
use axum::{Json, Router};
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{DocumentId, OrganizationId, UserId};
use fvoci_core::share::{SharePermission, ShareRecipientRole};
use fvoci_db::new_ids;
use fvoci_http::session::{CurrentUser, ExpectedUserIfSent, SessionState};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::require_manager;
use crate::http::AppState;

/// 저장된 공유 권한과 수신자의 현재 역할 상한. 기존 write 저장값도 그대로 반환한다.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct Share {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub organization_id: OrganizationId,
    pub grantee_user_id: UserId,
    pub email: Option<String>,
    pub display_name: String,
    pub permission: SharePermission,
    /// 개인 노트에는 공간 역할을 적용하지 않아 null이다.
    pub recipient_role: Option<ShareRecipientRole>,
    pub max_permission: SharePermission,
}

/// 공유할 사람의 계산 역할과 상한. 조회만으로 공유를 만들지 않는다.
#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ShareRecipient {
    pub user_id: UserId,
    pub email: Option<String>,
    pub display_name: String,
    pub recipient_role: Option<ShareRecipientRole>,
    pub max_permission: SharePermission,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ShareRecipientQuery {
    pub organization_id: OrganizationId,
    pub email: String,
}

/// 공유 추가 요청. 같은 이메일이면 저장 권한을 바꾼다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateShare {
    pub organization_id: Option<OrganizationId>,
    pub email: String,
    pub permission: SharePermission,
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
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Query(input): Query<ShareRecipientQuery>,
) -> Result<Json<ShareRecipient>, AppError> {
    let email = crate::features::members::validate_email(&input.email)?;
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
    State(state): State<AppState>,
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
           ("x-fvoci-expected-user" = Option<UserId>, Header)), request_body = CreateShare,
    responses((status = 201, body = Share), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
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
           ("x-fvoci-expected-user" = Option<UserId>, Header)),
    responses((status = 204), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "documents")]
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

/// 공유 자원과 명령. 모든 명령에 선택형 기대 사용자 검사를 공통 적용한다(D-96).
pub fn router(state: &AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/documents/{document_id}/shares",
            get(list_shares).post(create_share),
        )
        .route("/documents/{document_id}/share-recipient", get(recipient))
        .route("/document-shares/{share_id}", delete(delete_share))
        .route_layer(
            from_extractor_with_state::<ExpectedUserIfSent, SessionState>(SessionState::from_ref(
                state,
            )),
        )
}
