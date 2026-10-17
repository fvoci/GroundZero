#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 백그라운드 작업 (O-19): 만료된 휴지통 비우기, 반복 업무 회차 만들기, 주기 작업 한 번만.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, create_doc, create_workspace, fixture_sql, login, new_request_id, send};
use fvoci_server::worker;
use serde_json::{Value, json};

async fn trashed_doc(app: &Router, user: &TestUser, ws: &str, title: &str) -> (Value, String) {
    let doc = create_doc(app, user, ws, title).await;
    let r = send(
        app,
        Method::DELETE,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(user),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    (doc, r.json["batch_id"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn the_worker_purges_only_expired_trash_and_fills_repeating_tasks() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "작업").await;
    let (old, old_batch) = trashed_doc(&app, &admin, &ws, "오래된 문서").await;
    let (_fresh, _) = trashed_doc(&app, &admin, &ws, "새 문서").await;
    fixture_sql("UPDATE fvoci.trash_batches SET deleted_at = now() - interval '31 days' WHERE id = $1::uuid", &old_batch).await;

    // 반복 업무: 매일. 내일 회차를 지운 것처럼 만들고 작업이 다시 만드는지 본다.
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "W", "key": "WRK" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "점검" })),
    )
    .await
    .json;
    let t = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "백업 확인" })),
    )
    .await
    .json;
    let today: String = {
        let mut tx = state.db.begin_as(admin.id).await.unwrap();
        sqlx::query_scalar("SELECT ((now() AT TIME ZONE 'Asia/Seoul')::date)::text")
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "base_version": t["version"], "title": "백업 확인", "description": "", "status": "todo",
                              "priority": "none", "start_date": null, "due_date": today, "estimate_minutes": null,
                              "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}/recurrence", t["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "rule": {"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null}, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let series = r.json["series"]["id"].as_str().unwrap().to_owned();
    fixture_sql("DELETE FROM fvoci.tasks WHERE series_id = $1::uuid AND occurrence_date > (now() AT TIME ZONE 'Asia/Seoul')::date", &series).await;

    let report = worker::run_all_now(&state.db).await.unwrap();
    assert_eq!(report.purged, 1);
    assert_eq!(report.occurrences, 1);
    // 다시 돌려도 더 할 일이 없다.
    assert_eq!(
        worker::run_all_now(&state.db).await.unwrap(),
        worker::TickReport::default()
    );

    let entries = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let titles: Vec<&str> = entries["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["새 문서"]);
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let purge: (Option<uuid::Uuid>, bool) = sqlx::query_as(
        "SELECT actor, (detail->>'system')::boolean FROM audit_log WHERE action = 'purge' AND target_id = $1::uuid",
    )
    .bind(old["id"].as_str().unwrap())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(purge, (None, true));
    let made: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action = 'create' AND detail->>'series' = $1 AND detail ? 'system'",
    )
    .bind(&series)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(made, 1);
    drop(tx);

    // 주기 작업은 간격 안에서 한 번만 돈다(여러 프로세스가 떠 있어도).
    let (a, b) = tokio::join!(worker::run_once(&state.db), worker::run_once(&state.db));
    let _ = (a.unwrap(), b.unwrap());
    assert_eq!(
        worker::run_once(&state.db).await.unwrap(),
        worker::TickReport::default()
    );
}
