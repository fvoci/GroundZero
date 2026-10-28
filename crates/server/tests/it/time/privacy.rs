#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
use crate::{
    common::{self, send},
    sql_support::World,
};
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

#[tokio::test]
async fn explicit_server_admin_cannot_read_another_users_time_history_or_totals() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let task = w.task().await;
    let admin = w.member("admin").await;
    common::make_server_admin(&admin).await;
    let mut body = json!({"request_id":Uuid::now_v7(),"started_at":"2026-10-20T00:00:00Z",
        "ended_at":"2026-10-20T00:02:00Z","paused_seconds":0,"kind":"work","memo":"private",
        "target":{"kind":"task","id":task}});
    let entry = send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&w.owner),
        Some(body.clone()),
    )
    .await;
    assert_eq!(entry.status, StatusCode::CREATED, "{:?}", entry.json);
    let uri = format!("/api/time-entries/{}", entry.json["id"].as_str().unwrap());
    body["base_version"] = entry.json["version"].clone();
    body["memo"] = json!("private revision");
    let saved = send(&app, Method::PUT, &uri, Some(&w.owner), Some(body.clone())).await;
    assert_eq!(saved.status, StatusCode::OK);
    let history = send(
        &app,
        Method::GET,
        &format!("{uri}/revisions"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(
        history.json.as_array().unwrap().len(),
        1,
        "there is real history to hide"
    );
    let running = send(&app, Method::POST, "/api/me/timer", Some(&w.owner),
        Some(json!({"request_id":Uuid::now_v7(),"kind":"study","memo":"running private","target":null}))).await;
    assert_eq!(running.status, StatusCode::CREATED);
    body["request_id"] = json!(Uuid::now_v7());
    body["ended_at"] = json!("2026-10-20T00:01:00Z");
    body["memo"] = json!("admin own");
    let own = send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&admin),
        Some(body.clone()),
    )
    .await;
    assert_eq!(own.status, StatusCode::CREATED);
    let range = "/api/me/time-entries?from=2026-10-20&to=2026-10-20&tz=UTC";
    let list = send(&app, Method::GET, range, Some(&admin), None).await;
    assert_eq!(list.json, json!([own.json]));
    assert_eq!(
        send(&app, Method::GET, "/api/me/timer", Some(&admin), None)
            .await
            .json,
        Value::Null
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("{uri}/revisions"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let labels = send(
        &app,
        Method::GET,
        &format!(
            "/api/time-entry-targets?entry_id={}",
            entry.json["id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(labels.status, StatusCode::OK);
    assert_eq!(labels.json, json!([]));
    let summary = send(
        &app,
        Method::GET,
        "/api/me/time-summary?from=2026-10-20&to=2026-10-20&tz=UTC&group=day",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        summary.json,
        json!([{"period":"2026-10-20","kind":"work","seconds":60}])
    );
    let total = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{task}/time"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(total.status, StatusCode::OK);
    assert_eq!(total.json["mine_seconds"], 60);
    let total = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/time", w.item),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(total.status, StatusCode::OK);
    assert_eq!(
        total.json["kinds"],
        json!([{"kind":"work","mine_seconds":60}])
    );
    for (method, path, payload) in [
        (
            Method::POST,
            format!("{uri}/timer"),
            json!({"action":"stop"}),
        ),
        (Method::PUT, uri.clone(), body),
        (Method::DELETE, uri, Value::Null),
    ] {
        assert_eq!(
            send(&app, method, &path, Some(&admin), Some(payload))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    let mut tx = w.state.db.begin_as(admin.id).await.unwrap();
    let entries: i64 = sqlx::query_scalar("SELECT count(*) FROM time_entries WHERE user_id=$1")
        .bind(w.owner.id.0)
        .fetch_one(tx.conn())
        .await
        .unwrap();
    let revisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM time_entry_revisions WHERE user_id=$1")
            .bind(w.owner.id.0)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    assert_eq!(
        (entries, revisions),
        (0, 0),
        "app role RLS has no server administrator exception"
    );
}
