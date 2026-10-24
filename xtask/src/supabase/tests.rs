//! Exercise the actual setup commands without Docker. The live SCRAM negative control and
//! fresh/kept direct/session runs are separate integration evidence, not timing assertions.

use super::*;
use std::os::unix::fs::PermissionsExt;

const COMMAND: &str = r#"#!/bin/sh
case "$0" in
  */cargo) case "$*" in
    *'db bootstrap') step=bootstrap ;;
    *'db migrate') step=migrate ;;
    *) exit 91 ;;
  esac ;;
  */docker) case "$*" in
    *'stop supavisor') step=stop ;;
    *'up --detach --wait db rest') step=database ;;
    *'up --detach --wait supavisor') step=pooler ;;
    *'port db 5432') printf '127.0.0.1:30001\n'; exit 0 ;;
    *'ps --all --quiet '*) printf 'diagnose\n' >> "$SETUP_LOG"; exit 0 ;;
    *) exit 92 ;;
  esac ;;
esac
printf '%s\n' "$step" >> "$SETUP_LOG"
# Every Docker mutation must be scoped to the handle's project.
case "$0" in */docker)
  case "$*" in *'--project-name fvoci-synthetic-setup '*) ;; *) exit 93 ;; esac ;;
esac
if [ "$FAIL_AT" = "$step" ]; then
  printf 'deliberate %s failure\n' "$step" >&2
  exit 42
fi
"#;

fn preparation(fail_at: &str) -> Result<(Result<compose::DbUrls>, Vec<String>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("repository root")?
        .join("target/supabase-setup-tests")
        .join(uuid::Uuid::now_v7().to_string());
    std::fs::create_dir_all(&root)?;
    let result = (|| {
        for command in ["docker", "cargo"] {
            let file = root.join(command);
            std::fs::write(&file, COMMAND)?;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o700))?;
        }
        let sh = Shell::new()?;
        let _path = sh.push_env("PATH", &root);
        let _log = sh.push_env("SETUP_LOG", root.join("commands.log"));
        let _failure = sh.push_env("FAIL_AT", fail_at);
        let supabase = Supabase {
            root: root.clone(),
            upstream: root.clone(),
            project: "fvoci-synthetic-setup".into(),
        };
        let result = supabase.prepare_database(&sh);
        let commands = std::fs::read_to_string(root.join("commands.log"))?
            .lines()
            .map(str::to_owned)
            .collect();
        Ok((result, commands))
    })();
    std::fs::remove_dir_all(&root)?;
    result
}

/// Explicitly selected by Supabase CI; default unit checks never start Docker services.
#[test]
#[ignore = "isolated local Docker Supabase; run explicitly with --ignored --test-threads=1"]
fn kept_pooler_restarts_after_role_preparation() -> Result<()> {
    use sqlx::{ConnectOptions, Connection, PgConnection};
    use std::time::Duration;

    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("repository root")?;
    let root = repository
        .join("target/supabase-readiness-tests")
        .join(uuid::Uuid::now_v7().to_string());
    std::fs::create_dir_all(root.join("deploy/supabase"))?;
    std::fs::create_dir_all(root.join("target"))?;
    for file in ["compose.test.yaml", "test.env"] {
        std::fs::copy(
            repository.join("deploy/supabase").join(file),
            root.join("deploy/supabase").join(file),
        )?;
    }
    // Share only downloaded official configuration, never DB state or Compose resources.
    upstream(repository)?;
    std::os::unix::fs::symlink(
        repository.join("target/supabase"),
        root.join("target/supabase"),
    )?;
    let supabase = Supabase::new(&root)?;
    let sh = Shell::new()?;
    sh.change_dir(repository);
    let args = supabase.args();
    let args = &args;
    let prior = cmd!(sh, "docker {args...} ps --all --quiet")
        .quiet()
        .read()?;
    anyhow::ensure!(
        prior.trim().is_empty(),
        "refuse to reuse existing resources"
    );
    println!(
        "CREATE project={} root={}",
        supabase.project(),
        root.display()
    );
    let runtime = tokio::runtime::Runtime::new()?;
    let result = (|| -> Result<()> {
        let urls = supabase.prepare_database(&sh)?;
        let ids = cmd!(sh, "docker {args...} ps --all --quiet")
            .quiet()
            .read()?;
        println!("OWNED containers={ids}");
        let db_id = cmd!(sh, "docker {args...} ps --quiet db").quiet().read()?;
        cmd!(sh, "docker exec {db_id} psql -X -U supabase_admin -d postgres -At -c 'SELECT system_identifier FROM pg_control_system()'")
            .run()?;
        let port = supabase.host_port(&sh, "supavisor", 5432)?;
        let options: sqlx::postgres::PgConnectOptions =
            compose::DbUrls::supabase_session_pooler(port).parse()?;
        let options = options.disable_statement_logging();
        let direct: sqlx::postgres::PgConnectOptions = urls.app.parse()?;
        runtime.block_on(async {
            let mut held = PgConnection::connect_with(&options).await?;
            sqlx::query("SELECT 1").execute(&mut held).await?;
            for round in 1..=2 {
                // Exercise real bootstrap's same-password reset while the prior pooler was warm.
                supabase.prepare_database(&sh)?;
                let old = tokio::time::timeout(Duration::from_secs(5), sqlx::query("SELECT 1").execute(&mut held)).await?;
                anyhow::ensure!(old.is_err(), "old pooler connection survived restart");
                drop(held);
                // Restarting a stopped container can assign another dynamic host port.
                let port = supabase.host_port(&sh, "supavisor", 5432)?;
                let options = options.clone().port(port);
                let mut first = PgConnection::connect_with(&options).await?;
                let mut second = PgConnection::connect_with(&options).await?;
                for connection in [&mut first, &mut second] {
                    let user: String = sqlx::query_scalar("SELECT current_user").fetch_one(connection).await?;
                    anyhow::ensure!(user == "fvoci_app", "wrong session role");
                }
                let mut direct = PgConnection::connect_with(&direct).await?;
                sqlx::query("SELECT 1").execute(&mut direct).await?;
                direct.close().await?;
                let wrong = PgConnection::connect_with(&options.clone().password("deliberately-wrong-synthetic-password")).await;
                let error = wrong.err().context("wrong password must fail")?;
                anyhow::ensure!(error.as_database_error().and_then(|e| e.code()).as_deref() == Some("28P01"), "wrong-password error lost");
                println!("PASS kept round {round}: held backend replaced; two sessions + direct; wrong password rejected 28P01");
                second.close().await?;
                held = first;
            }
            held.close().await?;
            Ok(())
        })
    })();
    if result.is_err() {
        supabase.diagnose(&sh);
    }
    let cleanup = supabase.down(&sh);
    if let Err(error) = &cleanup {
        eprintln!("cleanup of {} failed: {error:#}", supabase.project());
    }
    result?;
    cleanup?;
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn pooler_stays_stopped_until_bootstrap_and_migration_finish() -> Result<()> {
    let (result, commands) = preparation("")?;
    let urls = result?;
    assert!(urls.app.contains("127.0.0.1:30001"));
    assert_eq!(
        commands,
        ["stop", "database", "bootstrap", "migrate", "pooler"]
    );
    Ok(())
}

#[test]
fn preparation_failure_stops_setup_and_preserves_original_error() -> Result<()> {
    let steps = ["stop", "database", "bootstrap", "migrate", "pooler"];
    for (index, step) in steps.iter().enumerate() {
        let (result, commands) = preparation(step)?;
        let error = result.expect_err("a setup command deliberately exits 42");
        assert!(format!("{error:#}").contains("42"), "{error:#}");
        assert_eq!(&commands[..=index], &steps[..=index]);
        assert_eq!(&commands[index + 1..], ["diagnose", "diagnose", "diagnose"]);
    }
    Ok(())
}
