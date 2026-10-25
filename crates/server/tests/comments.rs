#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 댓글과 알림 (D-27, D-20, D-39): 볼 수 있으면 댓글, 한 단계 답글, 고치기·지우기·해결, 댓글·답글·기한 알림,
//! 권한이 철회되면 알림도 사라진다.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use std::io::Read;

use common::{
    TestUser, add_member, create_doc, create_workspace, login, new_request_id, send, send_raw,
};
use serde_json::{Value, json};

async fn comment(app: &Router, user: &TestUser, body: Value) -> (StatusCode, Value) {
    let mut body = body;
    body["request_id"] = json!(new_request_id());
    let r = send(app, Method::POST, "/api/comments", Some(user), Some(body)).await;
    (r.status, r.json)
}

/// 내 알림. 초대 알림(`invite`, D-118)은 add_member의 가입 흐름이 남긴 것이라 댓글·기한 알림 시험의 대상이 아니므로 뺀다.
async fn notes(app: &Router, user: &TestUser) -> Vec<Value> {
    send(app, Method::GET, "/api/me/notifications", Some(user), None)
        .await
        .json
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] != "invite")
        .cloned()
        .collect()
}

#[tokio::test]
async fn comments_threads_and_notifications_follow_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "댓글").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &writer, &ws, "회의록").await;
    let doc_id = doc["id"].as_str().unwrap();
    let open = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc_id}/settings"),
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": false })),
    )
    .await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.json);

    // 읽기 전용은 댓글을 포함해 쓰지 않는다(D-20·D-112: 42501 → 403). 볼 수 없는 사람은 404. 관리자가 첫 댓글을 단다.
    assert_eq!(
        comment(
            &app,
            &reader,
            json!({ "document_id": doc_id, "body": "안건 2는 다음 주로" })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (status, first) = comment(
        &app,
        &admin,
        json!({ "document_id": doc_id, "body": "안건 2는 다음 주로" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first:?}");
    let first_id = first["id"].as_str().unwrap().to_owned();
    assert_eq!(
        comment(
            &app,
            &outsider,
            json!({ "document_id": doc_id, "body": "?" })
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        comment(&app, &admin, json!({ "document_id": doc_id, "body": "  " }))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    // 문서를 만든 사람에게 댓글 알림(자기 댓글은 알리지 않음).
    let n = notes(&app, &writer).await;
    assert_eq!(n[0]["kind"], "comment");
    assert_eq!(n[0]["excerpt"], "안건 2는 다음 주로");
    assert_eq!(n[0]["actor_name"], "admin");
    assert_eq!(n[0]["document_id"], doc["id"]);
    assert!(notes(&app, &admin).await.is_empty());
    assert!(notes(&app, &reader).await.is_empty());

    // 답글은 한 단계. 스레드 참여자에게 답글 알림.
    let (status, reply) = comment(
        &app,
        &writer,
        json!({ "document_id": doc_id, "parent_id": first_id, "body": "좋습니다" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply:?}");
    assert_eq!(
        comment(
            &app,
            &writer,
            json!({ "document_id": doc_id, "parent_id": reply["id"], "body": "더 깊게" })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(notes(&app, &admin).await[0]["kind"], "reply");
    assert!(notes(&app, &reader).await.is_empty());

    // 목록: 첫 댓글 다음 답글. 권한 표시는 쓴 사람 기준이고 읽기 전용도 목록은 본다.
    let list = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/comments"),
        Some(&admin),
        None,
    )
    .await;
    let list = list.json.as_array().unwrap().clone();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], first_id.as_str());
    assert_eq!(list[1]["parent_id"], first_id.as_str());
    assert_eq!(list[0]["can_edit"], true);
    assert_eq!(list[1]["can_edit"], false);
    let seen_by_reader = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/comments"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(
        seen_by_reader.status,
        StatusCode::OK,
        "{:?}",
        seen_by_reader.json
    );
    assert_eq!(seen_by_reader.json.as_array().unwrap().len(), 2);
    assert_eq!(seen_by_reader.json[0]["can_edit"], false);
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/documents/{doc_id}/comments"),
            Some(&outsider),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // 고치기는 쓴 사람만. 해결은 쓸 수 있는 사람(읽기 전용은 스레드를 열 수도 해결할 수도 없다, D-20).
    let uri = format!("/api/comments/{first_id}");
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &uri,
            Some(&writer),
            Some(json!({ "body": "바꿈" }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &uri,
            Some(&admin),
            Some(json!({ "body": "안건 2는 다다음 주로" }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{uri}/resolve"),
            Some(&reader),
            Some(json!({ "resolved": true }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{uri}/resolve"),
            Some(&writer),
            Some(json!({ "resolved": true }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/comments"),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(list[0]["edited"], true);
    assert_eq!(list[0]["resolved"], true);
    // 지우기는 쓴 사람이나 소유 권한(관리자). 답글이 있으면 "지운 댓글"로 남는다.
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&writer), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/comments"),
        Some(&reader),
        None,
    )
    .await
    .json;
    assert_eq!(list[0]["deleted"], true);
    assert_eq!(list[0]["body"], "(지운 댓글)");
    assert_eq!(list.as_array().unwrap().len(), 2);

    // 업무 댓글은 담당자에게도 알린다. 기한 알림은 하루 전·당일 한 번씩.
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "CMT" }),
        ),
    )
    .await;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "보고서" })),
    )
    .await
    .json;
    let today = time::OffsetDateTime::now_utc().date();
    let tomorrow = today.next_day().unwrap();
    let updated = send(&app, Method::PUT, &format!("/api/tasks/{}", task["id"].as_str().unwrap()), Some(&admin), Some(json!({
        "base_version": task["version"], "title": "보고서", "description": "", "status": "todo", "priority": task["priority"],
        "start_date": null, "due_date": tomorrow.to_string(), "estimate_minutes": null, "tags": [], "milestone": false,
        "assignee_ids": [writer.id.0] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let (status, _) = comment(
        &app,
        &reader,
        json!({ "task_id": task["id"], "body": "자료 첨부했어요" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let n = notes(&app, &writer).await;
    assert!(
        n.iter()
            .any(|x| x["kind"] == "comment" && x["task_id"] == task["id"]),
        "{n:?}"
    );
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    let due: Vec<Value> = notes(&app, &writer)
        .await
        .into_iter()
        .filter(|x| x["kind"] == "due")
        .collect();
    assert_eq!(due.len(), 1, "{due:?}");
    assert_eq!(due[0]["due_date"], tomorrow.to_string());
    assert!(due[0]["actor_name"].is_null());

    // 보관된 항목의 업무에는 새 댓글을 달 수 없다.
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/archived", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(
        comment(
            &app,
            &admin,
            json!({ "task_id": task["id"], "body": "늦게" })
        )
        .await
        .0,
        StatusCode::CONFLICT
    );

    // 멤버에서 빠지면 그 워크스페이스의 알림이 보이지 않는다.
    send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", reader.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(notes(&app, &reader).await.is_empty());
}

/// 문서를 옮긴 뒤 옛 워크스페이스 영구 삭제·새 워크스페이스 반출, 알림 첫 부분은 지금의 댓글에서,
/// 만든 사람이 담당자여도 알림은 한 번.
#[tokio::test]
async fn moved_documents_live_excerpts_and_single_notifications() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let old = create_workspace(&app, &admin, "옛 공간").await;
    let new = create_workspace(&app, &admin, "새 공간").await;
    let (member, _) = add_member(&app, &state, &admin, &old, "member", "member").await;
    let doc = create_doc(&app, &admin, &old, "옮길 문서").await;
    let doc_id = doc["id"].as_str().unwrap().to_owned();
    let open = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc_id}/settings"),
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": false })),
    )
    .await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.json);

    // 알림의 첫 부분은 고친 글을 따르고, 지운 댓글이면 없다.
    let (status, root) = comment(
        &app,
        &member,
        json!({ "document_id": doc_id, "body": "처음 글" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{root:?}");
    let root_uri = format!("/api/comments/{}", root["id"].as_str().unwrap());
    let edited = send(
        &app,
        Method::PUT,
        &root_uri,
        Some(&member),
        Some(json!({ "body": "고친 글" })),
    )
    .await;
    assert!(edited.status.is_success(), "{:?}", edited.json);
    assert_eq!(notes(&app, &admin).await[0]["excerpt"], "고친 글");
    let (status, _) = comment(
        &app,
        &admin,
        json!({ "document_id": doc_id, "parent_id": root["id"], "body": "답" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let removed = send(&app, Method::DELETE, &root_uri, Some(&member), None).await;
    assert!(removed.status.is_success(), "{:?}", removed.json);
    let n = notes(&app, &admin).await;
    assert_eq!(n[0]["kind"], "comment");
    assert!(n[0]["excerpt"].is_null(), "{n:?}");

    // 댓글이 달린 문서를 새 공간으로 옮기면 옛 공간을 영구 삭제할 수 있고, 반출은 새 공간에서 댓글을 담는다.
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/move"),
        Some(&admin),
        Some(json!({ "target_workspace_id": new })),
    )
    .await;
    let moved = if preview.status.is_success() {
        preview
    } else {
        // D-114: 확인은 사용자 id와 이동 전후 등급을 담은 변화 전체에 대해 한다.
        let ack = json!({ "changes": preview.json["changes"] });
        send(
            &app,
            Method::POST,
            &format!("/api/documents/{doc_id}/move"),
            Some(&admin),
            Some(json!({ "target_workspace_id": new, "acknowledged": ack })),
        )
        .await
    };
    assert!(moved.status.is_success(), "{:?}", moved.json);
    let gone = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{old}"),
        Some(&admin),
        Some(json!({ "name": "옛 공간" })),
    )
    .await;
    assert_eq!(gone.status, StatusCode::OK, "{:?}", gone.json);
    let purge = send(
        &app,
        Method::DELETE,
        &format!("/api/me/deleted-workspaces/{old}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(purge.status, StatusCode::NO_CONTENT, "{:?}", purge.json);
    let list = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/comments"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(list.json.as_array().unwrap().len(), 2, "{:?}", list.json);
    let (status, _, bytes) = send_raw(
        &app,
        Method::GET,
        &format!("/api/workspaces/{new}/export"),
        Some(&admin),
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut json_text = String::new();
    zip.by_name("export.json")
        .unwrap()
        .read_to_string(&mut json_text)
        .unwrap();
    let exported: Value = serde_json::from_str(&json_text).unwrap();
    assert_eq!(
        exported["comments"].as_array().unwrap().len(),
        2,
        "{}",
        exported["comments"]
    );

    // 업무를 만든 사람이 담당자이기도 하면 댓글 알림은 한 번.
    let (other, _) = add_member(&app, &state, &admin, &new, "other", "member").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{new}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "DUP" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "내 업무" })),
    )
    .await
    .json;
    let updated = send(&app, Method::PUT, &format!("/api/tasks/{}", task["id"].as_str().unwrap()), Some(&admin), Some(json!({
        "base_version": task["version"], "title": "내 업무", "description": "", "status": "todo", "priority": task["priority"],
        "start_date": null, "due_date": null, "estimate_minutes": null, "tags": [], "milestone": false,
        "assignee_ids": [admin.id.0] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let (status, _) = comment(
        &app,
        &other,
        json!({ "task_id": task["id"], "body": "확인 부탁" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let on_task = notes(&app, &admin)
        .await
        .into_iter()
        .filter(|x| x["task_id"] == task["id"])
        .count();
    assert_eq!(on_task, 1);
}

/// 항목 하나와 업무 하나(워크스페이스 공개). 업무 응답을 돌려준다.
async fn item_task(app: &Router, admin: &TestUser, ws: &str, key: &str) -> (Value, Value, Value) {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await
    .json;
    let item = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    let open = send(
        app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.json);
    let task = send(
        app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "보고서" })),
    )
    .await
    .json;
    (m, item, task)
}

/// 감사 W11: 보관된 항목에는 새 댓글을 달 수 없는데 쓴 사람의 고치기와 스레드 해결은 통과했다. 둘 다 409이고,
/// 목록의 고치기·해결 표시도 꺼진다.
#[tokio::test]
async fn comments_in_an_archived_item_are_not_edited_or_resolved() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관 댓글").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (_, item, task) = item_task(&app, &admin, &ws, "ARC").await;
    let (status, created) = comment(
        &app,
        &writer,
        json!({ "task_id": task["id"], "body": "초안 올렸어요" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created:?}");
    let uri = format!("/api/comments/{}", created["id"].as_str().unwrap());
    let list_uri = format!("/api/tasks/{}/comments", task["id"].as_str().unwrap());
    let list = send(&app, Method::GET, &list_uri, Some(&writer), None)
        .await
        .json;
    assert_eq!(list[0]["can_edit"], true, "{list:?}");
    assert_eq!(list[0]["can_resolve"], true, "{list:?}");

    let archived = send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/archived", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(archived.status, StatusCode::OK, "{:?}", archived.json);
    let edit = send(
        &app,
        Method::PUT,
        &uri,
        Some(&writer),
        Some(json!({ "body": "보관 뒤에 고침" })),
    )
    .await;
    assert_eq!(edit.status, StatusCode::CONFLICT, "{:?}", edit.json);
    let resolve = send(
        &app,
        Method::POST,
        &format!("{uri}/resolve"),
        Some(&writer),
        Some(json!({ "resolved": true })),
    )
    .await;
    assert_eq!(resolve.status, StatusCode::CONFLICT, "{:?}", resolve.json);
    let list = send(&app, Method::GET, &list_uri, Some(&writer), None)
        .await
        .json;
    assert_eq!(list[0]["body"], "초안 올렸어요");
    assert_eq!(list[0]["resolved"], false);
    assert_eq!(list[0]["can_edit"], false, "{list:?}");
    assert_eq!(list[0]["can_resolve"], false, "{list:?}");
}

/// 감사 S19: 모듈을 보관하면 그 아래 업무는 읽기 전용인데 마감 알림은 계속 나갔다(항목 알림·홈은 보관된 모듈을 뺀다).
/// 보관된 모듈의 업무는 마감일이 다가와도 알리지 않는다.
#[tokio::test]
async fn due_notices_skip_tasks_in_an_archived_module() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관 모듈 기한").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (m, _, task) = item_task(&app, &admin, &ws, "DUE").await;
    let task_id = task["id"].as_str().unwrap().to_owned();
    // 마감은 아직 멀다(다른 시험의 작업 실행이 먼저 알리지 않게).
    let later = time::OffsetDateTime::now_utc().date() + time::Duration::days(20);
    let updated = send(&app, Method::PUT, &format!("/api/tasks/{task_id}"), Some(&admin), Some(json!({
        "base_version": task["version"], "title": "보고서", "description": "", "status": "todo",
        "priority": task["priority"], "start_date": null, "due_date": later.to_string(), "estimate_minutes": null,
        "tags": [], "milestone": false, "assignee_ids": [writer.id.0] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let archived = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{}/archived", m["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert!(archived.status.is_success(), "{:?}", archived.json);
    // 날이 지나 마감이 내일이 된다.
    common::fixture_sql(
        "UPDATE fvoci.tasks SET due_date = current_date + 1 WHERE id = $1::uuid",
        &task_id,
    )
    .await;
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    let due: Vec<Value> = notes(&app, &writer)
        .await
        .into_iter()
        .filter(|x| x["kind"] == "due" && x["task_id"] == task["id"])
        .collect();
    assert!(due.is_empty(), "{due:?}");
}

/// 감사 C24: 업무 댓글 목록(`GET /api/tasks/{id}/comments`)을 읽는 시험이 없었다. 업무를 볼 수 있는 사람만 읽고(비공개
/// 항목의 멤버가 아니면 404), 스레드 순서와 권한 표시는 문서 댓글과 같다.
#[tokio::test]
async fn task_comments_follow_the_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "업무 댓글").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "TCM" }),
        ),
    )
    .await
    .json;
    // 새 항목은 비공개다(D-20).
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "비공개 항목" })),
    )
    .await
    .json;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "검토" })),
    )
    .await
    .json;
    let (status, first) = comment(
        &app,
        &admin,
        json!({ "task_id": task["id"], "body": "첫 의견" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first:?}");
    let (status, reply) = comment(
        &app,
        &admin,
        json!({ "task_id": task["id"], "parent_id": first["id"], "body": "답글" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{reply:?}");
    let uri = format!("/api/tasks/{}/comments", task["id"].as_str().unwrap());

    let hidden = send(&app, Method::GET, &uri, Some(&writer), None).await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{:?}", hidden.json);
    let list = send(&app, Method::GET, &uri, Some(&admin), None).await;
    assert_eq!(list.status, StatusCode::OK, "{:?}", list.json);
    let bodies: Vec<&str> = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["body"].as_str().unwrap())
        .collect();
    assert_eq!(bodies, ["첫 의견", "답글"]);
    assert_eq!(list.json[0]["can_edit"], true);
    assert_eq!(list.json[0]["can_resolve"], true);
    assert_eq!(list.json[1]["can_resolve"], false);

    // 항목을 공개하면 작성자도 읽는다(남의 댓글은 고치지 못한다).
    let open = send(
        &app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "base_version": 1, "title": "비공개 항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.json);
    let seen = send(&app, Method::GET, &uri, Some(&writer), None).await;
    assert_eq!(seen.status, StatusCode::OK, "{:?}", seen.json);
    assert_eq!(seen.json.as_array().unwrap().len(), 2);
    assert_eq!(seen.json[0]["can_edit"], false);
}

/// D-106(감사 P-28, 현행 유지): 보관된 항목·모듈에서는 쓴 사람만 자기 댓글을 지운다(개인 내용 회수). 지금 경로를 그대로
/// 고정하는 시험이다(수정 전에도 통과한다): 보관되면 대상 권한이 읽기로 낮아져(`app_task_access`·`app_document_access`)
/// 소유 권한자도 남의 댓글은 `app_delete_comment`의 쓴 사람·소유자 검사에서 403이고 목록의 지우기 표시도 꺼진다. 남의
/// 댓글을 지울 권한이 없는 사람도 403, 대상을 볼 수 없는 사람은 404다. 보관을 풀면 소유 권한자가 다시 지운다.
/// 고치기·해결·새 댓글의 409는 `comments_in_an_archived_item_are_not_edited_or_resolved`와 첫 시험이 본다.
#[tokio::test]
async fn archived_items_let_authors_delete_only_their_own_comments() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관 댓글 지우기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;
    let outsider = login(&state, "outsider").await;
    let (module, item, task) = item_task(&app, &admin, &ws, "ARD").await;
    let mut ids = Vec::new();
    for (user, body) in [
        (&writer, "내 메모"),
        (&writer, "둘째 메모"),
        (&other, "남의 메모"),
        (&writer, "모듈 보관 뒤"),
    ] {
        let (status, created) =
            comment(&app, user, json!({ "task_id": task["id"], "body": body })).await;
        assert_eq!(status, StatusCode::CREATED, "{created:?}");
        ids.push(format!("/api/comments/{}", created["id"].as_str().unwrap()));
    }
    let archive = |archived: bool| {
        let (app, admin) = (app.clone(), admin.clone());
        let uri = format!("/api/items/{}/archived", item["id"].as_str().unwrap());
        async move {
            let r = send(
                &app,
                Method::PUT,
                &uri,
                Some(&admin),
                Some(json!({ "archived": archived })),
            )
            .await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
        }
    };
    archive(true).await;

    let by_owner = send(&app, Method::DELETE, &ids[0], Some(&admin), None).await;
    assert_eq!(
        by_owner.status,
        StatusCode::FORBIDDEN,
        "{:?}",
        by_owner.json
    );
    let list_uri = format!("/api/tasks/{}/comments", task["id"].as_str().unwrap());
    let list = send(&app, Method::GET, &list_uri, Some(&admin), None)
        .await
        .json;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .all(|c| c["can_delete"] == false),
        "{list:?}"
    );
    let mine = send(&app, Method::GET, &list_uri, Some(&writer), None)
        .await
        .json;
    let flags: Vec<(String, bool)> = mine
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["body"].as_str().unwrap().to_owned(),
                c["can_delete"] == true,
            )
        })
        .collect();
    assert!(flags.contains(&("내 메모".to_owned(), true)), "{flags:?}");
    assert!(
        flags.contains(&("남의 메모".to_owned(), false)),
        "{flags:?}"
    );
    assert_eq!(
        send(&app, Method::DELETE, &ids[2], Some(&writer), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &ids[2], Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let own = send(&app, Method::DELETE, &ids[0], Some(&writer), None).await;
    assert_eq!(own.status, StatusCode::NO_CONTENT, "{:?}", own.json);
    let left: Vec<Value> = send(&app, Method::GET, &list_uri, Some(&admin), None)
        .await
        .json
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(left.len(), 3, "{left:?}");
    assert!(left.iter().all(|c| c["body"] != "내 메모"), "{left:?}");

    archive(false).await;
    let unarchived = send(&app, Method::DELETE, &ids[1], Some(&admin), None).await;
    assert_eq!(
        unarchived.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        unarchived.json
    );

    // 모듈을 보관해도 같다: 쓴 사람은 자기 댓글을 지우고, 소유 권한자는 남의 댓글을 지우지 못한다.
    let module_archived = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{}/archived", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(
        module_archived.status,
        StatusCode::OK,
        "{:?}",
        module_archived.json
    );
    assert_eq!(
        send(&app, Method::DELETE, &ids[2], Some(&admin), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let own_in_module = send(&app, Method::DELETE, &ids[3], Some(&writer), None).await;
    assert_eq!(
        own_in_module.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        own_in_module.json
    );
}
