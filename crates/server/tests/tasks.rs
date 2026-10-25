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
    let uri = format!("/api/items/{}", item["id"].as_str().unwrap());
    // 공개 범위만 바꾸는 도우미라 지금 판으로 보낸다.
    let version = send(app, Method::GET, &uri, Some(admin), None).await.json["version"].clone();
    let r = send(
        app,
        Method::PUT,
        &uri,
        Some(admin),
        Some(json!({ "base_version": version, "title": item["title"], "description": "", "visibility": "workspace" })),
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
        "milestone": task["milestone"],
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
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
        Some(json!({ "base_version": 2, "title": item["title"], "description": "", "visibility": "private" })),
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
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
        Some(json!({ "role": "member" })),
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

// 문구 고치기와 완료 바꾸기를 서로 다른 사람이 각자 옛 화면에서 해도 두 변경이 모두 남는다(보낸 칸만 바꾼다). 전에는
// 문구를 고치면 화면에 남은 옛 완료 상태를, 완료를 바꾸면 옛 문구를 함께 보내 다른 사람의 변경을 되돌렸다.
#[tokio::test]
async fn checklist_rename_and_toggle_by_two_people_both_stay() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "체크리스트 동시").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "CHK").await;
    make_public(&app, &admin, &item).await;
    let t = task(&app, &admin, &item, "보고서").await;
    let task_uri = format!("/api/tasks/{}", t["id"].as_str().unwrap());
    let added = send(
        &app,
        Method::POST,
        &format!("{task_uri}/checklist"),
        Some(&admin),
        Some(json!({ "items": [{ "request_id": new_request_id(), "text": "초안 작성", "checked": false }] })),
    )
    .await;
    assert_eq!(added.status, StatusCode::OK, "{:?}", added.json);
    let entry = format!(
        "/api/checklist-items/{}",
        added.json["checklist"][0]["id"].as_str().unwrap()
    );

    // 둘 다 "초안 작성 · 미완료"를 본 화면에서: 관리자는 문구만, 작성자는 완료만.
    let renamed = send(
        &app,
        Method::PUT,
        &entry,
        Some(&admin),
        Some(json!({ "text": "초안 다시 작성" })),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::NO_CONTENT, "{:?}", renamed.json);
    let checked = send(
        &app,
        Method::PUT,
        &entry,
        Some(&writer),
        Some(json!({ "checked": true })),
    )
    .await;
    assert_eq!(checked.status, StatusCode::NO_CONTENT, "{:?}", checked.json);
    let (_, detail) = get(&app, &writer, &task_uri).await;
    assert_eq!(detail["checklist"][0]["text"], "초안 다시 작성");
    assert_eq!(detail["checklist"][0]["checked"], true);

    // 반대 순서: 완료를 먼저 풀고 문구를 고쳐도 둘 다 남는다.
    let unchecked = send(
        &app,
        Method::PUT,
        &entry,
        Some(&writer),
        Some(json!({ "checked": false })),
    )
    .await;
    assert_eq!(unchecked.status, StatusCode::NO_CONTENT);
    let renamed = send(
        &app,
        Method::PUT,
        &entry,
        Some(&admin),
        Some(json!({ "text": "초안 쓰기" })),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::NO_CONTENT);
    let (_, detail) = get(&app, &admin, &task_uri).await;
    assert_eq!(detail["checklist"][0]["text"], "초안 쓰기");
    assert_eq!(detail["checklist"][0]["checked"], false);

    // 아무 칸도 없거나 빈 문구는 거부.
    for body in [json!({}), json!({ "text": "  " })] {
        let r = send(&app, Method::PUT, &entry, Some(&admin), Some(body)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    }
}

/// 칸반 이동 한 번. `base`를 주면 그 이동 판에서 읽은 업무만 옮긴다(D-89).
async fn move_to(
    app: &Router,
    user: &TestUser,
    task: &str,
    status: &str,
    before: Option<&str>,
    base: Option<i64>,
) -> (StatusCode, Value) {
    let mut body = json!({ "status": status, "before_id": before });
    if let Some(base) = base {
        body["base_move_version"] = json!(base);
    }
    let r = send(
        app,
        Method::PUT,
        &format!("/api/tasks/{task}/position"),
        Some(user),
        Some(body),
    )
    .await;
    (r.status, r.json)
}

/// 항목의 업무를 서버 순서대로: (제목, 상태, 이동 판, 필드 판).
async fn board(app: &Router, user: &TestUser, item: &Value) -> Vec<(String, String, i64, i64)> {
    let (status, list) = get(
        app,
        user,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{list:?}");
    list.as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["title"].as_str().unwrap().to_owned(),
                t["status"].as_str().unwrap().to_owned(),
                t["move_version"].as_i64().unwrap(),
                t["version"].as_i64().unwrap(),
            )
        })
        .collect()
}

/// D-89: 조건부 이동은 읽은 뒤 그사이 그 업무의 열·위치가 바뀌었으면(같은 열 안 순서만 바뀌어도) 아무것도 바꾸지 않고
/// 409를 돌려준다. 비교와 옮기기는 행을 잠근 한 트랜잭션이라 동시에 보내도 하나만 된다. 조건 없는 이동은 그대로다.
#[tokio::test]
async fn conditional_moves_refuse_a_place_changed_since_it_was_read() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "조건부 이동").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "CMV").await;
    make_public(&app, &admin, &item).await;
    let a = task(&app, &admin, &item, "가").await;
    let b = task(&app, &admin, &item, "나").await;
    let c = task(&app, &admin, &item, "다").await;
    let id = |t: &Value| t["id"].as_str().unwrap().to_owned();
    assert_eq!(a["move_version"], 0);

    // 다른 사람이 먼저 옮기면, 그 전에 읽은 판으로 보낸 이동은 409와 지금 업무를 받고 아무것도 바꾸지 못한다.
    let (status, _) = move_to(&app, &writer, &id(&a), "in_progress", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let before = board(&app, &admin, &item).await;
    let (status, conflict) = move_to(&app, &admin, &id(&a), "done", None, Some(0)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict:?}");
    assert_eq!(conflict["error"], "move_conflict");
    assert_eq!(conflict["current"]["status"], "in_progress");
    assert_eq!(conflict["current"]["move_version"], 1);
    assert_eq!(board(&app, &admin, &item).await, before);

    // 같은 열 안에서 순서만 바뀌어도 이동 판이 오른다(필드 판은 그대로라 상세 편집은 막지 않는다).
    let (status, reordered) = move_to(&app, &writer, &id(&c), "todo", Some(&id(&b)), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reordered["move_version"], 1);
    assert_eq!(reordered["version"], c["version"]);
    let before = board(&app, &admin, &item).await;
    let (status, _) = move_to(&app, &admin, &id(&c), "done", None, Some(0)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(board(&app, &admin, &item).await, before);
    // 지금 판으로 보내면 옮겨진다.
    let (status, moved) = move_to(&app, &admin, &id(&c), "done", None, Some(1)).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["move_version"], 2);

    // 응답을 잃은 뒤 같은 판으로 다시 보내면(이미 반영됨) 두 번 옮기지 않고 409다.
    let (status, first) = move_to(&app, &admin, &id(&b), "in_progress", None, Some(0)).await;
    assert_eq!(status, StatusCode::OK);
    let after_first = board(&app, &admin, &item).await;
    let (status, again) = move_to(&app, &admin, &id(&b), "in_progress", None, Some(0)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{again:?}");
    assert_eq!(again["current"]["move_version"], first["move_version"]);
    assert_eq!(board(&app, &admin, &item).await, after_first);

    // 되돌리기: 내 이동 뒤 다른 사람이 같은 열 안에서 순서를 바꿨으면 되돌리지 못한다.
    let mine = first["move_version"].as_i64().unwrap();
    let (status, _) = move_to(&app, &writer, &id(&b), "in_progress", Some(&id(&a)), None).await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = move_to(&app, &admin, &id(&b), "todo", None, Some(mine)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 필드 고치기로 상태를 바꿔도 이동 판이 오른다(어느 경로로 바뀌었든 같은 판).
    let (_, fresh_a) = get(&app, &admin, &format!("/api/tasks/{}", id(&a))).await;
    let read = fresh_a["move_version"].as_i64().unwrap();
    let (status, edited) = update(&app, &writer, &fresh_a, json!({ "status": "done" })).await;
    assert_eq!(status, StatusCode::OK, "{edited:?}");
    assert_eq!(edited["move_version"].as_i64().unwrap(), read + 1);
    let (status, _) = move_to(&app, &admin, &id(&a), "todo", None, Some(read)).await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 같은 판으로 동시에 보내면 하나만 옮겨지고 나머지는 409다(비교와 옮기기 사이에 끼지 못한다).
    let base = edited["move_version"].as_i64().unwrap();
    let ta = id(&a);
    let (one, two) = tokio::join!(
        move_to(&app, &admin, &ta, "todo", None, Some(base)),
        move_to(&app, &writer, &ta, "in_progress", None, Some(base)),
    );
    let mut statuses = [one.0, two.0];
    statuses.sort_by_key(StatusCode::as_u16);
    assert_eq!(statuses, [StatusCode::OK, StatusCode::CONFLICT]);
    let (_, now) = get(&app, &admin, &format!("/api/tasks/{ta}")).await;
    assert_eq!(now["move_version"].as_i64().unwrap(), base + 1);
    let winner = if one.0 == StatusCode::OK {
        "todo"
    } else {
        "in_progress"
    };
    assert_eq!(now["status"], winner);

    // 조건 없는 이동은 이전과 같다(판과 상관없이 옮김).
    let (status, plain) = move_to(&app, &writer, &ta, "done", None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(plain["status"], "done");
}

/// 초기화 복제는 모든 열을 "할 일"로 합친다. 원본의 열마다 있던 위치값이 겹치면 그 뒤의 이동이 엉뚱한 자리로 간다
/// (C를 B 앞으로 옮겼는데 [C, A, B]). 복제가 합친 열의 위치를 다시 매긴다(앞선 복제가 남긴 겹침은 0079가 한 번에 매긴다).
#[tokio::test]
async fn reset_copy_merges_lanes_without_duplicate_positions() {
    use sqlx::Connection;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "복제 위치").await;
    let item = module_item(&app, &admin, &ws, "CPY").await;
    let item_id = item["id"].as_str().unwrap().to_owned();
    let _a = task(&app, &admin, &item, "A").await;
    let b = task(&app, &admin, &item, "B").await;
    let _c = task(&app, &admin, &item, "C").await;
    // 원본: A(todo, 1), B(done, 1), C(todo, 3). 복제가 합치면 A와 B의 위치가 겹친다.
    let moved = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{}/position", b["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "status": "done", "before_id": null })),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    let positions = |item: &str| {
        let item = item.to_owned();
        async move {
            let mut conn = sqlx::PgConnection::connect(
                &std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`"),
            )
            .await
            .unwrap();
            // 상태는 항목별 정의(D-116): 이름과 열 순서는 task_statuses에서 온다.
            let rows: Vec<(String, String, String)> = sqlx::query_as(
                "SELECT t.title, s.name, t.position::text FROM fvoci.tasks t \
                 JOIN fvoci.task_statuses s ON s.id = t.status_id WHERE t.item_id = $1::uuid \
                 ORDER BY s.position, t.position, t.number",
            )
            .bind(item)
            .fetch_all(&mut conn)
            .await
            .unwrap();
            rows
        }
    };
    let src = positions(&item_id).await;
    assert_eq!(
        src.iter()
            .map(|(t, s, p)| format!("{t}:{s}:{p}"))
            .collect::<Vec<_>>(),
        vec!["A:todo:1", "C:todo:3", "B:done:1"],
        "{src:?}"
    );

    let copied = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/copy"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "module_id": item["module_id"],
                     "title": "복제", "reset": true }),
        ),
    )
    .await;
    assert_eq!(copied.status, StatusCode::CREATED, "{:?}", copied.json);
    let copy_id = copied.json["item_id"].as_str().unwrap().to_owned();
    let (_, list) = get(&app, &admin, &format!("/api/items/{copy_id}/tasks")).await;
    let by_title = |title: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|t| t["title"] == title)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let cb = by_title("B");
    // 합쳐진 "할 일" 열의 위치값은 서로 다르고 보이는 순서(A, B, C)를 유지한다.
    let merged = positions(&copy_id).await;
    assert_eq!(
        merged
            .iter()
            .map(|(t, s, _)| format!("{t}:{s}"))
            .collect::<Vec<_>>(),
        vec!["A:todo", "B:todo", "C:todo"]
    );
    let mut seen = std::collections::HashSet::new();
    assert!(
        merged.iter().all(|(_, _, p)| seen.insert(p.clone())),
        "duplicate positions {merged:?}"
    );

    // C를 B 바로 앞으로: 기대 [A, C, B].
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{}/position", by_title("C")),
        Some(&admin),
        Some(json!({ "status": "todo", "before_id": cb })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let (_, list) = get(&app, &admin, &format!("/api/items/{copy_id}/tasks")).await;
    let titles: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, vec!["A", "C", "B"]);
}

/// 되돌리기 원점이 잘못된 요청(감사 2번)을 서버가 복원하지 못함을 실제 API로 보인다: [A, B, C]에서 A를 옮긴 뒤 A를
/// "C 앞"으로 되돌리면(수정 전 웹이 보내던 요청, 정상 이동 판 포함) 서버는 그대로 [B, A, C]로 저장한다. 서버의
/// 조건(`base_move_version`)은 그사이 바뀐 자리만 막고 위치 의도는 검사할 수 없으므로 원점은 웹이 바르게 잡아야 한다.
#[tokio::test]
async fn a_wrong_undo_target_is_saved_as_sent_even_with_a_valid_move_version() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "되돌리기 원점").await;
    let item = module_item(&app, &admin, &ws, "UNDO").await;
    let item_id = item["id"].as_str().unwrap().to_owned();
    let a = task(&app, &admin, &item, "A").await;
    let _b = task(&app, &admin, &item, "B").await;
    let c = task(&app, &admin, &item, "C").await;
    let move_a = |status: &str, before: Value, base: Value| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/tasks/{}/position", a["id"].as_str().unwrap());
        let body = json!({ "status": status, "before_id": before, "base_move_version": base });
        async move { send(&app, Method::PUT, &uri, Some(&admin), Some(body)).await }
    };
    let moved = move_a("in_progress", Value::Null, Value::Null).await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);
    // 수정 전 웹의 되돌리기: 화면 배열([A, C, B], B의 낙관적 이동이 얹힘)에서 잡은 원점 "C 앞".
    let back = move_a("todo", c["id"].clone(), moved.json["move_version"].clone()).await;
    assert_eq!(back.status, StatusCode::OK, "{:?}", back.json);
    let (_, list) = get(&app, &admin, &format!("/api/items/{item_id}/tasks")).await;
    let titles: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        vec!["B", "A", "C"],
        "the server keeps the wrong intent as sent"
    );
}

/// D-105: 한 요청이 업무 한 건의 모든 칸을 가져온다. (작업 공간, 항목 id) 아래 남은 업무·담당·업무 감사 기록 수.
async fn task_rows(
    state: &fvoci_server::http::AppState,
    admin: &TestUser,
    item: &Value,
) -> (i64, i64, i64) {
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM tasks WHERE item_id = $1::uuid),
                (SELECT count(*) FROM task_assignees a JOIN tasks t ON t.id = a.task_id WHERE t.item_id = $1::uuid),
                (SELECT count(*) FROM audit_log l WHERE l.target_type = 'task'
                   AND l.workspace_id = (SELECT workspace_id FROM items WHERE id = $1::uuid))",
    )
    .bind(item["id"].as_str().unwrap())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    tx.commit().await.unwrap();
    counts
}

/// 만들기 요청 본문: 제목과 CSV 한 줄이 넣는 모든 칸.
fn full_create(request_id: &str, title: &str, changes: Value) -> Value {
    let mut body = json!({
        "request_id": request_id, "title": title, "description": "회의록 정리", "status": "in_progress",
        "priority": "high", "start_date": "2026-11-02", "due_date": "2026-11-09", "estimate_minutes": 30,
        "tags": ["회의", "보고", "회의"], "assignee_ids": [], "milestone": false,
    });
    for (k, v) in changes.as_object().unwrap() {
        body[k] = v.clone();
    }
    body
}

/// D-105(P-31): 만들기가 모든 칸을 받아 한 트랜잭션에서 확인·저장한다. 둘째 단계에서 거절될 칸(볼 수 없는 담당자,
/// 시작일이 마감일 뒤, 시작일 있는 이정표)이 있으면 업무·담당·감사 기록이 하나도 남지 않고 번호도 쓰지 않는다.
#[tokio::test]
async fn a_rejected_create_leaves_no_task_and_no_relations() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "한 번에 만들기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "CSV").await;
    make_public(&app, &admin, &item).await;
    let uri = format!("/api/items/{}/tasks", item["id"].as_str().unwrap());
    let people = json!([writer.id.0.to_string(), admin.id.0.to_string()]);
    for (case, changes) in [
        (
            "not a member",
            json!({ "assignee_ids": [writer.id.0.to_string(), outsider.id.0.to_string()] }),
        ),
        (
            "start after due",
            json!({ "assignee_ids": people, "start_date": "2026-11-10" }),
        ),
        (
            "milestone with a start",
            json!({ "assignee_ids": people, "milestone": true }),
        ),
        (
            "negative estimate",
            json!({ "assignee_ids": people, "estimate_minutes": -1 }),
        ),
    ] {
        let r = send(
            &app,
            Method::POST,
            &uri,
            Some(&admin),
            Some(full_create(&new_request_id(), "주간 회의", changes)),
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{case}: {:?}", r.json);
        assert_eq!(
            task_rows(&state, &admin, &item).await,
            (0, 0, 0),
            "{case}: nothing is left behind"
        );
    }
    // 거절된 만들기는 번호도 쓰지 않았다(같은 트랜잭션에서 되돌림): 다음 업무가 항목 바로 다음 번호다.
    let request_id = new_request_id();
    let r = send(
        &app,
        Method::POST,
        &uri,
        Some(&admin),
        Some(full_create(
            &request_id,
            "주간 회의",
            json!({ "assignee_ids": people }),
        )),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(r.json["reference"], "CSV-2");
    assert_eq!(
        task_rows(&state, &admin, &item).await,
        (1, 2, 3),
        "create + two assigns"
    );
}

/// D-105(P-31): 모든 칸이 첫 판(1)에 들어가고, 같은 요청 id의 재시도(응답을 잃은 뒤)는 처음 만든 업무를 지금 상태로
/// 돌려줄 뿐 다시 만들거나 칸을 다시 쓰지 않는다. 내용이 다른 재시도도 기존 계약대로 처음 업무를 돌려준다. 동시에
/// 겹친 재시도도 업무 하나·담당 한 벌이다.
#[tokio::test]
async fn a_create_carries_every_field_and_a_replay_returns_the_first_task() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "한 번에 만들기 재시도").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "CSV").await;
    make_public(&app, &admin, &item).await;
    let uri = format!("/api/items/{}/tasks", item["id"].as_str().unwrap());
    let request_id = new_request_id();
    let body = full_create(
        &request_id,
        " 주간 회의 ",
        json!({ "assignee_ids": [writer.id.0.to_string()] }),
    );
    let first = send(&app, Method::POST, &uri, Some(&admin), Some(body.clone())).await;
    assert_eq!(first.status, StatusCode::CREATED, "{:?}", first.json);
    let t = &first.json;
    assert_eq!(t["title"], "주간 회의");
    assert_eq!(t["description"], "회의록 정리");
    assert_eq!(t["status"], "in_progress");
    assert_eq!(t["priority"], "high");
    assert_eq!(t["start_date"], "2026-11-02");
    assert_eq!(t["due_date"], "2026-11-09");
    assert_eq!(t["estimate_minutes"], 30);
    assert_eq!(t["tags"], json!(["회의", "보고"]));
    assert_eq!(t["assignees"][0]["user_id"], writer.id.0.to_string());
    assert_eq!(t["milestone"], false);
    assert_eq!(t["version"], 1, "one write, not create + update");
    assert_eq!(t["completed_at"], Value::Null);

    // 응답을 잃고 다시 보냄: 같은 업무, 칸은 그대로.
    let again = send(&app, Method::POST, &uri, Some(&admin), Some(body.clone())).await;
    assert_eq!(again.status, StatusCode::CREATED, "{:?}", again.json);
    assert_eq!(again.json["id"], t["id"]);
    assert_eq!(again.json["version"], 1);
    // 그사이 다른 사람이 고친 뒤의 재시도는 그 변경을 되돌리지 않는다. 내용이 다른 재시도도 처음 업무다(기존 계약).
    let (status, edited) = update(
        &app,
        &writer,
        &again.json,
        json!({ "status": "todo", "assignee_ids": [] }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited:?}");
    let other = full_create(
        &request_id,
        "다른 제목",
        json!({ "status": "done", "assignee_ids": [admin.id.0.to_string()] }),
    );
    let replay = send(&app, Method::POST, &uri, Some(&admin), Some(other)).await;
    assert_eq!(replay.status, StatusCode::CREATED, "{:?}", replay.json);
    assert_eq!(replay.json["id"], t["id"]);
    assert_eq!(replay.json["title"], "주간 회의");
    assert_eq!(replay.json["status"], "todo");
    assert_eq!(replay.json["assignees"], json!([]));
    assert_eq!(replay.json["version"], 2);
    assert_eq!(task_rows(&state, &admin, &item).await.0, 1);

    // 동시에 겹친 같은 요청: 하나만 만들고 모두 그 업무를 받는다. 완료로 만들면 완료 시각을 서버가 적는다.
    let request_id = new_request_id();
    let body = full_create(
        &request_id,
        "보고서",
        json!({
            "status": "done", "assignee_ids": [writer.id.0.to_string(), admin.id.0.to_string()],
        }),
    );
    let replies = futures_util::future::join_all(
        (0..4).map(|_| send(&app, Method::POST, &uri, Some(&admin), Some(body.clone()))),
    )
    .await;
    for r in &replies {
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
        assert_eq!(r.json["id"], replies[0].json["id"]);
        assert_eq!(r.json["assignees"].as_array().unwrap().len(), 2);
    }
    assert!(replies[0].json["completed_at"].is_string());
    // 감사 기록: 첫 업무(만들기 1·담당 1·담당 해제 1) + 이 업무(만들기 1·담당 2).
    assert_eq!(task_rows(&state, &admin, &item).await, (2, 2, 6));
    // 상태를 받은 업무는 그 열에 있다(할 일 열이 아니라).
    let lanes: Vec<(String, String)> = board(&app, &admin, &item)
        .await
        .into_iter()
        .map(|(title, status, _, _)| (title, status))
        .collect();
    assert_eq!(
        lanes,
        vec![
            ("주간 회의".to_owned(), "todo".to_owned()),
            ("보고서".to_owned(), "done".to_owned())
        ]
    );
}

/// D-105(P-31), CodeRabbit #98: 응답을 잃은 만들기를 다시 보낼 때 항목을 잠시 볼 수 없으면 404(존재를 드러내지 않음)이고 새
/// 업무를 만들지 않는다. 다시 볼 수 있게 되면 같은 요청 id가 처음 업무를 돌려준다. 그 요청 id의 업무를 지웠으면(휴지통)
/// 404와 구분되는 409 `task_deleted`라 웹은 그때만 새 요청 id로 다시 만든다.
#[tokio::test]
async fn a_replay_tells_a_deleted_task_from_a_lost_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "한 번에 만들기 404").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = module_item(&app, &admin, &ws, "CSV").await;
    make_public(&app, &admin, &item).await;
    let uri = format!("/api/items/{}/tasks", item["id"].as_str().unwrap());
    let request_id = new_request_id();
    let body = full_create(
        &request_id,
        "주간 회의",
        json!({ "assignee_ids": [writer.id.0.to_string()] }),
    );
    let first = send(&app, Method::POST, &uri, Some(&writer), Some(body.clone())).await;
    assert_eq!(first.status, StatusCode::CREATED, "{:?}", first.json);
    // 이 요청 id로 만든 업무 행 수. 휴지통에 든 업무는 앱 역할(RLS)에 보이지 않으므로 표 소유 역할로 센다.
    let made_by_request = || async {
        use sqlx::Connection;
        let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
        let mut owner = sqlx::PgConnection::connect(&url).await.unwrap();
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM fvoci.tasks WHERE item_id = $1::uuid AND request_id = $2::uuid",
        )
        .bind(item["id"].as_str().unwrap())
        .bind(&request_id)
        .fetch_one(&mut owner)
        .await
        .unwrap();
        n
    };

    // 항목이 비공개가 되어 작성자가 볼 수 없다: 404 not_found, 새 업무 없음.
    let item_uri = format!("/api/items/{}", item["id"].as_str().unwrap());
    let version = send(&app, Method::GET, &item_uri, Some(&admin), None)
        .await
        .json["version"]
        .clone();
    let r = send(
        &app,
        Method::PUT,
        &item_uri,
        Some(&admin),
        Some(json!({ "base_version": version, "title": item["title"], "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let hidden = send(&app, Method::POST, &uri, Some(&writer), Some(body.clone())).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{:?}", hidden.json);
    assert_eq!(hidden.json["error"], "not_found");
    assert_eq!(made_by_request().await, 1);

    // 다시 볼 수 있으면 같은 요청 id는 처음 업무다.
    make_public(&app, &admin, &item).await;
    let back = send(&app, Method::POST, &uri, Some(&writer), Some(body.clone())).await;
    assert_eq!(back.status, StatusCode::CREATED, "{:?}", back.json);
    assert_eq!(back.json["id"], first.json["id"]);
    assert_eq!(made_by_request().await, 1);

    // 그 업무를 지우면(휴지통) 같은 요청 id는 409 task_deleted: 볼 수 없음(404)과 다르다.
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/tasks/{}", first.json["id"].as_str().unwrap()),
        Some(&writer),
        None,
    )
    .await;
    assert!(trashed.status.is_success(), "{:?}", trashed.json);
    let gone = send(&app, Method::POST, &uri, Some(&writer), Some(body.clone())).await;
    assert_eq!(gone.status, StatusCode::CONFLICT, "{:?}", gone.json);
    assert_eq!(gone.json["error"], "task_deleted");
    assert_eq!(made_by_request().await, 1);
    // 새 요청 id로는 새 업무를 만든다.
    let fresh = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(full_create(&new_request_id(), "주간 회의", json!({}))),
    )
    .await;
    assert_eq!(fresh.status, StatusCode::CREATED, "{:?}", fresh.json);
    assert_ne!(fresh.json["id"], first.json["id"]);
}
