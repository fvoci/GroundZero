//! #210: serve unchanged product routing and T6 data for the browser measurement.
//! Run only through tools/measure-perf.sh in the isolated test Compose stack.

use std::path::{Path, PathBuf};
use std::time::Duration;

use fvoci_server::features::accounts::{SESSION_COOKIE, VerifiedIdentity, start_session};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};

fn verify_web_build(web: &Path, head: &str) -> Result<(), std::io::Error> {
    if !web.join("index.html").is_file() {
        return Err(std::io::Error::other("build the production web first"));
    }
    if std::fs::read_to_string(web.join(".perf-head"))?.trim() != head.trim() {
        return Err(std::io::Error::other(
            "web bundle is not from the current HEAD",
        ));
    }
    Ok(())
}

#[test]
fn web_build_rejects_missing_or_stale_evidence() -> Result<(), Box<dyn std::error::Error>> {
    let web = std::env::temp_dir().join(format!("fvoci-perf-web-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&web)?;
    assert!(verify_web_build(&web, "current").is_err());
    std::fs::write(web.join("index.html"), "fixture")?;
    assert!(verify_web_build(&web, "current").is_err());
    std::fs::write(web.join(".perf-head"), "old\n")?;
    assert!(verify_web_build(&web, "current").is_err());
    std::fs::write(web.join(".perf-head"), "current\n")?;
    verify_web_build(&web, "current")?;
    std::fs::remove_dir_all(web)?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "#210 baseline; isolated T6 stack and production web build required"]
async fn serve_t6() -> Result<(), Box<dyn std::error::Error>> {
    let (filter, format) = fvoci_server::logging::settings(|key| std::env::var(key).ok())?;
    fvoci_server::logging::init(filter, format);
    let root = PathBuf::from(std::env::var("FVOCI_MEASURE_DIR")?);
    let record: Value = serde_json::from_slice(&std::fs::read(root.join("state.json"))?)?;
    let mut database = url::Url::parse(&std::env::var("DATABASE_URL")?)?;
    database.set_path(record["source_db"].as_str().ok_or("missing source_db")?);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let files = root.join("source-files").to_string_lossy().into_owned();
    let web = std::env::current_dir()?.join("../../apps/web/dist");
    let head = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()?;
    if !head.status.success() {
        return Err("cannot determine the measurement HEAD".into());
    }
    verify_web_build(&web, std::str::from_utf8(&head.stdout)?)?;
    let config = fvoci_server::config::Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(database.to_string()),
        "FVOCI_PUBLIC_URL" => Some(origin.clone()),
        "FVOCI_WEB_DIR" => Some(web.to_string_lossy().into_owned()),
        "FVOCI_STORAGE_DIR" => Some(files.clone()),
        _ => None,
    })?;
    let state = fvoci_server::app::build_state(config)?;
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: record["subject"]
            .as_str()
            .ok_or("missing subject")?
            .to_owned(),
        email: None,
        email_verified: false,
        display_name: "실측 사용자".to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (user, token) = start_session(&state.db, state.config.session_ttl, &identity, None).await?;
    let mut fixture_url = url::Url::parse(&std::env::var("FVOCI_TEST_MIGRATION_URL")?)?;
    fixture_url.set_path(record["source_db"].as_str().ok_or("missing source_db")?);
    let mut fixture = PgConnection::connect(fixture_url.as_str()).await?;
    let item = record["task_items"][0].as_str().ok_or("missing item")?;
    let task: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM fvoci.tasks WHERE item_id=$1 ORDER BY created_at, id LIMIT 1",
    )
    .bind(uuid::Uuid::parse_str(item)?)
    .fetch_one(&mut fixture)
    .await?;
    fixture.close().await?;
    std::fs::write(
        root.join("browser.json"),
        serde_json::to_vec_pretty(&json!({
            "origin": origin, "cookie_name": SESSION_COOKIE, "cookie": token,
            "user": user.0, "workspace": record["workspace"], "item": item,
            "document": record["documents"][5], "task": task,
        }))?,
    )?;
    axum::serve(listener, fvoci_server::http::router(state.clone()))
        .with_graceful_shutdown(async move {
            while !root.join("STOP").exists() {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;
    state.db.close().await;
    Ok(())
}
