//! One approved first installation, retained only on the immutable evidence branch.
//! Child output and catalog definitions stay in memory; public output contains fixed status codes and hashes.

use super::{Failure, Target};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{ConnectOptions, Connection, PgConnection};
use std::{collections::BTreeMap, fs, io::Write, os::unix::fs::OpenOptionsExt, process::Command};

const HOST: &str = "aws-0-ap-southeast-1.pooler.supabase.com";
const PROJECT: &str = "fpbqhckdkhhnhqvsixnr";
const SQL_HASH: &str = "ba83f4bb97c060f49efe3c17ca47fd1c1297d594e1331949052a63d398d789f7";
const CHECKSUM: &str = "496c7802d169fd13f777cb7949574b09fdd4d2e65e64cb6e26243e827e6a9069f99b3f687c2ee2cbd47ff96998bfa7c7";
const SQL: &[u8] =
    include_bytes!("../../../crates/server/migrations/202610230001_initial_state.sql");
type Catalog = BTreeMap<(String, String), Value>;
type Result<T> = std::result::Result<T, Failure>;

#[derive(Clone, Copy)]
pub enum Operation {
    Install,
    Inspect,
    Verify,
    VerifyExpiry,
}

fn require(value: bool) -> Result<()> {
    if value {
        Ok(())
    } else {
        Err(Failure::UnexpectedQueryResult)
    }
}

fn setting(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| Failure::InvalidTarget)
}

fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn subprocess(command: &mut Command) -> Result<()> {
    // Never inherit or print child stdout/stderr: driver failures can contain connection details.
    let output = command.output().map_err(|_| Failure::ClientError)?;
    require(output.status.success())
}

fn test_process(command: &mut Command, expected: usize) -> Result<()> {
    let output = command.output().map_err(|_| Failure::ClientError)?;
    require(output.status.success())?;
    let expected_line =
        format!("test result: ok. {expected} passed; 0 failed; 0 ignored; 0 measured;");
    require(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.starts_with(&expected_line)),
    )
}

async fn empty(conn: &mut PgConnection) -> Result<Value> {
    sqlx::raw_sql("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
        .execute(&mut *conn)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    let catalog: Value = sqlx::query_scalar(include_str!("inventory.sql"))
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    sqlx::raw_sql("ROLLBACK")
        .execute(&mut *conn)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    let identity = &catalog["identity"];
    require(
        identity["database"] == "postgres"
            && identity["current_user"] == "postgres"
            && identity["session_user"] == "postgres"
            && identity["server_version_num"]
                .as_i64()
                .is_some_and(|v| v >= 170000)
            && identity["transaction_read_only"] == "on",
    )?;
    require(
        catalog["roles"]
            .as_array()
            .is_some_and(|v| v.len() == 1 && v[0]["name"] == "postgres"),
    )?;
    for key in [
        "memberships",
        "relations",
        "functions",
        "types",
        "default_privileges",
        "cross_schema_dependencies",
    ] {
        require(catalog[key] == json!([]))?;
    }
    let schemas: Vec<_> = catalog["schemas"]
        .as_array()
        .ok_or(Failure::UnexpectedQueryResult)?
        .iter()
        .map(|v| v["name"].as_str().unwrap_or(""))
        .collect();
    require(
        schemas
            == [
                "auth",
                "extensions",
                "graphql",
                "graphql_public",
                "pgbouncer",
                "public",
                "realtime",
                "storage",
                "vault",
            ],
    )?;
    Ok(identity.clone())
}

async fn snapshot(conn: &mut PgConnection) -> Result<Catalog> {
    sqlx::raw_sql("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL search_path=pg_catalog; SET LOCAL timezone='UTC'")
        .execute(&mut *conn).await.map_err(|_| Failure::QueryFailed)?;
    // Extend the existing comparison catalog's structural checks to every managed/application schema.
    let query = include_str!("../sql_compare/catalog.sql").replace(
        "WHERE n.nspname='fvoci'",
        "WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'",
    );
    let rows: Vec<(String, String, Value)> = sqlx::query_as(&query)
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    sqlx::raw_sql("ROLLBACK")
        .execute(&mut *conn)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    Ok(rows.into_iter().map(|(k, i, v)| ((k, i), v)).collect())
}

fn fingerprint(catalog: &Catalog) -> Result<String> {
    let rows: Vec<_> = catalog.iter().collect();
    Ok(sha(
        &serde_json::to_vec(&rows).map_err(|_| Failure::ClientError)?
    ))
}

fn managed(mut after: Catalog) -> Result<Catalog> {
    // Only the exact approved additions are removed. Every preexisting object definition remains compared.
    after.retain(|(kind, identity), _| {
        !((kind == "schema" && identity == "fvoci")
            || ([
                "function",
                "relation",
                "column",
                "constraint",
                "index",
                "policy",
                "trigger",
                "type",
                "sequence",
            ]
            .contains(&kind.as_str())
                && identity.starts_with("fvoci.")))
    });
    for role in ["fvoci_app", "fvoci_migrator"] {
        let attrs = after
            .remove(&("role".into(), role.into()))
            .ok_or(Failure::UnexpectedQueryResult)?;
        require(
            attrs["rolcanlogin"] == true
                && attrs["rolsuper"] == false
                && attrs["rolcreaterole"] == false
                && attrs["rolcreatedb"] == false
                && attrs["rolbypassrls"] == false
                && attrs["rolreplication"] == false,
        )?;
        require(
            after.remove(&("setting".into(), format!("{role}/*")))
                == Some(json!({"config":["search_path=fvoci"]})),
        )?;
        require(
            after.remove(&(
                "membership".into(),
                format!("{role}->postgres/supabase_admin"),
            )) == Some(json!({"admin":true,"inherit":false,"set":false})),
        )?;
    }
    let acl = after
        .get_mut(&("database".into(), "current".into()))
        .and_then(|v| v["acl"].as_array_mut())
        .ok_or(Failure::UnexpectedQueryResult)?;
    for grant in ["fvoci_app=c/postgres", "fvoci_migrator=c/postgres"] {
        let index = acl
            .iter()
            .position(|v| v == grant)
            .ok_or(Failure::UnexpectedQueryResult)?;
        acl.remove(index);
    }
    Ok(after)
}

fn legacy_url(
    target: &Target,
    role: &str,
    password: &str,
    cert: &std::path::Path,
) -> Result<String> {
    let mut url = sqlx::postgres::PgConnectOptions::new_without_pgpass()
        .host(HOST)
        .port(5432)
        .database("postgres")
        .username(&format!("{role}.{PROJECT}"))
        .ssl_mode(sqlx::postgres::PgSslMode::VerifyFull)
        .ssl_root_cert(cert)
        .application_name("fvoci-approved-first-install")
        .options([("statement_timeout", "60000"), ("lock_timeout", "5000")])
        .to_url_lossy();
    url.set_password(Some(password))
        .map_err(|_| Failure::InvalidTarget)?;
    require(target.host == HOST)?;
    Ok(url.to_string())
}

fn url(target: &Target, role: &str, password: &str, cert: &std::path::Path) -> Result<String> {
    require(target.host == HOST)?;
    let mut url = sqlx::postgres::PgConnectOptions::new_without_pgpass()
        .host(HOST)
        .port(5432)
        .database("postgres")
        .username(&format!("{role}.{PROJECT}"))
        .password(password)
        .ssl_mode(sqlx::postgres::PgSslMode::VerifyFull)
        .to_url_lossy();
    // SQLx 0.8.6 serializes CertificateInput::File as "file: <path>", but its parser expects a filename.
    url.query_pairs_mut()
        .append_pair("sslrootcert", cert.to_str().ok_or(Failure::ClientError)?)
        .append_pair("application_name", "fvoci-approved-first-install")
        .append_pair("options", "-c statement_timeout=60000 -c lock_timeout=5000");
    Ok(url.to_string())
}

async fn install(target: &Target, report: &mut Value, operation: Operation) -> Result<()> {
    let source = setting("GITHUB_SHA")?;
    require(
        source.len() == 40
            && source.bytes().all(|b| b.is_ascii_hexdigit())
            && setting("APPROVED_SHA")? == source
            && setting("GITHUB_REPOSITORY")? == "fvoci/GroundZero"
            && setting("GITHUB_ACTOR")? == "fvoci"
            && setting("GITHUB_REF")? == "refs/heads/fvoci/sql-consolidation-cloud-once"
            && target.host == HOST
            && target.user == format!("postgres.{PROJECT}")
            && sha(SQL) == SQL_HASH,
    )?;
    let actual = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|_| Failure::ClientError)?;
    require(actual.status.success() && String::from_utf8_lossy(&actual.stdout).trim() == source)?;
    let migrator_pw = setting("SUPABASE_TEST_MIGRATOR_PASSWORD")?;
    let app_pw = setting("SUPABASE_TEST_APP_PASSWORD")?;
    for password in [&migrator_pw, &app_pw] {
        require(password.len() == 64 && password.bytes().all(|b| b.is_ascii_hexdigit()))?;
    }
    require(migrator_pw != app_pw && migrator_pw != target.password && app_pw != target.password)?;
    let cert = std::env::temp_dir().join(format!("fvoci-first-install-{}.crt", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cert)
        .map_err(|_| Failure::ClientError)?;
    file.write_all(&target.ssl_root_cert)
        .map_err(|_| Failure::ClientError)?;
    let admin_url = url(target, "postgres", &target.password, &cert)?;
    let migration_url = url(target, "fvoci_migrator", &migrator_pw, &cert)?;
    let app_url = url(target, "fvoci_app", &app_pw, &cert)?;
    report["executed_sha"] = json!(source);
    report["sql_sha256"] = json!(SQL_HASH);
    report["target"] = json!({"project":PROJECT,"host":HOST,"port":5432,"database":"postgres"});
    report["connection_kind"] = json!("session_pooler");
    if matches!(operation, Operation::Verify | Operation::VerifyExpiry) {
        let result = verify_existing(
            target,
            &migration_url,
            &app_url,
            report,
            matches!(operation, Operation::VerifyExpiry),
        )
        .await;
        fs::remove_file(cert).map_err(|_| Failure::ClientError)?;
        return result;
    }
    if matches!(operation, Operation::Inspect) {
        report["operation"] = json!("read_only_failed_first_install_diagnosis");
        report["bootstrap"] = json!("FAIL in prior attempt; not retried");
        super::inventory::inspect(target, report).await?;
        let mut admin = PgConnection::connect_with(&super::connect_options(target))
            .await
            .map_err(super::classify_connection_error)?;
        let current = snapshot(&mut admin).await?;
        let hash = fingerprint(&current)?;
        report["managed_current_sha256"] = json!(hash);
        report["managed_equal_to_before_attempt"] =
            json!(hash == "8c4d03f3fbe1fbf1e698ad02f14c04d235cb84e2c8b4d4c70cb9d8bf7f2ab743");
        report["target_still_empty"] = json!(empty(&mut admin).await.is_ok());
        let requirements: Value = sqlx::query_scalar(
            "SELECT jsonb_build_object('uuid_extract_version_exists',to_regprocedure('pg_catalog.uuid_extract_version(uuid)') IS NOT NULL,
             'admin_createrole',(SELECT rolcreaterole FROM pg_roles WHERE rolname=current_user),
             'admin_database_create',has_database_privilege(current_database(),'CREATE'),
             'sql_usage',has_language_privilege('sql','USAGE'),'plpgsql_usage',has_language_privilege('plpgsql','USAGE'))")
            .fetch_one(&mut admin).await.map_err(|_| Failure::QueryFailed)?;
        report["requirements"] = requirements;
        admin.close().await.map_err(|_| Failure::ClientError)?;
        report["legacy_url_select_only"] =
            probe_url(&legacy_url(target, "postgres", &target.password, &cert)?).await;
        let canonical = sqlx::postgres::PgConnectOptions::new_without_pgpass()
            .host(HOST)
            .port(5432)
            .database("postgres")
            .username(&target.user)
            .password(&target.password)
            .ssl_mode(sqlx::postgres::PgSslMode::VerifyFull)
            .ssl_root_cert(&cert)
            .to_url_lossy()
            .to_string();
        report["sqlx_serializer_url_select_only"] = probe_url(&canonical).await;
        report["corrected_url_select_only"] = probe_url(&admin_url).await;
        fs::remove_file(cert).map_err(|_| Failure::ClientError)?;
        report["phase"] = json!("read_only_diagnosis_complete");
        report["result"] = json!("PASS");
        return Ok(());
    }
    report["phase"] = json!("read_only_preflight");
    let mut admin = PgConnection::connect_with(&super::connect_options(target))
        .await
        .map_err(super::classify_connection_error)?;
    report["identity"] = empty(&mut admin).await?;
    report["product_admin_url_read_only_probe"] = probe_url(&admin_url).await;
    require(report["product_admin_url_read_only_probe"]["result"] == "PASS")?;
    // Last preflight immediately precedes bootstrap; bootstrap also refuses existing roles inside its transaction.
    empty(&mut admin).await?;
    let before = snapshot(&mut admin).await?;
    report["managed_before_sha256"] = json!(fingerprint(&before)?);
    require(
        report["managed_before_sha256"]
            == "8c4d03f3fbe1fbf1e698ad02f14c04d235cb84e2c8b4d4c70cb9d8bf7f2ab743",
    )?;
    report["preflight"] = json!("PASS");
    report["backup"] = json!(
        "No preexisting GroundZero data; managed backup availability NOT VERIFIED; no managed data deleted."
    );
    report["admin_tls_verify_full"] = json!("PASS");
    report["phase"] = json!("bootstrap");
    report["bootstrap"] = json!("FAIL (attempt started; success not confirmed)");
    subprocess(
        Command::new("target/debug/fvoci-server")
            .args(["db", "bootstrap"])
            .env("DATABASE_ADMIN_URL", &admin_url)
            .env("FVOCI_DB_MIGRATOR_PASSWORD", &migrator_pw)
            .env("FVOCI_DB_APP_PASSWORD", &app_pw),
    )?;
    report["bootstrap"] = json!("PASS");
    report["phase"] = json!("migrate");
    report["migrate"] = json!("FAIL (attempt started; success not confirmed)");
    subprocess(
        Command::new("target/debug/fvoci-server")
            .args(["db", "migrate"])
            .env("DATABASE_MIGRATION_URL", &migration_url),
    )?;
    report["migrate"] = json!("PASS");
    let mut migrator = PgConnection::connect(&migration_url)
        .await
        .map_err(super::classify_connection_error)?;
    for pass in 0..2 {
        let (role, count, version, success, checksum): (String, i64, i64, bool, String) = sqlx::query_as(
            "SELECT current_user::text, count(*) OVER (), version, success, encode(checksum,'hex') FROM fvoci._sqlx_migrations")
            .fetch_one(&mut migrator).await.map_err(|_| Failure::QueryFailed)?;
        require(
            role == "fvoci_migrator"
                && count == 1
                && version == 202610230001
                && success
                && checksum == CHECKSUM,
        )?;
        if pass == 0 {
            report["phase"] = json!("repeat_migrate");
            subprocess(
                Command::new("target/debug/fvoci-server")
                    .args(["db", "migrate"])
                    .env("DATABASE_MIGRATION_URL", &migration_url),
            )?;
        }
    }
    report["repeat_migrate"] = json!("PASS");
    report["migration_identity"] = json!({"role":"fvoci_migrator","tls_verify_full":"PASS"});
    report["sqlx_checksum_sha384"] = json!(CHECKSUM);
    migrator.close().await.map_err(|_| Failure::ClientError)?;
    let mut app = PgConnection::connect(&app_url)
        .await
        .map_err(super::classify_connection_error)?;
    let safe: bool = sqlx::query_scalar("SELECT current_user='fvoci_app' AND NOT rolsuper AND NOT rolbypassrls FROM pg_roles WHERE rolname=current_user")
        .fetch_one(&mut app).await.map_err(|_| Failure::QueryFailed)?;
    require(safe)?;
    app.close().await.map_err(|_| Failure::ClientError)?;
    report["app_connection"] = json!("PASS");
    report["app_identity"] =
        json!({"role":"fvoci_app","superuser":false,"bypassrls":false,"tls_verify_full":"PASS"});
    let artifacts = fs::read_to_string("target/cloud-install-test-artifacts.jsonl")
        .map_err(|_| Failure::ClientError)?;
    for test in ["first_flow", "db_context", "trash"] {
        report["phase"] = json!(test);
        let executable = artifacts
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|v| {
                v["reason"] == "compiler-artifact"
                    && v["target"]["name"] == test
                    && v["profile"]["test"] == true
            })
            .and_then(|v| v["executable"].as_str().map(str::to_owned))
            .ok_or(Failure::ClientError)?;
        let mut command = Command::new(executable);
        command
            .env("DATABASE_URL", &app_url)
            .arg("--test-threads=1");
        if test == "trash" {
            command.args([
                "--exact",
                "deleted_documents_disappear_and_come_back_with_their_subtree",
            ]);
        }
        test_process(&mut command, if test == "first_flow" { 11 } else { 1 })?;
        report["tests"][test] = json!({"result":"PASS", "passed":if test == "first_flow" {11} else {1},
            "failed":0,"ignored":0,"selection":if test == "trash" {"deleted_documents_disappear_and_come_back_with_their_subtree"} else {"all tests in the named integration target"}});
    }
    report["phase"] = json!("managed_objects_unchanged");
    let after = managed(snapshot(&mut admin).await?)?;
    report["managed_after_sha256"] = json!(fingerprint(&after)?);
    require(before == after)?;
    report["managed_objects_unchanged"] = json!("PASS");
    admin.close().await.map_err(|_| Failure::ClientError)?;
    fs::remove_file(cert).map_err(|_| Failure::ClientError)?;
    report["phase"] = json!("complete");
    report["result"] = json!("PASS");
    Ok(())
}

const FIRST_FLOW: &[&str] = &[
    "workspace_create_is_idempotent_and_listed",
    "document_write_save_reopen_with_revisions",
    "save_retry_is_idempotent_and_stale_save_conflicts",
    "other_user_cannot_see_or_change_documents",
    "rls_hides_rows_from_other_users_under_app_role",
    "audit_records_creation_and_rename_without_body",
    "input_validation_and_origin_and_session_failures",
    "data_and_session_survive_fresh_state",
    "replay_after_a_newer_save_reports_conflict",
    "app_role_cannot_touch_internal_tables_or_rewind_revisions",
    "duplicate_session_cookies_are_rejected",
];

fn safe_test_result(output: &std::process::Output, expected: &[&str]) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut statuses = BTreeMap::new();
    let mut failure_test = None;
    let mut actual_http = None;
    let mut http_status_assertions = Vec::new();
    for line in stdout.lines() {
        if line.starts_with("---- ") {
            failure_test = line
                .strip_prefix("---- ")
                .and_then(|s| s.strip_suffix(" stdout ----"))
                .filter(|name| expected.contains(name));
            actual_http = None;
        }
        let http = |text: &str| {
            text.parse::<u16>().ok().filter(|code| {
                [
                    200, 201, 202, 204, 301, 302, 303, 304, 307, 308, 400, 401, 403, 404, 405, 409,
                    410, 422, 429, 500, 502, 503, 504,
                ]
                .contains(code)
            })
        };
        if let Some(left) = line.trim().strip_prefix("left: ") {
            actual_http = http(left);
        }
        if let Some(right) = line.trim().strip_prefix("right: ")
            && let (Some(test), Some(actual), Some(expected)) =
                (failure_test, actual_http, http(right))
        {
            http_status_assertions.push(json!({"test":test,"actual":actual,"expected":expected}));
            actual_http = None;
        }
        if let Some((name, status)) = line
            .strip_prefix("test ")
            .and_then(|line| line.split_once(" ... "))
            && expected.contains(&name)
            && ["ok", "FAILED", "ignored"].contains(&status)
        {
            statuses.insert(name.to_owned(), status.to_owned());
        }
    }
    let summary = stdout
        .lines()
        .find(|line| line.starts_with("test result: "));
    let mut counts = Vec::new();
    if let Some(summary) = summary {
        for (index, segment) in summary
            .trim_start_matches("test result: ")
            .split(';')
            .take(5)
            .enumerate()
        {
            let count = segment
                .split_whitespace()
                .nth(usize::from(index == 0))
                .and_then(|s| s.parse::<usize>().ok());
            counts.push(count);
        }
    }
    let kinds: Vec<_> = [
        "PoolTimedOut",
        "PoolClosed",
        "RowNotFound",
        "ColumnNotFound",
        "TypeNotFound",
        "Tls(",
        "Io(",
    ]
    .into_iter()
    .filter(|kind| stdout.contains(kind) || stderr.contains(kind))
    .collect();
    let sqlstates: Vec<_> = [
        "42501", "42P01", "42704", "23505", "23514", "57014", "55P03", "08006", "28P01", "3F000",
        "28000", "53300", "53100", "57P03",
    ]
    .into_iter()
    .filter(|code| {
        let marker = format!("code: \"{code}\"");
        stdout.contains(&marker) || stderr.contains(&marker)
    })
    .collect();
    let passed = output.status.success()
        && counts.first() == Some(&Some(expected.len()))
        && counts.get(1) == Some(&Some(0))
        && counts.get(2) == Some(&Some(0))
        && statuses.len() == expected.len()
        && statuses.values().all(|status| status == "ok");
    json!({"result":if passed {"PASS"} else {"FAIL"},"exit_success":output.status.success(),
        "test_statuses":statuses,"summary_counts_passed_failed_ignored_measured_filtered":counts,
        "error_kinds":kinds,"sqlstates":sqlstates,"http_status_assertions":http_status_assertions})
}

async fn verify_existing(
    target: &Target,
    migration_url: &str,
    app_url: &str,
    report: &mut Value,
    expiry_only: bool,
) -> Result<()> {
    report["operation"] = json!("verify_existing_approved_cloud_installation");
    report["verification_scope"] = json!(if expiry_only {
        "exactly one corrected expiry fixture; prior 12 PASS are separate 20f68bd evidence"
    } else {
        "first_flow 11, db_context 1, trash exact 1"
    });
    report["backup"] = json!(
        "Existing synthetic GroundZero data is retained; no backup was created; managed backup availability NOT VERIFIED."
    );
    report["installed_by_sha"] = json!("3f18a08ac412b95febf40cbc3fbac6a236f80cdd");
    report["bootstrap"] = json!("NOT RUN (previous installation PASS)");
    report["migrate"] = json!("NOT RUN (previous installation PASS)");
    report["repeat_migrate"] = json!("NOT RUN (previous installation PASS)");
    report["phase"] = json!("managed_objects_before_app_tests");
    super::inventory::inspect(target, report).await?;
    report["phase"] = json!("managed_objects_before_app_tests");
    let mut admin = PgConnection::connect_with(&super::connect_options(target))
        .await
        .map_err(super::classify_connection_error)?;
    let current = snapshot(&mut admin).await?;
    let effects: Vec<_> = current.iter().filter(|((kind, identity),_)| {
        (kind=="role" && ["fvoci_app","fvoci_migrator"].contains(&identity.as_str()))
        || (kind=="setting" && (identity.starts_with("fvoci_app/") || identity.starts_with("fvoci_migrator/")))
        || (kind=="membership" && (identity.starts_with("fvoci_app->") || identity.starts_with("fvoci_migrator->")
            || identity.contains("->fvoci_app/") || identity.contains("->fvoci_migrator/")))
    }).map(|((kind,identity),definition)|json!({"kind":kind,"identity":identity,"definition":definition})).collect();
    report["installation_effects"] = json!({"roles_settings_memberships":effects,
        "database_acl":current.get(&("database".into(),"current".into())).map(|v|v["acl"].clone())});
    let before = managed(current)?;
    report["managed_before_app_tests_sha256"] = json!(fingerprint(&before)?);
    require(
        report["managed_before_app_tests_sha256"]
            == "8c4d03f3fbe1fbf1e698ad02f14c04d235cb84e2c8b4d4c70cb9d8bf7f2ab743",
    )?;
    report["managed_objects_before_app_tests"] = json!("PASS");
    report["phase"] = json!("ledger_and_roles_read_only");
    let mut migrator = PgConnection::connect(migration_url)
        .await
        .map_err(super::classify_connection_error)?;
    sqlx::raw_sql("BEGIN READ ONLY")
        .execute(&mut migrator)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    let (role,count,version,success,checksum):(String,i64,i64,bool,String)=sqlx::query_as(
        "SELECT current_user::text,count(*) OVER (),version,success,encode(checksum,'hex') FROM fvoci._sqlx_migrations")
        .fetch_one(&mut migrator).await.map_err(|_| Failure::QueryFailed)?;
    require(
        role == "fvoci_migrator"
            && count == 1
            && version == 202610230001
            && success
            && checksum == CHECKSUM,
    )?;
    sqlx::raw_sql("ROLLBACK")
        .execute(&mut migrator)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    migrator.close().await.map_err(|_| Failure::ClientError)?;
    report["ledger"] =
        json!({"result":"PASS","rows":1,"version":version,"checksum_sha384":CHECKSUM});
    report["migration_identity"] = json!({"role":"fvoci_migrator","tls_verify_full":"PASS"});
    let mut app = PgConnection::connect(app_url)
        .await
        .map_err(super::classify_connection_error)?;
    sqlx::raw_sql("BEGIN READ ONLY")
        .execute(&mut app)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    let safe:bool=sqlx::query_scalar("SELECT current_user='fvoci_app' AND NOT rolsuper AND NOT rolbypassrls FROM pg_roles WHERE rolname=current_user")
        .fetch_one(&mut app).await.map_err(|_| Failure::QueryFailed)?;
    require(safe)?;
    sqlx::raw_sql("ROLLBACK")
        .execute(&mut app)
        .await
        .map_err(|_| Failure::QueryFailed)?;
    app.close().await.map_err(|_| Failure::ClientError)?;
    report["app_connection"] = json!("PASS");
    report["app_identity"] =
        json!({"role":"fvoci_app","superuser":false,"bypassrls":false,"tls_verify_full":"PASS"});
    let artifacts = fs::read_to_string("target/cloud-install-test-artifacts.jsonl")
        .map_err(|_| Failure::ClientError)?;
    let mut all_passed = true;
    let all_tests = [
        ("first_flow", FIRST_FLOW),
        (
            "db_context",
            &["user_context_does_not_outlive_commit_or_rollback"][..],
        ),
        (
            "trash",
            &["deleted_documents_disappear_and_come_back_with_their_subtree"][..],
        ),
    ];
    let selected = if expiry_only {
        &[("first_flow", &FIRST_FLOW[6..7])][..]
    } else {
        &all_tests[..]
    };
    for &(test, names) in selected {
        report["phase"] = json!(test);
        let executable = artifacts
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .find(|v| {
                v["reason"] == "compiler-artifact"
                    && v["target"]["name"] == test
                    && v["profile"]["test"] == true
            })
            .and_then(|v| v["executable"].as_str().map(str::to_owned))
            .ok_or(Failure::ClientError)?;
        let mut command = Command::new(executable);
        command.env("DATABASE_URL", app_url).arg("--test-threads=1");
        if test == "trash" || expiry_only {
            command.args(["--exact", names[0]]);
        }
        let output = command.output().map_err(|_| Failure::ClientError)?;
        let result = safe_test_result(&output, names);
        all_passed &= result["result"] == "PASS";
        report["tests"][test] = result;
    }
    report["phase"] = json!("managed_objects_after_app_tests");
    let after = managed(snapshot(&mut admin).await?)?;
    report["managed_after_app_tests_sha256"] = json!(fingerprint(&after)?);
    require(before == after)?;
    report["managed_objects_unchanged"] = json!("PASS");
    admin.close().await.map_err(|_| Failure::ClientError)?;
    report["phase"] = json!("verification_complete");
    require(all_passed)?;
    report["result"] = json!("PASS");
    Ok(())
}

async fn probe_url(url: &str) -> Value {
    let probe = async {
        let mut conn = PgConnection::connect(url)
            .await
            .map_err(super::classify_connection_error)?;
        sqlx::raw_sql("BEGIN READ ONLY")
            .execute(&mut conn)
            .await
            .map_err(|_| Failure::QueryFailed)?;
        let one: i32 = sqlx::query_scalar("SELECT 1")
            .fetch_one(&mut conn)
            .await
            .map_err(|_| Failure::QueryFailed)?;
        sqlx::raw_sql("ROLLBACK")
            .execute(&mut conn)
            .await
            .map_err(|_| Failure::QueryFailed)?;
        conn.close().await.map_err(|_| Failure::ClientError)?;
        require(one == 1)
    };
    match super::bounded(std::time::Duration::from_secs(10), probe).await {
        Ok(()) => json!({"result":"PASS"}),
        Err(error) => json!({"result":"FAIL","failure_code":error.code()}),
    }
}

pub fn run(connect: bool, operation: Operation) -> i32 {
    super::print_report(super::run_with(
        connect,
        |name| {
            if super::AMBIENT_SETTINGS.contains(&name) {
                std::env::var_os(name).is_some().then(String::new)
            } else {
                std::env::var(name).ok()
            }
        },
        |target| {
            let mut report = json!({"operation":"approved_cloud_first_install", "result":"FAIL",
            "phase":"validation", "bootstrap":"NOT RUN", "migrate":"NOT RUN", "repeat_migrate":"NOT RUN",
            "app_connection":"NOT RUN", "tests":{}, "managed_objects_unchanged":"NOT RUN",
            "backup":"GroundZero target state not yet verified; managed backup availability NOT VERIFIED.",
            "sharing":"NOT DETERMINED; pg_depend does not capture every PL/pgSQL body or dynamic dependency.",
            "timeout":"GitHub installation step enforces a 9 minute wall-clock timeout; the 480 second async bound cannot interrupt a synchronous child.",
            "retention":"Any newly created synthetic GroundZero test data remains; no DROP, TRUNCATE, reset or cleanup is run."});
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return super::fail(report, Failure::ClientError, 1);
            };
            let result = tracing::subscriber::with_default(
                tracing::subscriber::NoSubscriber::default(),
                || {
                    runtime.block_on(super::bounded(
                        std::time::Duration::from_secs(480),
                        install(target, &mut report, operation),
                    ))
                },
            );
            runtime.shutdown_background();
            super::inventory::redact_metadata(&mut report, &target.password);
            for name in [
                "SUPABASE_TEST_MIGRATOR_PASSWORD",
                "SUPABASE_TEST_APP_PASSWORD",
            ] {
                if let Ok(password) = setting(name)
                    && !password.is_empty()
                {
                    super::inventory::redact_metadata(&mut report, &password);
                }
            }
            match result {
                Ok(()) => (report, 0),
                Err(error) => super::fail(report, error, 1),
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn test_parser_publishes_only_allowed_names_statuses_counts_and_error_codes() {
        let name = FIRST_FLOW[6];
        let stdout = format!(
            "test {name} ... FAILED\ntest untrusted-secret-name ... FAILED\n\n---- {name} stdout ----\nURL postgres://user:secret-value@host/db\n  left: 401\n right: 200\nDatabase error code: \"42501\"\nPoolTimedOut\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s\n"
        );
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: stdout.into_bytes(),
            stderr: b"secret stderr value".to_vec(),
        };
        let report = safe_test_result(&output, &[name]);
        assert_eq!(report["result"], "FAIL");
        assert_eq!(report["test_statuses"], json!({name:"FAILED"}));
        assert_eq!(report["sqlstates"], json!(["42501"]));
        assert_eq!(report["error_kinds"], json!(["PoolTimedOut"]));
        assert_eq!(
            report["http_status_assertions"],
            json!([{"test":name,"actual":401,"expected":200}])
        );
        let encoded = report.to_string();
        for secret in [
            "postgres://",
            "secret-value",
            "untrusted-secret-name",
            "secret stderr",
        ] {
            assert!(!encoded.contains(secret));
        }
    }

    #[test]
    fn zero_tests_or_missing_expected_test_names_cannot_pass() {
        let stdout = format!(
            "test {} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0s\n",
            FIRST_FLOW[0]
        );
        let normal = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.into_bytes(),
            stderr: vec![],
        };
        assert_eq!(
            safe_test_result(&normal, &[FIRST_FLOW[0]])["result"],
            "PASS"
        );
        assert_eq!(
            safe_test_result(&normal, &FIRST_FLOW[..2])["result"],
            "FAIL"
        );
        let output=std::process::Output{status:std::process::ExitStatus::from_raw(0),stdout:b"test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0s\n".to_vec(),stderr:vec![]};
        assert_eq!(
            safe_test_result(&output, &[FIRST_FLOW[0]])["result"],
            "FAIL"
        );
    }

    #[test]
    fn diagnosis_metadata_redacts_all_three_credentials_and_credential_shaped_values() {
        let mut report = json!({"catalog":{"names":["admin-fixture", "migrator-fixture", "app-fixture", "postgres://fixture", "safe"]}});
        for password in ["admin-fixture", "migrator-fixture", "app-fixture"] {
            super::super::inventory::redact_metadata(&mut report, password);
        }
        assert_eq!(
            report["catalog"]["names"],
            json!([
                "<redacted>",
                "<redacted>",
                "<redacted>",
                "<redacted>",
                "safe"
            ])
        );
    }

    #[test]
    fn raw_url_set_password_corrupts_percent_sequences_but_sqlx_serializer_preserves_them() {
        let target = Target {
            host: HOST.into(),
            user: format!("postgres.{PROJECT}"),
            password: "fixture%41!".into(),
            ssl_root_cert: vec![],
        };
        let legacy = legacy_url(
            &target,
            "postgres",
            &target.password,
            std::path::Path::new("/tmp/fixture.crt"),
        )
        .unwrap();
        let legacy_options: sqlx::postgres::PgConnectOptions = legacy.parse().unwrap();
        let canonical = sqlx::postgres::PgConnectOptions::new_without_pgpass()
            .password(&target.password)
            .to_url_lossy();
        let canonical_options: sqlx::postgres::PgConnectOptions =
            canonical.as_str().parse().unwrap();
        assert_ne!(
            legacy_options.to_url_lossy().password(),
            canonical.password()
        );
        assert_eq!(
            canonical_options.to_url_lossy().password(),
            canonical.password()
        );
    }

    #[test]
    fn product_url_roundtrip_preserves_password_ca_path_and_verify_full() {
        let target = Target {
            host: HOST.into(),
            user: format!("postgres.{PROJECT}"),
            password: "fixture%41!".into(),
            ssl_root_cert: vec![],
        };
        let path = std::path::Path::new("/tmp/fixture CA.crt");
        let fixed = url(&target, "postgres", &target.password, path).unwrap();
        let parsed: sqlx::postgres::PgConnectOptions = fixed.parse().unwrap();
        assert!(matches!(
            parsed.get_ssl_mode(),
            sqlx::postgres::PgSslMode::VerifyFull
        ));
        let encoded = parsed.to_url_lossy();
        assert_eq!(encoded.password(), Some("fixture%2541%21"));
        // Re-serialization exposes the known Display prefix and proves the parser retained the exact filename.
        assert_eq!(
            encoded
                .query_pairs()
                .find(|(key, _)| key == "sslrootcert")
                .unwrap()
                .1,
            "file: /tmp/fixture CA.crt"
        );
        assert_eq!(
            parsed.get_options(),
            Some("-c statement_timeout=60000 -c lock_timeout=5000")
        );
    }

    fn fixture() -> (Catalog, Catalog) {
        let before = Catalog::from([
            (
                ("schema".into(), "auth".into()),
                json!({"owner":"supabase_auth_admin"}),
            ),
            (
                ("function".into(), "public.kept()".into()),
                json!({"definition":"SELECT 'kept'"}),
            ),
            (
                ("database".into(), "current".into()),
                json!({"acl":["=Tc/postgres","postgres=CTc/postgres"]}),
            ),
        ]);
        let mut after = before.clone();
        after.insert(
            ("schema".into(), "fvoci".into()),
            json!({"owner":"fvoci_migrator"}),
        );
        after.insert(
            ("relation".into(), "fvoci.document".into()),
            json!({"owner":"fvoci_migrator"}),
        );
        for role in ["fvoci_app", "fvoci_migrator"] {
            after.insert(("role".into(), role.into()), json!({"rolcanlogin":true,"rolsuper":false,
                "rolcreaterole":false,"rolcreatedb":false,"rolbypassrls":false,"rolreplication":false}));
            after.insert(
                ("setting".into(), format!("{role}/*")),
                json!({"config":["search_path=fvoci"]}),
            );
            after.insert(
                (
                    "membership".into(),
                    format!("{role}->postgres/supabase_admin"),
                ),
                json!({"admin":true,"inherit":false,"set":false}),
            );
            after
                .get_mut(&("database".into(), "current".into()))
                .unwrap()["acl"]
                .as_array_mut()
                .unwrap()
                .push(json!(format!("{role}=c/postgres")));
        }
        (before, after)
    }

    #[test]
    fn only_authorized_additions_leave_managed_catalog_equal() {
        let (before, after) = fixture();
        assert_eq!(managed(after).unwrap(), before);
    }

    #[test]
    fn observed_supabase_grantor_does_not_allow_other_grantors_or_extra_memberships() {
        let (before, after) = fixture();
        let mut wrong = after.clone();
        let value = wrong
            .remove(&(
                "membership".into(),
                "fvoci_app->postgres/supabase_admin".into(),
            ))
            .unwrap();
        wrong.insert(
            ("membership".into(), "fvoci_app->postgres/postgres".into()),
            value,
        );
        assert!(managed(wrong).is_err());
        let mut extra = after;
        extra.insert(
            (
                "membership".into(),
                "fvoci_app->other/supabase_admin".into(),
            ),
            json!({"admin":false,"inherit":false,"set":true}),
        );
        assert_ne!(managed(extra).unwrap(), before);
    }

    #[test]
    fn changed_managed_function_foreign_objects_and_extra_grants_are_not_hidden() {
        let (before, after) = fixture();
        let mut changed = after.clone();
        changed
            .get_mut(&("function".into(), "public.kept()".into()))
            .unwrap()["definition"] = json!("SELECT 'changed'");
        assert_ne!(managed(changed).unwrap(), before);
        let mut changed = after.clone();
        changed.insert(
            ("function".into(), "public.new()".into()),
            json!({"definition":"SELECT 1"}),
        );
        assert_ne!(managed(changed).unwrap(), before);
        let mut changed = after.clone();
        changed
            .get_mut(&("database".into(), "current".into()))
            .unwrap()["acl"]
            .as_array_mut()
            .unwrap()
            .push(json!("fvoci_app=C/postgres"));
        assert_ne!(managed(changed).unwrap(), before);
        let mut changed = after;
        changed
            .get_mut(&("role".into(), "fvoci_app".into()))
            .unwrap()["rolbypassrls"] = json!(true);
        assert!(managed(changed).is_err());
    }
}
