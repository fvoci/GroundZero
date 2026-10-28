#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! ETag·조건부 요청·캐시 범위(D-77). CDN 없이 앱이 내는 머리글과 판정만 본다(실제 CDN은 NOT RUN).

use crate::common;

use axum::Router;
use axum::http::{HeaderMap, Method, StatusCode};
use common::{TestUser, add_member, create_doc, create_workspace, doc_body, login, new_request_id};
use fvoci_http::session::EXPECTED_USER_HEADER;
use fvoci_server::app::build_state;
use serde_json::{Value, json};

#[path = "cache_routes.rs"]
mod routes;

fn header(h: &HeaderMap, name: &str) -> Option<String> {
    h.get(name).map(|v| v.to_str().unwrap().to_owned())
}

struct Got {
    status: StatusCode,
    headers: HeaderMap,
    json: Value,
    raw: Vec<u8>,
}

async fn call(
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

fn save_body(request_id: &str, base: i64, text: &str) -> Value {
    json!({ "request_id": request_id, "base_revision": base, "title": "문서", "body": doc_body(text) })
}

const REVALIDATE: &str = "private, no-cache, no-transform";
const NO_STORE: &str = "no-store, no-transform";

/// 같은 조회는 304, 바뀐 조회는 200. 권한 철회·로그아웃·사용자 전환 뒤에는 이전 ETag로 304를 받지 못한다.
/// 압축을 요청해도 API는 압축하지 않아 ETag가 같고 강하다.
#[tokio::test]
async fn document_reads_revalidate_only_after_checking_access() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "캐시").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let (bob, _) = add_member(&app, &state, &alice, &ws, "bob", "readonly").await;

    let first = call(&app, Method::GET, &uri, Some(&alice), &[], None).await;
    assert_eq!(first.status, StatusCode::OK);
    let e1 = header(&first.headers, "etag").unwrap();
    assert!(
        e1.starts_with('"') && e1.ends_with('"'),
        "strong etag: {e1}"
    );
    assert_eq!(
        header(&first.headers, "cache-control").as_deref(),
        Some(REVALIDATE)
    );
    assert_eq!(first.json["revision"], 1);

    // 압축 켜짐/꺼짐: 같은 표현, 같은 ETag, 앱은 압축하지 않는다.
    let gz = call(
        &app,
        Method::GET,
        &uri,
        Some(&alice),
        &[("accept-encoding", "gzip, br")],
        None,
    )
    .await;
    assert_eq!(header(&gz.headers, "etag").as_deref(), Some(e1.as_str()));
    assert_eq!(header(&gz.headers, "content-encoding"), None);
    assert_eq!(gz.raw, first.raw);

    // 같은 조회 → 304(약한 비교: CDN이 W/로 바꾼 값도 맞음).
    for tag in [e1.clone(), format!("W/{e1}"), format!("\"other\", {e1}")] {
        let again = call(
            &app,
            Method::GET,
            &uri,
            Some(&alice),
            &[("if-none-match", &tag)],
            None,
        )
        .await;
        assert_eq!(again.status, StatusCode::NOT_MODIFIED, "{tag}");
        assert!(again.raw.is_empty());
        assert_eq!(header(&again.headers, "etag").as_deref(), Some(e1.as_str()));
        assert_eq!(
            header(&again.headers, "cache-control").as_deref(),
            Some(REVALIDATE)
        );
    }

    // 사용자 전환: 같은 브라우저 캐시의 앞 사람 ETag로는 304가 아니다.
    let as_bob = call(
        &app,
        Method::GET,
        &uri,
        Some(&bob),
        &[("if-none-match", &e1)],
        None,
    )
    .await;
    assert_eq!(as_bob.status, StatusCode::OK);
    let bob_tag = header(&as_bob.headers, "etag").unwrap();
    assert_ne!(bob_tag, e1);
    assert_eq!(as_bob.json["can_edit"], false);

    // 변경 뒤 조회 → 200과 새 ETag. 저장 응답의 ETag는 곧바로 조회한 ETag와 같다.
    let saved = call(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[("if-match", &e1)],
        Some(save_body(&new_request_id(), 1, "고침")),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let e2 = header(&saved.headers, "etag").unwrap();
    assert_ne!(e2, e1);
    assert_eq!(
        header(&saved.headers, "cache-control").as_deref(),
        Some(NO_STORE)
    );
    let changed = call(
        &app,
        Method::GET,
        &uri,
        Some(&alice),
        &[("if-none-match", &e1)],
        None,
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(
        header(&changed.headers, "etag").as_deref(),
        Some(e2.as_str())
    );
    assert_eq!(changed.json["revision"], 2);

    // 권한 철회 → 이전 ETag로도 404.
    let bob_now = call(&app, Method::GET, &uri, Some(&bob), &[], None).await;
    let bob_tag = header(&bob_now.headers, "etag").unwrap();
    let removed = call(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", bob.id.0),
        Some(&alice),
        &[],
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);
    let revoked = call(
        &app,
        Method::GET,
        &uri,
        Some(&bob),
        &[("if-none-match", &bob_tag)],
        None,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::NOT_FOUND);
    assert_eq!(
        header(&revoked.headers, "cache-control").as_deref(),
        Some(NO_STORE)
    );

    // 로그아웃 → 이전 세션 쿠키와 ETag로 401, 다시 로그인하면 새로 받는다.
    let out = call(&app, Method::POST, "/auth/logout", Some(&alice), &[], None).await;
    assert_eq!(out.status, StatusCode::OK);
    assert!(header(&out.headers, "set-cookie").is_some());
    assert_eq!(
        header(&out.headers, "cache-control").as_deref(),
        Some(NO_STORE)
    );
    let after = call(
        &app,
        Method::GET,
        &uri,
        Some(&alice),
        &[("if-none-match", &e2)],
        None,
    )
    .await;
    assert_eq!(after.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        header(&after.headers, "cache-control").as_deref(),
        Some(NO_STORE)
    );
}

/// `If-Match`는 잠근 행 기준의 강한 비교. 동시에 같은 ETag로 저장하면 하나만 성공하고 다른 쪽은 412와 서버 최신본을
/// 받는다(진 쪽 내용은 저장되지 않음). 같은 요청의 재시도는 멱등, 약한 ETag는 맞지 않음, `If-Match`가 없으면 기존 409.
#[tokio::test]
async fn saves_with_if_match_are_atomic_and_idempotent() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "저장").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let e1 = header(
        &call(&app, Method::GET, &uri, Some(&alice), &[], None)
            .await
            .headers,
        "etag",
    )
    .unwrap();

    let (r1, r2) = (new_request_id(), new_request_id());
    let cond = [("if-match", e1.as_str())];
    let (a, b) = tokio::join!(
        call(
            &app,
            Method::PUT,
            &uri,
            Some(&alice),
            &cond,
            Some(save_body(&r1, 1, "탭 1"))
        ),
        call(
            &app,
            Method::PUT,
            &uri,
            Some(&alice),
            &cond,
            Some(save_body(&r2, 1, "탭 2"))
        ),
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::PRECONDITION_FAILED]);
    let (won, lost, won_rid, won_text) = if a.status == StatusCode::OK {
        (a, b, r1, "탭 1")
    } else {
        (b, a, r2, "탭 2")
    };
    assert_eq!(lost.json["error"], "precondition_failed");
    assert_eq!(lost.json["current"]["revision"], 2);
    assert_eq!(lost.json["current"]["body"], doc_body(won_text));
    let e2 = header(&won.headers, "etag").unwrap();
    // 412에 실린 ETag로 다음 저장을 이어 갈 수 있다(조회한 값과 같음).
    assert_eq!(lost.json["etag"], e2.as_str());
    let now = call(&app, Method::GET, &uri, Some(&alice), &[], None).await;
    assert_eq!(now.json["body"], doc_body(won_text));
    assert_eq!(header(&now.headers, "etag").as_deref(), Some(e2.as_str()));

    // 이긴 요청의 재시도(같은 request_id, 이제는 낡은 If-Match) → 멱등 200.
    let replay = call(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[("if-match", &e1)],
        Some(save_body(&won_rid, 1, won_text)),
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK, "{:?}", replay.json);
    assert_eq!(replay.json["revision"], 2);

    // 약한 ETag는 If-Match에서 맞지 않는다.
    let weak = format!("W/{e2}");
    let r = call(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[("if-match", &weak)],
        Some(save_body(&new_request_id(), 2, "약함")),
    )
    .await;
    assert_eq!(r.status, StatusCode::PRECONDITION_FAILED);
    // `*`는 문서가 있으면 통과하고 리비전 비교가 이어진다.
    let r = call(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[("if-match", "*")],
        Some(save_body(&new_request_id(), 2, "별")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    // If-Match 없이 낡은 base_revision → 기존 409(서버 최신본과 ETag).
    let r = call(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[],
        Some(save_body(&new_request_id(), 2, "낡음")),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.json["error"], "conflict");
    assert_eq!(r.json["current"]["revision"], 3);
    let latest = call(&app, Method::GET, &uri, Some(&alice), &[], None).await;
    assert_eq!(
        r.json["etag"],
        header(&latest.headers, "etag").unwrap().as_str()
    );
}

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
