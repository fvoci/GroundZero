//! Global consumers in a fresh isolated cluster, including managed-trigger identity impostors.

use super::{Inventory, Target};
use anyhow::Result;
use sqlx::{Connection, PgConnection};

async fn sql(conn: &mut PgConnection, text: &str) -> Result<()> {
    sqlx::raw_sql(text).execute(conn).await?;
    Ok(())
}

pub(super) async fn run(target: &Target) -> Result<Vec<String>> {
    let mut conn = PgConnection::connect_with(&target.admin).await?;
    let mut failures = Vec::new();
    for definition in [
        "FOR ALL TABLES",
        "FOR TABLE fvoci.legacy",
        "FOR TABLES IN SCHEMA fvoci",
        "FOR TABLE other_app.preserved",
    ] {
        let before = Inventory::read(&mut conn, &target.binding).await?;
        sql(
            &mut conn,
            &format!("CREATE PUBLICATION inspection_publication {definition}"),
        )
        .await?;
        let after = Inventory::read(&mut conn, &target.binding).await?;
        let blocked = definition != "FOR TABLE other_app.preserved";
        if after.blocked != blocked
            || after.digest == before.digest
            || !after.report.to_string().contains("inspection_publication")
        {
            failures.push(format!(
                "publication omitted or misclassified: {definition}"
            ));
        }
        sql(&mut conn, "DROP PUBLICATION inspection_publication").await?;
    }
    sql(&mut conn,"CREATE SCHEMA extensions; CREATE FUNCTION extensions.inspection_handler() RETURNS event_trigger LANGUAGE plpgsql AS 'BEGIN RETURN; END';
        CREATE EVENT TRIGGER inspection_event ON ddl_command_end EXECUTE FUNCTION extensions.inspection_handler()").await?;
    let active = Inventory::read(&mut conn, &target.binding).await?;
    if !active.blocked || !active.report.to_string().contains("inspection_event") {
        failures.push("unrecognized event trigger accepted or hidden".to_owned());
    }
    sql(&mut conn, "ALTER EVENT TRIGGER inspection_event DISABLE").await?;
    let disabled = Inventory::read(&mut conn, &target.binding).await?;
    if disabled.blocked || disabled.digest == active.digest {
        failures.push("disabled trigger state omitted or blocked".to_owned());
    }
    sql(&mut conn,"DROP EVENT TRIGGER inspection_event; DROP FUNCTION extensions.inspection_handler() RESTRICT;
        CREATE ROLE supabase_admin SUPERUSER; CREATE FUNCTION extensions.pgrst_ddl_watch() RETURNS event_trigger LANGUAGE plpgsql AS 'BEGIN RETURN; END';
        ALTER FUNCTION extensions.pgrst_ddl_watch() OWNER TO supabase_admin;
        CREATE EVENT TRIGGER pgrst_ddl_watch ON ddl_command_end EXECUTE FUNCTION extensions.pgrst_ddl_watch();
        ALTER EVENT TRIGGER pgrst_ddl_watch OWNER TO supabase_admin").await?;
    let managed = Inventory::read(&mut conn, &target.binding).await?;
    if managed.blocked || !managed.report.to_string().contains("pgrst_ddl_watch") {
        failures.push("managed PostgREST trigger blocked or hidden".to_owned());
    }
    sql(&mut conn,"CREATE OR REPLACE FUNCTION extensions.pgrst_ddl_watch() RETURNS event_trigger LANGUAGE plpgsql AS 'BEGIN PERFORM 1; RETURN; END'").await?;
    let changed = Inventory::read(&mut conn, &target.binding).await?;
    if changed.digest == managed.digest {
        failures.push("external event-handler definition change omitted".to_owned());
    }
    sql(
        &mut conn,
        "ALTER FUNCTION extensions.pgrst_ddl_watch() OWNER TO other_app",
    )
    .await?;
    if !Inventory::read(&mut conn, &target.binding).await?.blocked {
        failures.push("managed name with untrusted handler owner accepted".to_owned());
    }
    sql(&mut conn,"DROP EVENT TRIGGER pgrst_ddl_watch; DROP FUNCTION extensions.pgrst_ddl_watch() RESTRICT; DROP SCHEMA extensions RESTRICT; DROP ROLE supabase_admin").await?;
    conn.close().await?;
    Ok(failures)
}
