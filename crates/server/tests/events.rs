#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 회의·발표·행사 일정과 워크스페이스 달력 (D-31, D-50, D-57).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn module_item(app: &Router, admin: &TestUser, ws: &str, key: &str, public: bool) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": key })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    if public {
        let r = send(
            app,
            Method::PUT,
            &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
            Some(admin),
            Some(json!({ "title": key, "description": "", "visibility": "workspace" })),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    }
    i.json
}

fn timed(title: &str, starts: &str, ends: &str, tz: &str) -> Value {
    json!({ "request_id": new_request_id(), "kind": "meeting", "title": title, "description": "",
            "all_day": false, "start_date": null, "end_date": null, "starts_at": starts, "ends_at": ends,
            "timezone": tz })
}

async fn create(app: &Router, user: &TestUser, item: &Value, body: Value) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!("/api/items/{}/events", item["id"].as_str().unwrap()),
        Some(user),
        Some(body),
    )
    .await;
    (r.status, r.json)
}

async fn calendar(
    app: &Router,
    user: &TestUser,
    ws: &str,
    from: &str,
    to: &str,
    tz: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::GET,
        &format!("/api/workspaces/{ws}/calendar?from={from}&to={to}&tz={tz}"),
        Some(user),
        None,
    )
    .await;
    (r.status, r.json)
}

#[tokio::test]
async fn events_follow_item_access_keep_their_zone_and_guard_versions() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "일정").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "EVT", true).await;

    let body = timed(
        "주간 회의",
        "2026-11-02T10:00:00+09:00",
        "2026-11-02T11:00:00+09:00",
        "Asia/Seoul",
    );
    let (status, event) = create(&app, &admin, &item, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{event:?}");
    assert_eq!(event["starts_at"], "2026-11-02T01:00:00Z");
    assert_eq!(event["timezone"], "Asia/Seoul");
    // 같은 요청 id로 다시 보내면 같은 일정.
    let (_, again) = create(&app, &admin, &item, body).await;
    assert_eq!(again["id"], event["id"]);

    let bad_zone = timed(
        "x",
        "2026-11-02T10:00:00+09:00",
        "2026-11-02T11:00:00+09:00",
        "Mars/Olympus",
    );
    assert_eq!(
        create(&app, &admin, &item, bad_zone).await.0,
        StatusCode::BAD_REQUEST
    );
    let backwards = timed(
        "x",
        "2026-11-02T10:00:00+09:00",
        "2026-11-02T09:00:00+09:00",
        "Asia/Seoul",
    );
    assert_eq!(
        create(&app, &admin, &item, backwards).await.0,
        StatusCode::BAD_REQUEST
    );
    let all_day = json!({ "request_id": new_request_id(), "kind": "event", "title": "학회", "description": "",
        "all_day": true, "start_date": "2026-11-05", "end_date": "2026-11-07", "starts_at": null, "ends_at": null,
        "timezone": "Asia/Seoul" });
    let (status, conf) = create(&app, &admin, &item, all_day.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{conf:?}");
    assert_eq!(
        (conf["start_date"].clone(), conf["end_date"].clone()),
        (json!("2026-11-05"), json!("2026-11-07"))
    );
    assert_eq!(
        create(&app, &reader, &item, all_day).await.0,
        StatusCode::FORBIDDEN
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/events", item["id"].as_str().unwrap()),
        Some(&outsider),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::NOT_FOUND);

    // 버전 충돌과 날짜 변경 감사.
    let uri = format!("/api/events/{}", event["id"].as_str().unwrap());
    let edit = |version: &Value, starts: &str| {
        json!({ "base_version": version, "kind": "meeting", "title": "주간 회의",
        "description": "", "all_day": false, "start_date": null, "end_date": null, "starts_at": starts,
        "ends_at": "2026-11-02T12:00:00+09:00", "timezone": "Asia/Seoul" })
    };
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(edit(&event["version"], "2026-11-02T10:30:00+09:00")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(edit(&event["version"], "2026-11-02T09:00:00+09:00")),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.json["current"]["starts_at"], "2026-11-02T01:30:00Z");

    // 지우기: 독자는 못 하고, 지운 일정은 없는 것과 같다.
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&reader), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(event["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(actions, ["create", "reschedule", "delete"]);
}

#[tokio::test]
async fn the_calendar_shows_visible_tasks_and_events_by_the_viewers_days() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "달력").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let outsider = login(&state, "outsider").await;
    let public = module_item(&app, &admin, &ws, "PUB", true).await;
    let private = module_item(&app, &admin, &ws, "PRV", false).await;

    let t = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", public["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "보고서" })),
    )
    .await
    .json;
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "base_version": t["version"], "title": "보고서", "description": "", "status": "todo",
                              "priority": "none", "start_date": "2026-10-28", "due_date": "2026-11-03",
                              "estimate_minutes": null, "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    // 서울 11월 1일 00:30 = UTC 10월 31일 15:30.
    let (status, _) = create(
        &app,
        &admin,
        &public,
        timed(
            "새벽 점검",
            "2026-11-01T00:30:00+09:00",
            "2026-11-01T01:00:00+09:00",
            "Asia/Seoul",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = create(
        &app,
        &admin,
        &private,
        timed(
            "비공개 회의",
            "2026-11-01T10:00:00+09:00",
            "2026-11-01T11:00:00+09:00",
            "Asia/Seoul",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let titles = |cal: &Value| -> Vec<String> {
        let mut out: Vec<String> = cal["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .chain(cal["events"].as_array().unwrap().iter())
            .map(|v| v["title"].as_str().unwrap().to_owned())
            .collect();
        out.sort();
        out
    };
    let (status, seoul_nov1) =
        calendar(&app, &writer, &ws, "2026-11-01", "2026-11-01", "Asia/Seoul").await;
    assert_eq!(status, StatusCode::OK, "{seoul_nov1:?}");
    assert_eq!(titles(&seoul_nov1), ["보고서", "새벽 점검"]);
    assert_eq!(seoul_nov1["items"].as_array().unwrap().len(), 1);
    let (_, seoul_oct31) =
        calendar(&app, &writer, &ws, "2026-10-31", "2026-10-31", "Asia/Seoul").await;
    assert_eq!(titles(&seoul_oct31), ["보고서"]);
    let (_, utc_oct31) = calendar(&app, &writer, &ws, "2026-10-31", "2026-10-31", "UTC").await;
    assert_eq!(titles(&utc_oct31), ["보고서", "새벽 점검"]);
    let (_, admin_nov1) =
        calendar(&app, &admin, &ws, "2026-11-01", "2026-11-01", "Asia/Seoul").await;
    assert_eq!(titles(&admin_nov1), ["보고서", "비공개 회의", "새벽 점검"]);
    let (_, later) = calendar(&app, &admin, &ws, "2026-11-10", "2026-11-30", "Asia/Seoul").await;
    assert_eq!(titles(&later), Vec::<String>::new());

    assert_eq!(
        calendar(&app, &writer, &ws, "2026-01-01", "2026-12-31", "UTC")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        calendar(&app, &writer, &ws, "2026-11-01", "2026-11-01", "Nowhere")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        calendar(&app, &outsider, &ws, "2026-11-01", "2026-11-01", "UTC")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}
