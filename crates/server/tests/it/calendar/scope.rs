//! 모든 출처가 같은 공간·모듈·항목 범위를 사용한다(D-80).

use super::{Visibility, calendar, create, module_item};
use crate::common;
use axum::{
    Router,
    http::{Method, StatusCode},
};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn populate(app: &Router, user: &TestUser, item: &Value, from: &str, to: &str) {
    let task = send(
        app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": "기간 업무" })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    let id = task.json["id"].as_str().unwrap();
    let dated = send(
        app,
        Method::PATCH,
        &format!("/api/tasks/{id}"),
        Some(user),
        Some(json!({ "base_version": task.json["version"], "due_date": from })),
    )
    .await;
    assert_eq!(dated.status, StatusCode::OK, "{:?}", dated.json);
    let repeated = send(app, Method::PUT, &format!("/api/tasks/{id}/recurrence"), Some(user),
        Some(json!({ "base": null, "rule": { "freq": "daily", "interval": 1, "weekdays": [], "until": to, "count": null }, "timezone": "UTC" }))).await;
    assert_eq!(repeated.status, StatusCode::OK, "{:?}", repeated.json);
    let event = create(app, user, item, json!({
        "request_id": new_request_id(), "kind": "event", "title": "종일 일정", "description": "",
        "all_day": true, "start_date": from, "end_date": to, "timezone": "UTC",
        "starts_at": null, "ends_at": null
    })).await;
    assert_eq!(event.0, StatusCode::CREATED, "{:?}", event.1);
}

#[tokio::test]
async fn tasks_projected_tasks_and_event_occurrences_share_calendar_scope() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "공통 범위").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let first = module_item(&app, &admin, &ws, "SCOPE", Visibility::Workspace).await;
    let second = module_item(&app, &admin, &ws, "OTHER", Visibility::Workspace).await;
    let private = module_item(&app, &admin, &ws, "HIDE", Visibility::Private).await;
    let first_day = time::OffsetDateTime::now_utc().date() + time::Duration::days(1);
    let from = first_day.to_string();
    let to = (first_day + time::Duration::days(4)).to_string();
    for item in [&first, &second, &private] {
        populate(&app, &admin, item, &from, &to).await;
    }
    let (status, all) = calendar(&app, &writer, &ws, &from, &to, "UTC").await;
    assert_eq!(status, StatusCode::OK, "{all:?}");
    assert_eq!(all["events"].as_array().unwrap().len(), 2);
    assert!(
        !all["projected_tasks"].as_array().unwrap().is_empty(),
        "{all:?}"
    );
    for extra in [
        String::new(),
        format!("&module_id={}", first["module_id"].as_str().unwrap()),
        format!("&item_id={}", first["id"].as_str().unwrap()),
        format!(
            "&module_id={}&item_id={}",
            first["module_id"].as_str().unwrap(),
            first["id"].as_str().unwrap()
        ),
    ] {
        let result = send(
            &app,
            Method::GET,
            &format!("/api/workspaces/{ws}/calendar?from={from}&to={to}&tz=UTC{extra}"),
            Some(&writer),
            None,
        )
        .await;
        assert_eq!(result.status, StatusCode::OK, "{:?}", result.json);
        for source in ["tasks", "projected_tasks", "events"] {
            let expected: Vec<&Value> = all[source]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| extra.is_empty() || row["item_id"] == first["id"])
                .collect();
            let actual: Vec<&Value> = result.json[source].as_array().unwrap().iter().collect();
            assert_eq!(actual, expected, "{source} {extra}");
        }
        let event_ids: Vec<&Value> = result.json["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| &event["id"])
            .collect();
        let occurrences: Vec<&Value> = all["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| event_ids.contains(&&row["event_id"]))
            .collect();
        assert_eq!(
            result.json["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .collect::<Vec<_>>(),
            occurrences
        );
        assert!(
            result.json["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|i| i["id"] != private["id"])
        );
        if !extra.is_empty() {
            assert_eq!(result.json["items"].as_array().unwrap().len(), 1);
        }
    }
}
