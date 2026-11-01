use std::path::Path;

use anyhow::{Result, ensure};
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection, PgConnection};
use xshell::cmd;

use super::{Inventory, Target, install, verify};
use crate::compose::{self, Compose, DbUrls, Env};

const OLD: &str = "CREATE ROLE fvoci_app LOGIN NOSUPERUSER NOCREATEROLE NOBYPASSRLS;
    CREATE ROLE fvoci_migrator LOGIN NOSUPERUSER NOCREATEROLE NOBYPASSRLS;
    CREATE SCHEMA fvoci AUTHORIZATION fvoci_migrator;
    CREATE TABLE fvoci.legacy(id int PRIMARY KEY, secret text);
    ALTER TABLE fvoci.legacy OWNER TO fvoci_migrator;
    INSERT INTO fvoci.legacy VALUES(1,'synthetic old row');
    CREATE TABLE fvoci.child(id int REFERENCES fvoci.legacy(id));
    ALTER TABLE fvoci.child OWNER TO fvoci_migrator;
    CREATE VIEW fvoci.legacy_view AS SELECT id FROM fvoci.legacy;
    ALTER VIEW fvoci.legacy_view OWNER TO fvoci_migrator;
    CREATE SEQUENCE fvoci.legacy_sequence;
    ALTER SEQUENCE fvoci.legacy_sequence OWNER TO fvoci_migrator;
    ALTER SEQUENCE fvoci.legacy_sequence OWNED BY fvoci.legacy.id;
    CREATE TABLE fvoci._sqlx_migrations(version bigint);
    ALTER TABLE fvoci._sqlx_migrations OWNER TO fvoci_migrator;
    INSERT INTO fvoci._sqlx_migrations VALUES(202610250001);
    GRANT USAGE ON SCHEMA fvoci TO fvoci_app;
    GRANT SELECT ON fvoci.legacy TO fvoci_app;
    CREATE SCHEMA auth;
    CREATE SCHEMA storage;
    CREATE TABLE auth.preserved(value text);
    INSERT INTO auth.preserved VALUES('managed synthetic');
    CREATE TABLE storage.preserved(value text);
    INSERT INTO storage.preserved VALUES('managed synthetic');
    CREATE ROLE other_app;
    CREATE SCHEMA other_app AUTHORIZATION other_app;
    CREATE TABLE other_app.preserved(value text);
    INSERT INTO other_app.preserved VALUES('other synthetic');
    CREATE ROLE anon;
    CREATE ROLE authenticated;
    CREATE ROLE service_role;
    CREATE ROLE authenticator;";

async fn sql(conn: &mut PgConnection, query: &str) -> Result<()> {
    sqlx::raw_sql(query).execute(conn).await?;
    Ok(())
}

async fn legacy_preserved(conn: &mut PgConnection) -> Result<()> {
    let value: String = sqlx::query_scalar("SELECT secret FROM fvoci.legacy WHERE id=1")
        .fetch_one(conn)
        .await?;
    ensure!(
        value == "synthetic old row",
        "legacy data lost during refusal"
    );
    Ok(())
}

async fn failure_tests(conn: &mut PgConnection, binding: &str) -> Result<()> {
    let first = Inventory::read(conn, binding).await?;
    ensure!(
        !first.blocked,
        "unexpected fixture blockers: {}",
        first.report["blockers"]
    );
    ensure!(
        first.report["candidates"]
            .as_array()
            .is_some_and(|c| c.len() >= 5),
        "missing deletion candidates"
    );
    let output = first.report.to_string();
    for secret in [
        "fvoci_migrator",
        "fvoci_app",
        "legacy",
        "other_app",
        "synthetic old row",
    ] {
        ensure!(!output.contains(secret), "identifier leaked in inventory");
    }
    ensure!(
        first.delete(conn, binding, &"0".repeat(64)).await.is_err(),
        "mismatch accepted"
    );
    legacy_preserved(conn).await?;
    sql(conn, "CREATE TABLE fvoci.drift(id int)").await?;
    ensure!(
        first.delete(conn, binding, &first.digest).await.is_err(),
        "catalog drift accepted"
    );
    legacy_preserved(conn).await?;
    sql(conn, "DROP TABLE fvoci.drift RESTRICT; CREATE VIEW other_app.shared AS SELECT id FROM fvoci.legacy").await?;
    let shared = Inventory::read(conn, binding).await?;
    ensure!(
        shared.blocked
            && shared.report["blockers"]["external_dependents"]
                .as_i64()
                .is_some_and(|c| c > 0),
        "shared dependency was not excluded"
    );
    ensure!(
        shared.delete(conn, binding, &shared.digest).await.is_err(),
        "shared dependency accepted"
    );
    legacy_preserved(conn).await?;
    sql(
        conn,
        "DROP VIEW other_app.shared RESTRICT; GRANT fvoci_app TO other_app",
    )
    .await?;
    let shared_role = Inventory::read(conn, binding).await?;
    ensure!(shared_role.blocked, "shared role not blocked");
    ensure!(
        shared_role
            .delete(conn, binding, &shared_role.digest)
            .await
            .is_err(),
        "shared role accepted"
    );
    sql(
        conn,
        "REVOKE fvoci_app FROM other_app; GRANT SELECT ON auth.preserved TO fvoci_app",
    )
    .await?;
    let shared_acl = Inventory::read(conn, binding).await?;
    ensure!(shared_acl.blocked, "external role ACL not blocked");
    ensure!(
        shared_acl
            .delete(conn, binding, &shared_acl.digest)
            .await
            .is_err(),
        "external ACL accepted"
    );
    sql(
        conn,
        "REVOKE SELECT ON auth.preserved FROM fvoci_app; SET ROLE other_app",
    )
    .await?;
    let restricted = Inventory::read(conn, binding).await?;
    ensure!(
        restricted.blocked && restricted.report["blockers"]["authority_missing"] == true,
        "missing authority accepted"
    );
    sql(conn, "RESET ROLE").await?;
    automatic_dependency_test(conn, binding).await?;
    unsupported_object_test(conn, binding).await?;
    legacy_preserved(conn).await?;
    println!(
        "PASS cloud-rebuild refusals: mismatched approval, drift, shared object/role/ACL, outside automatic object, missing authority, unsupported catalog; redacted inventory"
    );
    Ok(())
}

async fn unsupported_object_test(conn: &mut PgConnection, binding: &str) -> Result<()> {
    sql(
        conn,
        "CREATE COLLATION fvoci.unsupported FROM pg_catalog.\"C\"",
    )
    .await?;
    let unsupported = Inventory::read(conn, binding).await?;
    ensure!(
        unsupported.blocked
            && unsupported
                .delete(conn, binding, &unsupported.digest)
                .await
                .is_err(),
        "unsupported catalog object accepted"
    );
    sql(conn, "DROP COLLATION fvoci.unsupported RESTRICT").await?;
    Ok(())
}

async fn automatic_dependency_test(conn: &mut PgConnection, binding: &str) -> Result<()> {
    sql(conn, "CREATE TABLE fvoci.partitioned(id int) PARTITION BY RANGE(id);
        ALTER TABLE fvoci.partitioned OWNER TO fvoci_migrator;
        CREATE TABLE other_app.shared_partition PARTITION OF fvoci.partitioned FOR VALUES FROM(0) TO(10)").await?;
    let automatic_shared = Inventory::read(conn, binding).await?;
    ensure!(
        automatic_shared.blocked,
        "outside automatic dependency not blocked"
    );
    ensure!(
        automatic_shared
            .delete(conn, binding, &automatic_shared.digest)
            .await
            .is_err(),
        "outside automatic dependency accepted"
    );
    sql(
        conn,
        "DROP TABLE other_app.shared_partition RESTRICT; DROP TABLE fvoci.partitioned RESTRICT",
    )
    .await?;
    Ok(())
}

async fn cases(root: &Path, target: &Target) -> Result<()> {
    let mut admin = PgConnection::connect_with(&target.admin).await?;
    let empty: bool = sqlx::query_scalar("SELECT current_database()='fvoci'
        AND NOT EXISTS (SELECT FROM pg_roles WHERE rolname !~ '^pg_' AND rolname <> current_user)
        AND NOT EXISTS (SELECT FROM pg_database WHERE datname NOT IN ('postgres','fvoci','template0','template1'))
        AND NOT EXISTS (SELECT FROM pg_namespace WHERE nspname !~ '^pg_' AND nspname NOT IN ('public','information_schema'))
        AND NOT EXISTS (SELECT FROM pg_class WHERE relnamespace='public'::regnamespace)")
        .fetch_one(&mut admin).await?;
    ensure!(
        empty,
        "cloud fixture requires a fresh isolated test PostgreSQL cluster"
    );
    sql(&mut admin, OLD).await?;
    failure_tests(&mut admin, &target.binding).await?;
    let restricted = restricted_admin(&mut admin, target).await?;
    let mut operator = PgConnection::connect_with(&restricted.admin).await?;
    let inventory = Inventory::read(&mut operator, &target.binding).await?;
    ensure!(
        !inventory.blocked,
        "fixture blockers: {}",
        inventory.report["blockers"]
    );
    inventory
        .delete(&mut operator, &target.binding, &inventory.digest)
        .await?;
    operator.close().await?;
    let gone: bool = sqlx::query_scalar(
        "SELECT to_regnamespace('fvoci') IS NULL AND NOT EXISTS
        (SELECT FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))",
    )
    .fetch_one(&mut admin)
    .await?;
    ensure!(gone, "approved objects not deleted");
    for schema in ["auth", "storage", "other_app"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {schema}.preserved"))
            .fetch_one(&mut admin)
            .await?;
        ensure!(count == 1, "outside object was deleted");
    }
    let mut report = serde_json::json!({});
    sql(&mut admin, "ALTER ROLE cloud_fixture_admin NOCREATEROLE").await?;
    ensure!(
        install(root, &restricted, &mut report).await.is_err()
            && report["stage"] == "bootstrap"
            && report.get("migrate").is_none(),
        "migrate ran after failed bootstrap"
    );
    let unchanged: bool = sqlx::query_scalar(
        "SELECT to_regnamespace('fvoci') IS NULL AND NOT EXISTS
        (SELECT FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))",
    )
    .fetch_one(&mut admin)
    .await?;
    ensure!(unchanged, "failed bootstrap left changes");
    sql(&mut admin, "ALTER ROLE cloud_fixture_admin CREATEROLE").await?;
    report = serde_json::json!({});
    install(root, &restricted, &mut report).await?;
    verification_failures(&mut admin, target).await?;
    let verified = verify::run(target).await?;
    ensure!(
        verified["checksums"] == "10/10" && verified["rls"] == "PASS",
        "verification failed"
    );
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM fvoci.users")
        .fetch_one(&mut admin)
        .await?;
    ensure!(users == 0, "synthetic users persisted");
    println!(
        "PASS cloud-rebuild: non-superuser deletion/bootstrap, outside objects preserved, checksums 10/10, app login CRUD/RLS/denial, rollback cleanup"
    );
    // Leave only the installed product for the standard integration suite.
    sql(&mut admin, "DROP TABLE auth.preserved RESTRICT; DROP SCHEMA auth RESTRICT;
        DROP TABLE storage.preserved RESTRICT; DROP SCHEMA storage RESTRICT;
        DROP TABLE other_app.preserved RESTRICT; DROP SCHEMA other_app RESTRICT;
        DROP ROLE other_app; DROP ROLE anon; DROP ROLE authenticated; DROP ROLE service_role; DROP ROLE authenticator;
        SET ROLE cloud_fixture_admin; REVOKE CONNECT ON DATABASE fvoci FROM fvoci_app, fvoci_migrator RESTRICT;
        RESET ROLE; GRANT CONNECT ON DATABASE fvoci TO fvoci_app, fvoci_migrator;
        REVOKE ALL ON DATABASE fvoci FROM cloud_fixture_admin RESTRICT; DROP ROLE cloud_fixture_admin").await?;
    admin.close().await?;
    Ok(())
}

async fn restricted_admin(conn: &mut PgConnection, target: &Target) -> Result<Target> {
    let create: String = sqlx::query_scalar(
        "SELECT format('CREATE ROLE cloud_fixture_admin LOGIN CREATEROLE PASSWORD %L', $1::text)",
    )
    .bind(compose::TEST_MIGRATOR_PASSWORD)
    .fetch_one(&mut *conn)
    .await?;
    sql(conn, &create).await?;
    sql(conn, "GRANT ALL ON DATABASE fvoci TO cloud_fixture_admin WITH GRANT OPTION;
        GRANT fvoci_app, fvoci_migrator TO cloud_fixture_admin WITH ADMIN TRUE, SET TRUE, INHERIT FALSE").await?;
    Ok(Target {
        admin: target
            .admin
            .clone()
            .username("cloud_fixture_admin")
            .password(compose::TEST_MIGRATOR_PASSWORD),
        migrator: target.migrator.clone(),
        app: target.app.clone(),
        binding: target.binding.clone(),
        migrator_password: target.migrator_password.clone(),
        app_password: target.app_password.clone(),
        certificate: None,
    })
}

async fn verification_failures(admin: &mut PgConnection, target: &Target) -> Result<()> {
    // Tampered SQLx history must fail; restore the test row only after the refusal assertion.
    let checksum: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM fvoci._sqlx_migrations ORDER BY version LIMIT 1")
            .fetch_one(&mut *admin)
            .await?;
    sql(
        admin,
        "UPDATE fvoci._sqlx_migrations SET checksum=decode('00','hex') WHERE version=202610280001",
    )
    .await?;
    ensure!(verify::run(target).await.is_err(), "bad checksum accepted");
    sqlx::query("UPDATE fvoci._sqlx_migrations SET checksum=$1 WHERE version=202610280001")
        .bind(checksum)
        .execute(&mut *admin)
        .await?;
    sql(admin, "GRANT USAGE ON SCHEMA fvoci TO anon").await?;
    ensure!(
        verify::run(target).await.is_err(),
        "Data API grant accepted"
    );
    sql(admin, "REVOKE USAGE ON SCHEMA fvoci FROM anon; ALTER TABLE fvoci.documents DISABLE ROW LEVEL SECURITY").await?;
    ensure!(verify::run(target).await.is_err(), "disabled RLS accepted");
    sql(
        admin,
        "ALTER TABLE fvoci.documents ENABLE ROW LEVEL SECURITY; ALTER ROLE fvoci_app BYPASSRLS",
    )
    .await?;
    ensure!(
        verify::run(target).await.is_err(),
        "BYPASSRLS login accepted"
    );
    sql(admin, "ALTER ROLE fvoci_app NOBYPASSRLS").await?;
    println!(
        "PASS cloud-rebuild verify refusals: checksum mismatch, Data API grant, disabled RLS, privileged app login"
    );
    Ok(())
}

pub fn external(root: &Path, host_port: &str) -> Result<()> {
    let (host, port) = host_port
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("invalid fixture endpoint"))?;
    ensure!(
        (host == "127.0.0.1" || host == "postgres") && port.parse::<u16>().is_ok_and(|p| p > 0),
        "cloud fixture accepts only the isolated local/CI PostgreSQL endpoint"
    );
    let sh = xshell::Shell::new()?;
    sh.change_dir(root);
    cmd!(sh, "cargo build --locked --package fvoci-server").run()?;
    let urls = DbUrls::at(host_port);
    let target = Target {
        admin: urls
            .admin
            .parse::<PgConnectOptions>()?
            .disable_statement_logging(),
        migrator: urls
            .migration
            .parse::<PgConnectOptions>()?
            .disable_statement_logging(),
        app: urls
            .app
            .parse::<PgConnectOptions>()?
            .disable_statement_logging(),
        migrator_password: compose::TEST_MIGRATOR_PASSWORD.to_owned(),
        app_password: compose::TEST_APP_PASSWORD.to_owned(),
        binding: "isolated-compose-fixture".to_owned(),
        certificate: None,
    };
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(cases(root, &target))
}

pub fn local(root: &Path) -> Result<()> {
    let compose = Compose::new(root, Env::Test);
    let _lock = compose.lock_run()?;
    compose::watch_interrupts()?;
    let sh = compose.shell()?;
    let result = compose.up_services(&sh, &["postgres"]).and_then(|()| {
        let port = compose.host_port(&sh, "postgres", 5432)?;
        external(root, &format!("127.0.0.1:{port}"))
    });
    let cleanup = compose.down(&sh);
    result?;
    cleanup
}
