#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 마일스톤·선행 관계·후속 재배치 승인 (D-31, D-57).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn module_item(app: &Router, admin: &TestUser, ws: &str, key: &str) -> Value {
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
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    i.json
}

async fn task(app: &Router, user: &TestUser, item: &Value, title: &str) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

async fn fetch(app: &Router, user: &TestUser, t: &Value) -> Value {
    send(
        app,
        Method::GET,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
        Some(user),
        None,
    )
    .await
    .json
}

async fn update(app: &Router, user: &TestUser, t: &Value, changes: Value) -> (StatusCode, Value) {
    let mut body = json!({
        "base_version": t["version"], "title": t["title"], "description": t["description"],
        "status": t["status"], "priority": t["priority"], "start_date": t["start_date"],
        "due_date": t["due_date"], "estimate_minutes": t["estimate_minutes"], "tags": t["tags"],
        "assignee_ids": [], "milestone": t["milestone"],
    });
    for (k, v) in changes.as_object().unwrap() {
        body[k] = v.clone();
    }
    let r = send(
        app,
        Method::PUT,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
        Some(user),
        Some(body),
    )
    .await;
    (r.status, r.json)
}

async fn dated(
    app: &Router,
    user: &TestUser,
    item: &Value,
    title: &str,
    start: &str,
    due: &str,
) -> Value {
    let t = task(app, user, item, title).await;
    let (status, t) = update(
        app,
        user,
        &t,
        json!({ "start_date": start, "due_date": due }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{t:?}");
    t
}

async fn link(app: &Router, user: &TestUser, pred: &Value, succ: &Value) -> StatusCode {
    send(
        app,
        Method::PUT,
        &format!(
            "/api/tasks/{}/predecessors/{}",
            succ["id"].as_str().unwrap(),
            pred["id"].as_str().unwrap()
        ),
        Some(user),
        None,
    )
    .await
    .status
}

#[tokio::test]
async fn milestones_have_a_single_date() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "마일스톤").await;
    let item = module_item(&app, &admin, &ws, "MS").await;
    let t = task(&app, &admin, &item, "출시").await;
    assert_eq!(t["milestone"], false);
    let (status, _) = update(
        &app,
        &admin,
        &t,
        json!({ "milestone": true, "start_date": "2026-11-01", "due_date": "2026-11-02" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, m) = update(
        &app,
        &admin,
        &t,
        json!({ "milestone": true, "due_date": "2026-11-02" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{m:?}");
    assert_eq!(m["milestone"], true);
    assert_eq!(m["due_date"], "2026-11-02");
}

#[tokio::test]
async fn dependencies_stay_in_one_item_reject_cycles_and_follow_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "선행").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "DEP").await;
    let other = module_item(&app, &admin, &ws, "OTH").await;
    let a = task(&app, &admin, &item, "A").await;
    let b = task(&app, &admin, &item, "B").await;
    let c = task(&app, &admin, &item, "C").await;
    let x = task(&app, &admin, &other, "X").await;

    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);
    assert_eq!(
        link(&app, &admin, &a, &b).await,
        StatusCode::OK,
        "idempotent"
    );
    assert_eq!(link(&app, &admin, &b, &c).await, StatusCode::OK);
    assert_eq!(
        link(&app, &admin, &c, &a).await,
        StatusCode::BAD_REQUEST,
        "cycle"
    );
    assert_eq!(
        link(&app, &admin, &a, &a).await,
        StatusCode::BAD_REQUEST,
        "self"
    );
    assert_eq!(
        link(&app, &admin, &x, &a).await,
        StatusCode::BAD_REQUEST,
        "other item"
    );
    assert_eq!(link(&app, &reader, &a, &c).await, StatusCode::FORBIDDEN);
    assert_eq!(link(&app, &outsider, &a, &c).await, StatusCode::NOT_FOUND);
    assert_eq!(
        fetch(&app, &reader, &c).await["predecessor_ids"],
        json!([b["id"]])
    );

    let r = send(
        &app,
        Method::DELETE,
        &format!(
            "/api/tasks/{}/predecessors/{}",
            b["id"].as_str().unwrap(),
            a["id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["predecessor_ids"], json!([]));

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid AND action LIKE '%predecessor' ORDER BY created_at, id",
    )
    .bind(b["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(actions, ["link_predecessor", "unlink_predecessor"]);

    // 서로를 선행으로 삼는 두 요청을 동시에 보내도 순환이 생기지 않는다.
    for _ in 0..5 {
        let p = task(&app, &admin, &item, "P").await;
        let q = task(&app, &admin, &item, "Q").await;
        let (r1, r2) = tokio::join!(link(&app, &admin, &p, &q), link(&app, &admin, &q, &p));
        assert_eq!(
            [r1, r2].iter().filter(|s| **s == StatusCode::OK).count(),
            1,
            "{r1} {r2}"
        );
    }
}

#[tokio::test]
async fn moving_a_predecessor_previews_impact_and_shifts_only_approved_successors() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "재배치").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let item = module_item(&app, &admin, &ws, "SCH").await;
    let a = dated(&app, &admin, &item, "설계", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "구현", "2026-10-06", "2026-10-08").await;
    let c = dated(&app, &admin, &item, "검증", "2026-10-09", "2026-10-09").await;
    let far = dated(&app, &admin, &item, "출시", "2026-12-01", "2026-12-01").await;
    for (p, s) in [(&a, &b), (&b, &c), (&a, &far)] {
        assert_eq!(link(&app, &admin, p, s).await, StatusCode::OK);
    }

    let preview = send(
        &app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-preview", a["id"].as_str().unwrap()),
        Some(&writer),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    let shifts = preview.json.as_array().unwrap();
    assert_eq!(shifts.len(), 2, "{shifts:?}");
    assert_eq!(
        (
            shifts[0]["task_id"].clone(),
            shifts[0]["start_date"].clone(),
            shifts[0]["due_date"].clone()
        ),
        (b["id"].clone(), json!("2026-10-11"), json!("2026-10-13"))
    );
    assert_eq!(
        (
            shifts[1]["task_id"].clone(),
            shifts[1]["start_date"].clone()
        ),
        (c["id"].clone(), json!("2026-10-14"))
    );
    let approved: Vec<Value> = shifts.iter().map(|s| json!({
        "task_id": s["task_id"], "base_version": s["base_version"], "start_date": s["start_date"], "due_date": s["due_date"],
    })).collect();

    // 승인 사이에 후속 업무가 바뀌면 아무것도 바꾸지 않는다.
    let (status, _) = update(&app, &admin, &c, json!({ "title": "검증(수정)" })).await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = update(
        &app,
        &writer,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": approved }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"], "shift_conflict");
    assert_eq!(fetch(&app, &admin, &a).await["due_date"], "2026-10-05");
    assert_eq!(fetch(&app, &admin, &b).await["start_date"], "2026-10-06");

    // 다시 미리 보고 승인하면 함께 옮겨진다.
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-preview", a["id"].as_str().unwrap()),
        Some(&writer),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    let approved: Vec<Value> = preview.json.as_array().unwrap().iter().map(|s| json!({
        "task_id": s["task_id"], "base_version": s["base_version"], "start_date": s["start_date"], "due_date": s["due_date"],
    })).collect();
    let (status, moved) = update(
        &app,
        &writer,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": approved }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["due_date"], "2026-10-10");
    let b2 = fetch(&app, &admin, &b).await;
    assert_eq!(
        (b2["start_date"].clone(), b2["due_date"].clone()),
        (json!("2026-10-11"), json!("2026-10-13"))
    );
    assert_eq!(fetch(&app, &admin, &c).await["start_date"], "2026-10-14");
    assert_eq!(fetch(&app, &admin, &far).await["start_date"], "2026-12-01");

    // 승인 없이 날짜만 고치면 후속은 그대로다. 후속이 아닌 업무를 옮기라는 요청은 거부한다.
    let a2 = fetch(&app, &admin, &a).await;
    let (status, _) = update(&app, &writer, &a2, json!({ "due_date": "2026-10-20" })).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetch(&app, &admin, &b).await["start_date"], "2026-10-11");
    let c2 = fetch(&app, &admin, &c).await;
    let (status, _) = update(
        &app,
        &writer,
        &c2,
        json!({ "successor_shifts": [{
        "task_id": a["id"], "base_version": fetch(&app, &admin, &a).await["version"],
        "start_date": "2026-10-02", "due_date": "2026-10-21" }] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let reasons: Vec<String> = sqlx::query_scalar(
        "SELECT detail->>'reason' FROM audit_log WHERE action = 'reschedule' AND target_id = ANY($1::uuid[])",
    )
    .bind(vec![b["id"].as_str().unwrap(), c["id"].as_str().unwrap()])
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(reasons, ["predecessor_moved", "predecessor_moved"]);
}

#[tokio::test]
async fn malformed_reschedules_are_rejected() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검증").await;
    let item = module_item(&app, &admin, &ws, "VAL").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-07").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);

    let preview = send(
        &app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-preview", a["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "start_date": "2026-10-09", "due_date": "2026-10-08" })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::BAD_REQUEST);

    let shift = json!({ "task_id": b["id"], "base_version": b["version"], "start_date": "2026-10-11", "due_date": "2026-10-12" });
    let (status, _) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": [shift.clone(), shift] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(fetch(&app, &admin, &b).await["version"], b["version"]);
}
