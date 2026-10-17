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
    TestUser, add_member, create_doc, create_workspace, fixture_sql, login, new_request_id, send,
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
