#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

// run_all_now scans all due work. Serialize this binary's global worker ticks so one test cannot
// finish another's failed series after its trigger has been removed. Assertions remain ID-scoped.
static WORKER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

// nextval survives a failed subtransaction: is_called proves the trigger actually fired.
async fn install_failure(table: &str, operation: &str, predicate: &str, code: &str) -> String {
    let name = format!("worker_failure_{}", Uuid::now_v7().simple());
    let row = if operation == "DELETE" { "OLD" } else { "NEW" };
    let mut conn = common::fixture_conn().await;
    sqlx::raw_sql(&format!(
        "CREATE SEQUENCE fvoci.{name};
         GRANT SELECT ON SEQUENCE fvoci.{name} TO fvoci_app;
         CREATE FUNCTION fvoci.{name}() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER
         SET search_path TO fvoci,pg_temp AS $body$
         BEGIN IF {predicate} THEN
           PERFORM nextval('fvoci.{name}');
           RAISE EXCEPTION 'worker isolation injection' USING ERRCODE='{code}';
         END IF; RETURN {row}; END $body$;
         CREATE TRIGGER {name} BEFORE {operation} ON fvoci.{table}
         FOR EACH ROW EXECUTE FUNCTION fvoci.{name}();"
    ))
    .execute(&mut conn)
    .await
    .unwrap();
    name
}

async fn remove_failure(w: &World, table: &str, name: &str) -> bool {
    let fired = one(
        &w.state,
        &w.owner,
        &format!("SELECT is_called FROM fvoci.{name}"),
        &[],
    )
    .await;
    let mut conn = common::fixture_conn().await;
    sqlx::raw_sql(&format!("DROP TRIGGER {name} ON fvoci.{table}; DROP FUNCTION fvoci.{name}(); DROP SEQUENCE fvoci.{name};"))
        .execute(&mut conn).await.unwrap();
    fired["is_called"] == true
}

async fn series_failure(code: &str) {
    let w = World::new().await;
    let mut series = Vec::new();
    for _ in 0..3 {
        let root = w.task().await;
        fixture(
            "UPDATE fvoci.tasks SET due_date=current_date-2 WHERE id=$1",
            &[root],
        )
        .await;
        let row = one(&w.state, &w.owner,
            "SELECT * FROM fvoci.app_set_task_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[]}','UTC',NULL,@ids)", &[root]).await;
        series.push((
            Uuid::parse_str(row["series_id"].as_str().unwrap()).unwrap(),
            root,
        ));
    }
    let injection = install_failure(
        "tasks",
        "INSERT",
        &format!(
            "NEW.series_id='{}'::uuid AND NEW.occurrence_date=current_date",
            series[1].0
        ),
        code,
    )
    .await;
    // Exercise the production transaction/continue policy, never reproduce the worker loop here.
    let result = fvoci_server::worker::run_all_now(&w.state.db, None, None).await;
    let fired = remove_failure(&w, "tasks", &injection).await;
    result.unwrap();
    assert!(fired);
    for (index, (series, root)) in series.iter().enumerate() {
        let generated = rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.tasks WHERE series_id=$1 AND id<>$2",
            &[*series, *root],
        )
        .await;
        assert_eq!(generated.len(), if index == 1 { 0 } else { 3 });
    }
}

#[tokio::test]
async fn worker_continues_after_a_series_failure() {
    let _guard = WORKER.lock().await;
    series_failure("ZY085").await;
}

#[tokio::test]
async fn worker_continues_after_simulated_lock_timeout() {
    let _guard = WORKER.lock().await;
    // The production connection has no lock_timeout. Holding its first lane advisory lock would
    // wait indefinitely, so the trigger simulates PostgreSQL lock_timeout's SQLSTATE 55P03.
    series_failure("55P03").await;
}

#[tokio::test]
async fn worker_purge_rolls_back_one_batch_and_commits_the_other() {
    let _guard = WORKER.lock().await;
    let w = World::new().await;
    let bad = w.doc().await;
    let good = w.doc().await;
    let file = attachment(&w, bad).await;
    let mut batches = Vec::new();
    for doc in [bad, good] {
        batches.push(
            made(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_trash_document($1,@ids) AS id",
                &[doc],
            )
            .await,
        );
    }
    let injection = install_failure(
        "documents",
        "DELETE",
        &format!("OLD.id='{bad}'::uuid"),
        "ZY085",
    )
    .await;
    fixture(
        "UPDATE fvoci.trash_batches SET deleted_at=now()-interval '40 days' WHERE id IN ($1,$2)",
        &batches,
    )
    .await;
    let result = fvoci_server::worker::run_all_now(&w.state.db, None, None).await;
    let fired = remove_failure(&w, "documents", &injection).await;
    result.unwrap();
    assert!(fired);
    let mut conn = common::fixture_conn().await;
    for (table, target, exists) in [
        ("documents", bad, true),
        ("files", file, true),
        ("documents", good, false),
    ] {
        let present: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM fvoci.{table} WHERE id=$1)"
        ))
        .bind(target)
        .fetch_one(&mut conn)
        .await
        .unwrap();
        assert_eq!(present, exists, "{table}/{target}");
    }
    let purged: Vec<(Uuid, bool)> =
        sqlx::query_as("SELECT id,purged_at IS NOT NULL FROM fvoci.trash_batches WHERE id=ANY($1)")
            .bind(&batches)
            .fetch_all(&mut conn)
            .await
            .unwrap();
    assert!(purged.contains(&(batches[0], false)));
    assert!(purged.contains(&(batches[1], true)));
}
