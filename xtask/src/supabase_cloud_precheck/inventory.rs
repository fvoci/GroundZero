//! Approved metadata inventory for the SQL baseline reset. No row contents or credentials are emitted.
//! The original main-only SELECT 1 precheck remains a separate command and workflow job.

use super::{Failure, Target, bounded};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::time::Duration;

const QUERY_LIMIT: Duration = Duration::from_secs(5);
const TOTAL_LIMIT: Duration = Duration::from_secs(60);
const CATALOG: &str = include_str!("inventory.sql");

fn project_ref(user: &str) -> Result<&str, Failure> {
    let (_, project) = user.rsplit_once('.').ok_or(Failure::InvalidTarget)?;
    if project.len() != 20
        || !project
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
    {
        return Err(Failure::InvalidTarget);
    }
    Ok(project)
}

fn empty_report() -> Value {
    json!({
        "operation":"sql_baseline_target_inventory", "connection_kind":"session_pooler",
        "connection":"NOT RUN", "tls_certificate_and_hostname":"NOT RUN",
        "inventory":"NOT RUN", "changes":"NOT RUN", "backup":"NOT VERIFIED",
        "backup_note":"Database catalog access cannot confirm Supabase Dashboard backups, retention or recoverability.",
        "sharing":"NOT DETERMINED",
        "sharing_note":"Object ownership, cross-schema dependencies and current connection counts are evidence, not proof that no other service uses the database."
    })
}

async fn query_json(conn: &mut PgConnection, query: &str) -> Result<Value, Failure> {
    bounded(QUERY_LIMIT, async {
        sqlx::query_scalar(query)
            .fetch_one(conn)
            .await
            .map_err(|_| Failure::QueryFailed)
    })
    .await
}

async fn read_inventory(conn: &mut PgConnection, report: &mut Value) -> Result<(), Failure> {
    report["phase"] = json!("begin_read_only");
    bounded(QUERY_LIMIT, async {
        sqlx::raw_sql("BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *conn)
            .await
            .map_err(|_| Failure::QueryFailed)?;
        Ok(())
    })
    .await?;
    report["phase"] = json!("catalog");
    let catalog = query_json(conn, CATALOG).await?;
    if catalog["identity"]["transaction_read_only"] != "on"
        || catalog["identity"]["transaction_isolation"] != "repeatable read"
    {
        return Err(Failure::UnexpectedQueryResult);
    }
    report["catalog"] = catalog;

    report["phase"] = json!("groundzero_counts");
    let tables: Vec<(String, String, bool, bool)> = bounded(QUERY_LIMIT, async {
        sqlx::query_as(
            "SELECT c.relname::text, pg_catalog.format('%I.%I', n.nspname, c.relname),
                    pg_catalog.has_table_privilege(c.oid, 'SELECT'), pg_catalog.row_security_active(c.oid)
             FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'fvoci' AND c.relkind IN ('r','p') AND c.relname <> '_sqlx_migrations'
             ORDER BY c.relname",
        ).fetch_all(&mut *conn).await.map_err(|_| Failure::QueryFailed)
    }).await?;
    let mut counts = Vec::new();
    for (table, quoted_name, readable, rls_filtered) in tables {
        if readable {
            let count: i64 = bounded(QUERY_LIMIT, async {
                // The catalog supplies format('%I.%I') quoted identifiers, never raw interpolated names.
                sqlx::query_scalar(&format!("SELECT count(*) FROM {quoted_name}"))
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(|_| Failure::QueryFailed)
            })
            .await?;
            counts.push(
                json!({"table":table,"count":count,"rls_filtered":rls_filtered,"status":"PASS"}),
            );
        } else {
            counts.push(json!({"table":table,"status":"NOT READ","reason":"no SELECT privilege"}));
        }
    }
    report["groundzero_counts"] = json!(counts);

    report["phase"] = json!("sqlx_history");
    let ledger = query_json(
        conn,
        "SELECT jsonb_build_object('exists',count(*) = 1,
                 'readable',coalesce(bool_or(pg_catalog.has_table_privilege(c.oid,'SELECT')),false))
         FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace
         WHERE n.nspname='fvoci' AND c.relname='_sqlx_migrations' AND c.relkind IN ('r','p')",
    )
    .await?;
    report["sqlx_history"] = if ledger["exists"] != true {
        json!({"exists":false,"rows":[]})
    } else if ledger["readable"] != true {
        json!({"exists":true,"status":"NOT READ","reason":"no SELECT privilege"})
    } else {
        let rows = query_json(conn,
            "SELECT coalesce(jsonb_agg(jsonb_build_object('version',version,'description',description,
                 'success',success,'checksum_sha384',pg_catalog.encode(checksum,'hex')) ORDER BY version),'[]'::jsonb)
             FROM fvoci._sqlx_migrations"
        ).await?;
        json!({"exists":true,"status":"PASS","rows":rows})
    };
    Ok(())
}

pub(super) async fn inspect(target: &Target, report: &mut Value) -> Result<(), Failure> {
    let project = project_ref(&target.user)?;
    report["connection"] = json!("UNCONFIRMED");
    report["tls_certificate_and_hostname"] = json!("UNCONFIRMED");
    let mut conn = bounded(Duration::from_secs(5), async {
        PgConnection::connect_with(&super::connect_options(target))
            .await
            .map_err(super::classify_connection_error)
    })
    .await?;
    report["connection"] = json!("PASS");
    report["tls_certificate_and_hostname"] = json!("PASS");
    // This is the authenticated session-pooler route, not a guessed historical project identifier.
    report["target"] = json!({"host":target.host,"port":5432,"database":"postgres",
        "project_ref_from_authenticated_session_route":project});
    report["inventory"] = json!("FAIL");
    let inspected = read_inventory(&mut conn, report).await;
    let rolled_back = bounded(QUERY_LIMIT, async {
        sqlx::raw_sql("ROLLBACK")
            .execute(&mut conn)
            .await
            .map_err(|_| Failure::QueryFailed)?;
        Ok(())
    })
    .await;
    let closed = bounded(QUERY_LIMIT, async {
        conn.close().await.map_err(|_| Failure::ClientError)
    })
    .await;
    report["rollback"] = json!(if rolled_back.is_ok() { "PASS" } else { "FAIL" });
    report["close"] = json!(if closed.is_ok() { "PASS" } else { "FAIL" });
    inspected?;
    rolled_back?;
    closed?;
    report["inventory"] = json!("PASS");
    report["phase"] = json!("complete");
    Ok(())
}

// Metadata is authorized for publication, but a credential-shaped object name is still redacted.
pub(super) fn redact_metadata(value: &mut Value, password: &str) {
    match value {
        Value::String(text) => {
            if text.contains(password) || text.contains("://") || text.contains("-----BEGIN") {
                *text = "<redacted>".into();
            }
        }
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| redact_metadata(value, password)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| redact_metadata(value, password)),
        _ => {}
    }
}

fn execute(target: &Target) -> (Value, i32) {
    let mut report = empty_report();
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return super::fail(report, Failure::ClientError, 1);
    };
    let result =
        tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
            runtime.block_on(bounded(TOTAL_LIMIT, inspect(target, &mut report)))
        });
    runtime.shutdown_background();
    redact_metadata(&mut report, &target.password);
    match result {
        Ok(()) => (report, 0),
        Err(error) => super::fail(report, error, 1),
    }
}

pub fn run(connect: bool) -> i32 {
    super::print_report(super::run_with(
        connect,
        |name| {
            if super::AMBIENT_SETTINGS.contains(&name) {
                std::env::var_os(name).is_some().then(String::new)
            } else {
                std::env::var(name).ok()
            }
        },
        execute,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_route_is_explicit_and_credential_shaped_metadata_is_redacted() {
        assert_eq!(
            project_ref("postgres.abcdefghijklmnopqrst"),
            Ok("abcdefghijklmnopqrst")
        );
        for invalid in [
            "postgres",
            "postgres.bad-ref",
            "postgres.abcdefghijklmnopqrs/",
        ] {
            assert!(project_ref(invalid).is_err());
        }
        let mut metadata = json!({"host":"aws-0-test-1.pooler.supabase.com", "db":"postgres",
            "names":["safe","prefix-password-value", "postgres://user:secret@host/db", "-----BEGIN CERTIFICATE-----"]});
        redact_metadata(&mut metadata, "password-value");
        assert_eq!(
            metadata["names"],
            json!(["safe", "<redacted>", "<redacted>", "<redacted>"])
        );
        assert_eq!(metadata["db"], "postgres");
    }

    #[test]
    fn inventory_without_connect_never_reads_settings_or_connects() {
        let (report, code) = super::super::run_with(
            false,
            |_| panic!("settings read without --connect"),
            |_| panic!("connection without --connect"),
        );
        assert_eq!(code, 2);
        assert_eq!(report["changes"], "NOT RUN");
    }
}
