#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 업무 관계 모음 읽기(노르웨이숲 태비 초안 1·2): 담당·선행 관계·반복 계열·체크리스트를 항목 또는 업무 범위로 한 번에
//! 읽는다. 범위는 꼭 하나이고(400), 볼 수 없는 범위는 404다.

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn project_item(app: &Router, admin: &TestUser, ws: &str, key: &str) -> String {
    let module = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
    let item = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": key })),
    )
    .await;
    assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
    item.json["id"].as_str().unwrap().to_owned()
}

async fn new_task(app: &Router, user: &TestUser, item: &str, fields: Value) -> String {
    let mut body = json!({ "request_id": new_request_id() });
    body.as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    let created = send(
        app,
        Method::POST,
        &format!("/api/items/{item}/tasks"),
        Some(user),
        Some(body),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    created.json["id"].as_str().unwrap().to_owned()
}

async fn read(app: &Router, user: &TestUser, uri: &str) -> (StatusCode, Value) {
    let reply = send(app, Method::GET, uri, Some(user), None).await;
    (reply.status, reply.json)
}

#[tokio::test]
async fn relation_reads_cover_one_item_or_one_task_and_hide_what_the_reader_cannot_see() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "relations-admin").await;
    let outsider = login(&state, "relations-outsider").await;
    let admin_id = admin.id.0.to_string();
    let ws = create_workspace(&app, &admin, "관계 모음").await;
    let item = project_item(&app, &admin, &ws, "REL").await;
    let other = project_item(&app, &admin, &ws, "OTH").await;
    let first = new_task(
        &app,
        &admin,
        &item,
        json!({ "title": "설계", "due_date": "2026-10-05", "assignee_ids": [admin_id] }),
    )
    .await;
    let second = new_task(
        &app,
        &admin,
        &item,
        json!({ "title": "구현", "due_date": "2026-10-12" }),
    )
    .await;
    // 다른 항목의 업무·관계는 이 항목 범위에 나오지 않는다.
    let elsewhere = new_task(
        &app,
        &admin,
        &other,
        json!({ "title": "다른 곳", "assignee_ids": [admin_id] }),
    )
    .await;
    for (task, items) in [(&first, json!(["A", "B"])), (&elsewhere, json!(["X"]))] {
        let items: Vec<Value> = items
            .as_array()
            .unwrap()
            .iter()
            .map(|t| json!({ "request_id": new_request_id(), "text": t }))
            .collect();
        let added = send(
            &app,
            Method::POST,
            &format!("/api/tasks/{task}/checklist"),
            Some(&admin),
            Some(json!({ "items": items })),
        )
        .await;
        assert_eq!(added.status, StatusCode::OK, "{:?}", added.json);
    }
    let checklist_a = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{first}"),
        Some(&admin),
        None,
    )
    .await
    .json["checklist"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let checked = send(
        &app,
        Method::PUT,
        &format!("/api/checklist-items/{checklist_a}"),
        Some(&admin),
        Some(json!({ "checked": true })),
    )
    .await;
    assert_eq!(checked.status, StatusCode::NO_CONTENT, "{:?}", checked.json);
    let linked = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{second}/predecessors/{first}"),
        Some(&admin),
        Some(json!({ "kind": "FS", "lag_days": 2, "base": null })),
    )
    .await;
    assert_eq!(linked.status, StatusCode::OK, "{:?}", linked.json);
    let repeated = send(&app, Method::PUT, &format!("/api/tasks/{second}/recurrence"), Some(&admin),
        Some(json!({ "rule": {"freq": "weekly", "interval": 1, "weekdays": [], "until": null, "count": 1},
                     "base": null, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(repeated.status, StatusCode::OK, "{:?}", repeated.json);

    // 담당: 항목 범위와 업무 범위.
    let (status, assignees) = read(&app, &admin, &format!("/api/task-assignees?item={item}")).await;
    assert_eq!(status, StatusCode::OK, "{assignees:?}");
    assert_eq!(
        assignees,
        json!([{ "task_id": first, "user_id": admin_id, "display_name": assignees[0]["display_name"] }])
    );
    assert_eq!(
        read(&app, &admin, &format!("/api/task-assignees?task={first}"))
            .await
            .1,
        assignees
    );
    assert_eq!(
        read(&app, &admin, &format!("/api/task-assignees?task={second}"))
            .await
            .1,
        json!([])
    );
    // 선행 관계: 업무 범위는 그 업무가 선행이든 후속이든 나온다.
    let link =
        json!([{ "predecessor_id": first, "successor_id": second, "kind": "FS", "lag_days": 2 }]);
    for scope in [
        format!("item={item}"),
        format!("task={first}"),
        format!("task={second}"),
    ] {
        let (status, links) = read(&app, &admin, &format!("/api/task-dependencies?{scope}")).await;
        assert_eq!(status, StatusCode::OK, "{scope}: {links:?}");
        assert_eq!(links, link, "{scope}");
    }
    // 반복 계열: 규칙을 정한 업무가 첫 회차다. 반복이 아닌 업무는 빈 목록.
    let (status, series) = read(&app, &admin, &format!("/api/task-series?item={item}")).await;
    assert_eq!(status, StatusCode::OK, "{series:?}");
    assert_eq!(series.as_array().unwrap().len(), 1, "{series:?}");
    assert_eq!(series[0]["root_task_id"], json!(second));
    assert_eq!(series[0]["rule"]["freq"], "weekly");
    assert_eq!(
        read(&app, &admin, &format!("/api/task-series?task={second}"))
            .await
            .1,
        series
    );
    assert_eq!(
        read(&app, &admin, &format!("/api/task-series?task={first}"))
            .await
            .1,
        json!([])
    );
    // 체크리스트: 항목 범위로 읽어 화면이 업무마다 센다(Q2 (나)). 다른 항목의 줄은 없다.
    let (status, checklist) =
        read(&app, &admin, &format!("/api/checklist-items?item={item}")).await;
    assert_eq!(status, StatusCode::OK, "{checklist:?}");
    let rows = checklist.as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|c| (
                c["task_id"].clone(),
                c["text"].clone(),
                c["checked"].clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            (json!(first), json!("A"), json!(true)),
            (json!(first), json!("B"), json!(false)),
        ]
    );
    assert_eq!(
        read(&app, &admin, &format!("/api/checklist-items?task={first}"))
            .await
            .1,
        checklist
    );

    // 범위는 꼭 하나: 없거나 둘이면 400(칸·이유).
    for uri in [
        "/api/task-assignees".to_owned(),
        format!("/api/checklist-items?item={item}&task={first}"),
    ] {
        let (status, body) = read(&app, &admin, &uri).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{uri}: {body:?}");
        assert_eq!(body["reason"], "required", "{uri}: {body:?}");
        assert_eq!(body["field"], "item", "{uri}: {body:?}");
    }
    // 볼 수 없는 범위는 존재를 숨기고 404.
    for uri in [
        format!("/api/task-assignees?item={item}"),
        format!("/api/task-dependencies?task={first}"),
        format!("/api/task-series?item={item}"),
        format!("/api/checklist-items?task={first}"),
    ] {
        let (status, body) = read(&app, &outsider, &uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri}: {body:?}");
    }
}
