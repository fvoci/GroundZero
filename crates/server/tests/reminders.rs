#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 항목 알림 (D-37 만료, D-36 복습, D-34 투고 마감, D-33 산출물 마감): 작업 프로세스가 하루 한 번씩 만들고, 받는
//! 사람이 지금 항목을 볼 수 있을 때만 보인다.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn module(app: &Router, admin: &TestUser, ws: &str, kind: &str, key: &str) -> String {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": kind, "name": key, "key": key })),
    )
    .await;
    m.json["id"].as_str().unwrap().to_owned()
}

async fn open_item(app: &Router, admin: &TestUser, module: &str, title: &str) -> String {
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    let id = i.json["id"].as_str().unwrap().to_owned();
    send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "title": title, "description": "", "visibility": "workspace" })),
    )
    .await;
    id
}

async fn notes(app: &Router, user: &TestUser) -> Vec<Value> {
    send(app, Method::GET, "/api/me/notifications", Some(user), None)
        .await
        .json
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn item_reminders_reach_people_who_can_see_the_item_once_a_day() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 알림").await;
    let (learner, _) = add_member(&app, &state, &admin, &ws, "learner", "writer").await;
    let (coauthor, _) = add_member(&app, &state, &admin, &ws, "coauthor", "writer").await;
    let today = time::OffsetDateTime::now_utc().date();
    let week = today.saturating_add(time::Duration::days(7));
    let tomorrow = today.next_day().unwrap();

    // 강좌: 수강권이 7일 뒤 만료. 진도를 남긴 사람에게.
    let lecture = module(&app, &admin, &ws, "course", "LEC").await;
    let course = open_item(&app, &admin, &lecture, "회귀분석").await;
    let r = send(&app, Method::PUT, &format!("/api/items/{course}/course"), Some(&admin), Some(json!({
        "subject": "", "instructor": "", "platform": "", "url": "", "textbook": "", "starts_on": null,
        "expires_on": week.to_string(), "target_on": null }))).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
    send(
        &app,
        Method::POST,
        &format!("/api/items/{course}/episodes"),
        Some(&admin),
        Some(json!({
        "episodes": [{ "request_id": new_request_id(), "title": "1강" }] })),
    )
    .await;
    let c = send(
        &app,
        Method::GET,
        &format!("/api/items/{course}/course"),
        Some(&learner),
        None,
    )
    .await;
    let episode = c.json["episodes"][0]["id"].as_str().unwrap().to_owned();
    send(
        &app,
        Method::PUT,
        &format!("/api/episodes/{episode}/progress"),
        Some(&learner),
        Some(json!({ "status": "in_progress", "note": "" })),
    )
    .await;

    // 연구 투고: 내일 제출 예정, 공동저자에게도. 끝난 투고는 보내지 않는다.
    let research = module(&app, &admin, &ws, "research", "RES").await;
    let study = open_item(&app, &admin, &research, "동기 연구").await;
    let uri = format!("/api/items/{study}/records");
    let submission = |status: &str, title: &str| {
        json!({ "request_id": new_request_id(), "kind": "submission",
        "title": title, "occurred_on": tomorrow.to_string(),
        "fields": { "status": status, "coauthors": [coauthor.id.0] } })
    };
    let s = send(
        &app,
        Method::POST,
        &uri,
        Some(&admin),
        Some(submission("preparing", "학회 투고")),
    )
    .await;
    assert_eq!(s.status, StatusCode::CREATED, "{:?}", s.json);
    send(
        &app,
        Method::POST,
        &uri,
        Some(&admin),
        Some(submission("rejected", "끝난 투고")),
    )
    .await;

    // 하루에 두 번 돌아도 한 번.
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();

    let mine = notes(&app, &learner).await;
    let expiry: Vec<&Value> = mine.iter().filter(|n| n["kind"] == "expiry").collect();
    assert_eq!(expiry.len(), 1, "{mine:?}");
    assert_eq!(expiry[0]["item_id"], course.as_str());
    assert_eq!(expiry[0]["due_date"], week.to_string());
    assert_eq!(expiry[0]["document_title"], "회귀분석");
    assert!(expiry[0]["actor_name"].is_null());
    // 진도를 남긴 사람이 있으면 강좌를 만든 사람에게는 보내지 않는다.
    assert!(
        notes(&app, &admin)
            .await
            .iter()
            .all(|n| n["kind"] != "expiry")
    );

    let theirs = notes(&app, &coauthor).await;
    let deadlines: Vec<&Value> = theirs.iter().filter(|n| n["kind"] == "deadline").collect();
    assert_eq!(deadlines.len(), 1, "{theirs:?}");
    assert_eq!(deadlines[0]["subject"], "학회 투고");
    assert_eq!(deadlines[0]["item_id"], study.as_str());
    assert_eq!(
        notes(&app, &admin)
            .await
            .iter()
            .filter(|n| n["kind"] == "deadline")
            .count(),
        1
    );

    // 항목을 볼 수 없게 되면 알림도 보이지 않는다.
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{study}"),
        Some(&admin),
        Some(json!({ "title": "동기 연구", "description": "", "visibility": "private" })),
    )
    .await;
    assert!(
        notes(&app, &coauthor)
            .await
            .iter()
            .all(|n| n["kind"] != "deadline")
    );
}
