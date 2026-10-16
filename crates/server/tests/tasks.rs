#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 업무·하위 업무·담당자·업무 체크리스트 (D-14, D-18, D-30, D-31, D-48, D-57).

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
    i.json
}

async fn make_public(app: &Router, admin: &TestUser, item: &Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "title": item["title"], "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
}

async fn create_task(
    app: &Router,
    user: &TestUser,
    item: &Value,
    title: &str,
    parent: Option<&Value>,
) -> (StatusCode, Value) {
    let r = send(app, Method::POST, &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()), Some(user),
                 Some(json!({ "request_id": new_request_id(), "title": title, "parent_id": parent.map(|p| p["id"].clone()) }))).await;
    (r.status, r.json)
}

async fn task(app: &Router, user: &TestUser, item: &Value, title: &str) -> Value {
    let (status, json) = create_task(app, user, item, title, None).await;
    assert_eq!(status, StatusCode::CREATED, "{json:?}");
    json
}

fn edit(task: &Value, changes: Value) -> Value {
    let mut body = json!({
        "base_version": task["version"], "title": task["title"], "description": task["description"],
        "status": task["status"], "priority": task["priority"], "start_date": task["start_date"],
        "due_date": task["due_date"], "estimate_minutes": task["estimate_minutes"], "tags": task["tags"],
        "assignee_ids": task["assignees"].as_array().unwrap().iter().map(|a| a["user_id"].clone()).collect::<Vec<_>>(),
    });
    for (k, v) in changes.as_object().unwrap() {
        body[k] = v.clone();
    }
    body
}

async fn update(
    app: &Router,
    user: &TestUser,
    task: &Value,
    changes: Value,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/tasks/{}", task["id"].as_str().unwrap()),
        Some(user),
        Some(edit(task, changes)),
    )
    .await;
    (r.status, r.json)
}

async fn get(app: &Router, user: &TestUser, uri: &str) -> (StatusCode, Value) {
    let r = send(app, Method::GET, uri, Some(user), None).await;
    (r.status, r.json)
}

#[tokio::test]
async fn tasks_share_the_module_sequence_and_follow_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "업무").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "DEV").await;
    assert_eq!(item["reference"], "DEV-1");

    let t = task(&app, &admin, &item, "로그인 화면").await;
    assert_eq!(t["reference"], "DEV-2");
    let d = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "설계" })),
    )
    .await
    .json;
    assert_eq!(d["reference"], "DEV-3");
    let (status, sub) = create_task(&app, &admin, &item, "입력 검증", Some(&t)).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(sub["reference"], "DEV-4");
    assert_eq!(sub["parent_id"], t["id"]);
    // 하위 업무의 하위 업무, 다른 항목의 상위 업무는 안 된다.
    assert_eq!(
        create_task(&app, &admin, &item, "너무 깊음", Some(&sub))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let other = module_item(&app, &admin, &ws, "OPS").await;
    assert_eq!(
        create_task(&app, &admin, &other, "다른 항목", Some(&t))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    // 비공개 항목: 작성자도 볼 수 없다. 공개하면 역할대로.
    let task_uri = format!("/api/tasks/{}", t["id"].as_str().unwrap());
    let list_uri = format!("/api/items/{}/tasks", item["id"].as_str().unwrap());
    assert_eq!(get(&app, &writer, &task_uri).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, &writer, &list_uri).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        get(&app, &outsider, &task_uri).await.0,
        StatusCode::NOT_FOUND
    );
    make_public(&app, &admin, &item).await;
    let (_, list) = get(&app, &reader, &list_uri).await;
    let refs: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["reference"].as_str().unwrap())
        .collect();
    assert_eq!(refs, vec!["DEV-2", "DEV-4"], "subtask follows its parent");
    let (_, current) = get(&app, &reader, &task_uri).await;
    assert_eq!(current["access"], "read");
    assert_eq!(
        update(&app, &reader, &current, json!({ "title": "독자 수정" }))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let (status, edited) = update(&app, &writer, &current, json!({ "status": "done", "priority": "high", "due_date": "2026-10-31", "tags": ["화면", "화면", " 검토 "] })).await;
    assert_eq!(status, StatusCode::OK, "{edited:?}");
    assert_eq!(edited["status"], "done");
    assert!(
        edited["completed_at"].is_string(),
        "completion time is recorded by the server"
    );
    assert_eq!(edited["due_date"], "2026-10-31");
    assert_eq!(edited["tags"], json!(["화면", "검토"]));
    let (_, reopened) = update(&app, &writer, &edited, json!({ "status": "todo" })).await;
    assert_eq!(reopened["completed_at"], Value::Null);
    assert_eq!(
        update(
            &app,
            &writer,
            &reopened,
            json!({ "start_date": "2026-11-01", "due_date": "2026-10-01" })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );

    // 보관된 항목 아래 업무는 읽기 전용.
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/archived", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    let (_, archived) = get(&app, &admin, &task_uri).await;
    assert_eq!(archived["access"], "read");
    assert_eq!(
        update(&app, &admin, &archived, json!({ "title": "보관 중" }))
            .await
            .0,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn versions_guard_field_edits_but_not_board_moves() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "칸반").await;
    let item = module_item(&app, &admin, &ws, "DEV").await;
    let a = task(&app, &admin, &item, "가").await;
    let b = task(&app, &admin, &item, "나").await;

    // 같은 열 안에서 순서만 바꾸면 버전이 그대로다.
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{}/position", b["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "status": "todo", "before_id": a["id"] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["version"], b["version"]);
    let (_, list) = get(
        &app,
        &admin,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
    )
    .await;
    let titles: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["나", "가"]);
    // 그래서 그 전에 읽은 버전으로도 필드를 고칠 수 있다.
    assert_eq!(
        update(&app, &admin, &b, json!({ "title": "나 수정" }))
            .await
            .0,
        StatusCode::OK
    );

    // 다른 열로 옮기면 버전이 올라, 오래된 필드 편집이 상태를 조용히 되돌리지 못한다(409와 최신본).
    let moved = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{}/position", a["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "status": "in_progress", "before_id": null })),
    )
    .await
    .json;
    assert_eq!(moved["status"], "in_progress");
    let (status, conflict) = update(&app, &admin, &a, json!({ "title": "가 수정" })).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["current"]["status"], "in_progress");
}

#[tokio::test]
async fn assignees_must_see_the_item_and_lose_assignments_with_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "담당").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "writer").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "DEV").await;
    let t = task(&app, &admin, &item, "배포").await;

    // 비공개 항목: 볼 수 없는 동료는 담당자가 될 수 없다. 멤버가 아닌 사람도.
    let (status, _) = update(
        &app,
        &admin,
        &t,
        json!({ "assignee_ids": [peer.id.0.to_string()] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        update(
            &app,
            &admin,
            &t,
            json!({ "assignee_ids": [outsider.id.0.to_string()] })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    make_public(&app, &admin, &item).await;
    let (_, picks) = get(
        &app,
        &admin,
        &format!("/api/items/{}/assignable", item["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(picks.as_array().unwrap().len(), 3);
    let (status, assigned) = update(
        &app,
        &admin,
        &t,
        json!({ "assignee_ids": [peer.id.0.to_string(), writer.id.0.to_string()] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{assigned:?}");
    assert_eq!(assigned["assignees"].as_array().unwrap().len(), 2);

    // 항목이 다시 비공개가 되면 볼 수 없게 된 담당자는 빠진다.
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "title": item["title"], "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let (_, after_private) = get(
        &app,
        &admin,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(after_private["assignees"], json!([]));

    // 멤버에서 빠지면 담당에서도 빠진다.
    make_public(&app, &admin, &item).await;
    let (_, again) = update(
        &app,
        &admin,
        &after_private,
        json!({ "assignee_ids": [writer.id.0.to_string()] }),
    )
    .await;
    assert_eq!(again["assignees"].as_array().unwrap().len(), 1);
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", writer.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
    let (_, removed) = get(
        &app,
        &admin,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(removed["assignees"], json!([]));

    // 담당 변경은 감사 기록에 남는다 (D-57).
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid AND target_type = 'task' ORDER BY created_at, id",
    )
    .bind(t["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert!(
        actions.starts_with(&["create".to_owned()])
            && actions.iter().filter(|a| *a == "assign").count() == 3,
        "{actions:?}"
    );
}

#[tokio::test]
async fn checklists_are_idempotent_and_reader_cannot_toggle() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "체크리스트").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let item = module_item(&app, &admin, &ws, "DEV").await;
    make_public(&app, &admin, &item).await;
    let t = task(&app, &admin, &item, "준비").await;
    let uri = format!("/api/tasks/{}/checklist", t["id"].as_str().unwrap());
    let batch = json!({ "items": [
        { "request_id": new_request_id(), "text": "자료 수집", "checked": true },
        { "request_id": new_request_id(), "text": "초안 작성", "checked": false },
    ] });
    let first = send(&app, Method::POST, &uri, Some(&admin), Some(batch.clone())).await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.json);
    let replay = send(&app, Method::POST, &uri, Some(&admin), Some(batch)).await;
    assert_eq!(
        replay.json["checklist"].as_array().unwrap().len(),
        2,
        "replayed batch adds nothing"
    );
    assert_eq!(replay.json["checklist_done"], 1);
    assert_eq!(replay.json["checklist_total"], 2);

    let item_id = replay.json["checklist"][1]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let toggle = |user: &TestUser| {
        let app = app.clone();
        let user = user.clone();
        let item_id = item_id.clone();
        async move {
            send(
                &app,
                Method::PUT,
                &format!("/api/checklist-items/{item_id}"),
                Some(&user),
                Some(json!({ "text": "초안 작성", "checked": true })),
            )
            .await
            .status
        }
    };
    assert_eq!(toggle(&reader).await, StatusCode::FORBIDDEN);
    assert_eq!(toggle(&admin).await, StatusCode::NO_CONTENT);
    let (_, detail) = get(
        &app,
        &reader,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(detail["checklist_done"], 2);
    // 본문이 아닌 업무 체크리스트는 독자가 지울 수 없다.
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/checklist-items/{item_id}"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn tasks_are_reference_targets_and_concurrent_parent_swaps_do_not_cycle() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "참조").await;
    let item = module_item(&app, &admin, &ws, "DEV").await;
    let t = task(&app, &admin, &item, "배포 준비").await;

    // @DEV-2로 업무를 찾고, 문서가 가리키면 업무의 역방향 링크에 나온다.
    let (_, found) = get(
        &app,
        &admin,
        &format!("/api/reference-candidates?workspace_id={ws}&q=dev-2"),
    )
    .await;
    assert_eq!(found[0]["kind"], "task");
    assert_eq!(found[0]["id"], t["id"]);
    let resolved = send(
        &app,
        Method::POST,
        "/api/references/resolve",
        Some(&admin),
        Some(json!({ "refs": [{ "kind": "task", "id": t["id"] }] })),
    )
    .await
    .json;
    assert_eq!(resolved[0]["reference"], "DEV-2");
    assert_eq!(resolved[0]["item_id"], item["id"]);
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "회의" })),
    )
    .await
    .json;
    let body = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
        { "type": "reference", "attrs": { "kind": "task", "id": t["id"] } } ] }] });
    let saved = send(&app, Method::PUT, &format!("/api/documents/{}", doc["id"].as_str().unwrap()), Some(&admin),
                     Some(json!({ "request_id": new_request_id(), "base_revision": doc["revision"], "title": "회의", "body": body }))).await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let (_, links) = get(
        &app,
        &admin,
        &format!("/api/tasks/{}/backlinks", t["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(links[0]["id"], doc["id"]);

    // 서로를 상위 업무로 삼는 두 변경을 동시에 보내도 하나만 성공한다.
    for _ in 0..5 {
        let a = task(&app, &admin, &item, "A").await;
        let b = task(&app, &admin, &item, "B").await;
        let set = |child: &Value, parent: &Value| {
            let app = app.clone();
            let admin = admin.clone();
            let uri = format!("/api/tasks/{}/parent", child["id"].as_str().unwrap());
            let parent = parent["id"].clone();
            async move {
                send(
                    &app,
                    Method::PUT,
                    &uri,
                    Some(&admin),
                    Some(json!({ "parent_id": parent })),
                )
                .await
                .status
            }
        };
        let (ra, rb) = tokio::join!(set(&a, &b), set(&b, &a));
        assert_eq!(
            [ra, rb].iter().filter(|s| **s == StatusCode::OK).count(),
            1,
            "{ra} {rb}"
        );
    }
}

#[tokio::test]
async fn demotion_drops_assignments_on_items_the_member_can_no_longer_see() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "강등").await;
    let (deputy, _) = add_member(&app, &state, &admin, &ws, "deputy", "admin").await;
    // 비공개 항목: 관리자만 본다.
    let item = module_item(&app, &admin, &ws, "SEC").await;
    let t = task(&app, &admin, &item, "점검").await;
    let (status, assigned) = update(
        &app,
        &admin,
        &t,
        json!({ "assignee_ids": [deputy.id.0.to_string()] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{assigned:?}");

    // 기존 담당자는 그대로 두고 다른 필드만 고쳐도 된다(새로 넣는 담당자만 확인한다).
    let (status, renamed) = update(&app, &admin, &assigned, json!({ "title": "정기 점검" })).await;
    assert_eq!(status, StatusCode::OK, "{renamed:?}");
    assert_eq!(renamed["assignees"].as_array().unwrap().len(), 1);

    let r = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", deputy.id.0),
        Some(&admin),
        Some(json!({ "role": "writer" })),
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
    let (_, after) = get(
        &app,
        &admin,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(after["assignees"], json!([]));

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let reasons: Vec<String> = sqlx::query_scalar(
        "SELECT detail->>'reason' FROM audit_log
         WHERE target_id = $1::uuid AND target_type = 'task' AND action = 'unassign'",
    )
    .bind(t["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(reasons, vec!["role_change".to_owned()]);
}
