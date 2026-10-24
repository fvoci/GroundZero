//! 캐시 범위와 조건부 요청(D-77). CDN이 없는 셀프호스트와 CDN 뒤에서 같은 머리글을 낸다.
//!
//! - `/api`·`/auth`·`/healthz`: 핸들러가 재검증을 허용한 GET·HEAD의 200·304(문서 단건 조회)만
//!   `private, no-cache, no-transform`, 나머지는 모두 `no-store, no-transform`. 공유 캐시에 남지 않고, 중간 장치가
//!   본문을 압축·변환하며 ETag를 약하게 바꾸지 않게 한다.
//! - 해시 파일명 정적 자원(`/assets/*`, `/rhwp/<입력 해시>/*`): `public, max-age=31536000, immutable`.
//! - 그 밖의 정적 응답(HTML 진입점·SPA 경로·`/pptx-frame.js`·`/rhwp/manifest.json`): `no-cache`(매번 재검증).
//! - 정적 오류 응답은 `no-store`. 정적 응답은 압축 사본을 고를 수 있으므로 `Vary: Accept-Encoding`.

use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use sha2::{Digest, Sha256};

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
    let safe = matches!(*request.method(), Method::GET | Method::HEAD);
    let mut response = next.run(request).await;
    let status = response.status();
    let ok = status == StatusCode::OK || status == StatusCode::NOT_MODIFIED;
    let headers = response.headers_mut();
    let value = match scope {
        Scope::Api => {
            let revalidate = safe
                && ok
                && headers
                    .get(header::CACHE_CONTROL)
                    .is_some_and(|v| v == PRIVATE_REVALIDATE);
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

/// 강한 ETag(따옴표 포함). 실제 응답 바이트와 요청한 사용자로 만든다. 같은 바이트라도 사용자가 다르면 값이
/// 달라 사용자를 바꾼 브라우저가 앞 사람의 검증자로 304를 받지 않는다. 앞의 표지는 계산 방식이 바뀌면 올린다.
pub fn strong_etag(user: &[u8], body: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"fvoci-representation-1\0");
    hasher.update(user);
    hasher.update(b"\0");
    hasher.update(body);
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut etag = String::with_capacity(34);
    etag.push('"');
    for byte in &digest[..16] {
        etag.push(char::from(HEX[usize::from(byte >> 4)]));
        etag.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    etag.push('"');
    etag
}

/// 조건 머리글에 담긴 엔터티 태그.
enum Tags {
    Absent,
    /// `*`
    Any,
    /// (약한지, 따옴표 포함 태그)
    List(Vec<(bool, String)>),
}

fn tags(headers: &HeaderMap, name: header::HeaderName) -> Tags {
    let mut found = false;
    let mut out = Vec::new();
    for value in headers.get_all(name) {
        found = true;
        // 읽을 수 없는 값은 아무것과도 맞지 않는 것으로 본다.
        let Ok(text) = value.to_str() else { continue };
        for part in text.split(',') {
            let part = part.trim();
            if part == "*" {
                return Tags::Any;
            }
            match part.strip_prefix("W/") {
                Some(tag) => out.push((true, tag.to_owned())),
                None if !part.is_empty() => out.push((false, part.to_owned())),
                None => {}
            }
        }
    }
    if found { Tags::List(out) } else { Tags::Absent }
}

/// `If-None-Match`가 지금 표현과 맞는지(약한 비교). 맞으면 304를 돌려준다.
pub fn none_match(headers: &HeaderMap, etag: &str) -> bool {
    match tags(headers, header::IF_NONE_MATCH) {
        Tags::Absent => false,
        Tags::Any => true,
        Tags::List(list) => list.iter().any(|(_, tag)| tag == etag),
    }
}

/// `If-Match` 판정(강한 비교). 머리글이 없으면 `None`, 있으면 맞는지. 약한 태그는 맞지 않는다(RFC 9110 13.1.1).
pub fn if_match(headers: &HeaderMap, etag: &str) -> Option<bool> {
    match tags(headers, header::IF_MATCH) {
        Tags::Absent => None,
        Tags::Any => Some(true),
        Tags::List(list) => Some(list.iter().any(|(weak, tag)| !weak && tag == etag)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(name: header::HeaderName, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            name,
            HeaderValue::from_str(value).unwrap_or(HeaderValue::from_static("")),
        );
        h
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

    #[test]
    fn etags_depend_on_bytes_and_user() {
        let a = strong_etag(b"u1", b"{}");
        assert!(a.starts_with('"') && a.ends_with('"') && a.len() == 34);
        assert_eq!(a, strong_etag(b"u1", b"{}"));
        assert_ne!(a, strong_etag(b"u2", b"{}"));
        assert_ne!(a, strong_etag(b"u1", b"{ }"));
    }

    #[test]
    fn etags_preserve_the_domain_and_truncated_digest_bytes() {
        for (user, body, expected) in [
            (
                b"".as_slice(),
                b"".as_slice(),
                "\"4a46a75d39f81999433449f43c77bfe1\"",
            ),
            (b"u1", b"{}", "\"e9139f4f87432a88b31104891eb1491e\""),
            (b"u2", b"{}", "\"a15e964cf2c9e415f3f33dc7c6cedf53\""),
            (b"u1", b"{ }", "\"b1c8bd8f4d20777303d4974130a6dcfe\""),
        ] {
            assert_eq!(strong_etag(user, body), expected);
        }

        // Binary users/bodies, including NUL and every byte value, must keep the
        // original domain separation, first 128 bits, padding and lowercase.
        let body: Vec<u8> = (0..=255).collect();
        let mut covers_zero_byte = false;
        for len in 0..=body.len() {
            let user = &body[len..];
            let bytes = &body[..len];
            let mut hasher = Sha256::new();
            hasher.update(b"fvoci-representation-1\0");
            hasher.update(user);
            hasher.update(b"\0");
            hasher.update(bytes);
            let digest = hasher.finalize();
            covers_zero_byte |= digest[..16].contains(&0);
            let hex: String = digest.iter().take(16).map(|b| format!("{b:02x}")).collect();
            assert_eq!(strong_etag(user, bytes), format!("\"{hex}\""));
        }
        assert!(
            covers_zero_byte,
            "corpus must exercise leading-zero padding"
        );
    }

    #[test]
    fn if_none_match_uses_weak_comparison() {
        let tag = "\"abc\"";
        assert!(none_match(&with(header::IF_NONE_MATCH, "\"abc\""), tag));
        assert!(none_match(&with(header::IF_NONE_MATCH, "W/\"abc\""), tag));
        assert!(none_match(
            &with(header::IF_NONE_MATCH, "\"x\", W/\"abc\""),
            tag
        ));
        assert!(none_match(&with(header::IF_NONE_MATCH, "*"), tag));
        assert!(!none_match(&with(header::IF_NONE_MATCH, "\"abd\""), tag));
        assert!(!none_match(&HeaderMap::new(), tag));
    }

    #[test]
    fn if_match_uses_strong_comparison() {
        let tag = "\"abc\"";
        assert_eq!(if_match(&HeaderMap::new(), tag), None);
        assert_eq!(
            if_match(&with(header::IF_MATCH, "\"abc\""), tag),
            Some(true)
        );
        assert_eq!(
            if_match(&with(header::IF_MATCH, "\"x\", \"abc\""), tag),
            Some(true)
        );
        assert_eq!(if_match(&with(header::IF_MATCH, "*"), tag), Some(true));
        assert_eq!(
            if_match(&with(header::IF_MATCH, "W/\"abc\""), tag),
            Some(false)
        );
        assert_eq!(
            if_match(&with(header::IF_MATCH, "\"abd\""), tag),
            Some(false)
        );
        assert_eq!(if_match(&with(header::IF_MATCH, ""), tag), Some(false));
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
