#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 마일스톤·선행 관계·후속 재배치 승인 (D-31, D-57).

use crate::common;

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
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
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
    common::task_snapshot(app, user, t["id"].as_str().unwrap()).await
}

async fn update(app: &Router, user: &TestUser, t: &Value, changes: Value) -> (StatusCode, Value) {
    let mut body = json!({ "base_version": t["version"] });
    for (k, v) in changes.as_object().unwrap() {
        body[k] = v.clone();
    }
    let r = send(
        app,
        Method::PATCH,
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

/// 새 관계(종료 후 시작·지연 0). 이미 있으면 409 `link_conflict`(기준 조건 `base: null`).
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
        Some(json!({ "kind": "FS", "lag_days": 0, "base": null })),
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
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
        fetch(&app, &reader, &c).await["predecessors"],
        json!([{ "task_id": b["id"], "kind": "FS", "lag_days": 0 }])
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
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(fetch(&app, &admin, &b).await["predecessors"], json!([]));

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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
        &format!("/api/tasks/{}/schedule-plan", a["id"].as_str().unwrap()),
        Some(&writer),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    let shifts = preview.json["shifts"].as_array().unwrap();
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
        &format!("/api/tasks/{}/schedule-plan", a["id"].as_str().unwrap()),
        Some(&writer),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    let approved: Vec<Value> = preview.json["shifts"].as_array().unwrap().iter().map(|s| json!({
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

    // 승인 없이 날짜만 고치면 후속은 그대로다. 후속이 아닌 업무를 옮기라는 요청은 아무것도 바꾸지 않는다(같은 항목의
    // 선행 쪽 업무는 미리본 뒤 관계가 바뀐 오래된 계획으로 보아 409, 0082).
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
    assert_eq!(status, StatusCode::CONFLICT);

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
        &format!("/api/tasks/{}/schedule-plan", a["id"].as_str().unwrap()),
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

// ---------------------------------------------------------------------------
// 관계 종류·지연일 (D-91)
// ---------------------------------------------------------------------------

fn path_of(pred: &Value, succ: &Value) -> String {
    format!(
        "/api/tasks/{}/predecessors/{}",
        succ["id"].as_str().unwrap(),
        pred["id"].as_str().unwrap()
    )
}

/// 관계 설정. 승인 목록이 없으면 관계만 바꾼다. 본문에 `base`가 없으면 보내기 직전에 읽은 지금 관계를 기준으로 채운다
/// (화면이 편집을 시작한 관계를 보내는 것과 같다).
async fn set_link(
    app: &Router,
    user: &TestUser,
    pred: &Value,
    succ: &Value,
    mut body: Value,
) -> common::Reply {
    if body.get("base").is_none() {
        let current = send(
            app,
            Method::GET,
            &format!(
                "/api/task-dependencies?task={}",
                succ["id"].as_str().unwrap()
            ),
            Some(user),
            None,
        )
        .await;
        body["base"] = current
            .json
            .as_array()
            .and_then(|links| {
                links
                    .iter()
                    .find(|l| l["predecessor_id"] == pred["id"] && l["successor_id"] == succ["id"])
            })
            .map_or(
                Value::Null,
                |l| json!({ "kind": l["kind"], "lag_days": l["lag_days"] }),
            );
    }
    send(
        app,
        Method::PUT,
        &path_of(pred, succ),
        Some(user),
        Some(body),
    )
    .await
}

async fn plan_link(
    app: &Router,
    user: &TestUser,
    pred: &Value,
    succ: &Value,
    kind: &str,
    lag: i64,
) -> common::Reply {
    send(
        app,
        Method::POST,
        &format!("{}/schedule-plan", path_of(pred, succ)),
        Some(user),
        Some(json!({ "kind": kind, "lag_days": lag })),
    )
    .await
}

fn approve(plan: &Value) -> Vec<Value> {
    plan["shifts"].as_array().unwrap().iter().map(|s| json!({
        "task_id": s["task_id"], "base_version": s["base_version"], "start_date": s["start_date"], "due_date": s["due_date"],
    })).collect()
}

fn dates(t: &Value) -> (Value, Value) {
    (t["start_date"].clone(), t["due_date"].clone())
}

#[tokio::test]
async fn existing_links_are_finish_to_start_and_stale_link_edits_conflict() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "관계 종류").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "KIND").await;
    let a = task(&app, &admin, &item, "A").await;
    let b = task(&app, &admin, &item, "B").await;
    let c = task(&app, &admin, &item, "C").await;

    // 새 관계: 종료 후 시작·지연 0.
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);
    let b1 = fetch(&app, &admin, &b).await;
    assert_eq!(
        b1["predecessors"],
        json!([{ "task_id": a["id"], "kind": "FS", "lag_days": 0 }])
    );
    // 중복이던 `predecessor_ids`는 없앴다(D-92).
    assert!(b1.get("predecessor_ids").is_none());

    // 종류·지연일 바꾸기(옮길 것이 없으면 관계만). 같은 쌍은 하나뿐이다.
    let r = set_link(&app, &admin, &a, &b, json!({ "kind": "SS", "lag_days": 2 })).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(
        fetch(&app, &admin, &b).await["predecessors"],
        json!([{ "task_id": a["id"], "kind": "SS", "lag_days": 2 }])
    );
    // 이미 있는 관계를 "새 관계"로 다시 보내면(편집을 시작한 뒤 다른 곳에서 만든 경우) 덮지 않고 409다.
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::CONFLICT);
    assert_eq!(
        fetch(&app, &admin, &b).await["predecessors"],
        json!([{ "task_id": a["id"], "kind": "SS", "lag_days": 2 }])
    );

    // 잘못된 종류·지연일, 자기 자신, 순환, 권한.
    for body in [
        json!({ "kind": "XX", "lag_days": 0 }),
        json!({ "kind": "FS", "lag_days": -1 }),
        json!({ "kind": "FS", "lag_days": 3651 }),
        json!({ "kind": "FS", "lag_days": 1.5 }),
    ] {
        let r = set_link(&app, &admin, &a, &b, body.clone()).await;
        assert!(r.status.is_client_error(), "{body} → {}", r.status);
    }
    assert_eq!(
        set_link(&app, &admin, &a, &a, json!({ "kind": "FS", "lag_days": 0 }))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(link(&app, &admin, &b, &c).await, StatusCode::OK);
    assert_eq!(
        set_link(&app, &admin, &c, &a, json!({ "kind": "SS", "lag_days": 0 }))
            .await
            .status,
        StatusCode::BAD_REQUEST,
        "cycle"
    );
    assert_eq!(
        plan_link(&app, &admin, &c, &a, "SS", 0).await.status,
        StatusCode::BAD_REQUEST,
        "cycle preview"
    );
    assert_eq!(
        set_link(
            &app,
            &reader,
            &a,
            &c,
            json!({ "kind": "FF", "lag_days": 0 })
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        set_link(
            &app,
            &outsider,
            &a,
            &c,
            json!({ "kind": "FF", "lag_days": 0 })
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        plan_link(&app, &outsider, &a, &c, "FF", 0).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        fetch(&app, &admin, &b).await["predecessors"],
        json!([{ "task_id": a["id"], "kind": "SS", "lag_days": 2 }]),
        "nothing changed by the rejected requests"
    );

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let changes: Vec<Value> = sqlx::query_scalar(
        "SELECT detail FROM audit_log WHERE target_id = $1::uuid AND action = 'change_predecessor'",
    )
    .bind(b["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0]["from"], json!({ "kind": "FS", "lag_days": 0 }));
    assert_eq!(changes[0]["to"], json!({ "kind": "SS", "lag_days": 2 }));
}

#[tokio::test]
async fn changing_a_link_previews_causes_and_moves_approved_tasks_in_one_transaction() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "관계 재배치").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "LNK").await;
    let a = dated(&app, &admin, &item, "설계", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "구현", "2026-10-06", "2026-10-07").await;
    let c = dated(&app, &admin, &item, "검증", "2026-10-08", "2026-10-08").await;
    let d = dated(&app, &admin, &item, "문서", "2026-10-08", "2026-10-09").await;
    for (p, s) in [(&a, &b), (&b, &c)] {
        assert_eq!(link(&app, &admin, p, s).await, StatusCode::OK);
    }

    // FS·0 → FS·3: b는 사흘 밀리고(원인 a FS 3), c는 b 때문에(원인 b FS 0) 밀린다. 미리보기는 아무것도 바꾸지 않는다.
    let plan = plan_link(&app, &writer, &a, &b, "FS", 3).await;
    assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
    let shifts = plan.json["shifts"].as_array().unwrap();
    assert_eq!(
        shifts
            .iter()
            .map(|s| (
                s["task_id"].clone(),
                s["start_date"].clone(),
                s["due_date"].clone()
            ))
            .collect::<Vec<_>>(),
        vec![
            (b["id"].clone(), json!("2026-10-09"), json!("2026-10-10")),
            (c["id"].clone(), json!("2026-10-11"), json!("2026-10-11")),
        ]
    );
    assert_eq!(
        shifts[0]["causes"],
        json!([{ "predecessor_id": a["id"], "predecessor_reference": a["reference"],
                 "predecessor_title": "설계", "kind": "FS", "lag_days": 3 }])
    );
    assert_eq!(shifts[1]["causes"][0]["predecessor_id"], b["id"]);
    assert_eq!(plan.json["undated"], json!([]));
    assert_eq!(
        fetch(&app, &admin, &b).await["predecessors"][0]["lag_days"],
        0
    );
    let approved = approve(&plan.json);

    // 그사이 다른 사람이 b → d를 이었다(d의 판은 그대로라 기준 버전만으로는 모른다). 승인 목록은 이제 계획과 달라
    // 아무것도 바꾸지 않는다(관계도, 날짜도).
    assert_eq!(link(&app, &admin, &b, &d).await, StatusCode::OK);
    let stale = set_link(
        &app,
        &writer,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 3, "successor_shifts": approved }),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    assert_eq!(stale.json["error"], "shift_conflict");
    let b_now = fetch(&app, &admin, &b).await;
    assert_eq!(
        b_now["predecessors"][0]["lag_days"], 0,
        "the link is unchanged"
    );
    assert_eq!(dates(&b_now), (json!("2026-10-06"), json!("2026-10-07")));
    assert_eq!(fetch(&app, &admin, &c).await["start_date"], "2026-10-08");

    // 다시 미리 보면 d도 들어 있고, 승인하면 관계와 날짜가 함께 바뀐다.
    let plan = plan_link(&app, &writer, &a, &b, "FS", 3).await;
    assert_eq!(plan.json["shifts"].as_array().unwrap().len(), 3);
    let done = set_link(
        &app,
        &writer,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 3, "successor_shifts": approve(&plan.json) }),
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(
        fetch(&app, &writer, &b).await["predecessors"],
        json!([{ "task_id": a["id"], "kind": "FS", "lag_days": 3 }])
    );
    assert_eq!(
        dates(&fetch(&app, &writer, &b).await),
        (json!("2026-10-09"), json!("2026-10-10"))
    );
    assert_eq!(fetch(&app, &admin, &c).await["start_date"], "2026-10-11");
    assert_eq!(
        dates(&fetch(&app, &admin, &d).await),
        (json!("2026-10-11"), json!("2026-10-12"))
    );

    // 그대로 다시 보내면 옮길 것도 바뀐 것도 없다.
    let again = set_link(
        &app,
        &writer,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 3 }),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK);
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let reasons: Vec<String> = sqlx::query_scalar(
        "SELECT detail->>'reason' FROM audit_log WHERE action = 'reschedule' AND target_id = ANY($1::uuid[])",
    )
    .bind(vec![
        b["id"].as_str().unwrap(),
        c["id"].as_str().unwrap(),
        d["id"].as_str().unwrap(),
    ])
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(reasons, ["dependency_changed"; 3]);
}

#[tokio::test]
async fn each_kind_and_lag_reaches_the_api_and_undated_followers_are_reported() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "종류별").await;
    let item = module_item(&app, &admin, &ws, "EACH").await;
    // 선행 A(10-05 ~ 10-09), 후속 B(10-01 ~ 10-03).
    let a = dated(&app, &admin, &item, "A", "2026-10-05", "2026-10-09").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-01", "2026-10-03").await;
    for (kind, lag, start, due) in [
        ("FS", 0, "2026-10-10", "2026-10-12"),
        ("SS", 2, "2026-10-07", "2026-10-09"),
        ("FF", 1, "2026-10-08", "2026-10-10"),
        ("SF", 0, "2026-10-02", "2026-10-04"),
    ] {
        let plan = plan_link(&app, &admin, &a, &b, kind, lag).await;
        assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
        assert_eq!(
            (
                plan.json["shifts"][0]["start_date"].clone(),
                plan.json["shifts"][0]["due_date"].clone()
            ),
            (json!(start), json!(due)),
            "{kind}·{lag}"
        );
    }
    // 날짜 없는 후속: 옮기지 않고 알린다.
    let u = task(&app, &admin, &item, "날짜 없음").await;
    let plan = plan_link(&app, &admin, &a, &u, "FS", 0).await;
    assert_eq!(plan.json["shifts"], json!([]));
    assert_eq!(plan.json["undated"][0]["task_id"], u["id"]);
}

#[tokio::test]
async fn approved_date_moves_are_recomputed_against_the_links_at_save_time() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "다시 계산").await;
    let item = module_item(&app, &admin, &ws, "RE").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-07").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);
    let plan = send(
        &app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-plan", a["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
    assert_eq!(plan.json["shifts"][0]["start_date"], "2026-10-11");
    let approved = approve(&plan.json);
    // 그사이 관계가 시작 후 시작으로 바뀌어 b를 옮길 까닭이 없어졌다(b의 판은 그대로).
    let r = set_link(&app, &admin, &a, &b, json!({ "kind": "SS", "lag_days": 0 })).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        fetch(&app, &admin, &b).await["version"],
        b["version"],
        "a link change does not bump the task"
    );
    let (status, body) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": approved }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"], "shift_conflict");
    assert_eq!(fetch(&app, &admin, &a).await["due_date"], "2026-10-05");
    assert_eq!(fetch(&app, &admin, &b).await["start_date"], "2026-10-06");
    // 지금 계획(옮길 것 없음)대로면 저장된다.
    let plan = send(
        &app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-plan", a["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "start_date": "2026-10-01", "due_date": "2026-10-10" })),
    )
    .await;
    assert_eq!(plan.json["shifts"], json!([]));
    let (status, _) = update(&app, &admin, &a, json!({ "due_date": "2026-10-10" })).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn copying_an_item_keeps_link_kinds_and_lags() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "복제").await;
    let item = module_item(&app, &admin, &ws, "CPY").await;
    let a = task(&app, &admin, &item, "A").await;
    let b = task(&app, &admin, &item, "B").await;
    let r = set_link(&app, &admin, &a, &b, json!({ "kind": "FF", "lag_days": 4 })).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let copied = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/copy", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "module_id": item["module_id"], "title": "", "reset": false })),
    )
    .await;
    assert_eq!(copied.status, StatusCode::CREATED, "{:?}", copied.json);
    let tasks = send(
        &app,
        Method::GET,
        &format!(
            "/api/items/{}/tasks",
            copied.json["item_id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    let list = tasks.json.as_array().unwrap();
    let copy_a = list.iter().find(|t| t["title"] == "A").unwrap();
    let copy_b = list.iter().find(|t| t["title"] == "B").unwrap();
    assert_ne!(copy_a["id"], a["id"]);
    assert_eq!(
        fetch(&app, &admin, copy_b).await["predecessors"],
        json!([{ "task_id": copy_a["id"], "kind": "FF", "lag_days": 4 }])
    );
}

#[tokio::test]
async fn two_approvals_of_the_same_preview_save_once() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "동시 승인").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "TWO").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-07").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);
    // 같은 미리보기를 두 사람이 동시에 승인한다(하나는 지연 2일, 하나는 지연 4일). 하나만 저장되고 다른 하나는
    // 그사이 바뀐 후속 업무 때문에 아무것도 바꾸지 않는다.
    let two = plan_link(&app, &admin, &a, &b, "FS", 2).await;
    let four = plan_link(&app, &writer, &a, &b, "FS", 4).await;
    let (r1, r2) = tokio::join!(
        set_link(
            &app,
            &admin,
            &a,
            &b,
            json!({ "kind": "FS", "lag_days": 2, "successor_shifts": approve(&two.json) })
        ),
        set_link(
            &app,
            &writer,
            &a,
            &b,
            json!({ "kind": "FS", "lag_days": 4, "successor_shifts": approve(&four.json) })
        ),
    );
    let statuses = [r1.status, r2.status];
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "{statuses:?}"
    );
    assert!(statuses.contains(&StatusCode::CONFLICT), "{statuses:?}");
    let won = if r1.status == StatusCode::OK { 2 } else { 4 };
    let b_now = fetch(&app, &admin, &b).await;
    assert_eq!(b_now["predecessors"][0]["lag_days"], won);
    let start = if won == 2 { "2026-10-08" } else { "2026-10-10" };
    assert_eq!(b_now["start_date"], start);
}

/// 관계 설정의 409는 두 모양이다(계약 `PredecessorConflict`): 오래된 승인은 `TaskConflict`(최신본 포함), 보관된 항목은
/// `ErrorBody`(`invalid_state`, 최신본 없음). 본문 없는 옛 요청도 보관이면 `ErrorBody`다.
#[tokio::test]
async fn link_conflicts_come_back_as_task_conflict_or_error_body() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "409 모양").await;
    let item = module_item(&app, &admin, &ws, "CNF").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-07").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);

    // 오래된 승인: 기준 버전을 하나 낮춰 보낸다.
    let plan = plan_link(&app, &admin, &a, &b, "FS", 2).await;
    let mut stale = approve(&plan.json);
    stale[0]["base_version"] = json!(b["version"].as_i64().unwrap() - 1);
    let r = set_link(
        &app,
        &admin,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 2, "successor_shifts": stale }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert_eq!(r.json["error"], "shift_conflict");
    assert_eq!(r.json["current"]["successor_id"], b["id"]);
    assert_eq!(r.json["changed"], json!(["lag_days"]));
    assert!(r.json["message"].is_string());

    // 보관된 항목: 일반 409(ErrorBody).
    let archived = send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/archived", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(archived.status, StatusCode::OK, "{:?}", archived.json);
    // 본문 없는 옛 요청은 없앴다(D-92): 본문이 없으면 받지 않는다.
    let bodyless = send(&app, Method::PUT, &path_of(&a, &b), Some(&admin), None).await;
    assert!(bodyless.status.is_client_error(), "{:?}", bodyless.json);
    let r = set_link(&app, &admin, &a, &b, json!({ "kind": "SS", "lag_days": 0 })).await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert_eq!(r.json["error"], "invalid_state");
    assert!(r.json["message"].is_string());
    assert!(r.json.get("current").is_none());
    assert_eq!(
        fetch(&app, &admin, &b).await["predecessors"],
        json!([{ "task_id": a["id"], "kind": "FS", "lag_days": 0 }])
    );
}

// ---------------------------------------------------------------------------
// 일정 경계(#41 감사): 오래된 계획, 건수 한도, 저장할 수 있는 날짜 범위
// ---------------------------------------------------------------------------

async fn date_plan(app: &Router, user: &TestUser, t: &Value, start: Value, due: Value) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-plan", t["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "start_date": start, "due_date": due })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

async fn unlink(app: &Router, user: &TestUser, pred: &Value, succ: &Value) {
    let r = send(app, Method::DELETE, &path_of(pred, succ), Some(user), None).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
}

/// 미리 본 뒤 관계가 지워져 승인 목록의 업무가 더는 후속이 아니면, 처음부터 잘못된 요청(400)이 아니라 오래된
/// 계획(409 `shift_conflict`)이다. 아무것도 바뀌지 않고 다시 미리 보면 저장된다. 선행 쪽·다른 항목 업무를 옮기라는
/// 요청은 그대로 400이다.
#[tokio::test]
async fn a_plan_made_stale_by_a_removed_link_is_a_conflict_not_a_bad_request() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "지운 관계").await;
    let item = module_item(&app, &admin, &ws, "STL").await;
    let other = module_item(&app, &admin, &ws, "OTR").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-07").await;
    let c = dated(&app, &admin, &item, "C", "2026-10-08", "2026-10-08").await;
    let x = dated(&app, &admin, &other, "X", "2026-10-08", "2026-10-08").await;
    for (p, s) in [(&a, &b), (&b, &c)] {
        assert_eq!(link(&app, &admin, p, s).await, StatusCode::OK);
    }

    // 날짜 저장 경로: A를 미루는 계획(B·C)을 본 뒤 B → C를 지운다.
    let plan = date_plan(&app, &admin, &a, json!("2026-10-01"), json!("2026-10-10")).await;
    assert_eq!(plan["shifts"].as_array().unwrap().len(), 2);
    let stale = approve(&plan);
    unlink(&app, &admin, &b, &c).await;
    let before = [
        fetch(&app, &admin, &a).await,
        fetch(&app, &admin, &b).await,
        fetch(&app, &admin, &c).await,
    ];
    let (status, body) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": stale }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"], "shift_conflict");
    for t in &before {
        let now = fetch(&app, &admin, t).await;
        assert_eq!(
            (dates(&now), now["version"].clone()),
            (dates(t), t["version"].clone()),
            "nothing saved"
        );
    }
    let plan = date_plan(&app, &admin, &a, json!("2026-10-01"), json!("2026-10-10")).await;
    assert_eq!(plan["shifts"].as_array().unwrap().len(), 1, "only B now");
    let (status, _) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": approve(&plan) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 관계 저장 경로: B → C를 다시 잇고 A → B를 FS·3으로 바꾸는 계획(B·C)을 본 뒤 B → C를 지운다.
    assert_eq!(link(&app, &admin, &b, &c).await, StatusCode::OK);
    let b_now = fetch(&app, &admin, &b).await;
    let plan = plan_link(&app, &admin, &a, &b, "FS", 3).await;
    assert_eq!(
        plan.json["shifts"].as_array().unwrap().len(),
        2,
        "{:?}",
        plan.json
    );
    let stale = approve(&plan.json);
    unlink(&app, &admin, &b, &c).await;
    let c_now = fetch(&app, &admin, &c).await;
    let r = set_link(
        &app,
        &admin,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 3, "successor_shifts": stale }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert_eq!(r.json["error"], "shift_conflict");
    let b_after = fetch(&app, &admin, &b).await;
    assert_eq!(b_after["predecessors"][0]["lag_days"], 0, "link unchanged");
    assert_eq!(
        (dates(&b_after), b_after["version"].clone()),
        (dates(&b_now), b_now["version"].clone())
    );
    let c_after = fetch(&app, &admin, &c).await;
    assert_eq!(
        (dates(&c_after), c_after["version"].clone()),
        (dates(&c_now), c_now["version"].clone())
    );

    // 처음부터 잘못된 요청은 400 그대로: 다른 항목 업무. 지금 선행 쪽인 업무는 미리본 뒤 관계가 바뀐 오래된 계획으로
    // 보아 409다(0082, 감사 S4).
    let a_now = fetch(&app, &admin, &a).await;
    let (status, body) = update(
        &app,
        &admin,
        &b_after,
        json!({ "successor_shifts": [{ "task_id": a_now["id"], "base_version": a_now["version"],
                "start_date": a_now["start_date"], "due_date": a_now["due_date"] }] }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    for bad in [&x] {
        let (status, body) = update(
            &app,
            &admin,
            &b_after,
            json!({ "successor_shifts": [{ "task_id": bad["id"], "base_version": bad["version"],
                    "start_date": bad["start_date"], "due_date": bad["due_date"] }] }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    }
}

/// 한 번에 옮길 수 있는 후속 업무(500개)보다 많이 옮겨야 하는 계획은 미리보기가 승인 전에 막혔다고 알린다(일부를
/// 빼거나 나눠 저장하지 않는다). 500개는 그대로 승인된다. 옮기지 않고 저장하기(빈 승인 목록)는 늘 된다.
#[tokio::test]
async fn plans_over_the_shift_limit_are_blocked_before_approval() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "한도").await;
    let item = module_item(&app, &admin, &ws, "LIM").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let mut followers = Vec::new();
    for n in 0..501 {
        let s = dated(
            &app,
            &admin,
            &item,
            &format!("S{n}"),
            "2026-10-06",
            "2026-10-06",
        )
        .await;
        assert_eq!(link(&app, &admin, &a, &s).await, StatusCode::OK);
        followers.push(s);
    }
    // 501개: 막힘. 제안 목록은 모두 보인다.
    let plan = date_plan(&app, &admin, &a, json!("2026-10-01"), json!("2026-10-06")).await;
    assert_eq!(plan["shifts"].as_array().unwrap().len(), 501);
    assert_eq!(
        plan["blocked"]["reason"], "too_many_shifts",
        "{:?}",
        plan["blocked"]
    );
    assert_eq!(plan["blocked"]["shift_count"], 501);
    assert_eq!(plan["blocked"]["max_shifts"], 500);
    let (status, _) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-06", "successor_shifts": approve(&plan) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        fetch(&app, &admin, &a).await["due_date"],
        "2026-10-05",
        "nothing saved"
    );
    // 500개: 막히지 않고 승인하면 모두 옮겨진다.
    unlink(&app, &admin, &a, &followers[500]).await;
    let plan = date_plan(&app, &admin, &a, json!("2026-10-01"), json!("2026-10-06")).await;
    assert_eq!(plan["shifts"].as_array().unwrap().len(), 500);
    assert!(plan["blocked"].is_null(), "{:?}", plan["blocked"]);
    let (status, body) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-06", "successor_shifts": approve(&plan) }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(
        fetch(&app, &admin, &followers[0]).await["start_date"],
        "2026-10-07"
    );
    assert_eq!(
        fetch(&app, &admin, &followers[500]).await["start_date"],
        "2026-10-06"
    );
}

/// 제안 날짜가 저장할 수 있는 범위(1900~2200년) 밖이면 미리보기가 승인 전에 막혔다고 알린다(기간을 자르거나 업무를
/// 빼지 않는다). 그 목록으로 저장하면 아무것도 바뀌지 않는다. 옮기지 않고 저장하기는 된다.
#[tokio::test]
async fn plans_with_dates_outside_the_saveable_range_are_blocked() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "날짜 범위").await;
    let item = module_item(&app, &admin, &ws, "RNG").await;
    let a = dated(&app, &admin, &item, "A", "2200-12-20", "2200-12-29").await;
    let b = dated(&app, &admin, &item, "B", "2200-12-30", "2200-12-31").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);
    let plan = date_plan(&app, &admin, &a, json!("2200-12-20"), json!("2200-12-30")).await;
    assert_eq!(
        plan["shifts"][0]["due_date"], "2201-01-01",
        "the plan is not clipped"
    );
    assert_eq!(
        plan["blocked"]["reason"], "date_out_of_range",
        "{:?}",
        plan["blocked"]
    );
    assert_eq!(plan["blocked"]["task_id"], b["id"]);
    assert_eq!(plan["blocked"]["date"], "2201-01-01");
    let (status, _) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2200-12-30", "successor_shifts": approve(&plan) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        dates(&fetch(&app, &admin, &a).await),
        dates(&a),
        "nothing saved"
    );
    assert_eq!(dates(&fetch(&app, &admin, &b).await), dates(&b));
    // 관계 경로도 같다: FS·5는 B를 2201년으로 민다.
    let plan = plan_link(&app, &admin, &a, &b, "FS", 5).await;
    assert_eq!(plan.json["blocked"]["reason"], "date_out_of_range");
    // 옮기지 않고 저장하기(빈 승인 목록)는 정책대로 된다.
    let (status, _) = update(&app, &admin, &a, json!({ "due_date": "2200-12-30" })).await;
    assert_eq!(status, StatusCode::OK);
}

/// 날짜 바꾸기의 재배치 계획(읽기만): `{shifts, undated, blocked}`.
async fn date_plan_of(app: &Router, user: &TestUser, t: &Value, start: &str, due: &str) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/tasks/{}/schedule-plan", t["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "start_date": start, "due_date": due })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

/// 감사 S2: 미리본 뒤 후속 업무의 날짜 모양이 바뀌면(시작일을 지움) 오래된 계획이다. 전에는 판 비교 뒤의 날짜 모양
/// 검사가 먼저 400을 내 화면이 다시 미리 보지 못했다. 두 경로(날짜 바꾸기·관계 바꾸기) 모두 409 `shift_conflict`다.
#[tokio::test]
async fn a_successor_that_lost_a_date_after_the_preview_makes_the_plan_stale() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "날짜 모양 바뀜").await;
    let item = module_item(&app, &admin, &ws, "SHP").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-08").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);

    // 날짜 바꾸기 경로.
    let plan = date_plan_of(&app, &admin, &a, "2026-10-01", "2026-10-10").await;
    let approved = approve(&plan);
    assert_eq!(approved.len(), 1, "{plan:?}");
    let (status, _) = update(
        &app,
        &admin,
        &fetch(&app, &admin, &b).await,
        json!({ "start_date": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = update(
        &app,
        &admin,
        &a,
        json!({ "due_date": "2026-10-10", "successor_shifts": approved }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"], "shift_conflict");

    // 관계 바꾸기 경로: 다시 시작일을 두고 계획한 뒤 시작일을 지운다.
    let (status, _) = update(
        &app,
        &admin,
        &fetch(&app, &admin, &b).await,
        json!({ "start_date": "2026-10-06" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let plan = plan_link(&app, &admin, &a, &b, "FS", 5).await;
    assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
    let approved = approve(&plan.json);
    assert_eq!(approved.len(), 1, "{:?}", plan.json);
    let (status, _) = update(
        &app,
        &admin,
        &fetch(&app, &admin, &b).await,
        json!({ "start_date": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let r = set_link(
        &app,
        &admin,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 5, "successor_shifts": approved }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert_eq!(r.json["error"], "shift_conflict");
}

/// 감사 S4: A→B로 미리본 뒤 관계가 뒤집히면(B→A) B는 지금 A의 선행 쪽이다. 미리볼 때는 후속이었으므로 오래된 계획(409)이지
/// 잘못된 요청(400)이 아니다.
#[tokio::test]
async fn a_plan_made_stale_by_a_reversed_link_conflicts() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "관계 뒤집힘").await;
    let item = module_item(&app, &admin, &ws, "REV").await;
    let a = dated(&app, &admin, &item, "A", "2026-10-01", "2026-10-05").await;
    let b = dated(&app, &admin, &item, "B", "2026-10-06", "2026-10-08").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);
    let plan = date_plan_of(&app, &admin, &a, "2026-10-01", "2026-10-10").await;
    let approved = approve(&plan);
    assert_eq!(approved.len(), 1, "{plan:?}");
    let removed = send(&app, Method::DELETE, &path_of(&a, &b), Some(&admin), None).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);
    assert_eq!(link(&app, &admin, &b, &a).await, StatusCode::OK);
    let (status, body) = update(
        &app,
        &admin,
        &fetch(&app, &admin, &a).await,
        json!({ "due_date": "2026-10-10", "successor_shifts": approved }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"], "shift_conflict");
    // 고치는 업무 자신이나 다른 항목 업무는 처음부터 잘못된 요청이라 400 그대로다.
    let self_shift = json!([{ "task_id": a["id"], "base_version": 1, "start_date": "2026-10-02", "due_date": "2026-10-06" }]);
    let (status, _) = update(
        &app,
        &admin,
        &fetch(&app, &admin, &a).await,
        json!({ "due_date": "2026-10-10", "successor_shifts": self_shift }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

/// 감사 W4: 관계 편집은 화면이 편집을 시작한 관계(`base`)를 함께 보낸다. 그사이 다른 사람이 종류·지연일을 바꿨거나
/// 관계를 지웠으면 아무것도 바꾸지 않고 409 `link_conflict`(최신본 포함)다. 전에는 기준 없이 저장해 다른 사람의 변경을
/// 말없이 덮었고, 지워진 관계를 다시 만들었다.
#[tokio::test]
async fn link_edits_from_a_stale_view_conflict_instead_of_overwriting() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "관계 기준").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "BAS").await;
    let a = task(&app, &admin, &item, "A").await;
    let b = task(&app, &admin, &item, "B").await;
    assert_eq!(link(&app, &admin, &a, &b).await, StatusCode::OK);

    // 두 사람 모두 FS·0을 보고 편집을 시작했다. 한 사람이 먼저 SS·3으로 바꾼다.
    let seen = json!({ "kind": "FS", "lag_days": 0 });
    let first = set_link(
        &app,
        &writer,
        &a,
        &b,
        json!({ "kind": "SS", "lag_days": 3, "base": seen }),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.json);
    // 다른 사람이 옛 화면(FS·0)에서 지연일만 1로 바꾸면 덮지 않는다.
    let stale = set_link(
        &app,
        &admin,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 1, "base": seen }),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    assert_eq!(stale.json["error"], "link_conflict");
    assert_eq!(
        stale.json["current"],
        json!({ "predecessor_id": a["id"], "successor_id": b["id"], "kind": "SS", "lag_days": 3 })
    );
    assert_eq!(
        fetch(&app, &admin, &b).await["predecessors"],
        json!([{ "task_id": a["id"], "kind": "SS", "lag_days": 3 }])
    );
    // 최신 관계를 기준으로 다시 보내면 저장된다.
    let again = set_link(
        &app,
        &admin,
        &a,
        &b,
        json!({ "kind": "FS", "lag_days": 1, "base": { "kind": "SS", "lag_days": 3 } }),
    )
    .await;
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);

    // 고치는 사이 관계가 지워졌으면 다시 만들지 않는다.
    let removed = send(&app, Method::DELETE, &path_of(&a, &b), Some(&writer), None).await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);
    let revived = set_link(
        &app,
        &admin,
        &a,
        &b,
        json!({ "kind": "FF", "lag_days": 0, "base": { "kind": "FS", "lag_days": 1 } }),
    )
    .await;
    assert_eq!(revived.status, StatusCode::CONFLICT, "{:?}", revived.json);
    assert_eq!(revived.json["error"], "link_conflict");
    assert_eq!(fetch(&app, &admin, &b).await["predecessors"], json!([]));
}
