#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: failures must panic"
)]
//! HTTP 타이머 시작의 선택·자동 전환·재시도 계약.
mod common;
mod sql_support;

use axum::http::Method;
use common::send;
use serde_json::{Value, json};
use sql_support::World;
use uuid::Uuid;

#[tokio::test]
async fn timer_same_target_choice_switch_and_retry_contract() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let a = w.task().await;
    let b = w.task().await;
    let start = |body: Value| {
        send(
            &app,
            Method::POST,
            "/api/me/timer",
            Some(&w.owner),
            Some(body),
        )
    };
    let input = |task, mode: Option<&str>| {
        let mut body = json!({"request_id":Uuid::now_v7(),"kind":"work","memo":"original",
            "target":{"kind":"task","id":task}});
        if let Some(mode) = mode {
            body["mode"] = json!(mode);
        }
        body
    };
    let original_request = input(a, None);
    let first = start(original_request.clone()).await;
    assert_eq!(first.status, 201, "{:?}", first.json);
    let old = first.json["id"].clone();
    let rejected = start(input(a, None)).await;
    assert_eq!(rejected.status, 409, "{:?}", rejected.json);
    assert_eq!(rejected.json["error"], "timer_running");
    let invalid = start(input(a, Some("later"))).await;
    assert_eq!(invalid.status, 422, "{:?}", invalid.json);
    let continuing_request = input(a, Some("continue"));
    let continued = start(continuing_request.clone()).await;
    assert_eq!(continued.status, 201, "{:?}", continued.json);
    assert_eq!(continued.json["id"], old);
    assert!(continued.json["stopped_entry_id"].is_null());
    assert_eq!(continued.json["started_at"], first.json["started_at"]);
    let replacement_request = input(a, Some("new"));
    let replacement = start(replacement_request.clone()).await;
    assert_eq!(replacement.status, 201, "{:?}", replacement.json);
    assert_ne!(replacement.json["id"], old);
    assert_eq!(replacement.json["stopped_entry_id"], old);
    let retry = start(replacement_request).await;
    assert_eq!(retry.status, 201, "{:?}", retry.json);
    assert_eq!(retry.json["id"], replacement.json["id"]);
    assert!(retry.json["stopped_entry_id"].is_null());
    let switched = start(input(b, None)).await;
    assert_eq!(switched.status, 201, "{:?}", switched.json);
    assert_eq!(switched.json["stopped_entry_id"], replacement.json["id"]);
    let retry = start(original_request).await;
    assert_eq!(retry.status, 201, "{:?}", retry.json);
    assert_eq!(retry.json["id"], old);
    let retry = start(continuing_request).await;
    assert_eq!(retry.status, 201, "{:?}", retry.json);
    assert_eq!(retry.json["id"], old);
    let running = send(&app, Method::GET, "/api/me/timer", Some(&w.owner), None).await;
    assert_eq!(
        running.json["id"], switched.json["id"],
        "retry must not switch the current timer"
    );
}
