//! Deterministic competing DDL on a second connection to the isolated fixture.

use super::{Inventory, Target};
use anyhow::{Result, ensure};
use sqlx::{Connection, PgConnection};

pub(super) async fn run(target: &Target) -> Result<()> {
    let mut owner = PgConnection::connect_with(&target.admin).await?;
    let mut peer = PgConnection::connect_with(&target.admin).await?;
    for statement in [
        "CREATE OR REPLACE FUNCTION fvoci.legacy_function() RETURNS int LANGUAGE sql AS 'SELECT 2'",
        "ALTER TYPE fvoci.legacy_state ADD VALUE 'concurrent'",
        "COMMENT ON FUNCTION fvoci.legacy_function() IS 'concurrent definition'",
        "GRANT USAGE ON SCHEMA fvoci TO other_app",
    ] {
        let approved = Inventory::read(&mut owner, &target.binding).await?;
        ensure!(!approved.blocked, "race seed is blocked");
        let changed = async {
            sqlx::raw_sql(statement).execute(&mut peer).await?;
            Ok(())
        };
        ensure!(
            approved
                .delete_schema_after_locks(&mut owner, &target.binding, &approved.digest, changed)
                .await
                .is_err(),
            "concurrent DDL was accepted: {statement}"
        );
        let preserved: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT FROM fvoci.legacy WHERE secret='synthetic old row')
            AND to_regprocedure('fvoci.legacy_function()') IS NOT NULL
            AND to_regtype('fvoci.legacy_state') IS NOT NULL
            AND (SELECT count(*) FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))=2",
        )
        .fetch_one(&mut owner)
        .await?;
        ensure!(
            preserved,
            "concurrent DDL refusal did not roll back deletion"
        );
    }
    sqlx::query("REVOKE USAGE ON SCHEMA fvoci FROM other_app")
        .execute(&mut owner)
        .await?;
    owner.close().await?;
    peer.close().await?;
    println!(
        "PASS cloud-rebuild concurrent DDL: second-connection function/enum/comment/schema ACL changes after table locks are refused; deletion transaction rolls back"
    );
    Ok(())
}
