#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 저장 요청의 기대 사용자(`x-fvoci-expected-user`, #60·이슈 #88). 편집을 시작한 사용자와 세션의 사용자가 다르면
//! 아무것도 바꾸지 않고 409 `account_changed`. 머리는 비교에만 쓰고 권한을 주지 않는다(사용자·권한은 세션 쿠키로만).

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, doc_body, login, new_request_id, send,
    send_raw,
};
use serde_json::{Value, json};

const HEADER: &str = "x-fvoci-expected-user";

/// HTTP 응답의 필터와 무관하게 업무 쓰기의 부수 효과까지 직접 비교한다.
async fn task_snapshot(workspace: &str) -> Value {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar(
        "SELECT jsonb_object_agg(kind, rows) FROM (
           SELECT 'tasks' AS kind, jsonb_agg(to_jsonb(t) ORDER BY t.id) AS rows
             FROM fvoci.tasks t WHERE workspace_id = $1::uuid
           UNION ALL SELECT 'checklist', jsonb_agg(to_jsonb(c) ORDER BY c.id)
             FROM fvoci.checklist_items c JOIN fvoci.tasks t ON t.id = c.task_id WHERE t.workspace_id = $1::uuid
           UNION ALL SELECT 'dependencies', jsonb_agg(to_jsonb(d) ORDER BY d.predecessor_id, d.successor_id)
             FROM fvoci.task_dependencies d JOIN fvoci.tasks t ON t.id = d.successor_id WHERE t.workspace_id = $1::uuid
           UNION ALL SELECT 'assignees', jsonb_agg(to_jsonb(a) ORDER BY a.task_id, a.user_id)
             FROM fvoci.task_assignees a JOIN fvoci.tasks t ON t.id = a.task_id WHERE t.workspace_id = $1::uuid
           UNION ALL SELECT 'series', jsonb_agg(to_jsonb(s) ORDER BY s.id)
             FROM fvoci.task_series s WHERE workspace_id = $1::uuid
           UNION ALL SELECT 'statuses', jsonb_agg(to_jsonb(s) ORDER BY s.id)
             FROM fvoci.task_statuses s JOIN fvoci.items i ON i.id = s.item_id WHERE i.workspace_id = $1::uuid
           UNION ALL SELECT 'audit', jsonb_agg(to_jsonb(a) ORDER BY a.id)
             FROM fvoci.audit_log a WHERE workspace_id = $1::uuid
           UNION ALL SELECT 'activity', jsonb_agg(to_jsonb(a) ORDER BY a.id)
             FROM fvoci.activity a WHERE workspace_id = $1::uuid
           UNION ALL SELECT 'trash', jsonb_agg(to_jsonb(t) ORDER BY t.id)
             FROM fvoci.trash_batches t WHERE workspace_id = $1::uuid
         ) snapshots",
    )
    .bind(workspace)
    .fetch_one(&mut conn)
    .await
    .unwrap()
}

#[tokio::test]
async fn every_task_write_requires_the_user_who_started_it() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let alice_id = alice.id.0.to_string();
    let bob_id = bob.id.0.to_string();

    for action in [
        "create",
        "update",
        "move",
        "parent",
        "add_checklist",
        "update_checklist",
        "delete_checklist",
        "add_predecessor",
        "remove_predecessor",
        "recurrence",
        "sync",
        "trash",
        "create_status",
        "update_status",
        "delete_status",
    ] {
        let ws = create_workspace(&app, &alice, action).await;
        let module = send(&app, Method::POST, &format!("/api/workspaces/{ws}/modules"), Some(&alice),
            Some(json!({"request_id": new_request_id(), "kind": "project", "name": action, "key": "TASK"}))).await;
        assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
        let item = send(
            &app,
            Method::POST,
            &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
            Some(&alice),
            Some(json!({"request_id": new_request_id(), "title": action})),
        )
        .await;
        assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
        let item_id = item.json["id"].as_str().unwrap();
        let statuses_uri = format!("/api/items/{item_id}/statuses");
        let statuses = send(&app, Method::GET, &statuses_uri, Some(&alice), None).await;
        assert_eq!(statuses.status, StatusCode::OK, "{:?}", statuses.json);
        let statuses = statuses.json.as_array().unwrap();
        let todo = statuses.iter().find(|s| s["category"] == "todo").unwrap()["id"]
            .as_str()
            .unwrap();
        let doing = statuses.iter().find(|s| s["category"] == "doing").unwrap()["id"]
            .as_str()
            .unwrap();
        let create_uri = format!("/api/items/{item_id}/tasks");
        let mut tasks = Vec::new();
        for title in ["task", "related"] {
            let task = send(&app, Method::POST, &create_uri, Some(&alice),
                Some(json!({"request_id": new_request_id(), "title": title, "due_date": "2026-01-01"}))).await;
            assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
            tasks.push(task.json);
        }
        let task = &tasks[0];
        let id = task["id"].as_str().unwrap();
        let related = tasks[1]["id"].as_str().unwrap();
        let uri = format!("/api/tasks/{id}");
        let checklist = send(
            &app,
            Method::POST,
            &format!("{uri}/checklist"),
            Some(&alice),
            Some(json!({"items": [{"request_id": new_request_id(), "text": "original"}]})),
        )
        .await;
        assert_eq!(checklist.status, StatusCode::OK, "{:?}", checklist.json);
        let checklist_id = checklist.json["checklist"][0]["id"].as_str().unwrap();
        let link_uri = format!("{uri}/predecessors/{related}");
        if action == "remove_predecessor" {
            let linked = send(
                &app,
                Method::PUT,
                &link_uri,
                Some(&alice),
                Some(json!({"kind": "FS", "lag_days": 0, "base": null})),
            )
            .await;
            assert_eq!(linked.status, StatusCode::OK, "{:?}", linked.json);
        }
        let rule = json!({"freq": "daily", "interval": 1, "weekdays": [], "count": 2});
        if action == "sync" {
            let repeated = send(
                &app,
                Method::PUT,
                &format!("{uri}/recurrence"),
                Some(&alice),
                Some(json!({"rule": rule, "base": null, "timezone": "Asia/Seoul"})),
            )
            .await;
            assert_eq!(repeated.status, StatusCode::OK, "{:?}", repeated.json);
            let mut conn = common::fixture_conn().await;
            sqlx::query("DELETE FROM fvoci.checklist_items WHERE task_id IN (SELECT id FROM fvoci.tasks WHERE workspace_id = $1::uuid AND id <> $2::uuid AND series_id IS NOT NULL)")
                .bind(&ws).bind(id).execute(&mut conn).await.unwrap();
            sqlx::query("DELETE FROM fvoci.tasks WHERE workspace_id = $1::uuid AND id <> $2::uuid AND series_id IS NOT NULL")
                .bind(&ws).bind(id).execute(&mut conn).await.unwrap();
        }
        let (method, path, body, success) = match action {
            "create" => (
                Method::POST,
                create_uri,
                json!({"request_id": new_request_id(), "title": "created"}),
                StatusCode::CREATED,
            ),
            "update" => (
                Method::PUT,
                uri.clone(),
                json!({"base_version": task["version"], "title": "changed", "description": "changed",
                "status": "할 일", "priority": "high", "start_date": null, "due_date": "2026-01-01", "estimate_minutes": null,
                "tags": [], "assignee_ids": [], "milestone": false}),
                StatusCode::OK,
            ),
            "move" => (
                Method::PUT,
                format!("{uri}/position"),
                json!({"status": "진행 중", "base_move_version": task["move_version"], "before_id": null}),
                StatusCode::OK,
            ),
            "parent" => (
                Method::PUT,
                format!("{uri}/parent"),
                json!({"parent_id": related}),
                StatusCode::OK,
            ),
            "add_checklist" => (
                Method::POST,
                format!("{uri}/checklist"),
                json!({"items": [{"request_id": new_request_id(), "text": "added"}]}),
                StatusCode::OK,
            ),
            "update_checklist" => (
                Method::PUT,
                format!("/api/checklist-items/{checklist_id}"),
                json!({"text": "changed", "checked": true}),
                StatusCode::NO_CONTENT,
            ),
            "delete_checklist" => (
                Method::DELETE,
                format!("/api/checklist-items/{checklist_id}"),
                Value::Null,
                StatusCode::NO_CONTENT,
            ),
            "add_predecessor" => (
                Method::PUT,
                link_uri.clone(),
                json!({"kind": "FS", "lag_days": 0, "base": null}),
                StatusCode::OK,
            ),
            "remove_predecessor" => (Method::DELETE, link_uri, Value::Null, StatusCode::OK),
            "recurrence" => (
                Method::PUT,
                format!("{uri}/recurrence"),
                json!({"rule": rule, "base": null, "timezone": "Asia/Seoul"}),
                StatusCode::OK,
            ),
            "sync" => (
                Method::POST,
                format!("/api/items/{item_id}/recurrences/sync"),
                Value::Null,
                StatusCode::OK,
            ),
            "trash" => (Method::DELETE, uri.clone(), Value::Null, StatusCode::OK),
            "create_status" => (
                Method::POST,
                statuses_uri.clone(),
                json!({"name": "review", "category": "doing"}),
                StatusCode::CREATED,
            ),
            "update_status" => (
                Method::PUT,
                format!("{statuses_uri}/{todo}"),
                json!({"name": "finished", "category": "done"}),
                StatusCode::OK,
            ),
            "delete_status" => (
                Method::DELETE,
                format!("{statuses_uri}/{todo}?move_to={doing}"),
                Value::Null,
                StatusCode::OK,
            ),
            _ => unreachable!("unknown test action: {action}"),
        };
        let before = task_snapshot(&ws).await;
        for (expected, status) in [
            (vec![], StatusCode::BAD_REQUEST),
            (vec![bob_id.as_str()], StatusCode::CONFLICT),
        ] {
            let (actual, json) =
                raw(&app, method.clone(), &path, Some(&alice), &expected, &body).await;
            assert_eq!(actual, status, "{action}: {json:?}");
            assert_eq!(
                json["error"],
                if expected.is_empty() {
                    "bad_request"
                } else {
                    "account_changed"
                },
                "{action}: {json:?}"
            );
            assert!(json.get("current").is_none(), "{action}: {json:?}");
            assert_eq!(
                task_snapshot(&ws).await,
                before,
                "{action} must not mutate the database on rejection"
            );
        }
        // 본문 역직렬화보다 먼저 검사하므로, 잘못된 본문도 앞 계정의 요청을 삼키지 않는다.
        if body != Value::Null {
            let (status, json) = raw(
                &app,
                method.clone(),
                &path,
                Some(&alice),
                &[&bob_id],
                &Value::Null,
            )
            .await;
            assert_eq!(status, StatusCode::CONFLICT, "{action}: {json:?}");
            assert_eq!(json["error"], "account_changed", "{action}: {json:?}");
            assert_eq!(task_snapshot(&ws).await, before, "{action}: invalid body");
        }
        // 일치하는 머리로 같은 본문을 보내면 성공하고 실제 상태가 바뀐다.
        let (status, json) = raw(&app, method, &path, Some(&alice), &[&alice_id], &body).await;
        assert_eq!(status, success, "{action}: {json:?}");
        assert_ne!(
            task_snapshot(&ws).await,
            before,
            "{action} must exercise a real write"
        );
    }
}

/// D-96 2026-10-28 개정 (가): 업무 라우터의 모든 명령은 머리가 있으면 늘 비교한다. 머리가 필수가 아닌 요청(재배치
/// 미리보기)으로 공통 처리만 본다. 머리가 없으면 그대로 처리하고, 있으면 형식 400·불일치 409다. 읽기(안전한 메서드)는
/// 비교하지 않는다.
#[tokio::test]
async fn a_sent_expected_user_is_always_compared_on_work_commands() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let alice_id = alice.id.0.to_string();
    let bob_id = bob.id.0.to_string();
    let ws = create_workspace(&app, &alice, "optional header").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&alice),
        Some(
            json!({"request_id": new_request_id(), "kind": "project", "name": "opt", "key": "OPT"}),
        ),
    )
    .await;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
        Some(&alice),
        Some(json!({"request_id": new_request_id(), "title": "opt"})),
    )
    .await;
    let item_id = item.json["id"].as_str().unwrap();
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&alice),
        Some(json!({"request_id": new_request_id(), "title": "plan", "due_date": "2026-01-01"})),
    )
    .await;
    let plan = format!(
        "/api/tasks/{}/schedule-plan",
        task.json["id"].as_str().unwrap()
    );
    let body = json!({"start_date": null, "due_date": "2026-01-05"});
    for (expected, status, error) in [
        (vec![], StatusCode::OK, None),
        (vec![alice_id.as_str()], StatusCode::OK, None),
        (
            vec![bob_id.as_str()],
            StatusCode::CONFLICT,
            Some("account_changed"),
        ),
        (
            vec!["not-a-user"],
            StatusCode::BAD_REQUEST,
            Some("bad_request"),
        ),
        (
            vec![alice_id.as_str(), alice_id.as_str()],
            StatusCode::BAD_REQUEST,
            Some("bad_request"),
        ),
    ] {
        let (actual, json) = raw(&app, Method::POST, &plan, Some(&alice), &expected, &body).await;
        assert_eq!(actual, status, "{expected:?}: {json:?}");
        if let Some(error) = error {
            assert_eq!(json["error"], error, "{expected:?}: {json:?}");
        }
    }
    // 머리가 있어도 세션이 없으면 401이다(머리는 신원이 아니다).
    let (actual, _) = raw(&app, Method::POST, &plan, None, &[&alice_id], &body).await;
    assert_eq!(actual, StatusCode::UNAUTHORIZED);
    // 읽기는 비교하지 않는다.
    let (actual, json) = raw(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/tasks"),
        Some(&alice),
        &[&bob_id],
        &Value::Null,
    )
    .await;
    assert_eq!(actual, StatusCode::OK, "{json:?}");
}

/// 머리를 그대로(자동으로 붙이지 않고) 정해 보낸다.
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
        headers.push((HEADER, value));
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

fn save_body(doc: &Value, text: &str) -> Value {
    json!({
        "request_id": new_request_id(),
        "base_revision": doc["revision"],
        "title": "바꾼 제목",
        "body": doc_body(text),
    })
}

/// 문서의 지금 리비전·제목과 리비전 기록 수.
async fn snapshot(app: &Router, user: &TestUser, id: &str) -> (i64, String, usize) {
    let doc = send(
        app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(doc.status, StatusCode::OK, "{:?}", doc.json);
    let revisions = send(
        app,
        Method::GET,
        &format!("/api/documents/{id}/revisions"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(revisions.status, StatusCode::OK, "{:?}", revisions.json);
    (
        doc.json["revision"].as_i64().unwrap(),
        doc.json["title"].as_str().unwrap().to_owned(),
        revisions.json.as_array().map_or(0, Vec::len),
    )
}

#[tokio::test]
async fn a_save_goes_through_only_for_the_user_who_started_the_edit() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "기대 사용자").await;
    let doc = create_doc(&app, &alice, &ws, "처음 제목").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/documents/{id}");
    let (other, _) = add_member(&app, &state, &alice, &ws, "bob", "member").await;
    let before = snapshot(&app, &alice, &id).await;

    // 다른 사용자(쓰기 권한이 있는 멤버라도)로 시작한 편집이 alice의 세션으로 나가면 바꾸지 않는다.
    let other_id = other.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[&other_id],
        &save_body(&doc, "다른 계정의 편집"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
    assert_eq!(json["error"], "account_changed");
    assert!(json.get("current").is_none(), "{json:?}");
    assert_eq!(snapshot(&app, &alice, &id).await, before);

    // 머리가 없거나 형식이 틀리거나 둘이면 400(머리 없는 옛 요청을 받는 길은 두지 않는다, D-92).
    for expected in [vec![], vec!["not-a-uuid"], vec![""]] {
        let (status, json) = raw(
            &app,
            Method::PUT,
            &uri,
            Some(&alice),
            &expected,
            &save_body(&doc, "머리 없음"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{expected:?} {json:?}");
        assert_eq!(json["error"], "bad_request");
    }
    let alice_id = alice.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[&alice_id, &alice_id],
        &save_body(&doc, "머리 둘"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json:?}");
    assert_eq!(snapshot(&app, &alice, &id).await, before);

    // 세션이 없으면 머리가 맞아도 401(머리는 신원이 아니다).
    let (status, _) = raw(
        &app,
        Method::PUT,
        &uri,
        None,
        &[&alice_id],
        &save_body(&doc, "세션 없음"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(snapshot(&app, &alice, &id).await, before);

    // 같은 사용자면 저장한다.
    let (status, json) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[&alice_id],
        &save_body(&doc, "내 편집"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["revision"], before.0 + 1);
    assert_eq!(json["title"], "바꾼 제목");
}

#[tokio::test]
async fn the_expected_user_grants_nothing() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "권한 없음").await;
    let doc = create_doc(&app, &alice, &ws, "alice의 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/documents/{id}");
    let (reader, _) = add_member(&app, &state, &alice, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let before = snapshot(&app, &alice, &id).await;
    let alice_id = alice.id.0.to_string();

    // 쓰기 권한이 있는 사람의 id를 머리에 넣어도 세션의 사용자로만 판단한다: 다르다고 거절할 뿐 쓰지 않는다.
    for who in [&reader, &outsider] {
        let (status, json) = raw(
            &app,
            Method::PUT,
            &uri,
            Some(who),
            &[&alice_id],
            &save_body(&doc, "남의 이름으로"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
        assert_eq!(json["error"], "account_changed");
    }
    // 자기 id를 넣으면 평소의 권한 응답 그대로다(읽기 멤버 403, 멤버가 아니면 404).
    let reader_id = reader.id.0.to_string();
    let (status, _) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&reader),
        &[&reader_id],
        &save_body(&doc, "읽기 멤버"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let outsider_id = outsider.id.0.to_string();
    let (status, _) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&outsider),
        &[&outsider_id],
        &save_body(&doc, "바깥 사람"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(snapshot(&app, &alice, &id).await, before);
}

/// 초안을 내 노트로 보존하는 길(노트를 만든 뒤 저장)도 같다. 계정이 바뀌었으면 새 계정에 빈 노트(앞 계정 초안의
/// 제목을 단)를 만들지 않는다.
#[tokio::test]
async fn a_note_is_created_only_for_the_user_who_started_the_edit() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let notes = |user: TestUser| {
        let app = app.clone();
        async move {
            let r = send(&app, Method::GET, "/api/me/notes", Some(&user), None).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json.as_array().map_or(0, Vec::len)
        }
    };
    let before = notes(alice.clone()).await;
    let create = || json!({ "request_id": new_request_id(), "title": "앞 계정 초안의 사본" });

    let bob_id = bob.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[&bob_id],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
    assert_eq!(json["error"], "account_changed");
    let (status, _) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(notes(alice.clone()).await, before);

    let alice_id = alice.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[&alice_id],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json:?}");
    assert_eq!(notes(alice).await, before + 1);
}

/// 업로드 시작도 같다(웹은 만료된 업로드를 스스로 다시 시작한다, CodeRabbit #60 지적). 다른 사용자면 새 계정의 업로드를
/// 만들지 않는다: 같은 요청 id로 맞는 사용자가 시작하면 처음 만드는 업로드다.
#[tokio::test]
async fn an_upload_starts_only_for_the_user_who_started_it() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "업로드 기대 사용자").await;
    let doc = create_doc(&app, &alice, &ws, "첨부 문서").await;
    let (bob, _) = add_member(&app, &state, &alice, &ws, "bob", "member").await;
    let body = json!({ "request_id": new_request_id(),
                       "target": { "kind": "document", "id": doc["id"] },
                       "name": "a.txt", "content_type": "text/plain", "size": 3 });

    // bob의 세션으로 alice의 화면이 다시 시작한 업로드: 거절. 이름을 달리해 두면 무엇이 만들어졌는지 보인다(같은 요청
    // id면 서버는 처음 만든 업로드를 돌려준다).
    let mut rejected = body.clone();
    rejected["name"] = json!("rejected.txt");
    let alice_id = alice.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&bob),
        &[&alice_id],
        &rejected,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
    assert_eq!(json["error"], "account_changed");
    let (status, _) = raw(&app, Method::POST, "/api/uploads", Some(&bob), &[], &body).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 같은 요청 id로 bob 자신이 시작하면 처음 만든다(앞의 거절이 아무것도 만들지 않았다).
    let bob_id = bob.id.0.to_string();
    let (status, first) = raw(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&bob),
        &[&bob_id],
        &body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first:?}");
    assert_eq!(first["name"], "a.txt", "{first:?}");
}
