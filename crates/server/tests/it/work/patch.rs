#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 업무 고치기 명령 `PATCH /api/tasks/{id}`(노르웨이숲 태비 초안 2): 보낸 칸만 바꾸고 기준 판(D-94)을 지킨다. 판이 지났으면
//! 아무것도 바꾸지 않고 공통 409 본문(지금 업무와 바뀐 칸, #180 서버 부분)으로 답한다. 승인한 후속 재배치는 같은
//! 저장에서 옮긴다(D-91·D-125).

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn project_item(app: &Router, admin: &TestUser, ws: &str) -> String {
    let module = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "patch", "key": "PAT" })),
    )
    .await;
    let item = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "patch" })),
    )
    .await;
    assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
    item.json["id"].as_str().unwrap().to_owned()
}

async fn new_task(app: &Router, user: &TestUser, item: &str, fields: Value) -> Value {
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
    created.json
}

async fn patch(app: &Router, user: &TestUser, id: &str, body: Value) -> (StatusCode, Value) {
    let reply = send(
        app,
        Method::PATCH,
        &format!("/api/tasks/{id}"),
        Some(user),
        Some(body),
    )
    .await;
    (reply.status, reply.json)
}

/// 옛 읽기의 업무(체크포인트 2에서 새 읽기로 바뀐다). 판·칸 비교에만 쓴다.
async fn stored(app: &Router, user: &TestUser, id: &str) -> Value {
    let reply = send(
        app,
        Method::GET,
        &format!("/api/tasks/{id}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
    reply.json
}

#[tokio::test]
async fn a_patch_changes_only_the_sent_fields_and_null_clears() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "patch-fields").await;
    let admin_id = admin.id.0.to_string();
    let ws = create_workspace(&app, &admin, "칸만 고치기").await;
    let item = project_item(&app, &admin, &ws).await;
    let task = new_task(&app, &admin, &item, json!({ "title": "처음", "description": "설명", "priority": "high",
        "due_date": "2026-11-10", "estimate_minutes": 30, "tags": ["가"], "assignee_ids": [admin_id] })).await;
    let id = task["id"].as_str().unwrap();

    let (status, changed) = patch(
        &app,
        &admin,
        id,
        json!({ "base_version": task["version"], "title": " 새 제목 " }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{changed:?}");
    // 응답은 업무 자원(칸만)이다. 관계·등급은 싣지 않는다.
    for absent in [
        "assignees",
        "predecessors",
        "checklist",
        "checklist_done",
        "access",
        "series",
    ] {
        assert!(changed.get(absent).is_none(), "{absent}: {changed:?}");
    }
    assert_eq!(changed["title"], "새 제목");
    assert_eq!(changed["version"], task["version"].as_i64().unwrap() + 1);
    let kept = stored(&app, &admin, id).await;
    for field in [
        "description",
        "priority",
        "due_date",
        "estimate_minutes",
        "tags",
        "status",
        "milestone",
    ] {
        assert_eq!(kept[field], task[field], "{field} is kept");
    }
    assert_eq!(
        kept["assignees"][0]["user_id"],
        json!(admin_id),
        "assignees are kept"
    );

    // null은 지우기다.
    let (status, cleared) = patch(
        &app,
        &admin,
        id,
        json!({ "base_version": changed["version"], "due_date": null, "estimate_minutes": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared:?}");
    assert_eq!(cleared["due_date"], Value::Null);
    assert_eq!(cleared["estimate_minutes"], Value::Null);
    assert_eq!(cleared["title"], "새 제목");

    // 보낸 칸만 검사하고, 거절은 칸·이유·한도를 싣는다(#79).
    for (body, field, reason) in [
        (
            json!({ "estimate_minutes": -1 }),
            "estimate_minutes",
            "out_of_range",
        ),
        (json!({ "title": "가".repeat(201) }), "title", "too_long"),
        (
            json!({ "due_date": "2026-13-01" }),
            "due_date",
            "invalid_format",
        ),
    ] {
        let mut body = body;
        body["base_version"] = cleared["version"].clone();
        let (status, json) = patch(&app, &admin, id, body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{field}: {json:?}");
        assert_eq!(
            (json["field"].as_str(), json["reason"].as_str()),
            (Some(field), Some(reason)),
            "{json:?}"
        );
    }
    assert_eq!(
        stored(&app, &admin, id).await["version"],
        cleared["version"],
        "rejections change nothing"
    );
}

#[tokio::test]
async fn a_stale_patch_changes_nothing_and_names_the_fields_that_differ() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "patch-stale").await;
    let ws = create_workspace(&app, &admin, "판 충돌").await;
    let item = project_item(&app, &admin, &ws).await;
    let task = new_task(
        &app,
        &admin,
        &item,
        json!({ "title": "처음", "due_date": "2026-11-10" }),
    )
    .await;
    let id = task["id"].as_str().unwrap();
    let (status, first) = patch(
        &app,
        &admin,
        id,
        json!({ "base_version": task["version"], "title": "다른 탭" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first:?}");

    // 옛 판으로 보낸 고치기: 제목·마감은 지금과 다르고 우선순위(none)는 같다.
    let (status, conflict) = patch(
        &app,
        &admin,
        id,
        json!({ "base_version": task["version"], "title": "내 제목",
        "priority": "none", "due_date": "2026-12-01" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict:?}");
    assert_eq!(conflict["error"], "conflict");
    assert_eq!(conflict["changed"], json!(["due_date", "title"]));
    assert_eq!(conflict["current"]["title"], "다른 탭");
    assert_eq!(conflict["current"]["version"], first["version"]);
    let now = stored(&app, &admin, id).await;
    assert_eq!(
        (
            now["title"].clone(),
            now["due_date"].clone(),
            now["version"].clone()
        ),
        (
            json!("다른 탭"),
            json!("2026-11-10"),
            first["version"].clone()
        )
    );
}

#[tokio::test]
async fn a_patch_moves_the_approved_successors_in_the_same_save() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "patch-shifts").await;
    let ws = create_workspace(&app, &admin, "재배치").await;
    let item = project_item(&app, &admin, &ws).await;
    let a = new_task(
        &app,
        &admin,
        &item,
        json!({ "title": "선행", "start_date": "2026-10-01", "due_date": "2026-10-05" }),
    )
    .await;
    let b = new_task(
        &app,
        &admin,
        &item,
        json!({ "title": "후속", "start_date": "2026-10-06", "due_date": "2026-10-08" }),
    )
    .await;
    let (a_id, b_id) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    let linked = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{b_id}/predecessors/{a_id}"),
        Some(&admin),
        Some(json!({ "kind": "FS", "lag_days": 0, "base": null })),
    )
    .await;
    assert_eq!(linked.status, StatusCode::OK, "{:?}", linked.json);
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/tasks/{a_id}/schedule-plan"),
        Some(&admin),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    let approved: Vec<Value> = preview.json["shifts"].as_array().unwrap().iter().map(|s| json!({
        "task_id": s["task_id"], "base_version": s["base_version"], "start_date": s["start_date"], "due_date": s["due_date"],
    })).collect();
    assert_eq!(approved.len(), 1, "{:?}", preview.json);

    let (status, moved) = patch(&app, &admin, a_id,
        json!({ "base_version": a["version"], "due_date": "2026-10-10", "successor_shifts": approved })).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    let b_now = stored(&app, &admin, b_id).await;
    assert_eq!(
        (b_now["start_date"].clone(), b_now["due_date"].clone()),
        (json!("2026-10-11"), json!("2026-10-13"))
    );

    // 같은 승인 목록을 다시 보내면(후속 업무의 판이 지남) 아무것도 바꾸지 않고 409 shift_conflict다.
    let (status, conflict) = patch(&app, &admin, a_id,
        json!({ "base_version": moved["version"], "due_date": "2026-10-12", "successor_shifts": approved })).await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict:?}");
    assert_eq!(conflict["error"], "shift_conflict");
    assert_eq!(conflict["current"]["due_date"], "2026-10-10");
    assert_eq!(conflict["changed"], json!(["due_date"]));
    assert_eq!(
        stored(&app, &admin, b_id).await["version"],
        b_now["version"]
    );
}
