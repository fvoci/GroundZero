//! 상태를 바꾸는 요청의 `Origin` 검사(CSRF 방지).

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use fvoci_core::error::ErrorBody;

/// 공개 원점 문자열(`scheme://host[:port]`). Origin 검사는 미들웨어 상태에서 `FromRef`로 꺼낸다.
#[derive(Debug, Clone)]
pub struct PublicOrigin(pub String);

/// 상태를 바꾸는 요청은 브라우저의 `Origin`이 공개 원점과 같아야 한다 (CSRF 방지).
/// SameSite=Lax 쿠키만으로는 JSON API의 교차 출처 요청을 다 막지 못한다.
pub async fn require_same_origin(
    State(public): State<PublicOrigin>,
    request: Request,
    next: Next,
) -> Response {
    let safe = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !safe {
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        if origin != Some(public.0.as_str()) {
            let mut response = (
                StatusCode::FORBIDDEN,
                Json(ErrorBody {
                    error: "bad_origin",
                    message: "cross-origin request rejected".into(),
                    request_id: fvoci_core::logging::current_request_id(),
                }),
            )
                .into_response();
            response
                .extensions_mut()
                .insert(fvoci_core::logging::ErrorCode("bad_origin"));
            return response;
        }
    }
    next.run(request).await
}
