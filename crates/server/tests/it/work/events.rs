#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 회의·발표·행사 일정과 워크스페이스 달력 (D-31, D-50, D-57).

use crate::common;

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
            Some(json!({ "base_version": 1, "title": key, "description": "", "visibility": "workspace" })),
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
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
    assert_eq!(r.json["changed"], json!(["starts_at"]));
    assert!(r.json["message"].is_string());
    assert!(r.json["request_id"].is_string());
    // 끝 시각은 +09:00과 UTC 표기가 달라도 같은 시각이라 changed에 넣지 않는다.
    let (status, current) = {
        let reply = send(&app, Method::GET, &uri, Some(&admin), None).await;
        (reply.status, reply.json)
    };
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        current, r.json["current"],
        "stale update must leave the event unchanged"
    );

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

/// D-113: 보관은 등급 위의 쓰기 금지다. 보관된 항목이나 보관된 모듈의 항목 아래 일정은 등급 판정에서 읽기로 내려오고
/// (`app_grade_under_item`) 고치기는 거절된다. 웹은 쓰기 UI를 `access`로만 정하므로(따로 계산하지 않는다), 단건·목록·달력
/// 응답이 모두 같은 `access`를 내야 한다.
#[tokio::test]
async fn events_under_an_archived_item_or_module_are_read_only() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관 일정").await;
    let open = module_item(&app, &admin, &ws, "OPN", true).await;
    let archived_item = module_item(&app, &admin, &ws, "AIT", true).await;
    let archived_module = module_item(&app, &admin, &ws, "AMD", true).await;
    let mut events = Vec::new();
    for (item, title) in [
        (&open, "열린 일정"),
        (&archived_item, "보관된 항목의 일정"),
        (&archived_module, "보관된 모듈의 일정"),
    ] {
        let body = timed(
            title,
            "2026-11-10T10:00:00+09:00",
            "2026-11-10T11:00:00+09:00",
            "Asia/Seoul",
        );
        let (status, event) = create(&app, &admin, item, body).await;
        assert_eq!(status, StatusCode::CREATED, "{event:?}");
        assert_eq!(event["access"], "owner", "{event:?}");
        events.push(event);
    }
    let r = send(
        &app,
        Method::PUT,
        &format!(
            "/api/items/{}/archived",
            archived_item["id"].as_str().unwrap()
        ),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let r = send(
        &app,
        Method::PUT,
        &format!(
            "/api/modules/{}/archived",
            archived_module["module_id"].as_str().unwrap()
        ),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);

    let (status, cal) = calendar(&app, &admin, &ws, "2026-11-01", "2026-11-30", "Asia/Seoul").await;
    assert_eq!(status, StatusCode::OK, "{cal:?}");
    for (item, event, access, edit_status) in [
        (&open, &events[0], "owner", StatusCode::OK),
        (&archived_item, &events[1], "read", StatusCode::CONFLICT),
        (&archived_module, &events[2], "read", StatusCode::CONFLICT),
    ] {
        let id = event["id"].as_str().unwrap();
        let single = send(
            &app,
            Method::GET,
            &format!("/api/events/{id}"),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(single.status, StatusCode::OK, "{:?}", single.json);
        assert_eq!(single.json["access"], access, "single {:?}", single.json);
        let list = send(
            &app,
            Method::GET,
            &format!("/api/items/{}/events", item["id"].as_str().unwrap()),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(list.status, StatusCode::OK, "{:?}", list.json);
        let listed = list.json.as_array().unwrap();
        assert_eq!(listed.len(), 1, "{listed:?}");
        assert_eq!(listed[0]["access"], access, "list {listed:?}");
        let on_calendar = cal["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == event["id"])
            .unwrap_or_else(|| panic!("{id} missing from the calendar: {cal:?}"));
        assert_eq!(on_calendar["access"], access, "calendar {on_calendar:?}");
        let edit = send(
            &app,
            Method::PUT,
            &format!("/api/events/{id}"),
            Some(&admin),
            Some(json!({ "base_version": single.json["version"], "kind": "meeting", "title": "고친 제목",
                "description": "", "all_day": false, "start_date": null, "end_date": null,
                "starts_at": "2026-11-10T10:00:00+09:00", "ends_at": "2026-11-10T11:30:00+09:00",
                "timezone": "Asia/Seoul" })),
        )
        .await;
        assert_eq!(edit.status, edit_status, "{id}: {:?}", edit.json);
    }
}

#[tokio::test]
async fn event_field_errors_name_the_field_reason_and_limit_without_creating_rows() {
    let state = common::state();
    let app = common::app(&state);
    let user = login(&state, "event-fields").await;
    let ws = create_workspace(&app, &user, "event validation").await;
    let item = module_item(&app, &user, &ws, "EVA", false).await;
    let base = timed(
        "meeting",
        "2026-11-02T09:00:00+09:00",
        "2026-11-02T10:00:00+09:00",
        "Asia/Seoul",
    );
    let cases = [
        (
            "timezone",
            json!("Mars/Olympus"),
            "timezone",
            "invalid_format",
            None,
        ),
        ("timezone", json!(""), "timezone", "required", None),
        (
            "timezone",
            json!("z".repeat(65)),
            "timezone",
            "too_long",
            Some(64),
        ),
        ("starts_at", Value::Null, "starts_at", "required", None),
        (
            "starts_at",
            json!("tomorrow"),
            "starts_at",
            "invalid_format",
            None,
        ),
        (
            "ends_at",
            json!("2026-11-02T08:00:00+09:00"),
            "ends_at",
            "out_of_range",
            Some(366),
        ),
        (
            "ends_at",
            json!("2028-11-02T10:00:00+09:00"),
            "ends_at",
            "out_of_range",
            Some(366),
        ),
        (
            "start_date",
            json!("2026-11-02"),
            "start_date",
            "invalid_format",
            None,
        ),
        (
            "description",
            json!("x".repeat(10_001)),
            "description",
            "too_long",
            Some(10_000),
        ),
    ];
    for (key, value, field, reason, limit) in cases {
        let mut body = base.clone();
        body["request_id"] = json!(new_request_id());
        body[key] = value;
        let (status, error) = create(&app, &user, &item, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{key}: {error}");
        assert_eq!(error["error"], "bad_request", "{key}: {error}");
        assert_eq!(error["field"], field, "{key}: {error}");
        assert_eq!(error["reason"], reason, "{key}: {error}");
        assert_eq!(
            error.get("limit").and_then(Value::as_u64),
            limit,
            "{key}: {error}"
        );
    }
    let list = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/events", item["id"].as_str().unwrap()),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::OK, "{:?}", list.json);
    assert_eq!(list.json, json!([]));
}
