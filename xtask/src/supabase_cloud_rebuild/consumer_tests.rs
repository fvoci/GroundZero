//! Global consumers in a fresh isolated cluster, including managed-trigger identity impostors.

use super::{Inventory, Target};
use anyhow::{Result, ensure};
use sqlx::{Connection, PgConnection};

async fn sql(conn: &mut PgConnection, text: &str) -> Result<()> {
    sqlx::raw_sql(text).execute(conn).await?;
    Ok(())
}

pub(super) async fn run(target: &Target) -> Result<Vec<String>> {
    let mut conn = PgConnection::connect_with(&target.admin).await?;
    let mut failures = Vec::new();
    failures.extend(partition_publications(&mut conn, target).await?);
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

async fn partition_publications(conn: &mut PgConnection, target: &Target) -> Result<Vec<String>> {
    let mut failures = Vec::new();
    for outside_root in [true, false] {
        let (root, middle, leaf) = if outside_root {
            (
                "other_app.publication_root",
                "other_app.publication_middle",
                "fvoci.publication_leaf",
            )
        } else {
            (
                "fvoci.publication_root",
                "other_app.publication_middle",
                "other_app.publication_leaf",
            )
        };
        sql(conn,&format!("CREATE TABLE {root}(id int) PARTITION BY RANGE(id);
            CREATE TABLE {middle} PARTITION OF {root} FOR VALUES FROM(0) TO(100) PARTITION BY RANGE(id);
            CREATE TABLE {leaf} PARTITION OF {middle} FOR VALUES FROM(0) TO(10);
            INSERT INTO {root} VALUES(1)")).await?;
        let owned = if outside_root { leaf } else { root };
        sql(
            conn,
            &format!("ALTER TABLE {owned} OWNER TO fvoci_migrator"),
        )
        .await?;
        let before = Inventory::read(conn, &target.binding).await?;
        if outside_root {
            ensure!(
                !before.blocked,
                "partition publication fixture blocked before publication: {}",
                before.report["blockers"]
            );
        } else {
            ensure!(
                before.report["blockers"]["outside_automatic_objects"]
                    .as_i64()
                    .is_some_and(|n| n > 0),
                "outside child is not protected before publication"
            );
        }
        let schema = if outside_root { "other_app" } else { "fvoci" };
        for definition in [
            format!("FOR TABLE {root}"),
            format!("FOR TABLES IN SCHEMA {schema}"),
        ] {
            for via_root in [true, false] {
                sql(conn,&format!("CREATE PUBLICATION partition_publication {definition} WITH(publish_via_partition_root={via_root})")).await?;
                if outside_root && via_root {
                    let hidden:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT FROM pg_publication_tables WHERE pubname='partition_publication' AND schemaname='fvoci')").fetch_one(&mut *conn).await?;
                    ensure!(
                        hidden,
                        "via_root fixture does not hide the product descendant"
                    );
                }
                let inventory = Inventory::read(conn, &target.binding).await?;
                let publication = inventory.report["global_consumers"]["publications"]
                    .as_array()
                    .and_then(|items| items.iter().find(|p| p["name"] == "partition_publication"));
                let covered = inventory.report["blockers"]["product_publications"]
                    .as_i64()
                    .is_some_and(|n| n > 0)
                    && publication.is_some_and(|p| p["includes_product"] == true);
                // Restore the fixture even if a regression permits the nested deletion.
                let mut attempt = conn.begin().await?;
                let refused = inventory
                    .delete_schema(&mut attempt, &target.binding, &inventory.digest)
                    .await
                    .is_err();
                attempt.rollback().await?;
                if !covered || !refused {
                    failures.push(format!("partition publication missed: outside_root={outside_root}, {definition}, via_root={via_root}, product_blocker={covered}, deletion_refused={refused}"));
                }
                let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {leaf}"))
                    .fetch_one(&mut *conn)
                    .await?;
                ensure!(
                    rows == 1,
                    "partition row was lost after refused/rolled-back deletion"
                );
                sql(conn, "DROP PUBLICATION partition_publication").await?;
            }
        }
        sql(conn,&format!("DROP TABLE {leaf} RESTRICT; DROP TABLE {middle} RESTRICT; DROP TABLE {root} RESTRICT")).await?;
    }
    if failures.is_empty() {
        println!(
            "PASS cloud-rebuild partition publications: 8 direct/schema, both-direction, via_root true/false cases refused; rows preserved"
        );
    }
    Ok(failures)
}
