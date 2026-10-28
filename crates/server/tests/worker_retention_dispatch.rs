//! Global retention jobs run in a separate executable from resource HTTP tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
mod common;
mod sql_support;

use fvoci_core::error::AppError;
use fvoci_server::worker::{Job, run_jobs};
use sql_support::{World, rows};

fn retention_jobs() -> [Job; 2] {
    // Equivalent SQL with trailing whitespace must not lose its retention parameter.
    [
        Job {
            ids: None,
            call: "SELECT app_system_purge_audit($1)\n",
        },
        Job {
            ids: None,
            call: "SELECT app_system_purge_activity($1)\n",
        },
    ]
}

fn statement(call: &'static str) -> Job {
    Job { ids: None, call }
}

async fn seed_old_records(w: &World) -> (i64, i64) {
    w.task().await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_workspace_limits($1,'{\"audit_retention_days\":7,\"activity_retention_days\":7}',@ids)",
        &[w.ws],
    )
    .await;
    let mut conn = common::fixture_conn().await;
    for table in ["audit_log", "activity"] {
        sqlx::query(&format!(
            "UPDATE fvoci.{table} SET created_at=now()-interval '40 days' WHERE workspace_id=$1"
        ))
        .bind(w.ws)
        .execute(&mut conn)
        .await
        .unwrap();
    }
    counts(w).await
}

async fn counts(w: &World) -> (i64, i64) {
    let mut conn = common::fixture_conn().await;
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM fvoci.audit_log WHERE workspace_id=$1),
                (SELECT count(*) FROM fvoci.activity WHERE workspace_id=$1)",
    )
    .bind(w.ws)
    .fetch_one(&mut conn)
    .await
    .unwrap()
}

async fn inject_one_shortage(w: &World) -> String {
    let name = format!("retention_retry_{}", uuid::Uuid::now_v7().simple());
    let ws = w.ws;
    let mut conn = common::fixture_conn().await;
    // Sequences survive rollback. Audit's count proves the earlier domain ran again,
    // while the activity trigger injects FV001 only on the first attempt.
    sqlx::raw_sql(&format!(
        "CREATE SEQUENCE fvoci.{name}_audit;
         CREATE SEQUENCE fvoci.{name}_activity;
         CREATE FUNCTION fvoci.{name}() RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER
         SET search_path TO fvoci,pg_temp AS $body$
         BEGIN IF OLD.workspace_id='{ws}'::uuid THEN
           IF TG_TABLE_NAME='audit_log' THEN
             PERFORM nextval('fvoci.{name}_audit');
           ELSIF nextval('fvoci.{name}_activity')=1 THEN
             RAISE EXCEPTION 'retention retry injection' USING ERRCODE='FV001';
           END IF;
         END IF; RETURN OLD; END $body$;
         CREATE TRIGGER {name} BEFORE DELETE ON fvoci.audit_log
           FOR EACH ROW EXECUTE FUNCTION fvoci.{name}();
         CREATE TRIGGER {name} BEFORE DELETE ON fvoci.activity
           FOR EACH ROW EXECUTE FUNCTION fvoci.{name}();"
    ))
    .execute(&mut conn)
    .await
    .unwrap();
    name
}

async fn remove_injection(name: &str) -> (i64, i64) {
    let mut conn = common::fixture_conn().await;
    let attempts = sqlx::query_as(&format!(
        "SELECT (SELECT last_value FROM fvoci.{name}_audit),
                (SELECT last_value FROM fvoci.{name}_activity)"
    ))
    .fetch_one(&mut conn)
    .await
    .unwrap();
    sqlx::raw_sql(&format!(
        "DROP TRIGGER {name} ON fvoci.audit_log;
         DROP TRIGGER {name} ON fvoci.activity;
         DROP FUNCTION fvoci.{name}();
         DROP SEQUENCE fvoci.{name}_audit, fvoci.{name}_activity;"
    ))
    .execute(&mut conn)
    .await
    .unwrap();
    attempts
}

#[tokio::test]
async fn retention_dispatch_binds_parameters_rolls_back_and_retries_the_entire_batch() {
    let w = World::new().await;
    let before = seed_old_records(&w).await;
    assert!(before.0 > 0 && before.1 > 0);
    let [audit, activity] = retention_jobs();

    let failure = run_jobs(&w.state.db, &[audit, activity, statement("SELECT 1/0")])
        .await
        .unwrap_err();
    assert!(
        matches!(&failure, AppError::Database(e)
            if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("22012")),
        "both retention jobs must bind their parameter before the final failure: {failure:?}"
    );
    assert_eq!(counts(&w).await, before, "earlier jobs must roll back");

    let injection = inject_one_shortage(&w).await;
    let result = run_jobs(&w.state.db, &[audit, activity]).await;
    let attempts = remove_injection(&injection).await;
    assert_eq!(result.unwrap(), (before.0 + before.1) as i32);
    assert_eq!(counts(&w).await, (0, 0));
    assert_eq!(attempts, (2 * before.0, 1 + before.1));
}
