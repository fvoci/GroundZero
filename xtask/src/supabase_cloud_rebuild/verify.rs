//! Source-embedded SQLx SHA-384 checksums and an actual app login. Synthetic writes always roll back.

use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha384};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

use super::target::Target;

static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("../crates/db/migrations");

pub(super) fn source_fingerprint() -> String {
    super::fingerprint(&json!(
        MIGRATIONS
            .iter()
            .map(|m| (m.version, m.checksum.as_ref()))
            .collect::<Vec<_>>()
    ))
}

pub(super) async fn checksums(conn: &mut PgConnection) -> Result<()> {
    let rows: Vec<(i64, bool, Vec<u8>)> = sqlx::query_as(
        "SELECT version, success, checksum FROM fvoci._sqlx_migrations ORDER BY version",
    )
    .fetch_all(conn)
    .await?;
    let expected: Vec<_> = MIGRATIONS.iter().collect();
    ensure!(
        expected.len() == 10 && rows.len() == 10,
        "history_count_mismatch"
    );
    for ((version, success, checksum), migration) in rows.iter().zip(expected) {
        let source_checksum = Sha384::digest(migration.sql.as_bytes());
        ensure!(
            *success
                && *version == migration.version
                && checksum.as_slice() == &source_checksum[..]
                && checksum.as_slice() == migration.checksum.as_ref(),
            "history_checksum_mismatch"
        );
    }
    Ok(())
}

async fn data_api_privileges(conn: &mut PgConnection) -> Result<()> {
    // Effective privileges include PUBLIC and inherited grants; ownership grants also count.
    let leaks: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_roles r
        WHERE r.rolname IN ('anon','authenticated','service_role','authenticator') AND (
          has_schema_privilege(r.oid, 'fvoci', 'USAGE,CREATE')
          OR pg_has_role(r.oid, 'fvoci_app', 'MEMBER') OR pg_has_role(r.oid, 'fvoci_migrator', 'MEMBER')
          OR EXISTS (SELECT FROM pg_class c WHERE c.relnamespace = 'fvoci'::regnamespace
              AND has_table_privilege(r.oid, c.oid, 'SELECT,INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER'))
          OR EXISTS (SELECT FROM pg_proc p WHERE p.pronamespace = 'fvoci'::regnamespace AND p.prosecdef
              AND has_function_privilege(r.oid,p.oid,'EXECUTE'))
          OR EXISTS (SELECT FROM pg_proc p CROSS JOIN LATERAL aclexplode(p.proacl) a
              JOIN pg_roles granted ON granted.oid=a.grantee
              WHERE p.pronamespace = 'fvoci'::regnamespace AND a.privilege_type='EXECUTE'
              AND (a.grantee=r.oid OR pg_has_role(r.oid,a.grantee,'MEMBER')))
          )")
        .fetch_one(conn).await?;
    ensure!(leaks == 0, "data_api_privilege_leak");
    Ok(())
}

fn ids() -> Vec<Uuid> {
    (0..16).map(|_| Uuid::now_v7()).collect()
}

async fn user_context(conn: &mut PgConnection, user: Uuid) -> Result<()> {
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(user.to_string())
        .execute(conn)
        .await?;
    Ok(())
}

async fn document(conn: &mut PgConnection, workspace: Uuid) -> Result<Uuid> {
    Ok(sqlx::query_scalar(
        "SELECT fvoci.app_create_wiki_document($1,$2,'cloud synthetic',1,'{}',NULL,10,$3)",
    )
    .bind(workspace)
    .bind(Uuid::now_v7())
    .bind(ids())
    .fetch_one(conn)
    .await?)
}

async fn visible(conn: &mut PgConnection, document: Uuid) -> Result<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM fvoci.documents WHERE id=$1")
            .bind(document)
            .fetch_one(conn)
            .await?,
    )
}

async fn synthetic(conn: &mut PgConnection) -> Result<()> {
    let mut tx = conn.begin().await?;
    let result = synthetic_steps(&mut tx).await;
    tx.rollback().await?;
    result
}

async fn synthetic_steps(conn: &mut PgConnection) -> Result<()> {
    let mut users = Vec::new();
    for _ in 0..2 {
        let user: Uuid = sqlx::query_scalar(
            "SELECT fvoci.app_upsert_user('https://cloud-fixture.invalid',$1,
            'synthetic@cloud-fixture.invalid',true,'synthetic',$2)",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(ids())
        .fetch_one(&mut *conn)
        .await?;
        users.push(user);
    }
    // Only a fresh reinstall is accepted. No existing initial-admin or user data is overwritten.
    let first: Option<Uuid> =
        sqlx::query_scalar("SELECT fvoci.app_system_claim_initial_admin($1,'cloud synthetic',$2)")
            .bind(users[0])
            .bind(ids())
            .fetch_one(&mut *conn)
            .await?;
    let first = first.ok_or_else(|| anyhow::anyhow!("verification_requires_fresh_reinstall"))?;
    user_context(conn, users[0]).await?;
    let org: Uuid =
        sqlx::query_scalar("SELECT fvoci.app_create_organization($1,'cloud synthetic',$2,$3)")
            .bind(Uuid::now_v7())
            .bind(users[1])
            .bind(ids())
            .fetch_one(&mut *conn)
            .await?;
    let doc_a = document(conn, first).await?;
    ensure!(visible(conn, doc_a).await? == 1, "app_read_failed");
    let updated =
        sqlx::query("UPDATE fvoci.documents SET title='cloud synthetic updated', revision=revision+1 WHERE id=$1")
            .bind(doc_a)
            .execute(&mut *conn)
            .await?
            .rows_affected();
    ensure!(updated == 1, "app_update_failed");
    user_context(conn, users[1]).await?;
    let second: Uuid =
        sqlx::query_scalar("SELECT fvoci.app_create_workspace($1,$2,'cloud synthetic',$3)")
            .bind(Uuid::now_v7())
            .bind(org)
            .bind(ids())
            .fetch_one(&mut *conn)
            .await?;
    let doc_b = document(conn, second).await?;
    ensure!(
        visible(conn, doc_a).await? == 0 && visible(conn, doc_b).await? == 1,
        "rls_workspace_isolation_failed"
    );
    ensure!(
        sqlx::query("UPDATE fvoci.documents SET title='denied' WHERE id=$1")
            .bind(doc_a)
            .execute(&mut *conn)
            .await?
            .rows_affected()
            == 0,
        "rls_update_not_denied"
    );
    let mut denied = conn.begin().await?;
    let error = sqlx::query("SELECT fvoci.app_trash_document($1,$2)")
        .bind(doc_a)
        .bind(ids())
        .execute(&mut *denied)
        .await
        .err()
        .ok_or_else(|| anyhow::anyhow!("cross_workspace_delete_not_denied"))?;
    ensure!(
        error
            .as_database_error()
            .and_then(|e| e.code())
            .is_some_and(|c| c == "42501" || c == "P0002"),
        "unexpected_denial"
    );
    denied.rollback().await?;
    user_context(conn, users[0]).await?;
    ensure!(
        visible(conn, doc_b).await? == 0 && visible(conn, doc_a).await? == 1,
        "reused_connection_context_failed"
    );
    sqlx::query("SELECT fvoci.app_trash_document($1,$2)")
        .bind(doc_a)
        .bind(ids())
        .execute(&mut *conn)
        .await?;
    check_deleted_and_history_denied(conn, doc_a).await
}

async fn check_deleted_and_history_denied(conn: &mut PgConnection, doc_a: Uuid) -> Result<()> {
    ensure!(visible(conn, doc_a).await? == 0, "app_delete_failed");
    let mut denied = conn.begin().await?;
    let result = sqlx::query("SELECT * FROM fvoci._sqlx_migrations")
        .execute(&mut *denied)
        .await;
    ensure!(
        result.is_err_and(
            |e| e.as_database_error().and_then(|d| d.code()).as_deref() == Some("42501")
        ),
        "app_history_access_not_denied"
    );
    denied.rollback().await?;
    Ok(())
}

pub(super) async fn run(target: &Target, proof: &super::receipt::Proof) -> Result<Value> {
    let mut migrator = PgConnection::connect_with(&target.migrator).await?;
    super::receipt::check(&mut migrator, proof).await?;
    checksums(&mut migrator).await?;
    data_api_privileges(&mut migrator).await?;
    migrator.close().await?;
    let mut app = PgConnection::connect_with(&target.app).await?;
    let valid: bool = sqlx::query_scalar("SELECT current_user = 'fvoci_app' AND session_user = 'fvoci_app'
        AND NOT rolsuper AND NOT rolbypassrls AND rolcanlogin FROM pg_roles WHERE rolname=current_user")
        .fetch_one(&mut app).await?;
    ensure!(valid, "invalid_app_role_login");
    synthetic(&mut app).await?;
    let context: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.user_id',true),'')")
            .fetch_one(&mut app)
            .await?;
    ensure!(context.is_none(), "transaction_context_leaked");
    app.close().await?;
    Ok(
        json!({"phase": "verify", "status": "PASS", "checksums": "10/10", "app_login": "PASS",
        "crud": "PASS", "rls": "PASS", "denied": "PASS", "context_reuse": "PASS",
        "data_api_roles": "PASS", "synthetic_cleanup": "ROLLBACK", "rebuild_proof":"PASS",
        "source_sha":proof.source_sha,"approved_fingerprint":proof.fingerprint}),
    )
}
