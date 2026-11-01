//! DDL committed after the approved recheck must not escape the DROP transaction's checks.

use super::{Inventory, Target};
use anyhow::{Result, ensure};
use sqlx::{Connection, PgConnection};

pub(super) async fn run(target: &Target) -> Result<()> {
    let mut owner = PgConnection::connect_with(&target.admin).await?;
    let mut peer = PgConnection::connect_with(&target.admin).await?;
    let mut failures = Vec::new();
    for (setup, injection, cleanup, expected) in [
        ("", "ALTER TYPE fvoci.legacy_state ADD VALUE 'after_recheck'", "", "unapproved_object_deleted"),
        ("CREATE FUNCTION other_app.replacement() RETURNS int LANGUAGE sql AS 'SELECT 99'; ALTER FUNCTION other_app.replacement() OWNER TO other_app",
         "ALTER FUNCTION fvoci.legacy_function() RENAME TO displaced; ALTER FUNCTION fvoci.displaced() SET SCHEMA other_app;
          ALTER FUNCTION other_app.replacement() SET SCHEMA fvoci; ALTER FUNCTION fvoci.replacement() RENAME TO legacy_function;
          ALTER FUNCTION fvoci.legacy_function() OWNER TO fvoci_migrator",
         "DROP FUNCTION fvoci.legacy_function() RESTRICT; ALTER FUNCTION other_app.displaced() SET SCHEMA fvoci; ALTER FUNCTION fvoci.displaced() RENAME TO legacy_function",
         "drop_targets_remain"),
    ] {
        sqlx::raw_sql(setup).execute(&mut owner).await?;
        let approved = Inventory::read(&mut owner, &target.binding).await?;
        ensure!(!approved.blocked, "post-drop race seed blocked");
        // Keep a failing implementation from destroying the next test's fixture.
        let mut envelope = owner.begin().await?;
        let result = approved.delete_schema_after_recheck(
            &mut envelope, &target.binding, &approved.digest,
            async {sqlx::raw_sql(injection).execute(&mut peer).await?; Ok(())},
        ).await;
        let error = result.err().map(|e| e.to_string());
        let schema_preserved: bool = sqlx::query_scalar(
            "SELECT to_regclass('fvoci.legacy') IS NOT NULL AND to_regtype('fvoci.legacy_state') IS NOT NULL
             AND to_regprocedure('fvoci.legacy_function()') IS NOT NULL",
        ).fetch_one(&mut *envelope).await?;
        let rows = if schema_preserved {
            sqlx::query_scalar::<_,i64>("SELECT count(*) FROM fvoci.legacy WHERE secret='synthetic old row'")
                .fetch_one(&mut *envelope).await?
        } else {0};
        let outside: i64 = sqlx::query_scalar("SELECT count(*) FROM other_app.preserved")
            .fetch_one(&mut *envelope).await?;
        let roles: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator')")
            .fetch_one(&mut *envelope).await?;
        let payload: bool = sqlx::query_scalar(if expected == "unapproved_object_deleted" {
            "SELECT EXISTS(SELECT FROM pg_enum WHERE enumtypid=to_regtype('fvoci.legacy_state') AND enumlabel='after_recheck')"
        } else {
            "SELECT to_regprocedure('other_app.displaced()') IS NOT NULL"
        }).fetch_one(&mut *envelope).await?;
        // Check rollback before this outer rollback, which only preserves the test envelope.
        let refused = error.as_deref() == Some(expected) && schema_preserved && rows == 1 && outside == 1 && roles == 2 && payload;
        envelope.rollback().await?;
        sqlx::raw_sql(cleanup).execute(&mut owner).await?;
        if !refused {
            failures.push(format!("{expected}: error={error:?}, schema_preserved={schema_preserved}, rows={rows}, outside={outside}, roles={roles}, payload={payload}"));
        }
    }
    owner.close().await?;
    peer.close().await?;
    ensure!(
        failures.is_empty(),
        "post-drop race checks failed: {}",
        failures.join("; ")
    );
    println!(
        "PASS cloud-rebuild post-drop races: late enum value and replaced function OID refused, DROP rolled back, product/outside rows and roles preserved"
    );
    Ok(())
}
