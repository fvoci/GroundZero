#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 문서 ETag·조건부 저장·캐시 접근 순서(D-77).

use crate::cache::{NO_STORE, call, header};
use crate::common::{self, add_member, create_doc, create_workspace, login, new_request_id};
use axum::http::{Method, StatusCode};
use serde_json::json;

const REVALIDATE: &str = "private, no-cache, no-transform";

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

    // 변경 뒤 조회 → 200과 새 ETag. 쓰기 응답에는 ETag가 없다.
    let saved = call(
        &app,
        Method::PATCH,
        &uri,
        Some(&alice),
        &[],
        Some(json!({"request_id":new_request_id(),"base_revision":1,"title":"고침"})),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    assert!(header(&saved.headers, "etag").is_none());
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
    let e2 = header(&changed.headers, "etag").unwrap();
    assert_ne!(e2, e1);
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
