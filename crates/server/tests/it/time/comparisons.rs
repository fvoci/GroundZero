#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
use crate::{
    common::{self, send},
    sql_support::World,
};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn comparisons_keep_estimates_when_there_are_no_time_entries() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let task = w.task().await;
    let saved = send(
        &app,
        Method::PATCH,
        &format!("/api/tasks/{task}"),
        Some(&w.owner),
        Some(json!({"base_version":1,"estimate_minutes":30})),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let task_time = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{task}/time"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(task_time.status, StatusCode::OK);
    assert_eq!(
        task_time.json,
        json!({"estimate_minutes":30,"mine_seconds":0})
    );
    let item_time = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/time", w.item),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(item_time.status, StatusCode::OK);
    assert_eq!(item_time.json, json!({"estimate_minutes":30,"kinds":[]}));
}
