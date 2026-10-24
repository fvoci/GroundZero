#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: failures must panic"
)]
//! Home marks retain independent categories and current target authorization.

mod common;

#[path = "../../../target/ablation-wave2/raw.rs"]
mod raw;

use axum::http::{Method, StatusCode};
use common::{TestUser, new_request_id, send};
use fvoci_server::{http::AppState, ids::UserId};
use serde_json::{Value, json};
use sqlx::Connection;
use uuid::Uuid;

// Baseline projection, kept independent of the product implementation.
const BASELINE: &str =
    "SELECT m.target_kind, m.target_id, app_time_target_label(m.target_kind, m.target_id),
                    coalesce(t.workspace_id, d.workspace_id, i.workspace_id),
                    coalesce(t.item_id, d.item_id)
             FROM user_marks m
             LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
             LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
             LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
             WHERE m.mark = $1 ORDER BY m.created_at, m.target_id";

type Row = (String, Uuid, Option<String>, Option<Uuid>, Option<Uuid>);

async fn old_marks(state: &AppState, user: UserId, mark: &str) -> Value {
    let mut tx = state.db.begin_as(user).await.unwrap();
    let rows: Vec<Row> = sqlx::query_as(BASELINE)
        .bind(mark)
        .fetch_all(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    Value::Array(rows.into_iter().filter_map(|(kind, id, label, workspace_id, item_id)| {
        Some(json!({ "kind": kind, "id": id, "label": label?, "workspace_id": workspace_id, "item_id": item_id }))
    }).collect())
}

async fn check_home(state: &AppState, user: &TestUser) -> Value {
    let app = common::app(state);
    let home = send(&app, Method::GET, "/api/me/home?tz=UTC", Some(user), None).await;
    assert_eq!(home.status, StatusCode::OK, "{:?}", home.json);
    assert_eq!(home.json["pins"], old_marks(state, user.id, "pin").await);
    assert_eq!(
        home.json["favorites"],
        old_marks(state, user.id, "favorite").await
    );
    home.json
}

async fn mark(state: &AppState, user: &TestUser, kind: &str, id: &Value, category: &str) {
    let reply = send(
        &common::app(state),
        Method::PUT,
        &format!("/api/me/marks/{kind}/{}", id.as_str().unwrap()),
        Some(user),
        Some(json!({"mark":category,"on":true})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
}

#[tokio::test]
async fn home_marks_keep_both_categories_order_and_current_visibility() {
    let state = common::state();
    let app = common::app(&state);
    let owner = common::login(&state, "marks-owner").await;
    let ws = common::create_workspace(&app, &owner, "marks").await;
    let (admin, _) = common::add_member(&app, &state, &owner, &ws, "marks-admin", "admin").await;
    let (reader, _) = common::add_member(&app, &state, &owner, &ws, "marks-reader", "reader").await;
    let outsider = common::login(&state, "marks-outsider").await;
    let empty = check_home(&state, &owner).await;
    assert_eq!(empty["pins"], json!([]));
    assert_eq!(empty["favorites"], json!([]));

    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&owner),
        Some(json!({"request_id":new_request_id(),"kind":"project","name":"Marks","key":"MRK"})),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&owner),
        Some(json!({"request_id":new_request_id(),"title":"item"})),
    )
    .await
    .json;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&owner),
        Some(json!({"request_id":new_request_id(),"title":"task"})),
    )
    .await
    .json;
    let doc = common::create_doc(&app, &owner, &ws, "wiki").await;
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&owner),
        Some(json!({"request_id":new_request_id(),"title":"personal"})),
    )
    .await
    .json;

    mark(&state, &owner, "item", &item["id"], "pin").await;
    let pin_only = check_home(&state, &owner).await;
    assert_eq!(pin_only["pins"].as_array().unwrap().len(), 1);
    assert_eq!(pin_only["favorites"], json!([]));
    mark(&state, &owner, "item", &item["id"], "favorite").await;
    for (kind, target) in [("task", &task), ("document", &doc), ("document", &note)] {
        for category in ["favorite", "pin"] {
            mark(&state, &owner, kind, &target["id"], category).await;
        }
    }
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via cargo xtask test");
    let mut fixture = sqlx::PgConnection::connect(&url).await.unwrap();
    // Equal timestamps exercise the existing target_id tie ordering independently per category.
    sqlx::query("UPDATE user_marks SET created_at = '2026-01-01T00:00:00Z' WHERE user_id = $1")
        .bind(owner.id.0)
        .execute(&mut fixture)
        .await
        .unwrap();
    let both = check_home(&state, &owner).await;
    assert_eq!(both["pins"], both["favorites"]);
    assert_eq!(both["pins"].as_array().unwrap().len(), 4);
    for user in [&admin, &reader, &outsider] {
        let home = check_home(&state, user).await;
        assert_eq!(home["pins"], json!([]));
        assert_eq!(home["favorites"], json!([]));
    }
    // Stale/foreign marks cannot expose private items or personal notes, even to an admin.
    for (user, kind, id) in [
        (&admin, "document", &note["id"]),
        (&reader, "item", &item["id"]),
        (&outsider, "document", &doc["id"]),
    ] {
        for category in ["pin", "favorite"] {
            sqlx::query(
                "INSERT INTO user_marks (user_id,mark,target_kind,target_id) VALUES ($1,$2,$3,$4)",
            )
            .bind(user.id.0)
            .bind(category)
            .bind(kind)
            .bind(id.as_str().unwrap().parse::<Uuid>().unwrap())
            .execute(&mut fixture)
            .await
            .unwrap();
        }
        let home = check_home(&state, user).await;
        assert_eq!(home["pins"], json!([]));
        assert_eq!(home["favorites"], json!([]));
    }
    for category in ["pin", "favorite"] {
        mark(&state, &reader, "document", &doc["id"], category).await;
    }
    assert_eq!(
        check_home(&state, &reader).await["favorites"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    sqlx::query("UPDATE documents SET visibility = 'private' WHERE id = $1")
        .bind(doc["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .execute(&mut fixture)
        .await
        .unwrap();
    assert_eq!(check_home(&state, &reader).await["pins"], json!([]));
    assert_eq!(check_home(&state, &reader).await["favorites"], json!([]));
    mark(&state, &admin, "document", &doc["id"], "favorite").await;
    assert_eq!(
        check_home(&state, &admin).await["favorites"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(check_home(&state, &admin).await["pins"], json!([]));

    sqlx::query("UPDATE items SET title = 'renamed', archived_at = now() WHERE id = $1")
        .bind(item["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .execute(&mut fixture)
        .await
        .unwrap();
    let renamed = check_home(&state, &owner).await;
    assert!(
        renamed["pins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["label"].as_str().unwrap().contains("renamed"))
    );
    sqlx::query("UPDATE documents SET deleted_at = now() WHERE id = $1")
        .bind(doc["id"].as_str().unwrap().parse::<Uuid>().unwrap())
        .execute(&mut fixture)
        .await
        .unwrap();
    let deleted = check_home(&state, &owner).await;
    assert_eq!(deleted["pins"].as_array().unwrap().len(), 3);
    assert_eq!(check_home(&state, &admin).await["favorites"], json!([]));
    assert_eq!(
        send(&app, Method::GET, "/api/me/home?tz=UTC", None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            "/api/me/home?tz=invalid",
            Some(&owner),
            None
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let mut anon = state.db.begin_anonymous().await.unwrap();
    let marks: i64 = sqlx::query_scalar("SELECT count(*) FROM user_marks")
        .fetch_one(anon.conn())
        .await
        .unwrap();
    assert_eq!(marks, 0);
}
