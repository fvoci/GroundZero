#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 새 행의 id는 앱이 만든다(D-88). DB는 id를 만들지 않고, 빠지거나 UUIDv7이 아닌 id를 다른 값으로 채우지 않는다.

mod common;

use common::{login, new_request_id};
use sqlx::Connection;
use uuid::Uuid;

fn sqlstate(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|e| e.code())
        .map(|c| c.into_owned())
        .unwrap_or_default()
}

/// 앱 함수는 받은 id를 그대로 쓰고, 모자라거나 UUIDv7이 아니면 아무것도 만들지 않고 멈춘다.
#[tokio::test]
async fn functions_use_the_ids_the_server_passes_and_never_fill_in_missing_ones() {
    let state = common::state();
    let user = login(&state, "ids").await;
    let request = Uuid::parse_str(&new_request_id()).unwrap();

    // 모자람(FV001, 다시 하면 되는 경합)과 잘못된 id(FV002, 서버 버그)를 다른 코드로 낸다.
    for (ids, label, code) in [
        (Vec::new(), "no ids", "FV001"),
        (vec![Uuid::now_v7()], "one id short", "FV001"),
        (vec![not_v7(), Uuid::now_v7()], "not UUIDv7", "FV002"),
    ] {
        let mut tx = state.db.begin_as(user.id).await.unwrap();
        let err = sqlx::query_scalar::<_, Uuid>("SELECT app_create_workspace($1, 'ids', $2)")
            .bind(request)
            .bind(&ids)
            .fetch_one(tx.conn())
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&err), code, "{label}: {err}");
        // HTTP 경로: 모자람은 다시 보내라는 409, 잘못된 id는 500.
        let mapped = fvoci_server::error::AppError::from_db_function(err);
        assert_eq!(
            matches!(mapped, fvoci_server::error::AppError::Conflict(_)),
            code == "FV001",
            "{label}: {mapped:?}"
        );
    }
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let made: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM workspaces WHERE created_by = app_user_id() AND request_id = $1",
    )
    .bind(request)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(made, 0, "a failed call left a workspace behind");

    let ids = fvoci_server::db::new_ids(2);
    let id: Uuid = sqlx::query_scalar("SELECT app_create_workspace($1, 'ids', $2)")
        .bind(request)
        .bind(&ids)
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(id, ids[0]);
    let audit: Uuid = sqlx::query_scalar(
        "SELECT id FROM audit_log WHERE workspace_id = $1 AND action = 'create' AND target_type = 'workspace'",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(audit, ids[1]);
    tx.commit().await.unwrap();
}

/// 표에는 id 기본값이 없고 UUIDv7만 받는다. 함수 본문도 UUID를 만들지 않는다.
#[tokio::test]
async fn tables_have_no_id_default_and_accept_only_uuidv7() {
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let mut owner = sqlx::PgConnection::connect(&url).await.unwrap();

    let defaults: Vec<String> = sqlx::query_scalar(
        "SELECT table_name || '.' || column_name FROM information_schema.columns
         WHERE table_schema = 'fvoci' AND data_type = 'uuid' AND column_default IS NOT NULL",
    )
    .fetch_all(&mut owner)
    .await
    .unwrap();
    assert!(
        defaults.is_empty(),
        "uuid columns with a default: {defaults:?}"
    );
    let generators: Vec<String> = sqlx::query_scalar(
        "SELECT p.oid::regprocedure::text FROM pg_proc p
         WHERE p.pronamespace = 'fvoci'::regnamespace
           AND (p.prosrc ILIKE '%uuidv7(%' OR p.prosrc ILIKE '%gen_random_uuid(%' OR p.prosrc ILIKE '%uuid_generate%')",
    )
    .fetch_all(&mut owner)
    .await
    .unwrap();
    assert!(
        generators.is_empty(),
        "functions that make UUIDs: {generators:?}"
    );

    let subject = format!("ids-{}", Uuid::now_v7());
    let insert = "INSERT INTO fvoci.users (id, issuer, subject, display_name) VALUES ($1, 'https://ids.test', $2, 'ids')";
    let mut tx = owner.begin().await.unwrap();
    let missing = sqlx::query(
        "INSERT INTO fvoci.users (issuer, subject, display_name) VALUES ('https://ids.test', $1, 'ids')",
    )
    .bind(&subject)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert_eq!(sqlstate(&missing), "23502", "{missing}");
    tx.rollback().await.unwrap();
    let mut tx = owner.begin().await.unwrap();
    let v4 = sqlx::query(insert)
        .bind(not_v7())
        .bind(&subject)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&v4), "23514", "{v4}");
    tx.rollback().await.unwrap();
    let mut tx = owner.begin().await.unwrap();
    sqlx::query(insert)
        .bind(Uuid::now_v7())
        .bind(&subject)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
}

/// UUIDv7이 아닌 UUID(버전 4 자리).
fn not_v7() -> Uuid {
    Uuid::parse_str("4b1e8f6c-1d2a-4c3b-9a8e-0f1e2d3c4b5a").unwrap()
}

/// 대상이 없으면 `_ids` 상한은 NULL이고(0개로 본다), 본 함수가 404를 낸다(500이 아니다).
#[tokio::test]
async fn missing_targets_are_404_not_500() {
    use axum::http::{Method, StatusCode};
    let state = common::state();
    let app = common::app(&state);
    let user = login(&state, "ids-missing").await;
    let group = Uuid::now_v7();
    let deleted = common::send(
        &app,
        Method::DELETE,
        &format!("/api/groups/{group}"),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NOT_FOUND, "{:?}", deleted.json);
    let removed = common::send(
        &app,
        Method::DELETE,
        &format!("/api/groups/{group}/members/{}", user.id.0),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::NOT_FOUND, "{:?}", removed.json);
}

/// 작업 프로세스: 새 id가 모자라면(FV001) 트랜잭션을 통째로 되돌리고 상한부터 다시 해 정해진 횟수 뒤 오류로 끝낸다.
/// 실패한 시도는 아무것도 남기지 않고, 맞는 상한으로 돌리면 한 번만 만든다(같은 알림이 두 번 생기지 않음).
#[tokio::test]
async fn worker_retries_the_whole_job_and_leaves_nothing_on_short_ids() {
    use axum::http::{Method, StatusCode};
    use common::{create_workspace, fixture_sql, send};
    use fvoci_server::worker::{Job, run_jobs};
    use serde_json::json;
    let state = common::state();
    let app = common::app(&state);
    let user = login(&state, "ids-worker").await;
    let ws = create_workspace(&app, &user, "작업 id").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&user),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "I", "key": "IDS" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&user),
        Some(json!({ "request_id": new_request_id(), "title": "마감" })),
    )
    .await
    .json;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&user),
        Some(json!({ "request_id": new_request_id(), "title": "오늘 마감" })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    let task_id = task.json["id"].as_str().unwrap().to_owned();
    fixture_sql(
        "UPDATE fvoci.tasks SET due_date = current_date WHERE id = $1::uuid",
        &task_id,
    )
    .await;
    let due_count = || async {
        let mut tx = state.db.begin_as(user.id).await.unwrap();
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM notifications WHERE task_id = $1::uuid AND kind = 'due'",
        )
        .bind(&task_id)
        .fetch_one(tx.conn())
        .await
        .unwrap()
    };

    // 상한을 0으로 속이면 매번 모자란다: 정해진 횟수만큼 다시 하고 오류로 끝나며 아무것도 남지 않는다.
    let short = [Job {
        ids: Some("SELECT 0"),
        call: "SELECT app_system_due_notifications(current_date, $1)",
    }];
    let err = run_jobs(&state.db, &short).await.unwrap_err();
    assert!(
        matches!(&err, fvoci_server::error::AppError::Database(e)
            if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("FV001")),
        "{err:?}"
    );
    assert_eq!(due_count().await, 0, "a failed attempt left notifications");

    // 맞는 상한이면 한 번 만들고, 다시 돌려도 더 만들지 않는다.
    let right = [Job {
        ids: Some("SELECT app_system_due_notifications_ids(current_date)"),
        call: "SELECT app_system_due_notifications(current_date, $1)",
    }];
    run_jobs(&state.db, &right).await.unwrap();
    assert_eq!(due_count().await, 1);
    run_jobs(&state.db, &right).await.unwrap();
    assert_eq!(due_count().await, 1);
}
