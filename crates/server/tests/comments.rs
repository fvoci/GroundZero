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

async fn notes(app: &Router, user: &TestUser) -> Vec<Value> {
    send(app, Method::GET, "/api/me/notifications", Some(user), None)
        .await
        .json
        .as_array()
        .unwrap()
        .clone()
}

#[tokio::test]
async fn comments_threads_and_notifications_follow_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "댓글").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
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

    // 독자도 댓글을 단다. 볼 수 없는 사람은 404.
    let (status, first) = comment(
        &app,
        &reader,
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
        comment(
            &app,
            &reader,
            json!({ "document_id": doc_id, "body": "  " })
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    // 문서를 만든 사람에게 댓글 알림(자기 댓글은 알리지 않음).
    let n = notes(&app, &writer).await;
    assert_eq!(n[0]["kind"], "comment");
    assert_eq!(n[0]["excerpt"], "안건 2는 다음 주로");
    assert_eq!(n[0]["actor_name"], "reader");
    assert_eq!(n[0]["document_id"], doc["id"]);
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
    assert_eq!(notes(&app, &reader).await[0]["kind"], "reply");

    // 목록: 첫 댓글 다음 답글. 권한 표시.
    let list = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/comments"),
        Some(&reader),
        None,
    )
    .await;
    let list = list.json.as_array().unwrap().clone();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], first_id.as_str());
    assert_eq!(list[1]["parent_id"], first_id.as_str());
    assert_eq!(list[0]["can_edit"], true);
    assert_eq!(list[1]["can_edit"], false);
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

    // 고치기는 쓴 사람만. 해결은 스레드를 연 사람이나 쓸 수 있는 사람.
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
            Some(&reader),
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
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
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
    let (member, _) = add_member(&app, &state, &admin, &old, "member", "writer").await;
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
    let ack = json!({ "gains": preview.json["gains"], "loses": preview.json["loses"] });
    let moved = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/move"),
        Some(&admin),
        Some(json!({ "target_workspace_id": new, "acknowledged": ack })),
    )
    .await;
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
    let (other, _) = add_member(&app, &state, &admin, &new, "other", "writer").await;
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
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
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
