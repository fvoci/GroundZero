//! 캐시 범위와 조건부 요청(D-77). CDN이 없는 셀프호스트와 CDN 뒤에서 같은 머리글을 낸다.
//!
//! - `/api`·`/auth`·`/healthz`: 정확한 문서 단건 GET·HEAD의 200·304만
//!   `private, no-cache, no-transform`, 나머지는 모두 `no-store, no-transform`. 공유 캐시에 남지 않고, 중간 장치가
//!   본문을 압축·변환하며 ETag를 약하게 바꾸지 않게 한다.
//! - 해시 파일명 정적 자원(`/assets/*`, `/rhwp/<입력 해시>/*`): `public, max-age=31536000, immutable`.
//! - 그 밖의 정적 응답(HTML 진입점·SPA 경로·`/pptx-frame.js`·`/rhwp/manifest.json`): `no-cache`(매번 재검증).
//! - 정적 오류 응답은 `no-store`. 정적 응답은 압축 사본을 고를 수 있으므로 `Vary: Accept-Encoding`.

use axum::extract::Request;
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
pub use fvoci_http::cache::{if_match, none_match, strong_etag};

/// 재검증을 허용하는 개인 응답. 브라우저만 저장하고 쓰기 전에 매번 서버에 확인한다.
pub const PRIVATE_REVALIDATE: &str = "private, no-cache, no-transform";
/// API·인증의 기본값.
pub const NO_STORE: &str = "no-store, no-transform";
const IMMUTABLE: &str = "public, max-age=31536000, immutable";
const REVALIDATE: &str = "no-cache";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Api,
    Immutable,
    Revalidate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DocumentCache {
    Read,
    LegacyWrite,
    None,
}

fn document_cache(method: &Method, path: &str) -> DocumentCache {
    let Some(rest) = path.strip_prefix("/api/documents/") else {
        return DocumentCache::None;
    };
    if rest.is_empty() {
        return DocumentCache::None;
    }
    if !rest.contains('/') {
        return match *method {
            Method::GET | Method::HEAD => DocumentCache::Read,
            Method::PUT => DocumentCache::LegacyWrite,
            _ => DocumentCache::None,
        };
    }
    if *method == Method::POST
        && let Some((id, tail)) = rest.split_once('/')
        && !id.is_empty()
        && let Some(revision) = tail
            .strip_prefix("revisions/")
            .and_then(|tail| tail.strip_suffix("/restore"))
        && !revision.is_empty()
        && !revision.contains('/')
    {
        DocumentCache::LegacyWrite
    } else {
        DocumentCache::None
    }
}

fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn scope(path: &str) -> Scope {
    if under(path, "/api") || under(path, "/auth") || path == "/healthz" {
        Scope::Api
    } else if path.starts_with("/assets/") {
        Scope::Immutable
    } else if let Some(rest) = path.strip_prefix("/rhwp/") {
        if rest == "manifest.json" {
            Scope::Revalidate
        } else {
            Scope::Immutable
        }
    } else {
        Scope::Revalidate
    }
}

/// 모든 응답의 마지막에 범위별 `Cache-Control`을 정한다(라우터 맨 바깥 층).
pub async fn policy(request: Request, next: Next) -> Response {
    let scope = scope(request.uri().path());
    let document = document_cache(request.method(), request.uri().path());
    let mut response = next.run(request).await;
    let status = response.status();
    let ok = status == StatusCode::OK || status == StatusCode::NOT_MODIFIED;
    let headers = response.headers_mut();
    let value = match scope {
        Scope::Api => {
            let revalidate = document == DocumentCache::Read && ok;
            // #212 5행, PR4c가 본문 PUT을 지울 때 이 예외도 지운다.
            // 정확한 PUT 단건·POST 리비전 복원 200만 다음 If-Match용 ETag를 남긴다.
            let legacy_etag = document == DocumentCache::LegacyWrite && status == StatusCode::OK;
            if !revalidate && !legacy_etag {
                headers.remove(header::ETAG);
            }
            if revalidate {
                PRIVATE_REVALIDATE
            } else {
                NO_STORE
            }
        }
        Scope::Immutable | Scope::Revalidate => {
            headers.insert(header::VARY, HeaderValue::from_static("accept-encoding"));
            if !(status.is_success() || status == StatusCode::NOT_MODIFIED) {
                "no-store"
            } else if scope == Scope::Immutable {
                IMMUTABLE
            } else {
                REVALIDATE
            }
        }
    };
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, middleware};
    use tower::ServiceExt;

    // SR-221-3 / D-77: 처리기가 성공 응답에 잘못 단 머리글도 바깥 층이 강제한다.
    async fn downstream(method: Method, path: &str, status: StatusCode) -> Response {
        Router::new()
            .fallback(move || async move {
                (
                    status,
                    [
                        (header::CACHE_CONTROL, PRIVATE_REVALIDATE),
                        (header::ETAG, "\"downstream\""),
                    ],
                )
            })
            .layer(middleware::from_fn(policy))
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn assert_no_store(response: &Response) {
        assert_eq!(response.headers()[header::CACHE_CONTROL], NO_STORE);
        assert!(!response.headers().contains_key(header::ETAG));
    }

    #[tokio::test]
    async fn policy_overrides_sensitive_success_headers_outside_the_allowlist() {
        for path in [
            "/api/workspaces",
            "/api",
            "/auth/login",
            "/healthz",
            "/api/documents",
            "/api/documents/",
            "/api/documents/x/",
            "/api/documents/x/revisions",
            "/api/documents/x/collab",
            "/api/documents-other/x",
            "/api/documents//revisions/1/restore",
            "/api/files/x/content?inline=true",
        ] {
            for method in [Method::GET, Method::HEAD] {
                for status in [StatusCode::OK, StatusCode::NOT_MODIFIED] {
                    let response = downstream(method.clone(), path, status).await;
                    assert_no_store(&response);
                }
            }
        }
    }

    #[tokio::test]
    async fn policy_preserves_only_single_document_reads_and_two_legacy_write_etags() {
        for method in [Method::GET, Method::HEAD] {
            for status in [StatusCode::OK, StatusCode::NOT_MODIFIED] {
                let response =
                    downstream(method.clone(), "/api/documents/x?view=body", status).await;
                assert_eq!(
                    response.headers()[header::CACHE_CONTROL],
                    PRIVATE_REVALIDATE
                );
                assert_eq!(response.headers()[header::ETAG], "\"downstream\"");
            }
        }
        // #212 5행, PR4c가 본문 PUT을 지울 때 이 예외도 지운다
        for (method, path) in [
            (Method::PUT, "/api/documents/x"),
            (Method::POST, "/api/documents/x/revisions/1/restore"),
        ] {
            let response = downstream(method, path, StatusCode::OK).await;
            assert_eq!(response.headers()[header::CACHE_CONTROL], NO_STORE);
            assert_eq!(response.headers()[header::ETAG], "\"downstream\"");
        }
    }

    #[tokio::test]
    async fn policy_removes_etags_for_disallowed_methods_and_statuses() {
        for (method, path) in [
            (Method::POST, "/api/documents/x"),
            (Method::DELETE, "/api/documents/x"),
            (Method::PUT, "/api/documents/x/"),
            (Method::GET, "/api/documents/x/revisions/1/restore"),
            (Method::PUT, "/api/documents/x/revisions/1/restore"),
            (Method::POST, "/api/documents/x/revisions/1/restore/"),
            (Method::POST, "/api/documents/x/revisions//restore"),
            (Method::POST, "/api/documents/x/revisions/1/extra/restore"),
        ] {
            assert_no_store(&downstream(method, path, StatusCode::OK).await);
        }
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::CONFLICT,
            StatusCode::PRECONDITION_FAILED,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::SERVICE_UNAVAILABLE,
        ] {
            for method in [Method::GET, Method::HEAD, Method::PUT] {
                assert_no_store(&downstream(method, "/api/documents/x", status).await);
            }
            assert_no_store(
                &downstream(Method::POST, "/api/documents/x/revisions/1/restore", status).await,
            );
        }
        for status in [StatusCode::CREATED, StatusCode::NOT_MODIFIED] {
            assert_no_store(&downstream(Method::PUT, "/api/documents/x", status).await);
            assert_no_store(
                &downstream(Method::POST, "/api/documents/x/revisions/1/restore", status).await,
            );
        }
        assert_no_store(
            &downstream(
                Method::GET,
                "/api/documents/x/collab",
                StatusCode::SWITCHING_PROTOCOLS,
            )
            .await,
        );
    }

    #[test]
    fn scopes_follow_the_path() {
        assert_eq!(scope("/api/documents/x"), Scope::Api);
        assert_eq!(scope("/api"), Scope::Api);
        assert_eq!(scope("/auth/callback"), Scope::Api);
        assert_eq!(scope("/healthz"), Scope::Api);
        assert_eq!(scope("/apix"), Scope::Revalidate);
        assert_eq!(scope("/assets/index-abc.js"), Scope::Immutable);
        assert_eq!(
            scope("/rhwp/0123456789abcdef/rhwp_bg.wasm"),
            Scope::Immutable
        );
        assert_eq!(scope("/rhwp/manifest.json"), Scope::Revalidate);
        assert_eq!(scope("/"), Scope::Revalidate);
        assert_eq!(scope("/w/ws/docs/1"), Scope::Revalidate);
        assert_eq!(scope("/pptx-frame.js"), Scope::Revalidate);
    }

    /// `deploy/cdn`의 예시가 이 파일의 경로 범위와 맞는지(문서와 설정이 어긋나지 않게).
    #[test]
    fn cdn_examples_match_the_scopes() {
        let parse = |text: &str| -> serde_json::Value {
            serde_json::from_str(text).unwrap_or(serde_json::Value::Null)
        };
        let sensitive = [
            "\"/api\"",
            "\"/api/\"",
            "\"/auth\"",
            "\"/auth/\"",
            "\"/healthz\"",
        ];

        let rules = parse(include_str!(
            "../../../../deploy/cdn/cloudflare-cache-rules.json"
        ));
        let rules = rules["rules"].as_array().cloned().unwrap_or_default();
        let bypass = rules.last().cloned().unwrap_or_default();
        assert_eq!(bypass["action_parameters"]["cache"], false);
        assert_eq!(bypass["action_parameters"]["respect_strong_etags"], true);
        let expression = bypass["expression"].as_str().unwrap_or_default().to_owned();
        for path in sensitive {
            assert!(expression.contains(path), "bypass misses {path}");
        }
        for rule in &rules {
            if rule["action_parameters"]["cache"] == true {
                let e = rule["expression"].as_str().unwrap_or_default();
                assert!(!e.contains("/api") && !e.contains("/auth") && !e.contains("/healthz"));
            }
        }
        let compression = parse(include_str!(
            "../../../../deploy/cdn/cloudflare-compression-rules.json"
        ));
        assert_eq!(
            compression["rules"][0]["expression"].as_str(),
            Some(expression.as_str())
        );

        let disabled = "4135ea2d-6df8-44a3-9df3-4b5a84be39ad";
        let optimized = "658327ea-f89d-4fab-a63d-7e88639e58f6";
        let all_viewer = "216adef6-5c7f-47e4-b989-5492eafa07d3";
        let cf = parse(include_str!(
            "../../../../deploy/cdn/cloudfront-cache-behaviors.json"
        ));
        assert_eq!(cf["DefaultCacheBehavior"]["CachePolicyId"], disabled);
        assert_eq!(
            cf["DefaultCacheBehavior"]["OriginRequestPolicyId"],
            all_viewer
        );
        let items = cf["CacheBehaviors"]["Items"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(cf["CacheBehaviors"]["Quantity"], items.len());
        let mut patterns = Vec::new();
        for item in &items {
            let pattern = item["PathPattern"].as_str().unwrap_or_default();
            patterns.push(pattern.to_owned());
            let policy = item["CachePolicyId"].as_str().unwrap_or_default();
            match scope(&pattern.replace('*', "x.js")) {
                Scope::Api => {
                    assert_eq!(policy, disabled, "{pattern}");
                    assert_eq!(item["OriginRequestPolicyId"], all_viewer, "{pattern}");
                    assert_eq!(item["Compress"], false, "{pattern}");
                }
                Scope::Immutable => assert_eq!(policy, optimized, "{pattern}"),
                Scope::Revalidate => assert_eq!(policy, disabled, "{pattern}"),
            }
            let methods = item["AllowedMethods"]["Items"].to_string();
            if pattern != "/healthz" && scope(&pattern.replace('*', "x")) == Scope::Api {
                for m in ["PUT", "POST", "PATCH", "DELETE"] {
                    assert!(methods.contains(m), "{pattern} must pass {m}");
                }
            }
        }
        for required in ["/api/*", "/auth/*", "/healthz"] {
            assert!(patterns.iter().any(|p| p == required), "missing {required}");
        }
        // 더 구체적인 manifest 행동이 해시 자원 행동보다 앞이어야 한다(순서대로 맞춘다).
        let manifest = patterns.iter().position(|p| p == "/rhwp/manifest.json");
        let rhwp = patterns.iter().position(|p| p == "/rhwp/*");
        assert!(manifest.is_some() && manifest < rhwp);
    }
}
