#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 검색 큐 전체를 비우고 세대·비동기 작업 정착을 확인하는 독립 실행 파일.
mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, config_with, create_doc, doc_body, login, new_request_id, send};
use fvoci_server::search::{drain, prepare};
use serde_json::{Value, json};
use std::time::Duration;
use uuid::Uuid;
static SHARED_QUEUE: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

// D-111·D-112: 검색 대기열 전용 DB에서는 조직도 같은 DB에 준비한다.
async fn fresh_workspace(app: &Router, user: &TestUser, db: &sqlx::PgPool, name: &str) -> String {
    let mut tx = db.begin().await.unwrap();
    sqlx::query("INSERT INTO fvoci.server_admins (user_id) VALUES ($1)")
        .bind(user.id.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1::text, true)")
        .bind(user.id.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    let organization: Uuid =
        sqlx::query_scalar("SELECT fvoci.app_create_organization($1, $2, $3, $4)")
            .bind(Uuid::now_v7())
            .bind(name)
            .bind(user.id.0)
            .bind((0..3).map(|_| Uuid::now_v7()).collect::<Vec<_>>())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    let r = send(app, Method::POST, "/api/workspaces", Some(user),
        Some(json!({ "request_id": new_request_id(), "organization_id": organization, "name": name }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json["id"].as_str().unwrap().to_owned()
}

/// 상태를 시험이 정하는 가짜 Meilisearch. 문서 넣기·지우기 요청을 세고 작업 번호를 주며, 작업 상태는 시험이 바꾼다.
#[derive(Default)]
struct FakeSearch {
    next_task: u64,
    /// 새로 받는 문서 작업의 상태.
    new_status: String,
    statuses: std::collections::HashMap<u64, String>,
    /// 받은 문서 작업(작업 번호, 본문).
    submitted: Vec<(u64, Value)>,
}

type Fake = std::sync::Arc<std::sync::Mutex<FakeSearch>>;

async fn fake_search() -> (String, Fake) {
    use axum::extract::{Path, State};
    use axum::routing::{get, patch, post};
    let fake: Fake = std::sync::Arc::new(std::sync::Mutex::new(FakeSearch {
        next_task: 1,
        new_status: "enqueued".into(),
        ..FakeSearch::default()
    }));
    async fn task(
        State(f): State<Fake>,
        body: Option<axum::Json<Value>>,
        settings: bool,
    ) -> axum::Json<Value> {
        let mut f = f.lock().unwrap();
        let uid = f.next_task;
        f.next_task += 1;
        let status = if settings {
            "succeeded".to_owned()
        } else {
            f.new_status.clone()
        };
        f.statuses.insert(uid, status);
        if !settings {
            f.submitted
                .push((uid, body.map(|b| b.0).unwrap_or(Value::Null)));
        }
        axum::Json(json!({ "taskUid": uid }))
    }
    let app = Router::new()
        .route(
            "/indexes/fvoci",
            get(|| async { axum::Json(json!({ "uid": "fvoci" })) }),
        )
        .route(
            "/indexes/fvoci/settings",
            patch(|s: State<Fake>| async move { task(s, None, true).await }),
        )
        .route(
            "/indexes/fvoci/documents",
            post(
                |s: State<Fake>, b: axum::Json<Value>| async move { task(s, Some(b), false).await },
            ),
        )
        .route(
            "/indexes/fvoci/documents/delete-batch",
            post(
                |s: State<Fake>, b: axum::Json<Value>| async move { task(s, Some(b), false).await },
            ),
        )
        .route(
            "/tasks/{uid}",
            get(|State(f): State<Fake>, Path(uid): Path<u64>| async move {
                match f.lock().unwrap().statuses.get(&uid) {
                    Some(status) => (
                        StatusCode::OK,
                        axum::Json(json!({ "uid": uid, "status": status })),
                    ),
                    None => (
                        StatusCode::NOT_FOUND,
                        axum::Json(json!({ "code": "task_not_found" })),
                    ),
                }
            }),
        )
        .with_state(std::sync::Arc::clone(&fake));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}"), fake)
}

/// 남은 항목 ②: 색인 작업이 아직 끝나지 않았으면(대기·처리 중) 대기열 행을 지우지 않고 작업 번호와 제출 세대를 남긴다.
/// 대기 중에는 다시 보내거나 실패 횟수를 쓰지 않는다. 성공을 확인한 뒤에만 지우고, 그 사이 더 새 변경이 있으면 남겨 다시
/// 보낸다. 실패·취소는 다시 보낸다. 전체 DB를 꺼내는 작업이라 이 시험만의 DB를 쓴다.
#[tokio::test]
async fn pending_index_tasks_are_tracked_until_they_succeed() {
    let _queue = SHARED_QUEUE.read().await;
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin = <sqlx::PgConnection as sqlx::Connection>::connect(&admin_url)
        .await
        .unwrap();
    let started = std::time::Instant::now();
    let fresh = common::fresh_database(&mut admin, "search_tasks").await;
    eprintln!("fresh database ready in {:?}", started.elapsed());
    let (url, fake) = fake_search().await;
    let state = fvoci_server::app::build_state(config_with(&[
        ("DATABASE_URL", &fresh.app_url),
        ("FVOCI_SEARCH_URL", &url),
        ("FVOCI_SEARCH_KEY", "fake-search-key-for-tests"),
    ]))
    .unwrap();
    let app = common::app(&state);
    let search = state.search.as_ref().unwrap();
    prepare(&state.db, search).await.unwrap();
    let owner_url = common::with_database(
        &std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`"),
        &fresh.name,
    );
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let row = |id: String| {
        let owner = owner.clone();
        async move {
            sqlx::query_scalar::<_, i32>(
                "SELECT attempts FROM fvoci.search_queue WHERE kind = 'document' AND target_id = $1::uuid",
            )
            .bind(id)
            .fetch_optional(&owner)
            .await
            .unwrap()
        }
    };
    let task = |id: String| {
        let owner = owner.clone();
        async move {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT task_uid FROM fvoci.search_queue WHERE kind = 'document' AND target_id = $1::uuid",
            )
            .bind(id)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };
    let posts = || fake.lock().unwrap().submitted.len();
    let last_title = || fake.lock().unwrap().submitted.last().unwrap().1.to_string();
    let set_all = |status: &str| {
        let mut f = fake.lock().unwrap();
        for s in f.statuses.values_mut() {
            *s = status.to_owned();
        }
        f.new_status = status.to_owned();
    };

    let admin_user = login(&state, "admin").await;
    let ws = fresh_workspace(&app, &admin_user, &owner, "색인 작업").await;
    let doc = common::create_doc(&app, &admin_user, &ws, "처음 제목").await;
    let id = doc["id"].as_str().unwrap().to_owned();

    // 1. 보낸 작업이 끝나지 않았다: 행은 남고 작업 번호를 기억한다.
    drain(&state.db, search, None).await.unwrap();
    let attempts = row(id.clone())
        .await
        .expect("the document left the queue while its task was pending");
    assert!(
        task(id.clone()).await.is_some(),
        "the pending task was not recorded"
    );
    let sent = posts();
    // 2. 기다리는 동안 다시 꺼내도 다시 보내지 않고 실패 횟수도 그대로다.
    drain(&state.db, search, None).await.unwrap();
    assert_eq!(posts(), sent, "a pending task was submitted again");
    assert_eq!(row(id.clone()).await.unwrap(), attempts);
    // 3. 그 사이 제목을 바꾼다(더 새 세대).
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{id}"),
        Some(&admin_user),
        Some(
            json!({ "request_id": common::new_request_id(), "base_revision": doc["revision"],
                     "title": "바뀐 제목", "body": doc["body"] }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    // 4. 앞 작업이 성공해도 더 새 변경은 남아 다시 보내고, 그 성공 뒤에 지운다.
    set_all("succeeded");
    drain(&state.db, search, None).await.unwrap();
    assert!(posts() > sent, "the newer change was not submitted");
    assert!(last_title().contains("바뀐 제목"), "{}", last_title());
    assert!(
        row(id.clone()).await.is_none(),
        "the row stayed after its newest task succeeded"
    );

    // 5. 실패한 작업은 다시 보낸다. 여럿을 한 번에 보낸 작업이 기다림 뒤에 실패했으면 다시 보낼 때는 하나씩 보낸다(문제
    //    있는 한 건이 묶음 전체를 계속 막지 않게).
    set_all("enqueued");
    let other = common::create_doc(&app, &admin_user, &ws, "실패할 문서").await;
    let third = common::create_doc(&app, &admin_user, &ws, "같이 실패할 문서").await;
    let other_id = other["id"].as_str().unwrap().to_owned();
    let third_id = third["id"].as_str().unwrap().to_owned();
    drain(&state.db, search, None).await.unwrap();
    assert!(row(other_id.clone()).await.is_some());
    assert!(task(other_id.clone()).await.is_some());
    assert_eq!(
        task(other_id.clone()).await,
        task(third_id.clone()).await,
        "sent together"
    );
    let before = posts();
    set_all("failed");
    // Meilisearch가 느리다(새 작업이 대기에 머문다). 하나씩 보낼 때 건마다 기다리면 묶음 크기만큼 길어져 꺼낸 행의 표시
    // 시간(2분)을 넘길 수 있다. 하나씩 보낸 것은 기다리지 않고 작업 번호를 남겨 다음에 확인한다.
    fake.lock().unwrap().new_status = "enqueued".into();
    let resend = std::time::Instant::now();
    drain(&state.db, search, None).await.unwrap();
    assert!(
        resend.elapsed() < Duration::from_secs(4),
        "one-by-one resends waited on each task: {:?}",
        resend.elapsed()
    );
    assert!(task(other_id.clone()).await.is_some());
    assert_ne!(
        task(other_id.clone()).await,
        task(third_id.clone()).await,
        "each resent row has its own task"
    );
    set_all("succeeded");
    drain(&state.db, search, None).await.unwrap();
    let resent: Vec<usize> = fake.lock().unwrap().submitted[before..]
        .iter()
        .map(|(_, body)| body.as_array().map_or(0, Vec::len))
        .collect();
    assert!(
        !resent.is_empty(),
        "the failed task was not submitted again"
    );
    assert!(
        resent.iter().all(|n| *n == 1),
        "the failed batch was sent together again: {resent:?}"
    );
    assert!(row(other_id).await.is_none());
    assert!(row(third_id).await.is_none());
    owner.close().await;
    state.db.close().await;
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fresh.name))
        .execute(&mut admin)
        .await
        .unwrap();
}

/// 리뷰(감사 S12 후속): 꺼낸 행의 재확인은 SQL 함수 시험만 있어, `drain`이 재확인을 부르지 않거나 그 결과로 거르지
/// 않아도 시험이 통과했다. `drain`이 꺼낸 뒤 내용을 읽는 사이 그 행의 세대가 바뀌면(사용자가 다시 고침) 그 꺼냄으로는
/// 보내지 않고 행을 남긴다(새 세대는 다음 꺼냄이 보낸다). 내용 읽기를 문서 표 잠금으로 붙잡아 그 사이에 세대를 바꾼다.
/// 표를 잠그고 전체 대기열을 꺼내므로 이 시험만의 DB를 쓴다.
#[tokio::test]
async fn drain_does_not_send_a_row_whose_generation_changed_after_the_claim() {
    let _queue = SHARED_QUEUE.read().await;
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin = <sqlx::PgConnection as sqlx::Connection>::connect(&admin_url)
        .await
        .unwrap();
    let fresh = common::fresh_database(&mut admin, "search_recheck").await;
    let (url, fake) = fake_search().await;
    let state = fvoci_server::app::build_state(config_with(&[
        ("DATABASE_URL", &fresh.app_url),
        ("FVOCI_SEARCH_URL", &url),
        ("FVOCI_SEARCH_KEY", "fake-search-key-for-tests"),
    ]))
    .unwrap();
    let app = common::app(&state);
    let search = state.search.clone().unwrap();
    prepare(&state.db, &search).await.unwrap();
    let owner_url = common::with_database(
        &std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`"),
        &fresh.name,
    );
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    fake.lock().unwrap().new_status = "succeeded".into();

    let user = login(&state, "admin").await;
    let ws = fresh_workspace(&app, &user, &owner, "세대 재확인").await;
    while drain(&state.db, &search, None).await.unwrap() > 0 {}
    let doc = common::create_doc(&app, &user, &ws, "세대가 바뀔 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let doc_uid = format!("document_{}", id.replace('-', ""));
    let sent_before = fake.lock().unwrap().submitted.len();

    // 꺼낸 뒤 내용을 읽는 질의가 문서 표에서 기다리게 한다.
    let mut lock = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("LOCK TABLE fvoci.documents IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let draining = tokio::spawn({
        let state = state.clone();
        let search = search.clone();
        async move { drain(&state.db, &search, None).await }
    });
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&owner)
        .await
        .unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(blocked, "the drain never waited to read the document");
    // 그 사이 사용자가 다시 고쳐 세대가 바뀐다(꺼낸 표시는 그대로).
    let newer: time::OffsetDateTime = sqlx::query_scalar(
        "UPDATE fvoci.search_queue SET queued_at = queued_at + interval '1 second'
         WHERE kind = 'document' AND target_id = $1::uuid RETURNING queued_at",
    )
    .bind(&id)
    .fetch_one(&owner)
    .await
    .unwrap();
    lock.commit().await.unwrap();
    draining.await.unwrap().unwrap();

    let sent_stale = fake.lock().unwrap().submitted[sent_before..]
        .iter()
        .any(|(_, body)| body.to_string().contains(&doc_uid));
    let left: Option<time::OffsetDateTime> = sqlx::query_scalar(
        "SELECT queued_at FROM fvoci.search_queue WHERE kind = 'document' AND target_id = $1::uuid",
    )
    .bind(&id)
    .fetch_optional(&owner)
    .await
    .unwrap();
    // 꺼낸 표시가 지나면 다음 꺼냄이 새 세대를 보낸다.
    sqlx::query(
        "UPDATE fvoci.search_queue SET claimed_until = now() - interval '1 second'
         WHERE kind = 'document' AND target_id = $1::uuid",
    )
    .bind(&id)
    .execute(&owner)
    .await
    .unwrap();
    let sent_after = fake.lock().unwrap().submitted.len();
    drain(&state.db, &search, None).await.unwrap();
    let sent_newer = fake.lock().unwrap().submitted[sent_after..]
        .iter()
        .any(|(_, body)| body.to_string().contains(&doc_uid));
    owner.close().await;
    state.db.close().await;
    sqlx::raw_sql(&format!("DROP DATABASE {} WITH (FORCE)", fresh.name))
        .execute(&mut admin)
        .await
        .unwrap();
    assert!(!sent_stale, "the drain sent a row whose generation changed");
    assert_eq!(left, Some(newer), "the changed row left the queue");
    assert!(sent_newer, "the newer generation was not sent later");
}
