#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 일정·개인 보기의 선택적 기대 사용자 검증(D-96).
use crate::common;
use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, create_workspace, login, new_request_id, send, send_raw};
use serde_json::{Value, json};

async fn snapshot(workspace: &str) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_object_agg(kind, rows) FROM (
           SELECT 'events' AS kind, jsonb_agg(to_jsonb(e) ORDER BY e.id) AS rows
             FROM fvoci.events e WHERE workspace_id = $1::uuid
           UNION ALL SELECT 'views', jsonb_agg(to_jsonb(v) ORDER BY v.id)
             FROM fvoci.saved_views v JOIN fvoci.items i ON i.id = v.item_id WHERE i.workspace_id = $1::uuid
           UNION ALL SELECT 'audit', jsonb_agg(to_jsonb(a) ORDER BY a.id)
             FROM fvoci.audit_log a WHERE workspace_id = $1::uuid
           UNION ALL SELECT 'activity', jsonb_agg(to_jsonb(a) ORDER BY a.id)
             FROM fvoci.activity a WHERE workspace_id = $1::uuid
         ) snapshots",
    ).bind(workspace).fetch_one(&mut common::fixture_conn().await).await.unwrap()
}

async fn raw(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    expected: &[&str],
    body: &Value,
) -> (StatusCode, Value) {
    let mut headers = vec![("content-type", "application/json")];
    for value in expected {
        headers.push(("x-fvoci-expected-user", value));
    }
    let (status, _, bytes) = send_raw(
        app,
        method,
        uri,
        user,
        &headers,
        serde_json::to_vec(body).unwrap(),
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn event_and_view_commands_compare_a_sent_user_before_any_side_effect() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let alice_id = alice.id.0.to_string();
    let bob_id = bob.id.0.to_string();
    let ws = create_workspace(&app, &alice, "event expected user").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&alice),
        Some(json!({"request_id":new_request_id(),"kind":"project","name":"events","key":"EXE"})),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
        Some(&alice),
        Some(json!({"request_id":new_request_id(),"title":"events"})),
    )
    .await;
    assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
    let item_id = item.json["id"].as_str().unwrap();
    let events_uri = format!("/api/items/{item_id}/events");
    let views_uri = format!("/api/items/{item_id}/views");
    let event_body = json!({"request_id":new_request_id(),"kind":"meeting","title":"event","description":"",
        "all_day":true,"start_date":"2026-11-02","end_date":"2026-11-02","timezone":"UTC"});
    let event = send(
        &app,
        Method::POST,
        &events_uri,
        Some(&alice),
        Some(event_body.clone()),
    )
    .await;
    assert_eq!(event.status, StatusCode::CREATED, "{:?}", event.json);
    let event_uri = format!("/api/events/{}", event.json["id"].as_str().unwrap());
    let view = send(
        &app,
        Method::POST,
        &views_uri,
        Some(&alice),
        Some(json!({"request_id":new_request_id(),"name":"initial","config":{}})),
    )
    .await;
    assert_eq!(view.status, StatusCode::CREATED, "{:?}", view.json);
    let view_uri = format!("/api/views/{}", view.json["id"].as_str().unwrap());
    let mut updated = event_body.clone();
    updated["base_version"] = event.json["version"].clone();
    updated["title"] = json!("edited");
    let commands = [
        (
            Method::POST,
            events_uri.clone(),
            event_body.clone(),
            StatusCode::CREATED,
        ),
        (Method::PUT, event_uri.clone(), updated, StatusCode::OK),
        (
            Method::PUT,
            format!("{event_uri}/recurrence"),
            json!({"base":null,"rule":{"freq":"weekly","interval":1,"weekdays":[],"until":null,"count":null}}),
            StatusCode::OK,
        ),
        (
            Method::POST,
            format!("{event_uri}/exclusions"),
            json!({"date":"2026-11-09"}),
            StatusCode::NO_CONTENT,
        ),
        (
            Method::POST,
            views_uri.clone(),
            json!({"request_id":new_request_id(),"name":"another","config":{}}),
            StatusCode::CREATED,
        ),
        (
            Method::PUT,
            view_uri.clone(),
            json!({"request_id":new_request_id(),"base_version":1,"config":{"mode":"list"}}),
            StatusCode::OK,
        ),
        (
            Method::DELETE,
            view_uri,
            Value::Null,
            StatusCode::NO_CONTENT,
        ),
        (
            Method::DELETE,
            event_uri.clone(),
            Value::Null,
            StatusCode::NO_CONTENT,
        ),
    ];
    for (method, uri, body, success) in commands {
        let before = snapshot(&ws).await;
        for (headers, status, error) in [
            (
                vec![bob_id.as_str()],
                StatusCode::CONFLICT,
                "account_changed",
            ),
            (vec!["invalid-uuid"], StatusCode::BAD_REQUEST, "bad_request"),
            (
                vec![alice_id.as_str(), alice_id.as_str()],
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
        ] {
            let (actual, response) =
                raw(&app, method.clone(), &uri, Some(&alice), &headers, &body).await;
            assert_eq!(actual, status, "{method} {uri}: {response}");
            assert_eq!(response["error"], error, "{method} {uri}");
            assert_eq!(
                snapshot(&ws).await,
                before,
                "{method} {uri}: rejected write mutated rows"
            );
        }
        let (status, _) = raw(&app, method.clone(), &uri, None, &[&alice_id], &body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}");
        assert_eq!(
            snapshot(&ws).await,
            before,
            "{method} {uri}: unauthenticated mutation"
        );
        // 일정·보기는 필수 머리 범위가 아니다. 머리가 없어도 각 명령은 원래대로 성공한다.
        let (status, response) = raw(&app, method.clone(), &uri, Some(&alice), &[], &body).await;
        assert_eq!(status, success, "{method} {uri}: {response}");
    }
    // 일치하는 머리도 성공한다. 다른 사용자의 머리가 붙은 읽기는 비교하지 않는다.
    let mut matching_body = event_body.clone();
    matching_body["request_id"] = json!(new_request_id());
    let (status, response) = raw(
        &app,
        Method::POST,
        &events_uri,
        Some(&alice),
        &[&alice_id],
        &matching_body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{response}");
    for uri in [&events_uri, &views_uri] {
        let (status, response) = raw(
            &app,
            Method::GET,
            uri,
            Some(&alice),
            &[&bob_id],
            &Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{uri}: {response}");
    }
}
