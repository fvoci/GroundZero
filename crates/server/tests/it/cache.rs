#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! ETag·조건부 요청·캐시 범위(D-77). CDN 없이 앱이 내는 머리글과 판정만 본다(실제 CDN은 NOT RUN).

use crate::common;

use axum::Router;
use axum::http::{HeaderMap, Method, StatusCode};
use common::{TestUser, create_doc, create_workspace, login};
use fvoci_http::session::EXPECTED_USER_HEADER;
use fvoci_server::app::build_state;
use serde_json::{Value, json};

pub(crate) fn header(h: &HeaderMap, name: &str) -> Option<String> {
    h.get(name).map(|v| v.to_str().unwrap().to_owned())
}

pub(crate) struct Got {
    pub(crate) status: StatusCode,
    pub(crate) headers: HeaderMap,
    pub(crate) json: Value,
    pub(crate) raw: Vec<u8>,
}

pub(crate) async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    extra: &[(&str, &str)],
    body: Option<Value>,
) -> Got {
    let mut headers: Vec<(&str, &str)> = extra.to_vec();
    // 문서 저장은 기대 사용자를 함께 보낸다(웹과 같게 세션의 사용자, `expected_user.rs`).
    let expected = user.map(|u| u.id.0.to_string());
    if method == Method::PUT
        && let Some(id) = expected.as_deref()
    {
        headers.push((EXPECTED_USER_HEADER, id));
    }
    if body.is_some() {
        headers.push(("content-type", "application/json"));
    }
    let bytes = body
        .map(|b| serde_json::to_vec(&b).unwrap())
        .unwrap_or_default();
    let (status, headers, raw) = common::send_raw(app, method, uri, user, &headers, bytes).await;
    let json = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    Got {
        status,
        headers,
        json,
        raw,
    }
}

pub(crate) const NO_STORE: &str = "no-store, no-transform";

/// 캐시 범위: 해시 자원은 오래, HTML·해시 없는 자원은 재검증, 민감 응답은 공유 캐시 금지. 없는 해시 자원은 404.
#[tokio::test]
async fn cache_scopes_keep_sensitive_responses_out_of_shared_caches() {
    let dir = std::env::temp_dir().join(format!("fvoci-web-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::create_dir_all(dir.join("rhwp/0123456789abcdef")).unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>fvoci</title>",
    )
    .unwrap();
    std::fs::write(dir.join("assets/index-abc123.js"), "console.log(1)").unwrap();
    std::fs::write(dir.join("pptx-frame.js"), "void 0").unwrap();
    std::fs::write(
        dir.join("rhwp/manifest.json"),
        r#"{"dir":"0123456789abcdef"}"#,
    )
    .unwrap();
    std::fs::write(dir.join("rhwp/0123456789abcdef/rhwp.js"), "export {}").unwrap();
    let state = build_state(common::config_with(&[(
        "FVOCI_WEB_DIR",
        dir.to_str().unwrap(),
    )]))
    .unwrap();
    let app = common::app(&state);

    let immutable = "public, max-age=31536000, immutable";
    for (path, status, cache) in [
        ("/assets/index-abc123.js", StatusCode::OK, immutable),
        ("/rhwp/0123456789abcdef/rhwp.js", StatusCode::OK, immutable),
        ("/rhwp/manifest.json", StatusCode::OK, "no-cache"),
        ("/", StatusCode::OK, "no-cache"),
        ("/w/some/route", StatusCode::OK, "no-cache"),
        ("/pptx-frame.js", StatusCode::OK, "no-cache"),
        (
            "/assets/index-missing.js",
            StatusCode::NOT_FOUND,
            "no-store",
        ),
    ] {
        let r = call(&app, Method::GET, path, None, &[], None).await;
        assert_eq!(r.status, status, "{path}");
        assert_eq!(
            header(&r.headers, "cache-control").as_deref(),
            Some(cache),
            "{path}"
        );
        assert_eq!(
            header(&r.headers, "vary").as_deref(),
            Some("accept-encoding"),
            "{path}"
        );
        if status == StatusCode::NOT_FOUND {
            assert!(
                !String::from_utf8_lossy(&r.raw).contains("<title>"),
                "{path}"
            );
        }
    }

    // 민감 응답: 성공·오류·인증·내보내기·공유 링크 모두 공유 캐시에 남지 않는다.
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "범위").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let id = doc["id"].as_str().unwrap();
    let cases: Vec<(Method, String, Option<&TestUser>, Option<Value>)> = vec![
        (Method::GET, "/healthz".into(), None, None),
        (Method::GET, "/api/me".into(), None, None),
        (Method::GET, "/api/me".into(), Some(&alice), None),
        (Method::GET, "/api/workspaces".into(), Some(&alice), None),
        (
            Method::GET,
            format!("/api/documents/{id}/revisions"),
            Some(&alice),
            None,
        ),
        (
            Method::GET,
            format!("/api/workspaces/{ws}/export"),
            Some(&alice),
            None,
        ),
        (Method::GET, "/api/me/export".into(), Some(&alice), None),
        (Method::GET, "/api/public/not-a-token".into(), None, None),
        (Method::GET, "/api/nothing-here".into(), Some(&alice), None),
        (Method::GET, "/auth/login".into(), None, None),
        (
            Method::POST,
            format!("/api/documents/{id}/revisions/1/export"),
            Some(&alice),
            Some(json!({ "format": "markdown" })),
        ),
        (Method::POST, "/auth/logout".into(), Some(&alice), None),
    ];
    for (method, path, user, body) in cases {
        let r = call(&app, method.clone(), &path, user, &[], body).await;
        let cache = header(&r.headers, "cache-control").unwrap_or_default();
        assert_eq!(cache, NO_STORE, "{method} {path} ({})", r.status);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
