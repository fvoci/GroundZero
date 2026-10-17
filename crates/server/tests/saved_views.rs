#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 저장한 보기 (D-31): 사람마다 따로(본인만 읽음), 같은 이름이면 덮어쓰기, 항목을 볼 수 있어야 한다.

mod common;

use axum::http::{Method, StatusCode};
use common::{add_member, create_workspace, login, new_request_id, send};
use serde_json::json;

#[tokio::test]
async fn saved_views_are_personal_and_follow_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let outsider = login(&state, "outsider").await;
    let m = send(&app, Method::POST, &format!("/api/workspaces/{ws}/modules"), Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "VIEW" }))).await;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    let item_id = item.json["id"].as_str().unwrap().to_owned();
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    let uri = format!("/api/items/{item_id}/views");

    let config =
        json!({ "view": "list", "filter": { "assignee": "me", "status": "todo" }, "sort": "due" });
    let saved = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "name": "내 할 일", "config": config })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::CREATED, "{:?}", saved.json);
    assert_eq!(saved.json["config"]["sort"], "due");
    // 같은 이름이면 덮어쓴다.
    let again = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(),
        "name": "내 할 일", "config": { "view": "board" } })),
    )
    .await;
    assert_eq!(again.json["id"], saved.json["id"]);
    let mine = send(&app, Method::GET, &uri, Some(&writer), None).await;
    assert_eq!(mine.json.as_array().unwrap().len(), 1);
    assert_eq!(mine.json[0]["config"], json!({ "view": "board" }));
    // 다른 사람에게는 보이지 않고 지울 수도 없다.
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&admin), None).await.json,
        json!([])
    );
    let view_uri = format!("/api/views/{}", saved.json["id"].as_str().unwrap());
    assert_eq!(
        send(&app, Method::DELETE, &view_uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            Some(&writer),
            Some(json!({ "request_id": new_request_id(), "name": " ", "config": {} }))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    // 항목을 볼 수 없게 되면 보기도 보이지 않는다.
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(
        send(&app, Method::DELETE, &view_uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}

/// 내 시간대(D-50): 본인만 보고, 모르는 이름은 400, 비우면 지운다.
#[tokio::test]
async fn time_zone_is_a_personal_setting() {
    let state = common::state();
    let app = common::app(&state);
    let me = login(&state, "tz-me").await;
    let other = login(&state, "tz-other").await;
    let set = |zone: &str| {
        let app = app.clone();
        let me = me.clone();
        let body = json!({ "time_zone": zone });
        async move {
            send(&app, Method::PUT, "/api/me/settings", Some(&me), Some(body))
                .await
                .status
        }
    };
    assert_eq!(set("Asia/Seoul").await, StatusCode::NO_CONTENT);
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&me), None)
            .await
            .json["time_zone"],
        "Asia/Seoul"
    );
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&other), None)
            .await
            .json["time_zone"],
        ""
    );
    assert_eq!(set("Mars/Olympus").await, StatusCode::BAD_REQUEST);
    assert_eq!(set("").await, StatusCode::NO_CONTENT);
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&me), None)
            .await
            .json["time_zone"],
        ""
    );
}
