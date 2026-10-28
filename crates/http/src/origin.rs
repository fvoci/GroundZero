//! 상태를 바꾸는 요청의 `Origin` 검사(CSRF 방지).

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use fvoci_core::error::ErrorBody;

/// 공개 원점 문자열(`scheme://host[:port]`). Origin 검사는 미들웨어 상태에서 `FromRef`로 꺼낸다.
#[derive(Debug, Clone)]
pub struct PublicOrigin(pub String);

impl PublicOrigin {
    /// API와 실시간 연결이 같은 공개 원점 하나만 허용한다.
    pub fn matches(&self, headers: &HeaderMap) -> bool {
        headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            == Some(self.0.as_str())
    }
}

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
    if !safe && !public.matches(request.headers()) {
        let mut response = (
            StatusCode::FORBIDDEN,
            Json(ErrorBody::new(
                "bad_origin",
                "cross-origin request rejected".into(),
                fvoci_core::logging::current_request_id(),
            )),
        )
            .into_response();
        response
            .extensions_mut()
            .insert(fvoci_core::logging::ErrorCode("bad_origin"));
        return response;
    }
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn public_origin_matches_the_exact_scheme_host_and_port() {
        let origin = PublicOrigin("https://example.test:8443".into());
        for (value, expected) in [
            (None, false),
            (Some("https://example.test:8443"), true),
            (Some("http://example.test:8443"), false),
            (Some("https://example.test"), false),
            (Some("https://other.test:8443"), false),
            (Some("null"), false),
        ] {
            let mut headers = HeaderMap::new();
            if let Some(value) = value {
                headers.insert(header::ORIGIN, HeaderValue::from_static(value));
            }
            assert_eq!(origin.matches(&headers), expected, "{value:?}");
        }
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_bytes(b"\xff").unwrap());
        assert!(!origin.matches(&headers));
    }
}
