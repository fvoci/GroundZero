#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
use crate::{
    common::{self, send, send_raw},
    sql_support::World,
};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

fn input() -> Value {
    json!({"request_id":Uuid::now_v7(), "started_at":"2026-10-20T09:00:00+09:00",
        "ended_at":"2026-10-20T09:02:00+09:00", "paused_seconds":0, "kind":"work", "memo":"original", "target":null})
}
async fn add(w: &World, body: Value) -> Value {
    let r = send(
        &common::app(&w.state),
        Method::POST,
        "/api/me/time-entries",
        Some(&w.owner),
        Some(body),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

#[tokio::test]
async fn entries_are_resources_and_target_display_is_a_separate_collection() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let task = w.task().await;
    let mut body = input();
    body["target"] = json!({"kind":"task", "id":task});
    let entry = add(&w, body).await;
    assert_eq!(entry["target"], json!({"kind":"task","id":task}));
    let uri = format!(
        "/api/time-entry-targets?entry_id={}",
        entry["id"].as_str().unwrap()
    );
    let targets = send(&app, Method::GET, &uri, Some(&w.owner), None).await;
    assert_eq!(targets.status, StatusCode::OK, "{:?}", targets.json);
    assert_eq!(
        targets.json,
        json!([{"entry_id":entry["id"], "kind":"task", "id":task,
        "label":"SQL-2 SQL task", "workspace_id":w.ws}])
    );
    let range = send(
        &app,
        Method::GET,
        "/api/time-entry-targets?from=2026-10-20&to=2026-10-20&tz=Asia%2FSeoul",
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(range.json, targets.json);
    let invalid = send(
        &app,
        Method::GET,
        &format!("{uri}&from=2026-10-20"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(invalid.status, StatusCode::BAD_REQUEST);
    assert_eq!(invalid.json["field"], "entry_id");
}

#[tokio::test]
async fn stale_time_writes_keep_data_and_report_only_different_fields() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let mut body = input();
    let entry = add(&w, body.clone()).await;
    let uri = format!("/api/time-entries/{}", entry["id"].as_str().unwrap());
    body["base_version"] = entry["version"].clone();
    body["memo"] = json!("server");
    body["kind"] = json!("study");
    let saved = send(&app, Method::PUT, &uri, Some(&w.owner), Some(body.clone())).await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    body["memo"] = json!("client");
    body["kind"] = json!("work");
    // UTC와 +09:00은 같은 시각이라 changed에 넣지 않는다.
    body["started_at"] = json!("2026-10-20T00:00:00Z");
    body["ended_at"] = json!("2026-10-20T00:02:00Z");
    let stale = send(&app, Method::PUT, &uri, Some(&w.owner), Some(body)).await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(stale.json["error"], "conflict");
    assert_eq!(stale.json["changed"], json!(["kind", "memo"]));
    assert_eq!(stale.json["current"], saved.json);
    assert!(stale.json["message"].is_string());
    assert!(stale.json["request_id"].is_string());
    let revisions = send(
        &app,
        Method::GET,
        &format!("{uri}/revisions"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(revisions.json.as_array().unwrap().len(), 1);
    let listed = send(
        &app,
        Method::GET,
        "/api/me/time-entries?from=2026-10-20&to=2026-10-20&tz=UTC",
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(listed.json, json!([saved.json]));
}

#[tokio::test]
async fn time_field_rejections_have_reason_field_and_limit() {
    let w = World::new().await;
    let app = common::app(&w.state);
    for (field, value, reason, limit) in [
        ("started_at", json!("wrong"), "invalid_format", Value::Null),
        ("ended_at", Value::Null, "required", Value::Null),
        ("paused_seconds", json!(-1), "out_of_range", Value::Null),
        (
            "ended_at",
            json!("2026-10-19T00:00:00Z"),
            "out_of_range",
            Value::Null,
        ),
        ("paused_seconds", json!(121), "out_of_range", json!(120)),
        ("memo", json!("x".repeat(2001)), "too_long", json!(2000)),
    ] {
        let mut body = input();
        body[field] = value;
        let r = send(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(&w.owner),
            Some(body),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{field}: {:?}", r.json);
        assert_eq!(r.json["reason"], reason);
        assert_eq!(r.json["field"], field);
        assert_eq!(r.json["limit"], limit);
    }
    for (path, field, reason, limit) in [
        (
            "/api/me/time-entries?from=&to=2026-10-20&tz=UTC",
            "from",
            "required",
            Value::Null,
        ),
        (
            "/api/me/time-entries?from=2020-01-01&to=2026-10-20&tz=UTC",
            "to",
            "out_of_range",
            json!(400),
        ),
        (
            "/api/me/time-entries?from=2026-10-20&to=2026-10-20&tz=NoSuchZone",
            "tz",
            "invalid_format",
            Value::Null,
        ),
        (
            "/api/me/time-summary?from=2026-10-20&to=2026-10-20&tz=UTC&group=week&week_start=wrong",
            "week_start",
            "invalid_format",
            Value::Null,
        ),
    ] {
        let r = send(&app, Method::GET, path, Some(&w.owner), None).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
        assert_eq!(r.json["field"], field);
        assert_eq!(r.json["reason"], reason);
        assert_eq!(r.json["limit"], limit);
    }
}

async fn snapshot(w: &World) -> Value {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar("SELECT jsonb_build_object(
        'entries',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM fvoci.time_entries e WHERE user_id=$1),
        'revisions',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id) FROM fvoci.time_entry_revisions r WHERE user_id=$1),
        'requests',(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.request_id) FROM fvoci.time_entry_requests r WHERE user_id=$1))")
        .bind(w.owner.id.0).fetch_one(&mut conn).await.unwrap()
}

#[tokio::test]
async fn every_time_command_compares_a_sent_expected_user_before_writing() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let entry = add(&w, input()).await;
    let uri = format!("/api/time-entries/{}", entry["id"].as_str().unwrap());
    let mut update = input();
    update["base_version"] = entry["version"].clone();
    let before = snapshot(&w).await;
    let wrong = Uuid::now_v7().to_string();
    for (method, path, body) in [
        (
            Method::POST,
            "/api/me/timer".to_owned(),
            json!({"request_id":Uuid::now_v7(),"kind":"work","memo":"","target":null}),
        ),
        (
            Method::POST,
            format!("{uri}/timer"),
            json!({"action":"stop"}),
        ),
        (Method::POST, "/api/me/time-entries".to_owned(), input()),
        (Method::PUT, uri.clone(), update),
        (Method::DELETE, uri, Value::Null),
    ] {
        for (headers, expected) in [
            (
                vec![("x-fvoci-expected-user", wrong.as_str())],
                StatusCode::CONFLICT,
            ),
            (
                vec![("x-fvoci-expected-user", "invalid")],
                StatusCode::BAD_REQUEST,
            ),
            (
                vec![
                    ("x-fvoci-expected-user", wrong.as_str()),
                    ("x-fvoci-expected-user", wrong.as_str()),
                ],
                StatusCode::BAD_REQUEST,
            ),
        ] {
            let mut headers = headers;
            headers.push(("content-type", "application/json"));
            let (status, _, bytes) = send_raw(
                &app,
                method.clone(),
                &path,
                Some(&w.owner),
                &headers,
                serde_json::to_vec(&body).unwrap(),
            )
            .await;
            assert_eq!(
                status,
                expected,
                "{path}: {}",
                String::from_utf8_lossy(&bytes)
            );
        }
    }
    assert_eq!(snapshot(&w).await, before);
    for expected in [None, Some(w.owner.id.0.to_string())] {
        let mut headers = vec![("content-type", "application/json")];
        if let Some(ref expected) = expected {
            headers.push(("x-fvoci-expected-user", expected));
        }
        let (status, _, _) = send_raw(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(&w.owner),
            &headers,
            serde_json::to_vec(&input()).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "the header stays optional");
    }
    let (status, _, _) = send_raw(
        &app,
        Method::POST,
        "/api/me/time-entries",
        None,
        &[("x-fvoci-expected-user", &wrong)],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = send_raw(
        &app,
        Method::GET,
        "/api/me/timer",
        Some(&w.owner),
        &[("x-fvoci-expected-user", "invalid")],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}
