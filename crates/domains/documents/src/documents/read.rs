//! 문서 자원 읽기와 ETag 표현.

use crate::state::DocumentsState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{DocumentId, OrganizationId, UserId};
use fvoci_http::cache;
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::CurrentUser;
use serde_json::Value;
use time::OffsetDateTime;

use super::{Document, SharedDocument, load_with_access, to_document};

/// 나에게 공유된 문서(검증된 이메일 기준). 현재 조직 범위(D-111): 그 조직에서 받은 공유만.
#[utoipa::path(get, path = "/api/me/shared", operation_id = "shared_with_me",
    params(("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = [SharedDocument]), (status = 400, body = ErrorBody),
              (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "documents")]
pub async fn shared_with_me(
    State(state): State<DocumentsState>,
    scope: fvoci_http::organization::OrgScope,
) -> Result<Json<Vec<SharedDocument>>, AppError> {
    let mut tx = state.db.begin_as(scope.user).await?;
    let rows: Vec<(DocumentId, String, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, title, permission, shared_by, updated_at FROM app_shared_with_me($1)",
    )
    .bind(scope.organization)
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

/// 문서 표현(D-77): 단건 조회가 내보내는 바이트와 그 강한 ETag. 실제 조회 바이트로 계산한다.
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

/// 쓰기 결과는 ETag 없이 문서 자원만 돌려준다(#212 A, D-77 개정 대기).
pub(crate) fn document_reply(doc: &Document) -> Response {
    Json(doc).into_response()
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
    State(state): State<DocumentsState>,
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
        HeaderValue::from_static("private, no-cache, no-transform"),
    );
    Ok(response)
}

pub(crate) fn stale_reply(current: Document, sent: &Value) -> Response {
    StaleWrite::new(
        "conflict",
        "the document changed; reload it before retrying",
        current,
        sent,
    )
    .into_response()
}
