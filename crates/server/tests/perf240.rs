//! #240 / #245: opt-in sharing measurements on the unmodified isolated T6 fixture.
//! Run with tools/measure-240.sh; credentials and services come from that stack only.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Instant;

use fvoci_core::ids::UserId;
use fvoci_server::features::accounts::{SESSION_COOKIE, VerifiedIdentity, start_session};
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Actor {
    id: UserId,
    cookie: String,
}

async fn actor(
    state: &fvoci_server::http::AppState,
    subject: &str,
    email: Option<&str>,
) -> Result<Actor> {
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: subject.to_owned(),
        email: email.map(str::to_owned),
        email_verified: email.is_some(),
        display_name: subject.to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (id, token) = start_session(&state.db, state.config.session_ttl, &identity, None).await?;
    Ok(Actor {
        id,
        cookie: format!("{SESSION_COOKIE}={token}"),
    })
}

async fn request(
    client: &reqwest::Client,
    origin: &str,
    actor: &Actor,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value> {
    let mut req = client
        .request(method, format!("{origin}{path}"))
        .header("cookie", &actor.cookie)
        .header("origin", origin)
        .header("x-fvoci-expected-user", actor.id.0.to_string());
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req.send().await?;
    let status = response.status();
    let id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = response.bytes().await?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    if !status.is_success() {
        return Err(format!("{path}: {status} {value}").into());
    }
    Ok(json!({"body": value, "request_id": id}))
}

async fn explain(conn: &mut PgConnection, actor: UserId, sql: &str, org: Uuid) -> Result<Value> {
    let mut tx = conn.begin().await?;
    sqlx::query("SET LOCAL ROLE fvoci_app")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(actor.0.to_string())
        .execute(&mut *tx)
        .await?;
    let plan: Value = sqlx::query_scalar(&format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {sql}"))
        .bind(org)
        .fetch_one(&mut *tx)
        .await?;
    tx.rollback().await?;
    Ok(plan)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "#240: requires isolated T6 stack; tools/measure-240.sh"]
async fn shares() -> Result<()> {
    let (filter, format) = fvoci_server::logging::settings(|key| std::env::var(key).ok())?;
    fvoci_server::logging::init(filter, format);
    let root = PathBuf::from(std::env::var("FVOCI_MEASURE_DIR")?);
    let record: Value = serde_json::from_slice(&std::fs::read(root.join("state.json"))?)?;
    let source = record["source_db"].as_str().ok_or("missing source_db")?;
    let mut db = url::Url::parse(&std::env::var("DATABASE_URL")?)?;
    db.set_path(source);
    let mut admin_url = url::Url::parse(&std::env::var("FVOCI_TEST_ADMIN_URL")?)?;
    admin_url.set_path(source);
    let mut admin = PgConnection::connect(admin_url.as_str()).await?;
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_stat_statements")
        .execute(&mut admin)
        .await?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let files = root.join("source-files").to_string_lossy().into_owned();
    let config = fvoci_server::config::Config::from_lookup(|key| match key {
        "DATABASE_URL" => Some(db.to_string()),
        "FVOCI_PUBLIC_URL" => Some(origin.clone()),
        "FVOCI_STORAGE_DIR" => Some(files.clone()),
        "FVOCI_SEARCH_URL" => std::env::var("FVOCI_TEST_SEARCH_URL").ok(),
        "FVOCI_SEARCH_KEY" => std::env::var("FVOCI_TEST_SEARCH_KEY").ok(),
        _ => None,
    })?;
    let state = fvoci_server::app::build_state(config)?;
    let owner = actor(
        &state,
        record["subject"].as_str().ok_or("missing subject")?,
        None,
    )
    .await?;
    let email = "perf240-recipient@example.test";
    let recipient = actor(&state, "perf240-recipient", Some(email)).await?;
    let reader = actor(
        &state,
        "perf240-reader",
        Some("perf240-reader@example.test"),
    )
    .await?;
    let organization: Uuid =
        sqlx::query_scalar("SELECT organization_id FROM fvoci.workspaces WHERE id=$1")
            .bind(Uuid::parse_str(
                record["workspace"].as_str().ok_or("missing workspace")?,
            )?)
            .fetch_one(&mut admin)
            .await?;
    let server_state = state.clone();
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        axum::serve(listener, fvoci_server::http::router(server_state))
            .with_graceful_shutdown(async {
                let _shutdown_result = stopped.await;
            })
            .await
    });
    let client = reqwest::Client::new();
    let invitation = request(
        &client,
        &origin,
        &owner,
        Method::POST,
        &format!("/api/organizations/{organization}/invitations"),
        Some(json!({"email": email, "role":"member"})),
    )
    .await?;
    let invitation = invitation["body"]["id"]
        .as_str()
        .ok_or("missing invitation")?;
    request(
        &client,
        &origin,
        &recipient,
        Method::POST,
        &format!("/api/invitations/{invitation}/accept"),
        None,
    )
    .await?;
    // Two additional spaces use the product API; each gets activity from wiki documents.
    let mut spaces = vec![Uuid::parse_str(
        record["workspace"].as_str().ok_or("workspace")?,
    )?];
    let invitation = request(
        &client,
        &origin,
        &owner,
        Method::POST,
        &format!("/api/workspaces/{}/invitations", spaces[0]),
        Some(json!({"email":"perf240-reader@example.test", "role":"readonly"})),
    )
    .await?;
    let invitation = invitation["body"]["id"]
        .as_str()
        .ok_or("reader invitation")?;
    request(
        &client,
        &origin,
        &reader,
        Method::POST,
        &format!("/api/invitations/{invitation}/accept"),
        None,
    )
    .await?;
    for n in 2..=3 {
        let ws = request(&client, &origin, &owner, Method::POST, "/api/workspaces",
            Some(json!({"request_id":Uuid::now_v7(), "organization_id":organization, "name":format!("perf240-{n}")}))).await?;
        spaces.push(Uuid::parse_str(
            ws["body"]["id"].as_str().ok_or("workspace id")?,
        )?);
    }
    let search = state.search.as_ref().ok_or("search not configured")?;
    fvoci_server::search::prepare(&state.db, search).await?;
    let mut docs = Vec::new();
    let mut measurements = Vec::new();
    // #245 home decomposition can stop after a requested scale. The default
    // remains the original three-scale sharing experiment.
    let max_shares =
        std::env::var("FVOCI_PERF240_MAX_SHARES").map_or(Ok(10000usize), |value| value.parse())?;
    if ![100, 1000, 10000].contains(&max_shares) {
        return Err("FVOCI_PERF240_MAX_SHARES must be 100, 1000 or 10000".into());
    }
    for count in [100usize, 1000, 10000]
        .into_iter()
        .filter(|n| *n <= max_shares)
    {
        // Product SQL functions create both documents and shares, including audit/activity.
        // One transaction per document keeps setup bounded; no direct INSERT is used.
        while docs.len() < count {
            let mut tx = state.db.begin_as(owner.id).await?;
            let doc: Uuid =
                if let Some(id) = record["documents"].get(docs.len()).and_then(Value::as_str) {
                    Uuid::parse_str(id)?
                } else {
                    sqlx::query_scalar("SELECT app_create_wiki_document($1,$2,$3,1,$4,NULL,20,$5)")
                        .bind(spaces[docs.len() % spaces.len()])
                        .bind(Uuid::now_v7())
                        .bind(format!("문서 perf240 shared {:05}", docs.len()))
                        .bind(json!({"type":"doc","content":[]}))
                        .bind((0..3).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
                        .fetch_one(tx.conn())
                        .await?
                };
            sqlx::query("SELECT app_share_document($1,$2,'read',$3,$4)")
                .bind(doc)
                .bind(email)
                .bind(organization)
                .bind((0..10).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
                .execute(tx.conn())
                .await?;
            if docs.len() >= 9900 {
                let changed: bool = sqlx::query_scalar(
                    "SELECT app_set_document_settings($1,'private',NULL,'workspace',NULL,$2)",
                )
                .bind(doc)
                .bind((0..10).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
                .fetch_one(tx.conn())
                .await?;
                assert!(
                    changed,
                    "new wiki document must still have its initial visibility"
                );
            }
            tx.commit().await?;
            docs.push(doc);
        }
        if count == 100 {
            let mut tx = state.db.begin_as(recipient.id).await?;
            for doc in docs.iter().take(10) {
                sqlx::query("SELECT app_set_mark('pin','document',$1,true)")
                    .bind(doc)
                    .execute(tx.conn())
                    .await?;
            }
            tx.commit().await?;
        }
        let reindex_started = Instant::now();
        loop {
            fvoci_server::search::drain(&state.db, search, Some(&state.storage)).await?;
            let left: i64 = sqlx::query_scalar("SELECT count(*) FROM fvoci.search_queue")
                .fetch_one(&mut admin)
                .await?;
            if left == 0 {
                break;
            }
            if reindex_started.elapsed().as_secs() >= 1800 {
                return Err("isolated sharing reindex did not finish in 30 minutes".into());
            }
        }
        sqlx::query("ANALYZE").execute(&mut admin).await?;
        for repeat in 1..=3 {
            let mut http = Vec::new();
            for (kind, path) in [
                (
                    "document",
                    format!(
                        "/api/search?organization_id={organization}&q=%EB%AC%B8%EC%84%9C&kinds=document"
                    ),
                ),
                (
                    "file",
                    format!(
                        "/api/search?organization_id={organization}&q=%EB%A9%94%EB%AA%A8&kinds=file"
                    ),
                ),
                (
                    "home",
                    format!("/api/me/home?organization_id={organization}&tz=Asia%2FSeoul"),
                ),
                (
                    "shared",
                    format!("/api/me/shared?organization_id={organization}"),
                ),
            ] {
                let started = Instant::now();
                let response =
                    request(&client, &origin, &recipient, Method::GET, &path, None).await?;
                if kind == "document" || kind == "file" {
                    assert!(
                        !response["body"]
                            .as_array()
                            .ok_or("search response is not an array")?
                            .is_empty(),
                        "{kind} search must exercise visible candidates"
                    );
                }
                http.push(json!({"kind":kind,"elapsed_ms":started.elapsed().as_secs_f64()*1000.,"request_id":response["request_id"],"response":response["body"]}));
            }
            let mut plans = serde_json::Map::new();
            for (label, query) in [
                ("shared", "SELECT * FROM fvoci.app_shared_with_me($1)"),
                ("home", "SELECT fvoci.app_home($1,'Asia/Seoul')"),
                (
                    "documents",
                    "SELECT 'document'::text,id FROM fvoci.app_shared_with_me($1)",
                ),
                (
                    "files",
                    "SELECT 'file'::text,f.id FROM fvoci.files f JOIN fvoci.app_shared_with_me($1) s ON s.id=f.document_id",
                ),
            ] {
                plans.insert(
                    label.to_owned(),
                    explain(&mut admin, recipient.id, query, organization).await?,
                );
            }
            // Same UUID quoting and organization-only scope as visibility::scope_filter.
            let mut filters = serde_json::Map::new();
            let mut tx = state.db.begin_as(recipient.id).await?;
            let workspaces: Vec<Uuid> = sqlx::query_scalar("SELECT app_my_workspace_ids($1)")
                .bind(organization)
                .fetch_all(tx.conn())
                .await?;
            assert!(
                workspaces.is_empty(),
                "recipient fixture must have only explicit shares"
            );
            let shared: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM app_shared_with_me($1)")
                .bind(organization)
                .fetch_all(tx.conn())
                .await?;
            assert_eq!(shared.len(), count);
            for kind in ["document", "file"] {
                let ids: Vec<Uuid> = if kind == "document" {
                    shared.clone()
                } else {
                    sqlx::query_scalar("SELECT f.id FROM files f JOIN app_shared_with_me($1) s ON s.id=f.document_id")
                        .bind(organization).fetch_all(tx.conn()).await?
                };
                let mut scope = format!("owner_user_id = \"{}\"", recipient.id.0);
                if !ids.is_empty() {
                    let quoted: Vec<String> = ids.iter().map(|id| format!("\"{id}\"")).collect();
                    write!(
                        scope,
                        " OR (kind = \"{kind}\" AND id IN [{}])",
                        quoted.join(", ")
                    )?;
                }
                let filter = format!("((kind IN [\"{kind}\"]) AND ({scope}))");
                filters.insert(
                    kind.to_owned(),
                    json!({"bytes":filter.len(),"ids":ids.len()}),
                );
            }
            tx.commit().await?;
            measurements.push(
                json!({"n":count,"repeat":repeat,"http":http,"plans":plans,"filters":filters}),
            );
            std::fs::write(
                root.join("sharing.json"),
                serde_json::to_vec_pretty(&measurements)?,
            )?;
        }
        if std::env::var_os("FVOCI_PERF240_HOME_PROBE").is_some() {
            std::fs::write(
                root.join(format!("home-{count}-ready.json")),
                serde_json::to_vec_pretty(
                    &json!({"organization":organization,"actor":recipient.id,"n":count}),
                )?,
            )?;
            let started = Instant::now();
            while !root.join(format!("home-{count}-continue")).exists() {
                if started.elapsed().as_secs() >= 600 {
                    return Err("home probe did not complete in 10 minutes".into());
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
    std::fs::write(
        root.join("sharing-fixture.json"),
        serde_json::to_vec_pretty(
            &json!({"organization":organization,"owner":owner.id,"recipient":recipient.id,"reader":reader.id,"spaces":spaces,"documents":docs}),
        )?,
    )?;
    stop.send(()).map_err(|()| "server already stopped")?;
    server.await??;
    state.db.close().await;
    Ok(())
}
