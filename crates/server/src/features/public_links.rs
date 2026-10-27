//! 비로그인 공개 링크 (D-20). 소유 권한자가 만들고 만료·철회한다. 토큰 원문은 만들 때 한 번만 돌려준다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::features::accounts::random_token;
use crate::features::documents::require_manager;
use crate::http::AppState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::DocumentId;
use fvoci_db::new_ids;
use fvoci_http::session::{CurrentUser, token_hash};

/// 공개 링크(소유 권한자가 보는 목록 항목).
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicLink {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub expires_at: Option<OffsetDateTime>,
    /// 철회·만료되지 않았는지.
    pub active: bool,
}

/// 공개 링크 생성 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePublicLink {
    /// 만료 시각(RFC 3339). 없으면 철회할 때까지 유효하다.
    #[serde(default, with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub expires_at: Option<OffsetDateTime>,
}

/// 만든 링크. `url`은 이때 한 번만 받을 수 있다.
#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedPublicLink {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub url: String,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub expires_at: Option<OffsetDateTime>,
}

/// 공개 보기 응답.
#[derive(Debug, Serialize, ToSchema)]
pub struct PublicDocument {
    pub title: String,
    #[schema(value_type = Object)]
    pub body: Value,
    pub schema_version: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// 공개 링크 목록(소유 권한자).
#[utoipa::path(get, path = "/api/documents/{document_id}/public-links", operation_id = "list_public_links",
    params(("document_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [PublicLink]), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
) -> Result<Json<Vec<PublicLink>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    require_manager(&mut tx, id).await?;
    let rows: Vec<(Uuid, OffsetDateTime, Option<OffsetDateTime>, bool)> = sqlx::query_as(
        "SELECT id, created_at, expires_at,
                revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now())
         FROM public_links WHERE document_id = $1 ORDER BY created_at DESC",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, created_at, expires_at, active)| PublicLink {
                id,
                created_at,
                expires_at,
                active,
            })
            .collect(),
    ))
}

/// 공개 링크 생성(소유 권한자).
#[utoipa::path(post, path = "/api/documents/{document_id}/public-links", operation_id = "create_public_link",
    params(("document_id" = String, Path, format = Uuid)), request_body = CreatePublicLink,
    responses((status = 201, body = CreatedPublicLink), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<DocumentId>,
    Json(input): Json<CreatePublicLink>,
) -> Result<(StatusCode, Json<CreatedPublicLink>), AppError> {
    let token = random_token();
    let mut tx = state.db.begin_as(user).await?;
    let link: Uuid = sqlx::query_scalar("SELECT app_create_public_link($1, $2, $3, $4)")
        .bind(id)
        .bind(token_hash(&token))
        .bind(input.expires_at)
        .bind(new_ids(2))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    let url = state
        .config
        .public_url
        .join(&format!("/p/{token}"))
        .map_err(|e| AppError::Config(format!("public link url: {e}")))?
        .to_string();
    Ok((
        StatusCode::CREATED,
        Json(CreatedPublicLink {
            id: link,
            url,
            expires_at: input.expires_at,
        }),
    ))
}

/// 공개 링크 철회(소유 권한자). 다음 요청부터 열리지 않는다.
#[utoipa::path(delete, path = "/api/public-links/{link_id}", operation_id = "revoke_public_link",
    params(("link_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn revoke(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(link): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_revoke_public_link($1, $2)")
        .bind(link)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 공개 보기(로그인 불필요, 읽기 전용). 철회·만료되면 404. 캐시에 남지 않게 `no-store`.
#[utoipa::path(get, path = "/api/public/{token}", operation_id = "view_public_document", security(),
    params(("token" = String, Path)),
    responses((status = 200, body = PublicDocument), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn view(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Response, AppError> {
    if token.len() > 128 {
        return Err(AppError::NotFound);
    }
    let mut tx = state.db.begin_anonymous().await?;
    let row: Option<(String, Value, i32, OffsetDateTime)> = sqlx::query_as(
        "SELECT title, body, schema_version, updated_at FROM app_public_document($1)",
    )
    .bind(token_hash(&token))
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    let (title, mut body, schema_version, updated_at) = row.ok_or(AppError::NotFound)?;
    // 공개 보기는 로그인하지 않은 사람에게 내부 id를 내보내지 않는다. 참조 노드는 자리표시 글자로 바꾼다.
    crate::features::references::strip_references(&mut body);
    let doc = PublicDocument {
        title,
        body,
        schema_version,
        updated_at,
    };
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(doc)).into_response())
}
