#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! Migration generation checks use only newly created fixture databases.

mod common;

use fvoci_server::config::{DbCommandConfig, Secret};
use fvoci_server::db::{Db, admin};
use fvoci_server::error::AppError;
use sqlx::migrate::Migrate;
use sqlx::{Connection, PgConnection};

async fn empty_database(label: &str) -> (PgConnection, common::FreshDb) {
    static PREPARE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _serial = PREPARE.lock().await;
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via cargo xtask test");
    let migration_url =
        std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via cargo xtask test");
    let app_url = std::env::var("FVOCI_TEST_DIRECT_APP_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("run via cargo xtask test");
    let mut connection = PgConnection::connect(&admin_url).await.unwrap();
    let name = format!("fvoci_{label}_{}", uuid::Uuid::now_v7().simple());
    sqlx::raw_sql(&format!("CREATE DATABASE {name}"))
        .execute(&mut connection)
        .await
        .unwrap();
    let mut target_admin = PgConnection::connect(&common::with_database(&admin_url, &name))
        .await
        .unwrap();
    admin::prepare_database(&mut target_admin).await.unwrap();
    target_admin.close().await.unwrap();
    let db = DbCommandConfig {
        admin_url: Some(Secret::new(common::with_database(&admin_url, &name))),
        migration_url: Some(Secret::new(common::with_database(&migration_url, &name))),
        migrator_password: None,
        app_password: None,
    };
    let app_url = common::with_database(&app_url, &name);
    (connection, common::FreshDb { name, db, app_url })
}

/// 새 설치가 적용하는 마이그레이션 수: 기준(202610250001, 오버홀 재통합 D-126) 하나와 그 뒤에 새 번호로 더한 것.
fn migration_count() -> usize {
    sqlx::migrate!("./migrations").migrations.len()
}

async fn ledger(conn: &mut PgConnection) -> Vec<(i64, Vec<u8>, bool)> {
    sqlx::query_as("SELECT version, checksum, success FROM fvoci._sqlx_migrations ORDER BY version")
        .fetch_all(conn)
        .await
        .unwrap()
}

async fn startup_error(app_url: &str) -> AppError {
    let config = common::config_with(&[("DATABASE_URL", app_url)]);
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        fvoci_server::app::run(config),
    )
    .await
    .unwrap()
    .unwrap_err()
}

async fn schema_types(conn: &mut PgConnection) -> serde_json::Value {
    sqlx::query_scalar(
        "SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY t.oid), '[]'::jsonb)
         FROM pg_catalog.pg_type t JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
         WHERE n.nspname = 'fvoci'",
    )
    .fetch_one(conn)
    .await
    .unwrap()
}

#[tokio::test]
async fn bootstrap_only_database_requests_migration_and_can_initialize() {
    let (mut fixture_admin, fixture) = empty_database("uninitialized_guard").await;
    let mut conn = PgConnection::connect(fixture.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    let before = schema_types(&mut conn).await;
    assert_eq!(before, serde_json::json!([]));
    let error = startup_error(&fixture.app_url).await;
    assert!(matches!(error, AppError::Config(message)
            if message.contains("has no application objects")
                && message.contains("fvoci-server db migrate")
                && !message.contains("recreation")));
    assert_eq!(schema_types(&mut conn).await, before);
    let ledger_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('fvoci._sqlx_migrations') IS NOT NULL")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert!(
        !ledger_exists,
        "application startup must not create a ledger"
    );

    // SQLx의 빈 원장·행 타입·배열 타입만 남은 초기 설치 재시도도 정상 적용할 수 있다.
    conn.ensure_migrations_table().await.unwrap();
    let error = startup_error(&fixture.app_url).await;
    assert!(matches!(error, AppError::Config(message)
            if message.contains("fvoci-server db migrate")
                && message.contains("validate its migration history")
                && !message.contains("recreation")));
    assert!(ledger(&mut conn).await.is_empty());
    admin::migrate(&fixture.db).await.unwrap();
    let installed = ledger(&mut conn).await;
    assert_eq!(installed.len(), migration_count());
    assert_eq!(installed[0].0, admin::BASELINE_VERSION);
    let app = Db::connect_lazy(&Secret::new(fixture.app_url.clone())).unwrap();
    app.check_baseline().await.unwrap();
    app.close().await;
    conn.close().await.unwrap();
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fixture.name))
        .execute(&mut fixture_admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn standalone_types_are_rejected_without_creating_a_ledger() {
    for (label, create, read) in [
        (
            "enum_guard",
            "CREATE TYPE fvoci.users AS ENUM ('preserved')",
            "SELECT 'preserved'::fvoci.users::text",
        ),
        (
            "domain_guard",
            "CREATE DOMAIN fvoci.preserved_domain AS text CHECK (VALUE = 'preserved')",
            "SELECT 'preserved'::fvoci.preserved_domain::text",
        ),
    ] {
        let (mut fixture_admin, fixture) = empty_database(label).await;
        let mut conn = PgConnection::connect(fixture.db.migration_url.as_ref().unwrap().expose())
            .await
            .unwrap();
        sqlx::raw_sql(create).execute(&mut conn).await.unwrap();
        let before = schema_types(&mut conn).await;
        let error = admin::migrate(&fixture.db).await.unwrap_err();
        assert!(
            matches!(error, AppError::Config(message) if message.contains("requires recreation"))
        );
        let error = startup_error(&fixture.app_url).await;
        assert!(
            matches!(error, AppError::Config(message) if message.contains("requires recreation"))
        );
        assert_eq!(schema_types(&mut conn).await, before);
        let value: String = sqlx::query_scalar(read).fetch_one(&mut conn).await.unwrap();
        assert_eq!(value, "preserved");
        let ledger_exists: bool =
            sqlx::query_scalar("SELECT to_regclass('fvoci._sqlx_migrations') IS NOT NULL")
                .fetch_one(&mut conn)
                .await
                .unwrap();
        assert!(
            !ledger_exists,
            "rejected types must not create SQLx history"
        );
        conn.close().await.unwrap();
        sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fixture.name))
            .execute(&mut fixture_admin)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn previous_history_and_untracked_objects_are_rejected_without_data_loss() {
    let (mut fixture_admin, fixture) = empty_database("legacy_guard").await;
    let mut conn = PgConnection::connect(fixture.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    conn.ensure_migrations_table().await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO _sqlx_migrations(version, description, success, checksum, execution_time)
                    VALUES (1, 'old initial', true, decode('cafe', 'hex'), 1),
                           (98, 'old task fields', true, decode('beef', 'hex'), 1);
                   CREATE TABLE preserved_data(value text NOT NULL);
                   INSERT INTO preserved_data VALUES ('must remain');
                   GRANT USAGE ON SCHEMA fvoci TO fvoci_app;",
    )
    .execute(&mut conn)
    .await
    .unwrap();
    let before = ledger(&mut conn).await;
    let error = admin::migrate(&fixture.db).await.unwrap_err();
    assert!(matches!(error, AppError::Config(message) if message.contains("requires recreation")));
    assert_eq!(ledger(&mut conn).await, before);
    let error = startup_error(&fixture.app_url).await;
    assert!(matches!(error, AppError::Config(message) if message.contains("requires recreation")));
    sqlx::query("DELETE FROM _sqlx_migrations")
        .execute(&mut conn)
        .await
        .unwrap();
    assert!(
        matches!(admin::migrate(&fixture.db).await, Err(AppError::Config(message)) if message.contains("requires recreation"))
    );
    assert!(ledger(&mut conn).await.is_empty());
    let value: String = sqlx::query_scalar("SELECT value FROM preserved_data")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(value, "must remain");
    conn.close().await.unwrap();
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fixture.name))
        .execute(&mut fixture_admin)
        .await
        .unwrap();
}

#[tokio::test]
async fn concurrent_new_installs_repeat_and_preserve_checksums_without_app_ledger_access() {
    let (mut fixture_admin, fixture) = empty_database("baseline_guard").await;
    // 모두 같은 빈 DB를 사용한다. 선행 설치의 커밋과 사전점검이 겹쳐도 구 이력으로 오인하지 않는다.
    let results = tokio::join!(
        admin::migrate(&fixture.db),
        admin::migrate(&fixture.db),
        admin::migrate(&fixture.db),
        admin::migrate(&fixture.db),
    );
    for result in [results.0, results.1, results.2, results.3] {
        result.unwrap();
    }
    let mut conn = PgConnection::connect(fixture.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    let before = ledger(&mut conn).await;
    assert_eq!(before.len(), migration_count());
    assert_eq!(before[0].0, admin::BASELINE_VERSION);
    admin::migrate(&fixture.db).await.unwrap();
    assert_eq!(ledger(&mut conn).await, before);
    let app = Db::connect_lazy(&Secret::new(fixture.app_url.clone())).unwrap();
    app.check_baseline().await.unwrap();
    let mut app_conn = PgConnection::connect(&fixture.app_url).await.unwrap();
    let denied = sqlx::query("SELECT * FROM fvoci._sqlx_migrations")
        .fetch_all(&mut app_conn)
        .await
        .unwrap_err();
    assert_eq!(
        denied.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );
    app_conn.close().await.unwrap();
    app.close().await;
    sqlx::query("UPDATE _sqlx_migrations SET checksum = decode('bad0', 'hex')")
        .execute(&mut conn)
        .await
        .unwrap();
    assert!(
        matches!(admin::migrate(&fixture.db).await, Err(AppError::Migrate(sqlx::migrate::MigrateError::VersionMismatch(version))) if version == admin::BASELINE_VERSION)
    );
    assert_eq!(ledger(&mut conn).await[0].1, vec![0xba, 0xd0]);
    conn.close().await.unwrap();
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fixture.name))
        .execute(&mut fixture_admin)
        .await
        .unwrap();
}
