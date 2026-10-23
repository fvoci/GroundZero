//! Preparation-only comparison. Production migration/bootstrap commands stay unchanged.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, PostgresMajor};
use crate::supabase::Supabase;

mod capabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BehaviorScope {
    Full,
    Core,
}

const CORE_TESTS: &[&str] = &[
    "db_context",
    "first_flow",
    "ids",
    "sharing",
    "members",
    "recurrence",
    "time_tracking",
];

/// Requested app-role behavior phase. `oidc` adds the test Keycloak and the `oidc_` tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Behavior {
    pub scope: BehaviorScope,
    pub oidc: bool,
}

struct BehaviorRun<'a> {
    candidate: Option<&'a Path>,
    behavior: Behavior,
}

fn check_options(behavior: Option<Behavior>, supabase: bool, restricted_admin: bool) -> Result<()> {
    let scope = behavior.map(|behavior| behavior.scope);
    ensure!(
        !restricted_admin || !supabase,
        "restricted admin requires the isolated standard PostgreSQL fixture"
    );
    ensure!(
        !restricted_admin || scope != Some(BehaviorScope::Full),
        "restricted admin requires --behavior-scope core; the full suite needs CREATE DATABASE and restore capabilities"
    );
    ensure!(
        !behavior.is_some_and(|behavior| behavior.oidc && behavior.scope != BehaviorScope::Full),
        "--oidc requires the full behavior scope; the core scope excludes the OIDC tests"
    );
    Ok(())
}

const CATALOG: &str = include_str!("../../preparation/sql-consolidation/verification/catalog.sql");
type Snapshot = BTreeMap<String, Value>;

enum Cluster {
    Postgres(Compose),
    Supabase(Supabase),
}

impl Cluster {
    fn new(root: &Path, run: &str, supabase: bool, postgres_major: PostgresMajor) -> Result<Self> {
        if supabase {
            Ok(Self::Supabase(Supabase::for_comparison(root, run)?))
        } else {
            Ok(Self::Postgres(Compose::for_comparison(
                root,
                run,
                postgres_major,
            )))
        }
    }

    fn start(&self, sh: &Shell) -> Result<()> {
        match self {
            Self::Postgres(cluster) => {
                println!("cluster: {}", cluster.project());
                cluster.up_services(sh, &["postgres"])
            }
            Self::Supabase(cluster) => {
                println!("cluster: {}", cluster.project());
                cluster.up(sh)
            }
        }
    }

    fn urls(&self, sh: &Shell) -> Result<DbUrls> {
        Ok(match self {
            Self::Postgres(c) => DbUrls::local(c.host_port(sh, "postgres", 5432)?),
            Self::Supabase(c) => DbUrls::supabase(c.host_port(sh, "db", 5432)?),
        })
    }

    fn dump(&self, sh: &Shell) -> Result<String> {
        let (user, db) = match self {
            Self::Postgres(_) => ("fvoci_migrator", "fvoci"),
            Self::Supabase(_) => ("fvoci_migrator", "postgres"),
        };
        let mut command = [
            "pg_dump",
            "--schema-only",
            "--schema=fvoci",
            "--no-comments",
            "-U",
            user,
            "-d",
            db,
        ]
        .map(str::to_owned)
        .to_vec();
        match self {
            Self::Postgres(c) => c.exec_read(sh, "postgres", &command),
            Self::Supabase(c) => {
                // Supabase's local socket uses peer authentication. Use the same explicit
                // test-only migrator over TCP, passing its password via environment only.
                command.extend(["--host=127.0.0.1".to_owned()]);
                let _password = sh.push_env("PGPASSWORD", compose::TEST_MIGRATOR_PASSWORD);
                c.exec_read(sh, &command)
            }
        }
    }

    fn stop(&self, sh: &Shell) -> Result<()> {
        match self {
            Self::Postgres(c) => c.stop(sh),
            Self::Supabase(c) => c.stop(sh),
        }
    }
}

pub fn run(
    root: &Path,
    candidate: Option<&Path>,
    output: &Path,
    behavior: Option<Behavior>,
    supabase: bool,
    restricted_admin: bool,
    postgres_major: PostgresMajor,
) -> Result<()> {
    check_options(behavior, supabase, restricted_admin)?;
    let oidc = behavior.is_some_and(|behavior| behavior.oidc);
    let scope = behavior.map(|behavior| behavior.scope);
    let candidate = candidate.map(std::fs::canonicalize).transpose()?;
    let output = claim_output(output)?;
    let sh = Shell::new()?;
    sh.change_dir(root);
    let run = uuid::Uuid::now_v7().simple().to_string();
    let git = cmd!(sh, "git rev-parse HEAD").read()?;
    write_json(
        &output.join("inputs.json"),
        &json!({
            "source_sha":git.trim(), "run_id":run,
            "postgres_major":if supabase {None}else{Some(postgres_major.number())},
            "compose_test_sha256":format!("{:x}",Sha256::digest(std::fs::read(root.join("deploy/compose.test.yaml"))?)),
            "compose_pg18_sha256":format!("{:x}",Sha256::digest(std::fs::read(root.join("deploy/compose.test.pg18.yaml"))?)),
            "migrations":sql_hashes(&root.join("crates/server/migrations"))?,
            "candidate":candidate.as_deref().map(sql_hashes).transpose()?,
            "tool_sha256":Sha256::digest(include_bytes!("sql_compare.rs")).iter().map(|b|format!("{b:02x}")).collect::<String>(),
            "capabilities_sha256":format!("{:x}",Sha256::digest(include_bytes!("sql_compare/capabilities.rs"))),
            "catalog_sha256":format!("{:x}",Sha256::digest(CATALOG.as_bytes())),
            "supabase_upstream":if supabase {Some(crate::supabase::REVISION)}else{None},
            "capability_profile":if restricted_admin {"restricted-installer-v1"}else{"image-default"},
            "behavior_scope":scope.map(|scope|format!("{scope:?}").to_lowercase()),
            "oidc":oidc,
            "core_tests":if scope==Some(BehaviorScope::Core) {Some(CORE_TESTS)}else{None},
            "core_excluded":"All unlisted test binaries, including backup/restore, FreshDb search, OIDC, S3 and Supabase-specific paths; no Cloud execution.",
            "route_note":if oidc {
                "Supabase session uses pooler for DATABASE_URL; FreshDb backup/search fixture databases use direct connection. OIDC tests use the run's isolated test Keycloak, shared by A and B in sequence."
            } else {
                "Supabase session uses pooler for DATABASE_URL; FreshDb backup/search fixture databases use direct connection. OIDC is not requested."
            }
        }),
    )?;
    let mut phases: BTreeMap<String, &str> = [
        "a_install",
        "a_snapshot",
        "b_install",
        "b_snapshot",
        "objects",
        "self_test",
    ]
    .map(|name| (name.to_owned(), "NOT RUN"))
    .into_iter()
    .collect();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let mut clusters = Vec::new();
    let mut services = Vec::new();
    let result = (|| {
        let mut snapshots = Vec::new();
        let mut dump_snapshots = Vec::new();
        let mut urls = Vec::new();
        for (side, source) in [("a", None), ("b", candidate.as_deref())] {
            phases.insert(format!("{side}_install"), "FAIL");
            let cluster = register_and_start(
                &mut clusters,
                Cluster::new(root, &format!("{run}-{side}"), supabase, postgres_major)?,
                |cluster| cluster.start(&sh),
            )?;
            let mut connection = cluster.urls(&sh)?;
            let dir = output.join(side);
            std::fs::create_dir(&dir)?;
            if restricted_admin {
                runtime.block_on(capabilities::prepare(&mut connection))?;
            }
            bootstrap(&sh, &connection)?;
            runtime.block_on(install_source(&sh, &connection, source))?;
            std::fs::write(dir.join("behavior-direct.txt"), "NOT RUN\n")?;
            std::fs::write(dir.join("behavior-session.txt"), "NOT RUN\n")?;
            phases.insert(format!("{side}_install"), "PASS");
            phases.insert(format!("{side}_snapshot"), "FAIL");
            let snapshot = runtime.block_on(snapshot(&connection.migration, &dir))?;
            if restricted_admin {
                phases.insert(format!("{side}_capabilities"), "FAIL");
                runtime.block_on(capabilities::verify(
                    &sh,
                    &connection,
                    source,
                    &dir,
                    &snapshot,
                ))?;
                phases.insert(format!("{side}_capabilities"), "PASS");
            }
            let dump = cluster.dump(&sh)?;
            std::fs::write(dir.join("schema.sql"), &dump)?;
            let normalized = normalize_dump(&dump, &snapshot)?;
            write_json(&dir.join("dump-objects.json"), &json!(normalized))?;
            snapshots.push(snapshot);
            dump_snapshots.push(normalized);
            urls.push(connection);
            phases.insert(format!("{side}_snapshot"), "PASS");
        }
        let catalog_differences = differences(&snapshots[0], &snapshots[1]);
        let dump_differences = differences(&dump_snapshots[0], &dump_snapshots[1]);
        write_json(
            &output.join("differences.json"),
            &json!({"catalog":catalog_differences,"dump":dump_differences}),
        )?;
        phases.insert("self_test".to_owned(), "FAIL");
        let self_tests = runtime.block_on(self_test(&urls[1].migration))?;
        write_json(&output.join("self-test.json"), &self_tests)?;
        phases.insert("self_test".to_owned(), "PASS");
        phases.insert("objects".to_owned(), "FAIL");
        ensure!(
            catalog_differences.is_empty() && dump_differences.is_empty(),
            "object differences detected; see {}/differences.json",
            output.display()
        );
        println!("PASS: catalog, required initial rows and schema dump objects are equal");
        phases.insert("objects".to_owned(), "PASS");
        if let Some(behavior) = behavior {
            let mut fixture =
                Compose::for_comparison(root, &format!("{run}-services"), postgres_major);
            let mut wanted = vec!["meilisearch", "seaweedfs"];
            if behavior.oidc {
                fixture = fixture.with_profile("oidc");
                wanted.push("keycloak");
            }
            let services = register_and_start(&mut services, fixture, |services| {
                services.up_services(&sh, &wanted)
            })?;
            for (index, side) in ["a", "b"].iter().enumerate() {
                let source = if index == 1 {
                    candidate.as_deref()
                } else {
                    None
                };
                run_behavior(
                    root,
                    &sh,
                    services,
                    &clusters[index],
                    &urls[index],
                    BehaviorRun {
                        candidate: source,
                        behavior,
                    },
                    &output.join(side),
                )?;
            }
        } else {
            println!("NOT RUN: app-role behavioral suite (add --behavior)");
        }
        Ok::<_, anyhow::Error>(())
    })();
    finish_run(
        result,
        services
            .iter()
            .rev()
            .map(|service| service.stop(&sh))
            .chain(clusters.iter().rev().map(|cluster| cluster.stop(&sh))),
        |passed, cleanup_passed| {
            phases.insert(
                "cleanup".to_owned(),
                if cleanup_passed { "PASS" } else { "FAIL" },
            );
            let mut behavior_results = BTreeMap::new();
            for side in ["a", "b"] {
                for route in ["direct", "session"] {
                    let status = std::fs::read_to_string(
                        output.join(side).join(format!("behavior-{route}.txt")),
                    )
                    .unwrap_or_else(|_| "NOT RUN\n".to_owned());
                    behavior_results.insert(format!("{side}_{route}"), status);
                }
            }
            write_json(
                &output.join("result.json"),
                &json!({"passed":passed,"candidate":candidate,"supabase":supabase,"behavior_requested":behavior.is_some(),"behavior_scope":scope.map(|scope|format!("{scope:?}").to_lowercase()),"oidc":oidc,"restricted_admin":restricted_admin,"phases":phases,"behavior":behavior_results}),
            )
        },
        |message| eprintln!("{message}"),
    )
}

fn claim_output(output: &Path) -> Result<PathBuf> {
    if let Some(parent) = output.parent().filter(|path| !path.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    // Only one run may create the leaf, even when competitors share missing parents.
    std::fs::create_dir(output).with_context(|| {
        format!(
            "could not claim evidence directory {} (must not already exist)",
            output.display()
        )
    })?;
    std::fs::canonicalize(output).context("could not resolve claimed evidence directory")
}

// Register before starting: a failed Compose up can still have created containers.
fn register_and_start<T>(
    resources: &mut Vec<T>,
    resource: T,
    start: impl FnOnce(&T) -> Result<()>,
) -> Result<&T> {
    resources.push(resource);
    let resource = resources.last().context("registered resource missing")?;
    start(resource)?;
    Ok(resource)
}

// Consume every stop attempt and always attempt the final record. Retain the original
// operation error, otherwise the first stop error, otherwise a recording error.
fn finish_run(
    result: Result<()>,
    stops: impl IntoIterator<Item = Result<()>>,
    record: impl FnOnce(bool, bool) -> Result<()>,
    mut diagnose: impl FnMut(&str),
) -> Result<()> {
    let mut cleanup = Ok(());
    for stop in stops {
        if let Err(error) = stop {
            diagnose(&format!("comparison resource cleanup failed: {error:#}"));
            if cleanup.is_ok() {
                cleanup = Err(error);
            }
        }
    }
    let cleanup_passed = cleanup.is_ok();
    let result = result.and(cleanup);
    let recorded = record(result.is_ok(), cleanup_passed);
    if let Err(error) = &recorded {
        diagnose(&format!("comparison result recording failed: {error:#}"));
    }
    result.and(recorded)
}

fn sql_hashes(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|ext| ext == "sql") {
            result.insert(
                path.file_name()
                    .context("SQL file has no name")?
                    .to_string_lossy()
                    .into_owned(),
                format!("{:x}", Sha256::digest(std::fs::read(&path)?)),
            );
        }
    }
    Ok(result)
}

fn bootstrap(sh: &Shell, urls: &DbUrls) -> Result<()> {
    let _admin = sh.push_env("DATABASE_ADMIN_URL", &urls.admin);
    let _migrator = sh.push_env(
        "FVOCI_DB_MIGRATOR_PASSWORD",
        compose::TEST_MIGRATOR_PASSWORD,
    );
    let _app = sh.push_env("FVOCI_DB_APP_PASSWORD", compose::TEST_APP_PASSWORD);
    cmd!(
        sh,
        "cargo run --quiet --package fvoci-server --locked -- db bootstrap"
    )
    .run()?;
    Ok(())
}

async fn install_candidate(url: &str, path: &Path) -> Result<()> {
    let migrator = sqlx::migrate::Migrator::new(path).await?;
    ensure!(
        migrator.iter().next().is_some(),
        "candidate contains no SQLx migrations"
    );
    let mut conn = PgConnection::connect(url).await?;
    migrator.run(&mut conn).await?;
    conn.close().await?;
    Ok(())
}

async fn install_source(sh: &Shell, urls: &DbUrls, candidate: Option<&Path>) -> Result<()> {
    if let Some(path) = candidate {
        install_candidate(&urls.migration, path).await
    } else {
        let _migration = sh.push_env("DATABASE_MIGRATION_URL", &urls.migration);
        cmd!(
            sh,
            "cargo run --quiet --package fvoci-server --locked -- db migrate"
        )
        .run()?;
        Ok(())
    }
}

async fn catalog(conn: &mut PgConnection) -> Result<Snapshot> {
    sqlx::raw_sql("SET search_path = pg_catalog; SET timezone = 'UTC';")
        .execute(&mut *conn)
        .await?;
    let mut snapshot = Snapshot::new();
    for row in sqlx::query(CATALOG).fetch_all(&mut *conn).await? {
        let key = format!(
            "{}/{}",
            row.try_get::<String, _>("kind")?,
            row.try_get::<String, _>("identity")?
        );
        ensure!(
            snapshot
                .insert(key.clone(), row.try_get("definition")?)
                .is_none(),
            "duplicate catalog identity {key}"
        );
    }
    let tables: Vec<String> = sqlx::query_scalar("SELECT c.oid::regclass::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='fvoci' AND c.relkind IN ('r','p') AND c.relname<>'_sqlx_migrations' ORDER BY c.relname").fetch_all(&mut *conn).await?;
    for table in tables {
        let rows: Value = sqlx::query_scalar(&format!("SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM {table} t")).fetch_one(&mut *conn).await?;
        snapshot.insert(format!("initial_rows/{table}"), rows);
    }
    let sequences: Vec<String> = sqlx::query_scalar("SELECT c.oid::regclass::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='fvoci' AND c.relkind='S' ORDER BY c.relname").fetch_all(&mut *conn).await?;
    for sequence in sequences {
        let state: Value = sqlx::query_scalar(&format!("SELECT jsonb_build_object('last_value',last_value,'is_called',is_called) FROM {sequence}")).fetch_one(&mut *conn).await?;
        snapshot.insert(format!("sequence_state/{sequence}"), state);
    }
    Ok(snapshot)
}

async fn snapshot(url: &str, dir: &Path) -> Result<Snapshot> {
    let mut conn = PgConnection::connect(url).await?;
    let version: String = sqlx::query_scalar("SELECT version()")
        .fetch_one(&mut conn)
        .await?;
    write_json(&dir.join("environment.json"), &json!({"postgres":version}))?;
    let state = catalog(&mut conn).await?;
    write_json(&dir.join("catalog.json"), &json!(state))?;
    let ledger: Value = sqlx::query_scalar("SELECT coalesce(jsonb_agg(to_jsonb(m) - 'installed_on' - 'execution_time' ORDER BY version),'[]'::jsonb) FROM fvoci._sqlx_migrations m").fetch_one(&mut conn).await?;
    write_json(&dir.join("migration-ledger.json"), &ledger)?;
    conn.close().await?;
    Ok(state)
}

fn differences(a: &Snapshot, b: &Snapshot) -> Vec<Value> {
    let keys: BTreeSet<_> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|key| a.get(*key) != b.get(*key))
        .map(|key| json!({"object":key,"a":a.get(key),"b":b.get(key)}))
        .collect()
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(value)?))
        .with_context(|| format!("write {}", path.display()))
}

fn normalize_dump(dump: &str, catalog: &Snapshot) -> Result<Snapshot> {
    let mut objects = Snapshot::new();
    let mut identity = "preamble".to_owned();
    let mut lines = Vec::new();
    for line in dump.lines() {
        if line.starts_with("\\restrict ")
            || line.starts_with("\\unrestrict ")
            || line.starts_with("-- Dumped ")
            || line == "-- PostgreSQL database dump complete"
        {
            continue;
        }
        if line.starts_with("-- Name: ") {
            insert_dump(&mut objects, &identity, &lines)?;
            identity = line.to_owned();
            lines.clear();
        } else {
            lines.push(canonical_check(&identity, line, catalog));
        }
    }
    insert_dump(&mut objects, &identity, &lines)?;
    Ok(objects)
}

// Restoring PostgreSQL's own dump flattens nested AND trees in CHECK constraints.
// Use PostgreSQL's pretty constraint deparser (already compared in the catalog), only
// for a named, single-line CHECK inside its exact TABLE block. Function text is untouched.
fn canonical_check(identity: &str, line: &str, catalog: &Snapshot) -> String {
    let Some((table, rest)) = identity
        .strip_prefix("-- Name: ")
        .and_then(|s| s.split_once("; Type: TABLE; Schema: "))
    else {
        return line.to_owned();
    };
    let Some((schema, _)) = rest.split_once(';') else {
        return line.to_owned();
    };
    let Some((name, body)) = line
        .strip_prefix("    CONSTRAINT ")
        .and_then(|s| s.split_once(' '))
    else {
        return line.to_owned();
    };
    if !body.starts_with("CHECK (") {
        return line.to_owned();
    }
    let key = format!("constraint/{schema}.{table}.{name}");
    let Some(definition) = catalog.get(&key).and_then(|v| v["definition"].as_str()) else {
        return line.to_owned();
    };
    format!(
        "    CONSTRAINT {name} {definition}{}",
        if line.ends_with(',') { "," } else { "" }
    )
}

fn insert_dump(objects: &mut Snapshot, identity: &str, lines: &[String]) -> Result<()> {
    let text = lines.join("\n");
    let text = text.trim().trim_end_matches('-').trim();
    ensure!(
        objects.insert(identity.to_owned(), json!(text)).is_none(),
        "duplicate pg_dump identity {identity}"
    );
    Ok(())
}

async fn self_test(url: &str) -> Result<Value> {
    let mut conn = PgConnection::connect(url).await?;
    let before = catalog(&mut conn).await?;
    let tests = [
        (
            "policy",
            "CREATE POLICY sql_compare_probe ON fvoci.documents AS PERMISSIVE FOR SELECT TO fvoci_app USING (true)",
        ),
        ("relation", "GRANT DELETE ON fvoci.sessions TO fvoci_app"),
        (
            "function",
            "CREATE OR REPLACE FUNCTION fvoci.app_user_id() RETURNS uuid LANGUAGE sql STABLE AS 'SELECT NULL::uuid'",
        ),
        (
            "constraint",
            "ALTER TABLE fvoci.files DROP CONSTRAINT files_name_check, ADD CONSTRAINT files_name_check CHECK (true)",
        ),
    ];
    let mut results = Vec::new();
    for (category, mutation) in tests {
        sqlx::raw_sql("BEGIN").execute(&mut conn).await?;
        let tested = async {
            sqlx::raw_sql(mutation).execute(&mut conn).await?;
            let changed = catalog(&mut conn).await?;
            let delta = differences(&before, &changed);
            ensure!(
                delta.iter().any(|row| row["object"]
                    .as_str()
                    .is_some_and(|key| key.starts_with(&format!("{category}/")))),
                "comparator missed {category} injection"
            );
            Ok::<_, anyhow::Error>(delta)
        }
        .await;
        sqlx::raw_sql("ROLLBACK").execute(&mut conn).await?;
        let delta = tested?;
        ensure!(
            before == catalog(&mut conn).await?,
            "{category} injection did not restore baseline"
        );
        results
            .push(json!({"mutation":category,"detected":true,"restored":true,"differences":delta}));
    }
    conn.close().await?;
    Ok(json!(results))
}

fn run_behavior(
    root: &Path,
    sh: &Shell,
    services: &Compose,
    cluster: &Cluster,
    urls: &DbUrls,
    behavior: BehaviorRun<'_>,
    output: &Path,
) -> Result<()> {
    let _app = sh.push_env("DATABASE_URL", &urls.app);
    let _direct = sh.push_env("FVOCI_TEST_DIRECT_APP_URL", &urls.app);
    let _admin = sh.push_env("FVOCI_TEST_ADMIN_URL", &urls.admin);
    let _migration = sh.push_env("FVOCI_TEST_MIGRATION_URL", &urls.migration);
    let _normal_admin = sh.push_env("DATABASE_ADMIN_URL", &urls.admin);
    let _normal_migration = sh.push_env("DATABASE_MIGRATION_URL", &urls.migration);
    let _threads = sh.push_env("RUST_TEST_THREADS", "4");
    let _storage = sh.push_env("FVOCI_STORAGE", "local");
    let _files = sh.push_env("FVOCI_STORAGE_DIR", output.join("files"));
    let _web = sh.push_env("FVOCI_WEB_DIR", "");
    // The Zotero integration tests override this with their own HTTP fixture.
    let _zotero = sh.push_env("FVOCI_ZOTERO_API_URL", "http://127.0.0.1:9");
    let _candidate = sh.push_env(
        "FVOCI_TEST_CANDIDATE_MIGRATIONS",
        behavior.candidate.unwrap_or(Path::new("")),
    );
    let search = services.host_port(sh, "meilisearch", 7700)?;
    let s3 = services.host_port(sh, "seaweedfs", 8333)?;
    let _search = sh.push_env(
        "FVOCI_TEST_SEARCH_URL",
        format!("http://127.0.0.1:{search}"),
    );
    let _search_key = sh.push_env("FVOCI_TEST_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    let _normal_search = sh.push_env("FVOCI_SEARCH_URL", format!("http://127.0.0.1:{search}"));
    let _normal_search_key = sh.push_env("FVOCI_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    let _s3 = sh.push_env("FVOCI_TEST_S3_ENDPOINT", format!("http://127.0.0.1:{s3}"));
    let _s3_keys = crate::test::push_s3_credentials(sh);
    let _normal_s3 = [
        sh.push_env("FVOCI_S3_ENDPOINT", format!("http://127.0.0.1:{s3}")),
        sh.push_env("FVOCI_S3_REGION", "us-east-1"),
        sh.push_env("FVOCI_S3_BUCKET", compose::S3_TEST_BUCKET),
        sh.push_env("FVOCI_S3_ACCESS_KEY", compose::S3_TEST_ACCESS_KEY),
        sh.push_env("FVOCI_S3_SECRET_KEY", compose::S3_TEST_SECRET_KEY),
    ];
    let _oidc = if behavior.behavior.oidc {
        let keycloak = services.host_port(sh, "keycloak", 8080)?;
        Some(sh.push_env(
            "FVOCI_TEST_OIDC_ISSUER",
            format!("http://127.0.0.1:{keycloak}/realms/{}", compose::OIDC_REALM),
        ))
    } else {
        None
    };
    let passed = format!(
        "PASS: existing {:?} suite; {}\n",
        behavior.behavior.scope,
        if behavior.behavior.oidc {
            "OIDC run with the isolated test Keycloak"
        } else {
            "OIDC not run"
        }
    );
    let _root = sh.push_dir(root);
    let _rest = if let Cluster::Supabase(c) = cluster {
        let rest = c.host_port(sh, "rest", 3000)?;
        Some(sh.push_env(
            "FVOCI_TEST_SUPABASE_REST_URL",
            format!("http://127.0.0.1:{rest}"),
        ))
    } else {
        None
    };
    println!("==> app-role behavior direct, {}", output.display());
    std::fs::write(
        output.join("behavior-direct.txt"),
        "FAIL: direct suite started but did not pass\n",
    )?;
    run_behavior_tests(
        sh,
        behavior.behavior,
        cluster,
        &output.join("behavior-direct.log"),
    )?;
    std::fs::write(output.join("behavior-direct.txt"), &passed)?;
    if let Cluster::Supabase(c) = cluster {
        let port = c.host_port(sh, "supavisor", 5432)?;
        let _pooler = sh.push_env("DATABASE_URL", DbUrls::supabase_session_pooler(port));
        let _direct = sh.push_env("FVOCI_TEST_DIRECT_APP_URL", &urls.app);
        println!("==> app-role behavior session pooler, {}", output.display());
        std::fs::write(
            output.join("behavior-session.txt"),
            "FAIL: session suite started but did not pass\n",
        )?;
        run_behavior_tests(
            sh,
            behavior.behavior,
            cluster,
            &output.join("behavior-session.log"),
        )?;
        std::fs::write(output.join("behavior-session.txt"), &passed)?;
    }
    Ok(())
}

fn run_behavior_tests(sh: &Shell, behavior: Behavior, cluster: &Cluster, log: &Path) -> Result<()> {
    match behavior.scope {
        BehaviorScope::Full => crate::test::run_tests_with_log(
            sh,
            behavior.oidc,
            true,
            matches!(cluster, Cluster::Supabase(_)),
            Some(log),
        ),
        BehaviorScope::Core => {
            let args: Vec<_> = CORE_TESTS
                .iter()
                .flat_map(|name| ["--test", *name])
                .collect();
            println!(
                "Core tests: {}; all unlisted binaries NOT RUN",
                CORE_TESTS.join(", ")
            );
            let result = cmd!(sh, "cargo test --package fvoci-server --locked {args...}")
                .ignore_status()
                .output()?;
            let mut bytes = result.stdout;
            bytes.extend_from_slice(&result.stderr);
            std::fs::write(log, bytes)?;
            ensure!(
                result.status.success(),
                "core app-role suite failed; see {}",
                log.display()
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_admin_stays_core_and_full_suite_gates_oidc() {
        let full = |oidc| {
            Some(Behavior {
                scope: BehaviorScope::Full,
                oidc,
            })
        };
        let core = |oidc| {
            Some(Behavior {
                scope: BehaviorScope::Core,
                oidc,
            })
        };
        for (behavior, supabase, restricted_admin) in [
            (None, false, false),
            (full(true), false, false),
            (full(true), true, false),
            (core(false), false, true),
        ] {
            assert!(check_options(behavior, supabase, restricted_admin).is_ok());
        }
        for (behavior, supabase, restricted_admin, expected) in [
            (
                core(true),
                false,
                false,
                "--oidc requires the full behavior scope",
            ),
            (
                core(true),
                false,
                true,
                "--oidc requires the full behavior scope",
            ),
            (
                full(true),
                false,
                true,
                "restricted admin requires --behavior-scope core",
            ),
            (None, true, true, "restricted admin requires the isolated"),
        ] {
            let error = check_options(behavior, supabase, restricted_admin).unwrap_err();
            assert!(error.to_string().starts_with(expected), "{error}");
        }
    }

    #[test]
    fn concurrent_output_claim_has_one_winner_and_preserves_existing_evidence() -> Result<()> {
        let base = std::env::temp_dir().join(format!("sql-compare-{}", uuid::Uuid::now_v7()));
        let output = base.join("nested/run");
        let gate = std::sync::Barrier::new(8);
        let attempts = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|id| {
                    let output = &output;
                    let gate = &gate;
                    scope.spawn(move || -> Result<Option<usize>> {
                        gate.wait();
                        match claim_output(output) {
                            Ok(claimed) => {
                                std::fs::write(claimed.join("owner.txt"), id.to_string())?;
                                Ok(Some(id))
                            }
                            Err(error) => {
                                assert_eq!(
                                    error.downcast_ref::<std::io::Error>().map(|e| e.kind()),
                                    Some(std::io::ErrorKind::AlreadyExists)
                                );
                                Ok(None)
                            }
                        }
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("claim thread panicked"))
                .collect::<Result<Vec<_>>>()
        })?;
        let winners: Vec<_> = attempts.into_iter().flatten().collect();
        assert_eq!(winners.len(), 1);
        let expected = winners[0].to_string();
        assert_eq!(std::fs::read_to_string(output.join("owner.txt"))?, expected);
        assert!(claim_output(&output).is_err());
        assert_eq!(std::fs::read_to_string(output.join("owner.txt"))?, expected);
        std::fs::remove_file(output.join("owner.txt"))?;
        std::fs::remove_dir(&output)?;
        std::fs::remove_dir(base.join("nested"))?;
        std::fs::remove_dir(base)?;
        Ok(())
    }

    #[test]
    fn failed_database_and_service_starts_remain_registered_for_stop() {
        for resource in ["database", "services"] {
            let mut registered = Vec::new();
            let started = register_and_start(&mut registered, resource, |_| {
                anyhow::bail!("partial startup failed")
            })
            .map(|_| ());
            let mut stopped = Vec::new();
            let mut recorded = false;
            let result = finish_run(
                started,
                registered.iter().rev().map(|resource| {
                    stopped.push(*resource);
                    Ok(())
                }),
                |passed, cleanup_passed| {
                    assert!(!passed);
                    assert!(cleanup_passed);
                    recorded = true;
                    Ok(())
                },
                |_| {},
            );
            assert_eq!(result.unwrap_err().to_string(), "partial startup failed");
            assert_eq!(stopped, [resource]);
            assert!(recorded);
        }
    }

    #[test]
    fn stop_failures_attempt_all_resources_and_write_failed_result() -> Result<()> {
        let output = std::env::temp_dir().join(format!("sql-compare-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir(&output)?;
        let mut attempted = Vec::new();
        let result = finish_run(
            Ok(()),
            ["services", "b", "a"].into_iter().map(|resource| {
                attempted.push(resource);
                match resource {
                    "services" => anyhow::bail!("first stop failed"),
                    "b" => anyhow::bail!("second stop failed"),
                    _ => Ok(()),
                }
            }),
            |passed, cleanup_passed| {
                write_json(
                    &output.join("result.json"),
                    &json!({"passed":passed,"cleanup_passed":cleanup_passed}),
                )
            },
            |_| {},
        );
        let recorded: Value = serde_json::from_slice(&std::fs::read(output.join("result.json"))?)?;
        std::fs::remove_file(output.join("result.json"))?;
        std::fs::remove_dir(output)?;
        assert_eq!(result.unwrap_err().to_string(), "first stop failed");
        assert_eq!(attempted, ["services", "b", "a"]);
        assert_eq!(recorded, json!({"passed":false,"cleanup_passed":false}));
        Ok(())
    }

    #[test]
    fn recording_failure_is_diagnosed_without_replacing_an_earlier_error() {
        for (work_failed, stop_failed, expected) in [
            (true, true, "comparison failed"),
            (true, false, "comparison failed"),
            (false, true, "stop failed"),
            (false, false, "record failed"),
        ] {
            let mut recorded = false;
            let mut diagnostics = Vec::new();
            let result = finish_run(
                if work_failed {
                    Err(anyhow::anyhow!("comparison failed"))
                } else {
                    Ok(())
                },
                [if stop_failed {
                    Err(anyhow::anyhow!("stop failed"))
                } else {
                    Ok(())
                }],
                |passed, cleanup_passed| {
                    assert_eq!(passed, !work_failed && !stop_failed);
                    assert_eq!(cleanup_passed, !stop_failed);
                    recorded = true;
                    Err(anyhow::anyhow!("disk full").context("record failed"))
                },
                |message| diagnostics.push(message.to_owned()),
            );
            assert_eq!(result.unwrap_err().to_string(), expected);
            assert!(recorded);
            let mut expected_diagnostics = Vec::new();
            if stop_failed {
                expected_diagnostics.push("comparison resource cleanup failed: stop failed");
            }
            expected_diagnostics
                .push("comparison result recording failed: record failed: disk full");
            assert_eq!(diagnostics, expected_diagnostics);
        }
    }
}
