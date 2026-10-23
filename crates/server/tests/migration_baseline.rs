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

async fn ledger(conn: &mut PgConnection) -> Vec<(i64, Vec<u8>, bool)> {
    sqlx::query_as("SELECT version, checksum, success FROM fvoci._sqlx_migrations ORDER BY version")
        .fetch_all(conn)
        .await
        .unwrap()
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
    let config = common::config_with(&[("DATABASE_URL", &fixture.app_url)]);
    let error = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        fvoci_server::app::run(config),
    )
    .await
    .unwrap()
    .unwrap_err();
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
    assert_eq!(before.len(), 1);
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
