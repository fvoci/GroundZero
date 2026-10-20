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

    for (ids, label) in [
        (Vec::new(), "no ids"),
        (vec![Uuid::now_v7()], "one id short"),
        (vec![not_v7(), Uuid::now_v7()], "not UUIDv7"),
    ] {
        let mut tx = state.db.begin_as(user.id).await.unwrap();
        let err = sqlx::query_scalar::<_, Uuid>("SELECT app_create_workspace($1, 'ids', $2)")
            .bind(request)
            .bind(&ids)
            .fetch_one(tx.conn())
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&err), "FV001", "{label}: {err}");
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
