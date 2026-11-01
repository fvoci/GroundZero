//! Real catalog changes in the isolated fixture; no Cloud connection or catalog writes.

use anyhow::{Result, ensure};
use serde_json::Value;
use sqlx::PgConnection;

use super::{Inventory, approve, catalog};

async fn sql(conn: &mut PgConnection, statement: &str) -> Result<()> {
    sqlx::raw_sql(statement).execute(conn).await?;
    Ok(())
}

async fn relation(conn: &mut PgConnection) -> Result<Value> {
    Ok(sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM pg_class c WHERE oid='fvoci.maintenance_probe'::regclass",
    )
    .fetch_one(conn)
    .await?)
}

async fn maintenance(conn: &mut PgConnection, binding: &str) -> Result<()> {
    sql(
        conn,
        "CREATE TABLE fvoci.maintenance_probe(id int);
        INSERT INTO fvoci.maintenance_probe SELECT generate_series(1,32);
        CREATE INDEX maintenance_probe_index ON fvoci.maintenance_probe(id);
        DROP INDEX fvoci.maintenance_probe_index RESTRICT;
        INSERT INTO fvoci.maintenance_probe SELECT generate_series(33,96)",
    )
    .await?;
    let before = Inventory::read(conn, binding).await?;
    let before_relation = relation(conn).await?;
    sql(conn, "ANALYZE fvoci.maintenance_probe").await?;
    let analyzed = Inventory::read(conn, binding).await?;
    let analyzed_relation = relation(conn).await?;
    sql(conn, "VACUUM (FREEZE, ANALYZE) fvoci.maintenance_probe").await?;
    let vacuumed = Inventory::read(conn, binding).await?;
    let vacuumed_relation = relation(conn).await?;
    sql(
        conn,
        "CREATE INDEX maintenance_probe_index ON fvoci.maintenance_probe(id)",
    )
    .await?;
    let indexed = Inventory::read(conn, binding).await?;
    sql(conn, "DROP INDEX fvoci.maintenance_probe_index RESTRICT").await?;
    sql(conn, "DROP TABLE fvoci.maintenance_probe RESTRICT").await?;
    ensure!(
        before_relation["reltuples"] != analyzed_relation["reltuples"],
        "ANALYZE did not change test statistics"
    );
    ensure!(
        before_relation["relhasindex"] == true && vacuumed_relation["relhasindex"] == false,
        "VACUUM did not clear the stale index flag"
    );
    ensure!(
        before.digest == analyzed.digest,
        "ANALYZE changed the approval fingerprint"
    );
    ensure!(
        analyzed.digest == vacuumed.digest,
        "VACUUM FREEZE changed the approval fingerprint"
    );
    ensure!(
        vacuumed.digest != indexed.digest,
        "index creation reused the approved fingerprint"
    );
    Ok(())
}

async fn columns_present(conn: &mut PgConnection, _binding: &str) -> Result<()> {
    let raw = catalog::snapshot(conn).await?;
    let columns = raw["objects"]["pg_attribute"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("missing columns"))?;
    ensure!(
        columns.iter().any(|c| c["attname"] == "secret")
            && columns.iter().any(|c| c["attname"] == "id"),
        "column definitions are absent from the inventory"
    );
    Ok(())
}

async fn rename(conn: &mut PgConnection, binding: &str) -> Result<()> {
    let before = Inventory::read(conn, binding).await?;
    sql(
        conn,
        "ALTER TABLE fvoci.legacy RENAME COLUMN secret TO changed_secret",
    )
    .await?;
    let after = Inventory::read(conn, binding).await?;
    let refused = approve(&after.digest, &before.digest, after.blocked).is_err();
    sql(
        conn,
        "ALTER TABLE fvoci.legacy RENAME COLUMN changed_secret TO secret",
    )
    .await?;
    ensure!(
        before.digest != after.digest && refused,
        "column rename reused the approved fingerprint"
    );
    Ok(())
}

async fn column_type(conn: &mut PgConnection, binding: &str) -> Result<()> {
    let before = Inventory::read(conn, binding).await?;
    sql(
        conn,
        "ALTER TABLE fvoci.legacy ALTER COLUMN secret TYPE varchar",
    )
    .await?;
    let after = Inventory::read(conn, binding).await?;
    sql(
        conn,
        "ALTER TABLE fvoci.legacy ALTER COLUMN secret TYPE text",
    )
    .await?;
    ensure!(
        before.digest != after.digest,
        "column type change reused the approved fingerprint"
    );
    Ok(())
}

async fn type_modifier(conn: &mut PgConnection, binding: &str) -> Result<()> {
    sql(
        conn,
        "ALTER TABLE fvoci.legacy ALTER COLUMN secret TYPE varchar(80)",
    )
    .await?;
    let before = Inventory::read(conn, binding).await?;
    sql(
        conn,
        "ALTER TABLE fvoci.legacy ALTER COLUMN secret TYPE varchar(160)",
    )
    .await?;
    let after = Inventory::read(conn, binding).await?;
    sql(
        conn,
        "ALTER TABLE fvoci.legacy ALTER COLUMN secret TYPE text",
    )
    .await?;
    ensure!(
        before.digest != after.digest,
        "column type modifier reused the approved fingerprint"
    );
    Ok(())
}

pub(super) async fn run(conn: &mut PgConnection, binding: &str) -> Result<()> {
    let results = [
        maintenance(conn, binding).await,
        columns_present(conn, binding).await,
        rename(conn, binding).await,
        column_type(conn, binding).await,
        type_modifier(conn, binding).await,
    ];
    let failures: Vec<_> = results
        .into_iter()
        .filter_map(Result::err)
        .map(|e| e.to_string())
        .collect();
    ensure!(
        failures.is_empty(),
        "catalog regression failures: {}",
        failures.join("; ")
    );
    println!(
        "PASS cloud-rebuild catalog: maintenance preserves approval; index and column presence/name/type/modifier changes are detected"
    );
    Ok(())
}
