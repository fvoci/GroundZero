//! Deterministic competing DDL on a second connection to the isolated fixture.

use super::{Inventory, Target};
use anyhow::{Result, ensure};
use sqlx::{Connection, PgConnection};

pub(super) async fn run(target: &Target) -> Result<()> {
    during_scan(target).await?;
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

async fn during_scan(target: &Target) -> Result<()> {
    let mut owner = PgConnection::connect_with(&target.admin).await?;
    let mut peer = PgConnection::connect_with(&target.admin).await?;
    let approved = Inventory::read(&mut owner, &target.binding).await?;
    let comment: Option<String> =
        sqlx::query_scalar("SELECT obj_description('fvoci'::regnamespace,'pg_namespace')")
            .fetch_one(&mut owner)
            .await?;
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut owner)
        .await?;
    sqlx::query("COMMENT ON SCHEMA fvoci IS 'unapproved scan-time drift'")
        .execute(&mut owner)
        .await?;
    const GATE: i64 = 20261101;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(GATE)
        .execute(&mut peer)
        .await?;
    let inject = async {
        let result:Result<()>=async {
            tokio::time::timeout(std::time::Duration::from_secs(5),async {
                loop {
                    let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT FROM pg_locks WHERE pid=$1 AND locktype='advisory' AND NOT granted)").bind(pid).fetch_one(&mut peer).await?;
                    if waiting {break;}
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                anyhow::Ok(())
            }).await??;
            let mut tx=peer.begin().await?;
            sqlx::query("CREATE OR REPLACE FUNCTION fvoci.legacy_function() RETURNS int LANGUAGE sql AS 'SELECT 3'").execute(&mut *tx).await?;
            let restore:String=sqlx::query_scalar("SELECT format('COMMENT ON SCHEMA fvoci IS %L',$1::text)").bind(comment).fetch_one(&mut *tx).await?;
            sqlx::query(&restore).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(())
        }.await;
        sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(GATE)
            .execute(&mut peer)
            .await?;
        result
    };
    let refused = approved
        .delete_schema_with_probe(&mut owner, &target.binding, &approved.digest, GATE, inject)
        .await;
    ensure!(
        refused.is_err(),
        "split catalog snapshots accepted scan-time drift and committed deletion"
    );
    let changed: i32 = sqlx::query_scalar("SELECT fvoci.legacy_function()")
        .fetch_one(&mut owner)
        .await?;
    ensure!(changed == 3, "scan-time injection did not execute");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM fvoci.legacy")
        .fetch_one(&mut owner)
        .await?;
    ensure!(rows == 1, "scan-time refusal did not roll back deletion");
    owner.close().await?;
    peer.close().await?;
    println!(
        "PASS cloud-rebuild scan-time race: inconsistent intermediate state cannot assemble the approved fingerprint"
    );
    Ok(())
}
