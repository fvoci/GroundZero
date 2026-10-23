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
            after.remove(&("membership".into(), format!("{role}->postgres/postgres")))
                == Some(json!({"admin":true,"inherit":false,"set":false})),
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

fn url(target: &Target, role: &str, password: &str, cert: &std::path::Path) -> Result<String> {
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

async fn install(target: &Target, report: &mut Value, inspect_only: bool) -> Result<()> {
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
    if inspect_only {
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
        report["legacy_url_select_only"] = probe_url(&admin_url).await;
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
    let before = snapshot(&mut admin).await?;
    report["managed_before_sha256"] = json!(fingerprint(&before)?);
    // Last preflight immediately precedes bootstrap; bootstrap also refuses existing roles inside its transaction.
    empty(&mut admin).await?;
    report["preflight"] = json!("PASS");
    report["backup"] = json!(
        "No preexisting GroundZero data; managed backup availability NOT VERIFIED; no managed data deleted."
    );
    report["admin_tls_verify_full"] = json!("PASS");
    report["phase"] = json!("bootstrap");
    subprocess(
        Command::new("target/debug/fvoci-server")
            .args(["db", "bootstrap"])
            .env("DATABASE_ADMIN_URL", &admin_url)
            .env("FVOCI_DB_MIGRATOR_PASSWORD", &migrator_pw)
            .env("FVOCI_DB_APP_PASSWORD", &app_pw),
    )?;
    report["bootstrap"] = json!("PASS");
    report["phase"] = json!("migrate");
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

pub fn run(connect: bool, inspect_only: bool) -> i32 {
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
                        install(target, &mut report, inspect_only),
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
        let legacy = url(
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
                ("membership".into(), format!("{role}->postgres/postgres")),
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
