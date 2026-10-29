#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 본문 저장과 개인 노트 생성의 필수 기대 사용자(D-96).

use crate::common::{self, TestUser, login, new_request_id, send};
use crate::expected_user::raw;
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn a_note_is_created_only_for_the_user_who_started_the_edit() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let notes = |user: TestUser| {
        let app = app.clone();
        async move {
            let r = send(&app, Method::GET, "/api/me/notes", Some(&user), None).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json.as_array().map_or(0, Vec::len)
        }
    };
    let before = notes(alice.clone()).await;
    let create = || json!({ "request_id": new_request_id(), "title": "앞 계정 초안의 사본" });

    let bob_id = bob.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[&bob_id],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
    assert_eq!(json["error"], "account_changed");
    let (status, _) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(notes(alice.clone()).await, before);

    let alice_id = alice.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[&alice_id],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json:?}");
    assert_eq!(notes(alice).await, before + 1);
}
