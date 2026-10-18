#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 휴지통·복원·영구 삭제 (D-39, D-41, D-57).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, fixture_sql, login, login_with_email,
    new_request_id, send, unique_email,
};
use serde_json::{Value, json};

async fn item(app: &Router, admin: &TestUser, ws: &str, key: &str) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": key })),
    )
    .await;
    send(
        app,
        Method::PUT,
        &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "title": key, "description": "", "visibility": "workspace" })),
    )
    .await;
    i.json
}

async fn trash(app: &Router, user: &TestUser, ws: &str) -> Value {
    send(
        app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(user),
        None,
    )
    .await
    .json
}

#[tokio::test]
async fn deleted_documents_disappear_and_come_back_with_their_subtree() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "휴지통").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let parent = create_doc(&app, &writer, &ws, "상위").await;
    let child = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "하위", "parent_id": parent["id"] })),
    )
    .await
    .json;
    let uri = |d: &Value| format!("/api/documents/{}", d["id"].as_str().unwrap());

    assert_eq!(
        send(&app, Method::DELETE, &uri(&parent), Some(&reader), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri(&parent), Some(&other), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let r = send(&app, Method::DELETE, &uri(&parent), Some(&writer), None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let batch = r.json["batch_id"].as_str().unwrap().to_owned();
    for d in [&parent, &child] {
        assert_eq!(
            send(&app, Method::GET, &uri(d), Some(&admin), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    let resolved = send(
        &app,
        Method::POST,
        "/api/references/resolve",
        Some(&admin),
        Some(json!({ "refs": [{ "kind": "document", "id": child["id"] }] })),
    )
    .await
    .json;
    assert_eq!(resolved[0]["visible"], false);

    // 지운 사람과 관리자만 휴지통에서 본다. 다른 작성자는 되살릴 수 없다(없는 것과 같다).
    let mine = trash(&app, &writer, &ws).await;
    assert_eq!(mine["retention_days"], 30);
    assert_eq!(mine["entries"][0]["member_count"], 2);
    assert_eq!(mine["entries"][0]["can_purge"], false);
    assert_eq!(trash(&app, &other, &ws).await["entries"], json!([]));
    assert_eq!(
        trash(&app, &admin, &ws).await["entries"][0]["can_purge"],
        true
    );
    let restore = |user: TestUser| {
        let app = app.clone();
        let uri = format!("/api/trash/{batch}/restore");
        async move {
            send(&app, Method::POST, &uri, Some(&user), None)
                .await
                .status
        }
    };
    assert_eq!(restore(other.clone()).await, StatusCode::NOT_FOUND);
    assert_eq!(restore(writer.clone()).await, StatusCode::NO_CONTENT);
    let back = send(&app, Method::GET, &uri(&child), Some(&admin), None)
        .await
        .json;
    assert_eq!(back["parent_id"], parent["id"]);
    assert_eq!(
        back["reference"], child["reference"],
        "same number after restore"
    );
    assert_eq!(
        restore(writer.clone()).await,
        StatusCode::NOT_FOUND,
        "already restored"
    );

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid AND action IN ('delete', 'restore') ORDER BY created_at, id",
    )
    .bind(parent["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(actions, ["delete", "restore"]);
}

#[tokio::test]
async fn items_and_tasks_go_to_the_trash_together_and_expire() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "만료").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let it = item(&app, &admin, &ws, "TRS").await;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "상위 업무" })),
    )
    .await
    .json;
    let sub = send(&app, Method::POST, &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()), Some(&writer),
                   Some(json!({ "request_id": new_request_id(), "title": "하위 업무", "parent_id": task["id"] }))).await.json;
    let task_uri = |t: &Value| format!("/api/tasks/{}", t["id"].as_str().unwrap());

    // 업무는 하위 업무까지 함께 지워지고 함께 돌아온다.
    let r = send(&app, Method::DELETE, &task_uri(&task), Some(&writer), None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(
        send(&app, Method::GET, &task_uri(&sub), Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(list, json!([]));
    let batch = r.json["batch_id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/trash/{batch}/restore"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::GET, &task_uri(&sub), Some(&admin), None)
            .await
            .json["parent_id"],
        task["id"]
    );

    // 항목을 지우면 그 안의 업무도 보이지 않는다. 항목 소유자만 지울 수 있다.
    let item_uri = format!("/api/items/{}", it["id"].as_str().unwrap());
    assert_eq!(
        send(&app, Method::DELETE, &item_uri, Some(&writer), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let r = send(&app, Method::DELETE, &item_uri, Some(&admin), None).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(
        send(&app, Method::GET, &task_uri(&task), Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, &item_uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let item_batch = r.json["batch_id"].as_str().unwrap().to_owned();

    // 보존 기간은 관리자만 바꾼다. 기간이 지나면 되살릴 수 없고 영구 삭제만 된다.
    let retention = |user: TestUser, days: i64| {
        let app = app.clone();
        let uri = format!("/api/workspaces/{ws}/trash-retention");
        async move {
            send(
                &app,
                Method::PUT,
                &uri,
                Some(&user),
                Some(json!({ "days": days })),
            )
            .await
            .status
        }
    };
    assert_eq!(retention(writer.clone(), 7).await, StatusCode::FORBIDDEN);
    assert_eq!(retention(admin.clone(), 0).await, StatusCode::BAD_REQUEST);
    assert_eq!(retention(admin.clone(), 7).await, StatusCode::NO_CONTENT);
    fixture_sql(
        "UPDATE fvoci.trash_batches SET deleted_at = now() - interval '8 days' WHERE id = $1::uuid",
        &item_batch,
    )
    .await;
    let entry = trash(&app, &admin, &ws).await["entries"][0].clone();
    assert_eq!(entry["expired"], true);
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/trash/{item_batch}/restore"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    let r = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/trash/purge-expired"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.json["purged"], 1);
    assert_eq!(trash(&app, &admin, &ws).await["entries"], json!([]));

    // 영구 삭제해도 번호는 다시 쓰지 않는다.
    let next_item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", it["module_id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "다음" })),
    )
    .await
    .json;
    assert_eq!(
        next_item["reference"], "TRS-4",
        "TRS-1 item, TRS-2·3 tasks are never reused"
    );
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let purged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action = 'purge' AND target_id = $1::uuid",
    )
    .bind(it["id"].as_str().unwrap())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(purged, 1);
}

#[tokio::test]
async fn trashed_content_is_closed_everywhere_and_others_children_stay() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "닫기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "writer").await;

    // 공개 링크는 문서를 지우면 닫힌다.
    let public = create_doc(&app, &admin, &ws, "공개").await;
    let link = send(
        &app,
        Method::POST,
        &format!(
            "/api/documents/{}/public-links",
            public["id"].as_str().unwrap()
        ),
        Some(&admin),
        Some(json!({})),
    )
    .await
    .json;
    let token_path = format!(
        "/api/public/{}",
        link["url"].as_str().unwrap().rsplit('/').next().unwrap()
    );
    assert_eq!(
        send(&app, Method::GET, &token_path, None, None)
            .await
            .status,
        StatusCode::OK
    );
    send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", public["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        send(&app, Method::GET, &token_path, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // 항목이 휴지통에 있으면 그 안의 문서는 공유로도 볼 수 없다.
    let it = item(&app, &admin, &ws, "SHR").await;
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", it["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목 문서" })),
    )
    .await
    .json;
    let bob_email = unique_email("bob");
    let bob = login_with_email(&state, "bob", &bob_email, true).await;
    let r = send(
        &app,
        Method::POST,
        &format!("/api/documents/{}/shares", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "email": bob_email, "permission": "write" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    assert_eq!(
        send(&app, Method::GET, &doc_uri, Some(&bob), None)
            .await
            .status,
        StatusCode::OK
    );
    let item_batch = send(
        &app,
        Method::DELETE,
        &format!("/api/items/{}", it["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json["batch_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        send(&app, Method::GET, &doc_uri, Some(&bob), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, "/api/me/shared", Some(&bob), None)
            .await
            .json,
        json!([])
    );

    // 지우는 사람이 지울 수 없는 남의 하위 문서는 함께 지워지지 않고 부모 자리로 옮겨 남는다.
    let parent = create_doc(&app, &writer, &ws, "상위").await;
    let child = send(&app, Method::POST, &format!("/api/workspaces/{ws}/wiki/documents"), Some(&other),
                     Some(json!({ "request_id": new_request_id(), "title": "남의 하위", "parent_id": parent["id"] }))).await.json;
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", parent["id"].as_str().unwrap()),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let mine = trash(&app, &writer, &ws).await;
    let entry = mine["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["title"] == "상위")
        .unwrap()
        .clone();
    assert_eq!(entry["member_count"], 1);
    let kept = send(
        &app,
        Method::GET,
        &format!("/api/documents/{}", child["id"].as_str().unwrap()),
        Some(&other),
        None,
    )
    .await;
    assert_eq!(kept.status, StatusCode::OK);
    assert_eq!(kept.json["parent_id"], Value::Null);

    // 항목이 휴지통에 있는 동안 그 안의 업무는 되살릴 수 없다(항목 먼저).
    let it2 = item(&app, &admin, &ws, "RST").await;
    let t = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it2["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "업무" })),
    )
    .await
    .json;
    let task_batch = send(
        &app,
        Method::DELETE,
        &format!("/api/tasks/{}", t["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json["batch_id"]
        .as_str()
        .unwrap()
        .to_owned();
    send(
        &app,
        Method::DELETE,
        &format!("/api/items/{}", it2["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/trash/{task_batch}/restore"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    // 다른 항목의 묶음을 되살리면 그 문서는 다시 공유로 보인다.
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/trash/{item_batch}/restore"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::GET, &doc_uri, Some(&bob), None)
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn purged_repeating_occurrences_are_not_recreated() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반복 삭제").await;
    let it = item(&app, &admin, &ws, "RPT").await;
    let t = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "점검" })),
    )
    .await
    .json;
    let yesterday: String = {
        let mut tx = state.db.begin_as(admin.id).await.unwrap();
        sqlx::query_scalar("SELECT ((now() AT TIME ZONE 'Asia/Seoul')::date - 1)::text")
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    send(&app, Method::PUT, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(&admin),
         Some(json!({ "base_version": t["version"], "title": "점검", "description": "", "status": "todo", "priority": "none",
                      "start_date": null, "due_date": yesterday, "estimate_minutes": null, "tags": [], "assignee_ids": [],
                      "milestone": false }))).await;
    send(&app, Method::PUT, &format!("/api/tasks/{}/recurrence", t["id"].as_str().unwrap()), Some(&admin),
         Some(json!({ "rule": {"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null}, "timezone": "Asia/Seoul" }))).await;
    let tasks = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json;
    let second = tasks
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["id"] != t["id"])
        .unwrap()
        .clone();
    let batch = send(
        &app,
        Method::DELETE,
        &format!("/api/tasks/{}", second["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json["batch_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/trash/{batch}"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let sync = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/recurrences/sync", it["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(sync.json["created"], 0, "a purged occurrence stays gone");
}

/// 모듈 휴지통(D-78): 관리자만 지우고 되살린다. 모듈과 함께 그 항목·업무가 사라지고, 되살리면 모듈만 돌아온다.
/// 그 전에 따로 지운 항목은 휴지통에 남고(모듈이 지워진 동안은 되살릴 수 없음), 비공개 항목은 비공개 그대로다.
/// 영구 삭제는 모듈의 항목을 모두 지우고 따로 지운 항목의 묶음도 끝낸다. 설명은 관리자만 고친다.
#[tokio::test]
async fn module_trash_hides_its_items_and_restores_only_the_module() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "모듈 휴지통").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let post = |user: &TestUser, uri: String, body: Value| {
        let app = app.clone();
        let user = user.clone();
        async move { send(&app, Method::POST, &uri, Some(&user), Some(body)).await }
    };
    let module = post(
        &admin,
        format!("/api/workspaces/{ws}/modules"),
        json!({ "request_id": new_request_id(), "kind": "project", "name": "개발", "key": "DEV" }),
    )
    .await
    .json;
    let mid = module["id"].as_str().unwrap().to_owned();
    let mut items = Vec::new();
    for (title, visibility) in [
        ("공개", "workspace"),
        ("비공개", "private"),
        ("따로 지움", "workspace"),
    ] {
        let i = post(
            &admin,
            format!("/api/modules/{mid}/items"),
            json!({ "request_id": new_request_id(), "title": title }),
        )
        .await
        .json;
        let id = i["id"].as_str().unwrap().to_owned();
        let r = send(
            &app,
            Method::PUT,
            &format!("/api/items/{id}"),
            Some(&admin),
            Some(json!({ "title": title, "description": "", "visibility": visibility })),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
        items.push(id);
    }
    let (open, private, separate) = (items[0].clone(), items[1].clone(), items[2].clone());
    let task = post(
        &writer,
        format!("/api/items/{open}/tasks"),
        json!({ "request_id": new_request_id(), "title": "할 일" }),
    )
    .await
    .json;
    let task_id = task["id"].as_str().unwrap().to_owned();
    let get = |user: &TestUser, uri: String| {
        let app = app.clone();
        let user = user.clone();
        async move {
            send(&app, Method::GET, &uri, Some(&user), None)
                .await
                .status
        }
    };
    assert_eq!(
        get(&writer, format!("/api/items/{private}")).await,
        StatusCode::NOT_FOUND
    );

    // 설명: 관리자만, 길이 제한.
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{mid}/description"),
        Some(&admin),
        Some(json!({ "description": " 제품 개발 모듈 " })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["description"], "제품 개발 모듈");
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{mid}/description"),
        Some(&writer),
        Some(json!({ "description": "작성자" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{mid}/description"),
        Some(&admin),
        Some(json!({ "description": "가".repeat(2001) })),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 항목 하나를 따로 지운 뒤 모듈을 지운다. 작성자는 모듈을 지울 수 없다.
    let separate_batch = send(
        &app,
        Method::DELETE,
        &format!("/api/items/{separate}"),
        Some(&admin),
        None,
    )
    .await
    .json["batch_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/modules/{mid}"),
            Some(&writer),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/modules/{mid}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let module_batch = r.json["batch_id"].as_str().unwrap().to_owned();
    for user in [&admin, &writer] {
        assert_eq!(
            get(user, format!("/api/modules/{mid}")).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(user, format!("/api/items/{open}")).await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(user, format!("/api/tasks/{task_id}")).await,
            StatusCode::NOT_FOUND
        );
    }
    let modules = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        None,
    )
    .await;
    assert!(modules.json.as_array().unwrap().is_empty());
    let listed = trash(&app, &admin, &ws).await;
    let entry = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == module_batch.as_str())
        .unwrap()
        .clone();
    assert_eq!(
        (entry["kind"].as_str(), entry["member_count"].as_i64()),
        (Some("module"), Some(2))
    );
    // 작성자는 남이 지운 모듈을 휴지통에서 보지 못한다.
    assert!(
        trash(&app, &writer, &ws).await["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // 모듈이 지워진 동안 그 안의 항목은 되살릴 수 없다.
    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash/{separate_batch}/restore"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);

    // 되살리면 모듈과 그때 들어간 항목만 돌아온다. 따로 지운 항목은 휴지통에, 비공개 항목은 비공개로 남는다.
    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash/{module_batch}/restore"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash/{module_batch}/restore"),
        Some(&admin),
        None,
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
    assert_eq!(
        get(&writer, format!("/api/modules/{mid}")).await,
        StatusCode::OK
    );
    assert_eq!(
        get(&writer, format!("/api/items/{open}")).await,
        StatusCode::OK
    );
    assert_eq!(
        get(&writer, format!("/api/tasks/{task_id}")).await,
        StatusCode::OK
    );
    assert_eq!(
        get(&writer, format!("/api/items/{private}")).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&admin, format!("/api/items/{separate}")).await,
        StatusCode::NOT_FOUND
    );
    assert!(
        trash(&app, &admin, &ws).await["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == separate_batch.as_str())
    );

    // 다시 지우고 영구 삭제하면 항목이 모두 사라지고 따로 지운 항목의 묶음도 끝난다.
    let again = send(
        &app,
        Method::DELETE,
        &format!("/api/modules/{mid}"),
        Some(&admin),
        None,
    )
    .await
    .json["batch_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/trash/{again}"),
        Some(&admin),
        None,
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
    assert!(
        trash(&app, &admin, &ws).await["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        get(&admin, format!("/api/modules/{mid}")).await,
        StatusCode::NOT_FOUND
    );
}
