#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 문서 작성·재열기·멱등 저장·접근 거절.

use crate::common::{self, create_doc, create_workspace, doc_body, login, new_request_id, send};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn document_write_save_reopen_with_revisions() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "팀").await;

    let doc = create_doc(&app, &alice, &ws, "회의록").await;
    assert_eq!(doc["reference"], "WIKI-1");
    assert_eq!(doc["revision"], 1);
    assert_eq!(doc["schema_version"], 1);
    assert_eq!(doc["can_edit"], true);
    let second = create_doc(&app, &alice, &ws, "설계").await;
    assert_eq!(second["reference"], "WIKI-2");

    let id = doc["id"].as_str().unwrap();
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{id}"),
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": "회의록 (수정)", "body": doc_body("안건 정리") })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    assert_eq!(saved.json["revision"], 2);

    let reopened = send(
        &app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(reopened.json["title"], "회의록 (수정)");
    assert_eq!(reopened.json["body"], doc_body("안건 정리"));

    let list = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&alice),
        None,
    )
    .await;
    let refs: Vec<_> = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["reference"].clone())
        .collect();
    assert_eq!(refs, vec![json!("WIKI-1"), json!("WIKI-2")]);
}

#[tokio::test]
async fn save_retry_is_idempotent_and_stale_save_conflicts() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "팀").await;
    let doc = create_doc(&app, &alice, &ws, "초안").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    let request = json!({ "request_id": new_request_id(), "base_revision": 1,
                          "title": "초안", "body": doc_body("첫 저장") });
    let first = send(&app, Method::PUT, &uri, Some(&alice), Some(request.clone())).await;
    assert_eq!(first.json["revision"], 2);
    let retry = send(&app, Method::PUT, &uri, Some(&alice), Some(request)).await;
    assert_eq!(retry.status, StatusCode::OK);
    assert_eq!(retry.json["revision"], 2, "retry must not add a revision");

    // 다른 탭이 리비전 1에서 편집을 시작했다가 늦게 저장한다.
    let stale = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": "다른 탭", "body": doc_body("다른 탭 초안") })),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.json["error"], "conflict");
    assert_eq!(stale.json["current"]["revision"], 2);
    assert_eq!(stale.json["current"]["body"], doc_body("첫 저장"));

    let unchanged = send(&app, Method::GET, &uri, Some(&alice), None).await;
    assert_eq!(unchanged.json["revision"], 2);

    // 같은 내용을 키 순서만 바꿔 다시 저장해도 새 리비전이 생기지 않는다(클라이언트 저장 반복 방지).
    let reordered = json!({
        "content": [{ "content": [{ "text": "첫 저장", "type": "text" }], "type": "paragraph" }],
        "type": "doc"
    });
    let same = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "base_revision": 2, "title": "초안", "body": reordered })),
    )
    .await;
    assert_eq!(same.status, StatusCode::OK);
    assert_eq!(
        same.json["revision"], 2,
        "unchanged content must not add a revision"
    );
}

#[tokio::test]
async fn other_user_cannot_see_or_change_documents() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let mallory = login(&state, "mallory").await;
    let ws = create_workspace(&app, &alice, "비공개").await;
    let doc = create_doc(&app, &alice, &ws, "기밀").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    assert_eq!(
        send(&app, Method::GET, &uri, Some(&mallory), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let put = send(
        &app,
        Method::PUT,
        &uri,
        Some(&mallory),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1, "title": "x", "body": doc_body("x") })),
    )
    .await;
    assert_eq!(put.status, StatusCode::NOT_FOUND);
    let list_uri = format!("/api/workspaces/{ws}/wiki/documents");
    assert_eq!(
        send(&app, Method::GET, &list_uri, Some(&mallory), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let create = send(
        &app,
        Method::POST,
        &list_uri,
        Some(&mallory),
        Some(json!({ "request_id": new_request_id(), "title": "침입" })),
    )
    .await;
    assert_eq!(create.status, StatusCode::NOT_FOUND);
    let workspaces = send(&app, Method::GET, "/api/workspaces", Some(&mallory), None).await;
    assert!(
        workspaces
            .json
            .as_array()
            .unwrap()
            .iter()
            .all(|w| w["id"] != ws.as_str())
    );

    let still = send(&app, Method::GET, &uri, Some(&alice), None).await;
    assert_eq!(still.json["title"], "기밀");
    assert_eq!(still.json["revision"], 1);
}

/// 재시도한 요청이 만든 리비전 뒤에 다른 저장이 있었으면, 재시도 응답은 성공이 아니라 충돌이어야 한다.
/// (성공으로 돌려주면 클라이언트가 남의 최신본을 확인 없이 덮어쓴다.)
#[tokio::test]
async fn replay_after_a_newer_save_reports_conflict() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "재시도").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    let mine = json!({ "request_id": new_request_id(), "base_revision": 1, "title": "문서", "body": doc_body("내 저장") });
    assert_eq!(
        send(&app, Method::PUT, &uri, Some(&alice), Some(mine.clone()))
            .await
            .json["revision"],
        2
    );
    let other = json!({ "request_id": new_request_id(), "base_revision": 2, "title": "문서", "body": doc_body("다른 탭") });
    assert_eq!(
        send(&app, Method::PUT, &uri, Some(&alice), Some(other))
            .await
            .json["revision"],
        3
    );

    // 첫 저장의 응답을 잃어버린 클라이언트가 같은 request_id로 재시도한다.
    let retry = send(&app, Method::PUT, &uri, Some(&alice), Some(mine)).await;
    assert_eq!(retry.status, StatusCode::CONFLICT);
    assert_eq!(retry.json["current"]["revision"], 3);
    assert_eq!(retry.json["current"]["body"], doc_body("다른 탭"));
}
