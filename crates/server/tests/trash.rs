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
        Some(json!({ "base_version": 1, "title": key, "description": "", "visibility": "workspace" })),
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;

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
    // D-114: 공간 밖 공유 대상도 같은 조직에 준비한다.
    let organization = common::organization_of_workspace(&ws).await;
    common::join_organization(organization, &bob, "member").await;
    let r = send(
        &app,
        Method::POST,
        &format!("/api/documents/{}/shares", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "email": bob_email, "permission": "write", "organization_id": organization })),
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
    let updated = send(&app, Method::PUT, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(&admin),
         Some(json!({ "base_version": t["version"], "title": "점검", "description": "", "status": "할 일", "priority": "none",
                      "start_date": null, "due_date": yesterday, "estimate_minutes": null, "tags": [], "assignee_ids": [],
                      "milestone": false }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    // D-125: 반복 규칙 편집의 기준은 기존 규칙이며 최초 설정은 null이다.
    let recurrence = send(&app, Method::PUT, &format!("/api/tasks/{}/recurrence", t["id"].as_str().unwrap()), Some(&admin),
         Some(json!({ "base": null, "rule": {"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null}, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(recurrence.status, StatusCode::OK, "{:?}", recurrence.json);
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
            Some(json!({ "base_version": 1, "title": title, "description": "", "visibility": visibility })),
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

    // 모듈 안 문서를 밖의 사람에게 공유하고 공개 링크를 만든다(모듈을 지우면 둘 다 막혀야 한다, 감사 반영).
    let shared_doc = post(
        &admin,
        format!("/api/items/{open}/documents"),
        json!({ "request_id": new_request_id(), "title": "모듈 문서" }),
    )
    .await
    .json;
    let shared_uri = format!("/api/documents/{}", shared_doc["id"].as_str().unwrap());
    let carol_email = unique_email("carol");
    let carol = login_with_email(&state, "carol", &carol_email, true).await;
    // D-114: 모듈 밖 공유도 조직 경계 안에서만 허용한다.
    let organization = common::organization_of_workspace(&ws).await;
    common::join_organization(organization, &carol, "member").await;
    let r = post(
        &admin,
        format!("{shared_uri}/shares"),
        json!({ "email": carol_email, "permission": "write", "organization_id": organization }),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let link = post(&admin, format!("{shared_uri}/public-links"), json!({}))
        .await
        .json;
    let token_path = format!(
        "/api/public/{}",
        link["url"].as_str().unwrap().rsplit('/').next().unwrap()
    );
    assert_eq!(get(&carol, shared_uri.clone()).await, StatusCode::OK);
    assert_eq!(
        send(&app, Method::GET, &token_path, None, None)
            .await
            .status,
        StatusCode::OK
    );

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
    // 공유받은 사람·공개 링크도 막힌다(읽기·고치기·공유 목록).
    assert_eq!(get(&carol, shared_uri.clone()).await, StatusCode::NOT_FOUND);
    let r = send(
        &app,
        Method::PUT,
        &shared_uri,
        Some(&carol),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": 1, "title": "모듈 문서",
                     "body": common::doc_body("휴지통 안에서 고침") }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{:?}", r.json);
    assert_eq!(
        send(&app, Method::GET, "/api/me/shared", Some(&carol), None)
            .await
            .json,
        json!([])
    );
    assert_eq!(
        send(&app, Method::GET, &token_path, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
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
    assert_eq!(get(&carol, shared_uri.clone()).await, StatusCode::OK);
    assert_eq!(
        send(&app, Method::GET, &token_path, None, None)
            .await
            .status,
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

/// 감사 6번: 하위 업무를 지우는 동안 상위 업무를 지우면, 상위 업무는 이미 다른 묶음에 든 하위 업무를 자기 묶음으로
/// 옮기면 안 된다(먼저 지운 묶음을 되살려도 하위 업무가 돌아오지 않고, 묶음의 개수가 틀려진다).
#[tokio::test]
async fn trashing_a_parent_while_its_subtask_is_being_trashed_keeps_the_subtask_batch() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "업무 휴지통 경합").await;
    let it = item(&app, &admin, &ws, "TRC").await;
    let item_id = it["id"].as_str().unwrap();
    let parent = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "상위" })),
    )
    .await
    .json;
    let sub = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "하위", "parent_id": parent["id"] })),
    )
    .await
    .json;
    let sub_id: uuid::Uuid = sub["id"].as_str().unwrap().parse().unwrap();

    // T1: 하위 업무를 지운다(아직 끝나지 않음).
    let mut t1 = state.db.begin_as(admin.id).await.unwrap();
    let first: uuid::Uuid = sqlx::query_scalar("SELECT app_trash_task($1, $2)")
        .bind(sub_id)
        .bind(fvoci_server::db::new_ids(2))
        .fetch_one(t1.conn())
        .await
        .unwrap();
    // T2: 그동안 상위 업무를 지운다(하위 업무 행을 기다린다).
    let parent_uri = format!("/api/tasks/{}", parent["id"].as_str().unwrap());
    let second = tokio::spawn({
        let app = app.clone();
        let admin = admin.clone();
        async move { send(&app, Method::DELETE, &parent_uri, Some(&admin), None).await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    t1.commit().await.unwrap();
    let second = second.await.unwrap();
    assert_eq!(second.status, StatusCode::OK, "{:?}", second.json);

    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let batch: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT trash_batch_id FROM fvoci.tasks WHERE id = $1")
            .bind(sub_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(
        batch,
        Some(first),
        "the subtask moved to the parent's batch"
    );
    let count: i32 = sqlx::query_scalar(
        "SELECT member_count FROM fvoci.trash_batches WHERE root_id = $1::uuid AND root_kind = 'task'",
    )
    .bind(parent["id"].as_str().unwrap())
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(count, 1);
    owner.close().await;
}

/// 감사 7번: 휴지통에 든 모듈에는 항목을 만들 수 없다(템플릿 복제도 같은 길).
#[tokio::test]
async fn no_items_in_a_trashed_module() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "휴지통 모듈").await;
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "버림", "key": "TMI" })),
    )
    .await;
    let module = m.json["id"].as_str().unwrap();
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/modules/{module}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let r = send(
        &app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "들어가면 안 됨" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{:?}", r.json);
    // 복제는 만든 항목을 다시 읽지 않고 끝나므로 함수의 검사에만 기댄다.
    let source = item(&app, &admin, &ws, "SRC").await;
    let r = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/copy", source["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "module_id": module, "title": "복제", "reset": false })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{:?}", r.json);
}

/// 남은 항목 ③(D-18): 모듈을 영구 삭제해도 그 모듈의 지금 키와 과거 키는 예약으로 남아, 다른 모듈이 만들거나 이름을
/// 바꿀 때 쓰지 못한다(남은 글자 참조 `KEY-12`가 다른 모듈을 가리키지 않게). 워크스페이스 영구 삭제는 그대로 된다.
#[tokio::test]
async fn purged_module_keys_stay_reserved() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "키 예약").await;
    let create = |key: &'static str| {
        let app = app.clone();
        let admin = admin.clone();
        let ws = ws.clone();
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/workspaces/{ws}/modules"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
            )
            .await
        }
    };
    let gone = create("OLDK").await;
    assert_eq!(gone.status, StatusCode::CREATED, "{:?}", gone.json);
    let gone_id = gone.json["id"].as_str().unwrap().to_owned();
    // 키를 바꿔 과거 키(OLDK)와 지금 키(NEWK)를 둘 다 갖게 한다.
    let renamed = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{gone_id}"),
        Some(&admin),
        Some(json!({ "name": "OLDK", "key": "NEWK" })),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.json);
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/modules/{gone_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(trashed.status, StatusCode::OK, "{:?}", trashed.json);
    let purged = send(
        &app,
        Method::DELETE,
        &format!("/api/trash/{}", trashed.json["batch_id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    assert!(purged.status.is_success(), "{:?}", purged.json);

    for key in ["OLDK", "NEWK"] {
        let again = create(key).await;
        assert_eq!(
            again.status,
            StatusCode::CONFLICT,
            "create with {key}: {:?}",
            again.json
        );
    }
    let other = create("OTHR").await;
    assert_eq!(other.status, StatusCode::CREATED, "{:?}", other.json);
    for key in ["OLDK", "NEWK"] {
        let r = send(
            &app,
            Method::PUT,
            &format!("/api/modules/{}", other.json["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({ "name": "OTHR", "key": key })),
        )
        .await;
        assert_eq!(
            r.status,
            StatusCode::CONFLICT,
            "rename to {key}: {:?}",
            r.json
        );
    }

    // 워크스페이스 전체 영구 삭제는 예약까지 지운다.
    let deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}"),
        Some(&admin),
        Some(json!({ "name": "키 예약" })),
    )
    .await;
    assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/me/deleted-workspaces/{ws}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
}

/// 테이블 소유 역할 연결(시험 확인용).
async fn owner_pool() -> sqlx::PgPool {
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    sqlx::PgPool::connect(&url).await.unwrap()
}

/// 열어 둔 트랜잭션의 서버 프로세스 번호.
async fn backend_pid(tx: &mut fvoci_server::db::UserTx) -> i32 {
    sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(tx.conn())
        .await
        .unwrap()
}

/// 다른 세션이 `holder`의 잠금을 실제로 기다릴 때까지 기다린다. 고정 시간만 기다리면 느린 환경에서 요청이 잠금에
/// 닿기 전에 커밋해, 경합을 재현하지 못한 채 통과하거나 엉뚱한 이유로 실패한다. 끝내 기다리지 않으면 시험을 멈춘다.
async fn wait_until_blocked_by(holder: i32) {
    let db = owner_pool().await;
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&db)
        .await
        .unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    db.close().await;
    assert!(blocked, "no request waited on the held transaction");
}

async fn new_task(
    app: &Router,
    user: &TestUser,
    item_id: &str,
    title: &str,
    parent: Option<&Value>,
) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title, "parent_id": parent.map(|p| p["id"].clone()) })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

/// 감사 S1: 상위 업무를 지우는 사이 하위 업무가 떨어져 나가면(다른 상위로 옮기거나 최상위로) 그 업무는 함께 지우지 않는다.
/// 전에는 잠그기 전에 고른 목록을 잠근 뒤에도 그대로 써서, 방금 독립시킨 업무가 상위 업무의 묶음으로 휴지통에 들어갔다.
#[tokio::test]
async fn trashing_a_parent_leaves_a_subtask_detached_while_it_waited() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "하위 떼기 경합").await;
    let it = item(&app, &admin, &ws, "TRD").await;
    let item_id = it["id"].as_str().unwrap();
    let parent = new_task(&app, &admin, item_id, "상위", None).await;
    let sub = new_task(&app, &admin, item_id, "하위", Some(&parent)).await;
    let sub_id: uuid::Uuid = sub["id"].as_str().unwrap().parse().unwrap();

    // T1: 하위 업무를 최상위로 뗀다(아직 끝나지 않음, 하위 업무 행을 잡고 있다).
    let mut t1 = state.db.begin_as(admin.id).await.unwrap();
    let holder = backend_pid(&mut t1).await;
    sqlx::query("SELECT app_set_task_parent($1, NULL, $2)")
        .bind(sub_id)
        .bind(fvoci_server::db::new_ids(1))
        .execute(t1.conn())
        .await
        .unwrap();
    // T2: 그동안 상위 업무를 지운다(하위 업무 행을 기다린다).
    let parent_uri = format!("/api/tasks/{}", parent["id"].as_str().unwrap());
    let second = tokio::spawn({
        let app = app.clone();
        let admin = admin.clone();
        async move { send(&app, Method::DELETE, &parent_uri, Some(&admin), None).await }
    });
    wait_until_blocked_by(holder).await;
    t1.commit().await.unwrap();
    let second = second.await.unwrap();
    assert_eq!(second.status, StatusCode::OK, "{:?}", second.json);

    let owner = owner_pool().await;
    let (deleted, parent_id): (Option<String>, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT deleted_at::text, parent_id FROM fvoci.tasks WHERE id = $1")
            .bind(sub_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(
        deleted, None,
        "the detached subtask went to the trash with its old parent"
    );
    assert_eq!(parent_id, None);
    let count: i32 =
        sqlx::query_scalar("SELECT member_count FROM fvoci.trash_batches WHERE id = $1::uuid")
            .bind(second.json["batch_id"].as_str().unwrap())
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(count, 1);
    owner.close().await;
}

/// 감사 S5: 지우기가 문서 자리 잠금을 기다리는 사이 문서가 다른 워크스페이스로 옮겨지면, 옛 자리(옛 워크스페이스)
/// 기준으로 휴지통 묶음을 만들지 않고 409로 다시 하게 한다(옮기기·자리 바꾸기와 같은 규칙).
#[tokio::test]
async fn trashing_a_document_moved_to_another_workspace_meanwhile_conflicts() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let w1 = create_workspace(&app, &admin, "옮기기 전").await;
    let w2 = create_workspace(&app, &admin, "옮긴 뒤").await;
    let (writer, email) = add_member(&app, &state, &admin, &w1, "author", "member").await;
    // 쓴 사람은 옮겨 갈 워크스페이스에도 있다(읽기만).
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{w2}/invitations"),
        Some(&admin),
        Some(json!({ "email": email, "role": "readonly" })),
    )
    .await;
    let accepted = send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv.json["id"].as_str().unwrap()
        ),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.json);
    let doc = create_doc(&app, &writer, &w1, "옮겨질 문서").await;
    let doc_id: uuid::Uuid = doc["id"].as_str().unwrap().parse().unwrap();
    let w2_id: uuid::Uuid = w2.parse().unwrap();

    // T1: 관리자가 문서를 w2로 옮긴다(옮기기 처리와 같은 순서, 아직 끝나지 않음).
    let mut t1 = state.db.begin_as(admin.id).await.unwrap();
    let holder = backend_pid(&mut t1).await;
    sqlx::query("SELECT app_lock_move($1, $2, NULL)")
        .bind(doc_id)
        .bind(w2_id)
        .execute(t1.conn())
        .await
        .unwrap();
    let n: i32 = sqlx::query_scalar("SELECT app_move_document_ids($1)")
        .bind(doc_id)
        .fetch_one(t1.conn())
        .await
        .unwrap();
    sqlx::query("SELECT * FROM app_move_document($1, $2, NULL, $3)")
        .bind(doc_id)
        .bind(w2_id)
        .bind(fvoci_server::db::new_ids(usize::try_from(n).unwrap()))
        .execute(t1.conn())
        .await
        .unwrap();
    // T2: 쓴 사람이 그동안 문서를 지운다(옛 자리의 트리 잠금을 기다린다).
    let uri = format!("/api/documents/{doc_id}");
    let second = tokio::spawn({
        let app = app.clone();
        let writer = writer.clone();
        async move { send(&app, Method::DELETE, &uri, Some(&writer), None).await }
    });
    wait_until_blocked_by(holder).await;
    t1.commit().await.unwrap();
    let second = second.await.unwrap();
    assert_eq!(second.status, StatusCode::CONFLICT, "{:?}", second.json);

    let owner = owner_pool().await;
    let (deleted, ws): (Option<String>, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT deleted_at::text, workspace_id FROM fvoci.documents WHERE id = $1")
            .bind(doc_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(deleted, None);
    assert_eq!(ws, Some(w2_id));
    let batches: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.trash_batches WHERE root_id = $1 AND root_kind = 'document'",
    )
    .bind(doc_id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(batches, 0);
    owner.close().await;
}

/// 감사 S7: 휴지통에 든 항목의 업무 담당자·리더는 역할이 바뀌거나 그룹이 지워졌다는 이유만으로 빠지지 않는다(휴지통이라
/// 지금 볼 수 없을 뿐 권한을 잃은 것이 아니다). 워크스페이스에서 나간 사람은 휴지통에 있어도 빠진다.
#[tokio::test]
async fn trashed_items_keep_assignees_who_did_not_lose_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "휴지통 담당자").await;
    let (keep, _) = add_member(&app, &state, &admin, &ws, "keeper", "member").await;
    let (gone, _) = add_member(&app, &state, &admin, &ws, "leaver", "member").await;
    let it = item(&app, &admin, &ws, "TAS").await;
    let item_id = it["id"].as_str().unwrap();
    let task = new_task(&app, &admin, item_id, "담당 업무", None).await;
    let task_id = task["id"].as_str().unwrap();
    let assigned = send(&app, Method::PUT, &format!("/api/tasks/{task_id}"), Some(&admin), Some(json!({
        "base_version": task["version"], "title": "담당 업무", "description": "", "status": "할 일", "priority": "none",
        "start_date": null, "due_date": null, "estimate_minutes": null, "tags": [], "milestone": false,
        "assignee_ids": [keep.id.0, gone.id.0] }))).await;
    assert_eq!(assigned.status, StatusCode::OK, "{:?}", assigned.json);
    let lead = send(&app, Method::PUT, &format!("/api/items/{item_id}/profile"), Some(&admin), Some(json!({ "base_version": 1,
        "leader_id": gone.id.0, "goal": "", "scope": "", "starts_on": null, "ends_on": null, "status": "active",
        "details": {} }))).await;
    assert!(lead.status.is_success(), "{:?}", lead.json);

    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(trashed.status, StatusCode::OK, "{:?}", trashed.json);
    // 휴지통에 든 동안: 남는 사람은 역할만 바뀌고(올림), 떠나는 사람은 워크스페이스에서 나간다.
    let promoted = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", keep.id.0),
        Some(&admin),
        Some(json!({ "role": "admin" })),
    )
    .await;
    assert_eq!(
        promoted.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        promoted.json
    );
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", gone.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);

    let restored = send(
        &app,
        Method::POST,
        &format!(
            "/api/trash/{}/restore",
            trashed.json["batch_id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        restored.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        restored.json
    );
    let detail = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{task_id}"),
        Some(&admin),
        None,
    )
    .await;
    let assignees: Vec<String> = detail.json["assignees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["user_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        assignees,
        vec![keep.id.0.to_string()],
        "role change dropped a kept assignee or a leaver stayed"
    );
    let profile = send(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/profile"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        profile.json["leader_id"],
        Value::Null,
        "a person who left stayed leader of the restored item"
    );
}

/// 문서에 파일 하나를 올린다(한 조각). 파일 id를 돌려준다.
async fn attach(app: &Router, user: &TestUser, doc: &Value, name: &str, data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let started = send(app, Method::POST, "/api/uploads", Some(user), Some(json!({
        "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] }, "name": name,
        "size": data.len(), "content_type": "text/plain" }))).await;
    assert_eq!(started.status, StatusCode::CREATED, "{:?}", started.json);
    let id = started.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(data));
    let (status, _, _) = common::send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(user),
        &[
            ("x-content-sha256", hash.as_str()),
            ("content-type", "application/octet-stream"),
        ],
        data.to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    done.json["file_id"].as_str().unwrap().to_owned()
}

/// 감사 S13: 첨부를 휴지통에 넣은 사람은 그 문서에 쓸 수 없게 되면(공유 철회 등) 첨부를 되살리지 못한다. 전에는 항목
/// 권한만 다시 봐서, 항목이 없는 위키 문서의 첨부는 볼 수도 없는 문서에 되살렸다.
#[tokio::test]
async fn restoring_an_attachment_needs_write_access_to_its_document() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "첨부 되살리기").await;
    // 비공개 설정은 문서 소유 권한자(여기서는 관리자)가 한다.
    let author = admin.clone();
    let (helper, helper_email) =
        add_member(&app, &state, &admin, &ws, "doc-helper", "member").await;
    let doc = create_doc(&app, &author, &ws, "비공개 문서").await;
    let doc_id = doc["id"].as_str().unwrap();
    let organization = common::organization_of_workspace(&ws).await;
    let private = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc_id}/settings"),
        Some(&author),
        Some(json!({ "visibility": "private", "allow_copy": true })),
    )
    .await;
    assert!(private.status.is_success(), "{:?}", private.json);
    let shared = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&author),
        Some(json!({ "email": helper_email, "permission": "write", "organization_id": organization })),
    )
    .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{:?}", shared.json);
    let file_id = attach(&app, &author, &doc, "메모.txt", b"note").await;

    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}"),
        Some(&helper),
        None,
    )
    .await;
    assert_eq!(trashed.status, StatusCode::OK, "{:?}", trashed.json);
    let revoked = send(
        &app,
        Method::DELETE,
        &format!(
            "/api/document-shares/{}",
            shared.json["id"].as_str().unwrap()
        ),
        Some(&author),
        None,
    )
    .await;
    assert!(revoked.status.is_success(), "{:?}", revoked.json);

    let batch = trashed.json["batch_id"].as_str().unwrap();
    let restored = send(
        &app,
        Method::POST,
        &format!("/api/trash/{batch}/restore"),
        Some(&helper),
        None,
    )
    .await;
    assert_eq!(
        restored.status,
        StatusCode::NOT_FOUND,
        "{:?}",
        restored.json
    );
    let owner = owner_pool().await;
    let deleted: Option<String> =
        sqlx::query_scalar("SELECT deleted_at::text FROM fvoci.files WHERE id = $1::uuid")
            .bind(&file_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert!(
        deleted.is_some(),
        "the file was restored into a document the restorer can no longer write"
    );
    owner.close().await;
    // 다시 쓰기 공유를 받은 지운 사람은 되살릴 수 있다(관리자 우회가 아닌 지운 사람의 문서 쓰기 검사).
    let reshared = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&author),
        Some(json!({ "email": helper_email, "permission": "write", "organization_id": organization })),
    )
    .await;
    assert_eq!(reshared.status, StatusCode::CREATED, "{:?}", reshared.json);
    let by_helper = send(
        &app,
        Method::POST,
        &format!("/api/trash/{batch}/restore"),
        Some(&helper),
        None,
    )
    .await;
    assert_eq!(
        by_helper.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        by_helper.json
    );
}

/// 감사 C22: 개인 노트 휴지통(`GET /api/me/trash`, `POST /api/me/trash/purge-expired`)에는 시험이 없었다. 개인 노트는
/// 워크스페이스 관리자에게도 비공개다(AGENTS.md). 목록과 만료 비우기는 노트 주인의 것만 보고 지운다.
#[tokio::test]
async fn personal_trash_lists_and_purges_only_the_owners_notes() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "개인 휴지통").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let note = |user: &TestUser, title: &str| {
        let app = app.clone();
        let user = user.clone();
        let body = json!({ "request_id": new_request_id(), "title": title });
        async move {
            let created = send(&app, Method::POST, "/api/me/notes", Some(&user), Some(body)).await;
            assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
            let id = created.json["id"].as_str().unwrap().to_owned();
            let trashed = send(
                &app,
                Method::DELETE,
                &format!("/api/documents/{id}"),
                Some(&user),
                None,
            )
            .await;
            assert_eq!(trashed.status, StatusCode::OK, "{:?}", trashed.json);
            trashed.json["batch_id"].as_str().unwrap().to_owned()
        }
    };
    let mine_old = note(&member, "지난 메모").await;
    note(&member, "어제 메모").await;
    let theirs_old = note(&admin, "관리자 메모").await;
    for batch in [&mine_old, &theirs_old] {
        fixture_sql(
            "UPDATE fvoci.trash_batches SET deleted_at = now() - interval '31 days' WHERE id = $1::uuid",
            batch,
        )
        .await;
    }

    // 목록은 내 노트만. 관리자의 노트는 같은 워크스페이스 멤버에게도 보이지 않는다(반대도 같다).
    let titles = |v: &Value| -> Vec<String> {
        let mut t: Vec<String> = v["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["title"].as_str().unwrap().to_owned())
            .collect();
        t.sort();
        t
    };
    let mine = send(&app, Method::GET, "/api/me/trash", Some(&member), None).await;
    assert_eq!(mine.status, StatusCode::OK, "{:?}", mine.json);
    assert_eq!(titles(&mine.json), ["어제 메모", "지난 메모"]);
    let admins = send(&app, Method::GET, "/api/me/trash", Some(&admin), None).await;
    assert_eq!(titles(&admins.json), ["관리자 메모"]);
    // 워크스페이스 휴지통에는 개인 노트가 없다.
    let team = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(team.status, StatusCode::OK, "{:?}", team.json);
    assert!(
        titles(&team.json).iter().all(|t| !t.contains("메모")),
        "{:?}",
        team.json
    );

    // 만료 비우기는 내 만료된 노트만 지운다. 관리자의 만료된 노트와 내 새 노트는 남는다.
    let purged = send(
        &app,
        Method::POST,
        "/api/me/trash/purge-expired",
        Some(&member),
        None,
    )
    .await;
    assert_eq!(purged.status, StatusCode::OK, "{:?}", purged.json);
    assert_eq!(purged.json["purged"], 1);
    let mine = send(&app, Method::GET, "/api/me/trash", Some(&member), None).await;
    assert_eq!(titles(&mine.json), ["어제 메모"]);
    let admins = send(&app, Method::GET, "/api/me/trash", Some(&admin), None).await;
    assert_eq!(titles(&admins.json), ["관리자 메모"]);
    // 남의 묶음은 되살리지 못한다(볼 수 없으면 404).
    let restore = send(
        &app,
        Method::POST,
        &format!("/api/trash/{theirs_old}/restore"),
        Some(&member),
        None,
    )
    .await;
    assert_eq!(restore.status, StatusCode::NOT_FOUND, "{:?}", restore.json);
}
