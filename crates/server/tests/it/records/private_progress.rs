//! D-113 개정 SR1-11: 읽기 전용의 개인 진도·계획과 공유 내용 쓰기를 구분한다.

use crate::{common, sql_support::World};
use axum::http::{Method, StatusCode};
use common::{new_request_id, send};
use serde_json::{Value, json};

async fn unit(world: &World, kind: &str) -> String {
    let app = common::app(&world.state);
    let (path, body) = if kind == "course" {
        (
            "episodes",
            json!({"episodes":[{"request_id":new_request_id(),"title":"shared episode","duration_seconds":60}]}),
        )
    } else {
        (
            "study/units",
            json!({"units":[{"request_id":new_request_id(),"title":"shared unit","unit":"pages","amount":10}]}),
        )
    };
    let uri = format!("/api/items/{}/{path}", world.item);
    let added = send(&app, Method::POST, &uri, Some(&world.owner), Some(body)).await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.json);
    let rows = send(&app, Method::GET, &uri, Some(&world.owner), None).await;
    assert_eq!(rows.status, StatusCode::OK, "{}", rows.json);
    let id = rows.json[0]["id"].as_str().unwrap_or_default().to_owned();
    assert!(!id.is_empty(), "{}", rows.json);
    id
}

#[tokio::test]
async fn read_only_member_can_save_private_episode_progress() {
    private_progress("course").await;
}

#[tokio::test]
async fn read_only_member_can_save_private_unit_progress() {
    private_progress("study").await;
}

async fn private_progress(kind: &str) {
    let world = World::of_kind(kind).await;
    let app = common::app(&world.state);
    let reader = world.member("readonly").await;
    let id = unit(&world, kind).await;
    let (path, progress, mut shared) = if kind == "course" {
        (
            "episodes",
            "episode-progress",
            json!({"version":1,"number":1,"title":"tampered episode","duration_seconds":60}),
        )
    } else {
        (
            "study-units",
            "study/progress",
            json!({"version":1,"number":1,"title":"tampered unit","unit":"pages","amount":10}),
        )
    };
    let mut body =
        json!({"status":"in_progress","note":"reader private note","planned_on":"2026-11-02"});
    if kind == "course" {
        body["last_position_seconds"] = json!(25);
    } else {
        body["done_amount"] = json!(3);
    }
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/{path}/{id}/progress"),
        Some(&reader),
        Some(body),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT, "{}", saved.json);
    let progress_uri = format!("/api/items/{}/{progress}", world.item);
    let mine = send(&app, Method::GET, &progress_uri, Some(&reader), None).await;
    assert_eq!(mine.json[0]["note"], "reader private note");
    assert_eq!(mine.json[0]["planned_on"], "2026-11-02");
    let owner = send(&app, Method::GET, &progress_uri, Some(&world.owner), None).await;
    assert_eq!(owner.json, json!([]));
    shared["chapter"] = json!("");
    let refused = send(
        &app,
        Method::PUT,
        &format!("/api/{path}/{id}"),
        Some(&reader),
        Some(shared),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.json);
}

#[tokio::test]
async fn read_only_member_can_plan_only_their_own_study_units() {
    let world = World::of_kind("study").await;
    let app = common::app(&world.state);
    let reader = world.member("readonly").await;
    let id = unit(&world, "study").await;
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/study/plan", world.item),
        Some(&reader),
        Some(json!({"entries":[{"unit_id":id,"planned_on":"2026-11-02"}]})),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.json);
    assert_eq!(saved.json, json!({"planned":1}));
    let uri = format!("/api/items/{}/study/progress", world.item);
    let mine = send(&app, Method::GET, &uri, Some(&reader), None).await;
    assert_eq!(mine.json[0]["planned_on"], "2026-11-02");
    let owner = send(&app, Method::GET, &uri, Some(&world.owner), None).await;
    assert_eq!(owner.json, json!([]));
    let shared = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/study/units", world.item),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(shared.json[0]["title"], "shared unit");
    assert!(shared.json[0]["planned_on"].is_null());
    let refused = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/study/units", world.item),
        Some(&reader),
        Some(json!({"units":[{"request_id":new_request_id(),"title":"cannot add"}]})),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.json);
}
