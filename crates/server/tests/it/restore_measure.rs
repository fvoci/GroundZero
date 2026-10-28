#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 백업 → 빈 설치 복구 실측 (D-128 (2), 4단계 T6). 기본 실행에서는 빠지는 `#[ignore]` 시험 둘이다. 연구·프로젝트 예시
//! 데이터를 실제 API(`fvoci_app` 권한 경로, 프로세스 안 라우터)로 만들고(`restore_measure_generate`), 그 DB와 저장소를
//! `backup::create`(서버 CLI `backup create`가 부르는 함수)로 백업해 같은 마이그레이션의 빈 설치에 `backup::restore`로
//! 복구한 시간과 누락 여부를 잰다(`restore_measure_run`). RPO·RTO 목표는 정하지 않는다.
//!
//! 두 시험은 `cargo xtask test --keep`이 띄워 둔 시험 스택의 서비스 주소를 환경 변수로 받는다(`FVOCI_TEST_ADMIN_URL`·
//! `FVOCI_TEST_MIGRATION_URL`·`DATABASE_URL`·`FVOCI_TEST_SEARCH_URL`·`FVOCI_TEST_SEARCH_KEY`). 그 밖의 환경 변수:
//! - `FVOCI_MEASURE_DIR`(필수): 원본 저장소·백업·복구 저장소·`state.json`·`summary.json`을 두는 폴더. 지우지 않는다.
//! - `FVOCI_MEASURE_DOCS`(1000)·`_ITEMS`(종류마다 항목 수, 5)·`_TASKS`(프로젝트 항목마다 업무 수, 40)·`_BIG_FILES`(64)·
//!   `_BIG_MIB`(16)·`_SMALL_FILES`(20): 데이터 모양.
//!
//! 실행 예: `cargo test --release -p fvoci-server --test it -- --ignored --exact restore_measure::restore_measure_generate
//! --nocapture` 다음 같은 모양으로 `restore_measure_run`. 시간은 `--release`로 재야 제품 빌드와 같다.

use crate::common;

use std::path::{Path, PathBuf};
use std::time::Instant;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, config_with, fresh_database, new_request_id, send, send_raw, with_database,
};
use fvoci_core::secret::Secret;
use fvoci_db::config::DbCommandConfig;
use fvoci_server::backup;
use fvoci_server::features::accounts::{SESSION_COOKIE, VerifiedIdentity, start_session};
use fvoci_server::http::AppState;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

const MIB: usize = 1024 * 1024;
/// 서버 기본 업로드 조각 크기(`FVOCI_UPLOAD_CHUNK_BYTES`)와 같게 올린다.
const CHUNK: usize = 8 * MIB;

struct Shape {
    docs: usize,
    items_per_kind: usize,
    tasks_per_item: usize,
    big_files: usize,
    big_mib: usize,
    small_files: usize,
}

fn env_or(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .map_or(default, |v| v.parse().expect("a number"))
}

impl Shape {
    fn from_env() -> Self {
        Self {
            docs: env_or("FVOCI_MEASURE_DOCS", 1000),
            items_per_kind: env_or("FVOCI_MEASURE_ITEMS", 5),
            tasks_per_item: env_or("FVOCI_MEASURE_TASKS", 40),
            big_files: env_or("FVOCI_MEASURE_BIG_FILES", 64),
            big_mib: env_or("FVOCI_MEASURE_BIG_MIB", 16),
            small_files: env_or("FVOCI_MEASURE_SMALL_FILES", 20),
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(std::env::var("FVOCI_MEASURE_DIR").expect("set FVOCI_MEASURE_DIR"))
}

fn env(name: &str) -> String {
    std::env::var(name).expect("unset: run with the kept test stack")
}

/// 지금 시각과 호스트 부하(실측 창의 시작·끝 기록용).
fn stamp() -> String {
    let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
    format!(
        "{} loadavg {}",
        time::OffsetDateTime::now_utc(),
        load.trim()
    )
}

async fn session(state: &AppState, subject: &str) -> TestUser {
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: subject.to_owned(),
        email: None,
        email_verified: false,
        display_name: "실측 사용자".to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (id, token) = start_session(&state.db, state.config.session_ttl, &identity, None)
        .await
        .unwrap();
    TestUser {
        id,
        cookie: format!("{SESSION_COOKIE}={token}"),
    }
}

/// 이 실측의 DB 연결 구성(같은 클러스터의 이름 붙은 DB).
fn db_config(name: &str) -> (DbCommandConfig, String) {
    let app = std::env::var("FVOCI_TEST_DIRECT_APP_URL").unwrap_or_else(|_| env("DATABASE_URL"));
    let db = DbCommandConfig {
        admin_url: Some(Secret::new(with_database(
            &env("FVOCI_TEST_ADMIN_URL"),
            name,
        ))),
        migration_url: Some(Secret::new(with_database(
            &env("FVOCI_TEST_MIGRATION_URL"),
            name,
        ))),
        migrator_password: None,
        app_password: None,
    };
    (db, with_database(&app, name))
}

fn state_for(app_url: &str, files: &Path, search: bool) -> AppState {
    let dir = files.to_string_lossy().into_owned();
    let url = std::env::var("FVOCI_TEST_SEARCH_URL").unwrap_or_default();
    let key = std::env::var("FVOCI_TEST_SEARCH_KEY").unwrap_or_default();
    let mut overrides = vec![
        ("FVOCI_STORAGE_DIR", dir.as_str()),
        ("DATABASE_URL", app_url),
    ];
    if search {
        overrides.push(("FVOCI_SEARCH_URL", url.as_str()));
        overrides.push(("FVOCI_SEARCH_KEY", key.as_str()));
    }
    fvoci_server::app::build_state(config_with(&overrides)).unwrap()
}

/// 같은 시드면 같은 바이트(실측을 되풀이할 때 같은 내용). 압축으로 줄지 않는 의사 난수다.
fn pseudo_random(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed;
    let mut out = Vec::with_capacity(len + 8);
    while out.len() < len {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        out.extend_from_slice(&(z ^ (z >> 31)).to_le_bytes());
    }
    out.truncate(len);
    out
}

fn doc_text(n: usize, revision: usize) -> Value {
    let paragraphs: Vec<Value> = (0..4)
        .map(|p| {
            json!({ "type": "paragraph", "content": [{ "type": "text", "text": format!(
                "연구·프로젝트 노트 {n} 개정 {revision} 문단 {p}. {}",
                "실험 설정과 측정 결과, 회의에서 정한 일정과 담당을 기록한다. ".repeat(4)
            ) }] })
        })
        .collect();
    json!({ "type": "doc", "content": paragraphs })
}

async fn ok(app: &Router, user: &TestUser, method: Method, uri: &str, body: Value) -> Value {
    let r = send(app, method, uri, Some(user), Some(body)).await;
    assert!(r.status.is_success(), "{uri} -> {} {:?}", r.status, r.json);
    r.json
}

fn id_of(value: &Value) -> String {
    value["id"].as_str().unwrap().to_owned()
}

/// 파일 하나를 조각으로 올리고 파일 ID를 돌려준다(실제 업로드 API).
async fn upload(
    app: &Router,
    user: &TestUser,
    doc: &str,
    name: &str,
    content_type: &str,
    bytes: &[u8],
) -> String {
    let started = ok(
        app,
        user,
        Method::POST,
        "/api/uploads",
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc },
                 "name": name, "size": bytes.len(), "content_type": content_type }),
    )
    .await;
    let upload = id_of(&started);
    for (index, part) in bytes.chunks(CHUNK).enumerate() {
        let hash = hex::encode(Sha256::digest(part));
        let (status, _, _) = send_raw(
            app,
            Method::PUT,
            &format!("/api/uploads/{upload}/parts/{}", index + 1),
            Some(user),
            &[("x-content-sha256", hash.as_str())],
            part.to_vec(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::NO_CONTENT,
            "part {} of {name}",
            index + 1
        );
    }
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{upload}/complete"),
        Some(user),
        None,
    )
    .await;
    assert!(done.status.is_success(), "{name}: {:?}", done.json);
    done.json["file_id"].as_str().unwrap().to_owned()
}

fn dir_bytes(path: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_bytes(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

async fn db_bytes(url: &str) -> i64 {
    let mut conn = PgConnection::connect(url).await.unwrap();
    let size = sqlx::query_scalar("SELECT pg_database_size(current_database())")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    conn.close().await.unwrap();
    size
}

/// 원본 설치에 예시 데이터를 만든다. 만든 DB 이름·계정·ID 목록은 `state.json`에 남는다.
#[tokio::test]
#[ignore = "T6 measurement (D-128 (2)); needs the kept test stack, see the module comment"]
async fn restore_measure_generate() {
    let shape = Shape::from_env();
    let root = root();
    let started = Instant::now();
    println!("generate start {}", stamp());
    let mut admin = PgConnection::connect(&env("FVOCI_TEST_ADMIN_URL"))
        .await
        .unwrap();
    let source = fresh_database(&mut admin, "measure_src").await;
    let files = root.join("source-files");
    std::fs::create_dir_all(&files).unwrap();
    let state = state_for(&source.app_url, &files, false);
    let app = common::app(&state);
    let subject = format!("measure-{}", uuid::Uuid::now_v7());
    let owner = session(&state, &subject).await;

    // 별도 설치의 조직·관리자는 그 설치 DB에만 시드한다(D-111·D-112).
    let mut fixture = PgConnection::connect(source.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    sqlx::query("INSERT INTO fvoci.server_admins (user_id) VALUES ($1)")
        .bind(owner.id.0)
        .execute(&mut fixture)
        .await
        .unwrap();
    let mut tx = fixture.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1::text, true)")
        .bind(owner.id.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    let organization: uuid::Uuid =
        sqlx::query_scalar("SELECT fvoci.app_create_organization($1, $2, $3, $4)")
            .bind(uuid::Uuid::now_v7())
            .bind("실측 조직")
            .bind(owner.id.0)
            .bind((0..3).map(|_| uuid::Uuid::now_v7()).collect::<Vec<_>>())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    fixture.close().await.unwrap();
    let workspace = ok(
        &app,
        &owner,
        Method::POST,
        "/api/workspaces",
        json!({ "request_id": new_request_id(), "organization_id": organization, "name": "연구·프로젝트 실측" }),
    )
    .await;
    let ws = id_of(&workspace);

    // 모듈(연구·프로젝트)과 항목.
    let mut items: Vec<(&str, String)> = Vec::new();
    for (kind, key) in [("research", "RSCH"), ("project", "PROJ")] {
        let module = ok(
            &app,
            &owner,
            Method::POST,
            &format!("/api/workspaces/{ws}/modules"),
            json!({ "request_id": new_request_id(), "kind": kind, "name": format!("실측 {kind}"), "key": key }),
        )
        .await;
        for i in 0..shape.items_per_kind {
            let item = ok(
                &app,
                &owner,
                Method::POST,
                &format!("/api/modules/{}/items", id_of(&module)),
                json!({ "request_id": new_request_id(), "title": format!("{kind} 항목 {i}") }),
            )
            .await;
            items.push((kind, id_of(&item)));
        }
    }

    // 문서: 항목에 나눠 트리(40건마다 뿌리, 한 부모에 자식 넷)로 만들고 두 번 저장해 리비전을 쌓는다.
    let docs_started = Instant::now();
    let mut per_item: Vec<Vec<String>> = vec![Vec::new(); items.len()];
    let mut doc_ids = Vec::new();
    for n in 0..shape.docs {
        let slot = n % items.len();
        let ids = &per_item[slot];
        let j = ids.len();
        let parent = (!j.is_multiple_of(40)).then(|| ids[(j / 40) * 40 + (j % 40 - 1) / 4].clone());
        let doc = ok(
            &app,
            &owner,
            Method::POST,
            &format!("/api/items/{}/documents", items[slot].1),
            json!({ "request_id": new_request_id(), "title": format!("문서 {n:04}"), "parent_id": parent }),
        )
        .await;
        let id = id_of(&doc);
        let mut revision = doc["revision"].as_i64().unwrap();
        for save in 1..=2 {
            let saved = ok(
                &app,
                &owner,
                Method::PUT,
                &format!("/api/documents/{id}"),
                json!({ "request_id": new_request_id(), "base_revision": revision,
                         "title": format!("문서 {n:04}"), "body": doc_text(n, save) }),
            )
            .await;
            revision = saved["revision"].as_i64().unwrap();
        }
        per_item[slot].push(id.clone());
        doc_ids.push(id);
    }
    let docs_secs = docs_started.elapsed().as_secs_f64();

    // 업무: 프로젝트 항목마다 일정이 이어지는 업무(선행 관계), 다섯 건에 한 건은 담당 지정.
    let tasks_started = Instant::now();
    let mut task_count = 0usize;
    let day0 = time::Date::from_calendar_date(2026, time::Month::November, 2).unwrap();
    for (_, item) in items.iter().filter(|(kind, _)| *kind == "project") {
        let mut previous: Option<String> = None;
        for k in 0..shape.tasks_per_item {
            let start = day0 + time::Duration::days(i64::try_from(3 * k).unwrap());
            let due = start + time::Duration::days(2);
            let task = ok(
                &app,
                &owner,
                Method::POST,
                &format!("/api/items/{item}/tasks"),
                json!({ "request_id": new_request_id(), "title": format!("업무 {k:03}"), "parent_id": null,
                         "description": "예시 업무", "start_date": start.to_string(), "due_date": due.to_string() }),
            )
            .await;
            let id = id_of(&task);
            if let Some(pred) = &previous {
                ok(
                    &app,
                    &owner,
                    Method::PUT,
                    &format!("/api/tasks/{id}/predecessors/{pred}"),
                    json!({ "kind": "FS", "lag_days": 0, "base": null }),
                )
                .await;
            }
            if k % 5 == 0 {
                let current = send(
                    &app,
                    Method::GET,
                    &format!("/api/tasks/{id}"),
                    Some(&owner),
                    None,
                )
                .await;
                assert_eq!(current.status, StatusCode::OK, "{:?}", current.json);
                let task = &current.json;
                ok(
                    &app,
                    &owner,
                    Method::PATCH,
                    &format!("/api/tasks/{id}"),
                    json!({ "base_version": task["version"], "title": task["title"], "description": task["description"],
                             "status": task["status"], "priority": task["priority"], "start_date": task["start_date"],
                             "due_date": task["due_date"], "estimate_minutes": task["estimate_minutes"], "tags": task["tags"],
                             "milestone": task["milestone"], "assignee_ids": [owner.id.0.to_string()] }),
                )
                .await;
            }
            previous = Some(id);
            task_count += 1;
        }
    }
    let tasks_secs = tasks_started.elapsed().as_secs_f64();

    // 첨부: 조각이 둘 이상인 큰 파일(색인 대상이 아닌 형식)과 작은 글 파일(본문 추출 경로).
    let files_started = Instant::now();
    let mut uploaded = Vec::new();
    let mut total_bytes = 0usize;
    for k in 0..shape.big_files {
        let bytes = pseudo_random(1000 + k as u64, shape.big_mib * MIB);
        let doc = &doc_ids[(k * 13) % doc_ids.len()];
        let name = format!("측정자료-{k:03}.bin");
        let file_id = upload(&app, &owner, doc, &name, "application/octet-stream", &bytes).await;
        total_bytes += bytes.len();
        uploaded.push(json!({ "id": file_id, "name": name, "size": bytes.len(),
                               "sha256": hex::encode(Sha256::digest(&bytes)) }));
    }
    for k in 0..shape.small_files {
        let text = format!("연구 메모 {k} {}", "측정 결과와 회의 기록. ".repeat(200));
        let doc = &doc_ids[(k * 29 + 7) % doc_ids.len()];
        let name = format!("메모-{k:03}.txt");
        let file_id = upload(&app, &owner, doc, &name, "text/plain", text.as_bytes()).await;
        total_bytes += text.len();
        uploaded.push(json!({ "id": file_id, "name": name, "size": text.len(),
                               "sha256": hex::encode(Sha256::digest(text.as_bytes())) }));
    }
    let files_secs = files_started.elapsed().as_secs_f64();

    let record = json!({
        "subject": subject,
        "source_db": source.name,
        "workspace": ws,
        "items": items.iter().map(|(k, id)| json!({ "kind": k, "id": id })).collect::<Vec<_>>(),
        "documents": doc_ids,
        "task_items": items.iter().filter(|(k, _)| *k == "project").map(|(_, id)| id.clone()).collect::<Vec<_>>(),
        "files": uploaded,
    });
    std::fs::write(
        root.join("state.json"),
        serde_json::to_vec_pretty(&record).unwrap(),
    )
    .unwrap();
    println!(
        "generate done {}\n  documents {} ({docs_secs:.1}s, 3 revisions each) · tasks {task_count} ({tasks_secs:.1}s) · files {} totalling {} bytes ({files_secs:.1}s)\n  source db {} = {} bytes, source files {} bytes, total {:.1}s",
        stamp(),
        doc_ids.len(),
        uploaded.len(),
        total_bytes,
        source.name,
        db_bytes(&source.app_url).await,
        dir_bytes(&files),
        started.elapsed().as_secs_f64(),
    );
    state.db.close().await;
}

async fn table_rows(conn: &mut PgConnection, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT count(*) FROM fvoci.{table}"))
        .fetch_one(conn)
        .await
        .unwrap()
}

async fn search_documents(index_url: &str, key: &str) -> i64 {
    let stats: Value = reqwest::Client::new()
        .get(format!("{index_url}/indexes/fvoci/stats"))
        .bearer_auth(key)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    stats["numberOfDocuments"].as_i64().unwrap_or(0)
}

/// 읽기 응답이 원본과 복구본에서 같은지 본다. 다른 것의 경로를 모아 돌려준다.
async fn differing(
    src: (&Router, &TestUser),
    dst: (&Router, &TestUser),
    uris: &[String],
) -> Vec<String> {
    let mut bad = Vec::new();
    for uri in uris {
        let a = send(src.0, Method::GET, uri, Some(src.1), None).await;
        let b = send(dst.0, Method::GET, uri, Some(dst.1), None).await;
        if a.status != StatusCode::OK || a.status != b.status || a.json != b.json {
            bad.push(uri.clone());
        }
    }
    bad
}

/// 백업 → 빈 설치 → 복구 → 검색 색인 재생성의 시간을 재고 누락을 확인한다.
#[tokio::test]
#[ignore = "T6 measurement (D-128 (2)); needs the kept test stack, see the module comment"]
async fn restore_measure_run() {
    let root = root();
    let record: Value = serde_json::from_slice(
        &std::fs::read(root.join("state.json")).expect("run generate first"),
    )
    .unwrap();
    let strings = |key: &str| -> Vec<String> {
        record[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    let source_name = record["source_db"].as_str().unwrap().to_owned();
    let subject = record["subject"].as_str().unwrap().to_owned();
    let (source_db, source_url) = db_config(&source_name);
    let source_files = root.join("source-files");
    let source = state_for(&source_url, &source_files, false);
    let mut summary = serde_json::Map::new();
    let window_start = stamp();
    println!("measure window start {window_start}");

    // 1. 백업.
    let out = root.join("backup");
    let t = Instant::now();
    let manifest = backup::create(&source.config, &source_db, &out)
        .await
        .unwrap();
    let backup_secs = t.elapsed().as_secs_f64();
    let backup_bytes = dir_bytes(&out);
    println!(
        "backup create: {backup_secs:.2}s, {backup_bytes} bytes, {} tables, {} rows, {} files",
        manifest.tables.len(),
        manifest.tables.iter().map(|t| t.rows).sum::<i64>(),
        manifest.objects.len()
    );

    // 2. 같은 릴리스의 빈 설치(새 DB 만들기·역할 준비·마이그레이션, 새 저장소).
    let mut admin = PgConnection::connect(&env("FVOCI_TEST_ADMIN_URL"))
        .await
        .unwrap();
    let t = Instant::now();
    let target = fresh_database(&mut admin, "measure_dst").await;
    let target_files = root.join("restored-files");
    std::fs::create_dir_all(&target_files).unwrap();
    let restored = state_for(&target.app_url, &target_files, true);
    let install_secs = t.elapsed().as_secs_f64();
    println!("empty install (create db + bootstrap + migrate): {install_secs:.2}s");

    // 3. 복구.
    let t = Instant::now();
    backup::restore(&restored.config, &target.db, &out)
        .await
        .unwrap();
    let restore_secs = t.elapsed().as_secs_f64();
    println!("backup restore: {restore_secs:.2}s");

    // 4. 검색 색인 재생성: 대기열이 비고 Meilisearch 작업이 모두 끝날 때까지.
    let search_url = env("FVOCI_TEST_SEARCH_URL");
    let search_key = env("FVOCI_TEST_SEARCH_KEY");
    let mut owner_conn = PgConnection::connect(target.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    let indexed_before = search_documents(&search_url, &search_key).await;
    let queued = table_rows(&mut owner_conn, "search_queue").await;
    let t = Instant::now();
    let client = restored.search.as_ref().unwrap();
    fvoci_server::search::prepare(&restored.db, client)
        .await
        .unwrap();
    loop {
        while fvoci_server::search::drain(&restored.db, client, Some(&restored.storage))
            .await
            .unwrap()
            > 0
        {}
        if table_rows(&mut owner_conn, "search_queue").await == 0 {
            break;
        }
        assert!(
            t.elapsed().as_secs() < 1800,
            "reindex did not finish in 30 minutes"
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let reindex_secs = t.elapsed().as_secs_f64();
    let indexed_after = search_documents(&search_url, &search_key).await;
    println!(
        "search reindex: {reindex_secs:.2}s, queue {queued} -> 0, index documents {indexed_before} -> {indexed_after}"
    );
    let window_end = stamp();
    println!("measure window end {window_end}");

    // 5. 누락 확인(시간은 위와 따로).
    let verify_started = Instant::now();
    let mut missing = Vec::new();
    let mut source_conn = PgConnection::connect(source_db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    for table in &manifest.tables {
        let in_source = table_rows(&mut source_conn, &table.name).await;
        let in_restored = table_rows(&mut owner_conn, &table.name).await;
        if in_source != table.rows || in_restored != table.rows {
            missing.push(format!(
                "table {}: manifest {} source {in_source} restored {in_restored}",
                table.name, table.rows
            ));
        }
    }
    let restored_storage = &restored.storage;
    for object in &manifest.objects {
        match restored_storage.digest(&object.key).await {
            Ok((sha, size))
                if sha == object.sha256 && i64::try_from(size).ok() == Some(object.size) => {}
            other => missing.push(format!("object {}: {other:?}", object.key)),
        }
    }
    let source_app = common::app(&source);
    let restored_app = common::app(&restored);
    let src_user = session(&source, &subject).await;
    let dst_user = session(&restored, &subject).await;
    assert_eq!(
        src_user.id, dst_user.id,
        "the same account by issuer and subject"
    );
    let mut uris: Vec<String> = strings("documents")
        .iter()
        .map(|d| format!("/api/documents/{d}"))
        .collect();
    uris.extend(
        strings("task_items")
            .iter()
            .map(|i| format!("/api/items/{i}/tasks")),
    );
    uris.extend(
        record["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| format!("/api/items/{}/documents", i["id"].as_str().unwrap())),
    );
    let different = differing((&source_app, &src_user), (&restored_app, &dst_user), &uris).await;
    for uri in &different {
        missing.push(format!("read differs: {uri}"));
    }
    let mut downloaded = 0usize;
    for file in record["files"].as_array().unwrap() {
        let id = file["id"].as_str().unwrap();
        let (status, _, content) = send_raw(
            &restored_app,
            Method::GET,
            &format!("/api/files/{id}/content"),
            Some(&dst_user),
            &[],
            Vec::new(),
        )
        .await;
        if status == StatusCode::OK
            && hex::encode(Sha256::digest(&content)) == file["sha256"].as_str().unwrap()
            && content.len() == usize::try_from(file["size"].as_i64().unwrap()).unwrap()
        {
            downloaded += 1;
        } else {
            missing.push(format!("download {id} ({}) status {status}", file["name"]));
        }
    }
    let verify_secs = verify_started.elapsed().as_secs_f64();
    println!(
        "verify: {verify_secs:.1}s · tables {} · objects {} · API reads {} (differ {}) · downloads {downloaded}/{} · missing {}",
        manifest.tables.len(),
        manifest.objects.len(),
        uris.len(),
        different.len(),
        record["files"].as_array().unwrap().len(),
        missing.len()
    );

    for (key, value) in [
        ("window_start", json!(window_start)),
        ("window_end", json!(window_end)),
        ("backup_create_secs", json!(backup_secs)),
        ("backup_bytes", json!(backup_bytes)),
        ("empty_install_secs", json!(install_secs)),
        ("restore_secs", json!(restore_secs)),
        ("reindex_secs", json!(reindex_secs)),
        ("reindex_queue", json!(queued)),
        ("index_documents_before", json!(indexed_before)),
        ("index_documents_after", json!(indexed_after)),
        ("verify_secs", json!(verify_secs)),
        ("source_db", json!(source_name)),
        ("source_db_bytes", json!(db_bytes(&source_url).await)),
        ("restored_db", json!(target.name)),
        ("restored_db_bytes", json!(db_bytes(&target.app_url).await)),
        ("source_files_bytes", json!(dir_bytes(&source_files))),
        ("restored_files_bytes", json!(dir_bytes(&target_files))),
        ("tables", json!(manifest.tables.len())),
        (
            "rows",
            json!(manifest.tables.iter().map(|t| t.rows).sum::<i64>()),
        ),
        ("objects", json!(manifest.objects.len())),
        ("missing", json!(missing)),
    ] {
        summary.insert(key.to_owned(), value);
    }
    std::fs::write(
        root.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    source.db.close().await;
    restored.db.close().await;
    assert!(missing.is_empty(), "missing or different: {missing:#?}");
}
