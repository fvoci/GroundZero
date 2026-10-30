#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "measurement test: fixture, socket and procfs failures must fail the run"
)]
//! 방 메모리 전·후 실측(D-131). Linux에서 방 수마다 새 프로세스와 새 T6 데이터를 쓴다.
//! 새 이름의 격리 시험 스택에서 역할을 준비하고 T6와 같은 환경 변수를 주입한다:
//! `DATABASE_URL`, `FVOCI_TEST_ADMIN_URL`, `FVOCI_TEST_MIGRATION_URL`,
//! `FVOCI_TEST_SEARCH_URL`, `FVOCI_TEST_SEARCH_KEY`. 기존 DB에는 실행하지 않는다.
//!
//! 방 수 N=10,100,1000 각각 빈 `FVOCI_MEASURE_DIR`에서 T6 기본 입력(문서 1000건)을 생성:
//! `CARGO_BUILD_JOBS=10 TMPDIR=~/.cache/fvoci-tmp cargo test --release --locked
//! -p fvoci-server --test it restore_measure::restore_measure_generate -- --ignored --exact --nocapture`
//! 다음 별도 프로세스에서 `FVOCI_MEASURE_ROOMS=N`을 더해 실행:
//! `CARGO_BUILD_JOBS=10 TMPDIR=~/.cache/fvoci-tmp cargo test --release --locked
//! -p fvoci-server --test it room_memory_measure::room_memory_measure -- --ignored --exact --nocapture`
//!
//! `/proc/self/status`의 kB는 KiB다. RSS/HWM에는 서버와 시험 클라이언트 소켓이 함께 든다.
//! 클라이언트는 CRDT 복제본 없이 초기 상태를 받아 버리고, 편집은 보내지 않는다.
//! 기준선은 서버·세션·ID 목록 준비 뒤 방 0개일 때다. 해제 뒤 allocator가 RSS를 보유할 수 있다.
//! 결과는 `room-memory.json`에도 남는다. 현재 T6는 모든 본문을 CRDT로 저장하므로 저장 상태가 1000건이다.
//! 저장 상태를 지우지 않고 재며, 전 측정의 JSON→yrs 초기화 경로와 다르다는 점을 비교 제한으로 남긴다.

use crate::common::{self, TestUser};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use fvoci_core::ids::DocumentId;
use fvoci_server::features::accounts::{SESSION_COOKIE, VerifiedIdentity, start_session};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use yrs::sync::{Message, MessageReader, SyncMessage};
use yrs::updates::decoder::DecoderV1;
use yrs::updates::encoder::Encode;

/// 연결·방 해제 실패가 실측을 무한 대기로 만들지 않게 한다.
const WAIT: Duration = Duration::from_secs(120);

fn memory() -> Value {
    let status = std::fs::read_to_string("/proc/self/status").unwrap();
    let read = |key| -> u64 {
        status
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .expect("procfs memory field")
            .split_whitespace()
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    json!({ "rss_kib": read("VmRSS:"), "hwm_kib": read("VmHWM:") })
}

#[tokio::test]
#[ignore = "D-131 room memory measurement; fresh T6 data and isolated stack required"]
async fn room_memory_measure() {
    let started = Instant::now();
    let root = PathBuf::from(std::env::var("FVOCI_MEASURE_DIR").unwrap());
    assert!(!root.join("room-memory.json").exists(), "use fresh T6 data");
    let count: usize = std::env::var("FVOCI_MEASURE_ROOMS")
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        [10, 100, 1000].contains(&count),
        "rooms must be 10, 100 or 1000"
    );
    let record: Value =
        serde_json::from_slice(&std::fs::read(root.join("state.json")).unwrap()).unwrap();
    let all_docs = record["documents"].as_array().unwrap();
    assert_eq!(all_docs.len(), 1000, "use the T6 1000-document input");
    let docs: Vec<DocumentId> = all_docs[..count]
        .iter()
        .map(|id| DocumentId(id.as_str().unwrap().parse().unwrap()))
        .collect();
    let database = record["source_db"].as_str().unwrap();
    let app_url = common::with_database(&std::env::var("DATABASE_URL").unwrap(), database);
    let fixture_url = common::with_database(
        &std::env::var("FVOCI_TEST_MIGRATION_URL").unwrap(),
        database,
    );
    let mut fixture = PgConnection::connect(&fixture_url).await.unwrap();
    let saved: i64 = sqlx::query_scalar("SELECT count(*) FROM fvoci.document_collab_states")
        .fetch_one(&mut fixture)
        .await
        .unwrap();
    assert_eq!(
        saved, 1000,
        "current T6 stores every document through CRDT; use fresh complete T6 data"
    );
    let pending: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fvoci.document_collab_states WHERE pending")
            .fetch_one(&mut fixture)
            .await
            .unwrap();
    assert_eq!(
        pending, 0,
        "T6 generation must finish persisting every edit"
    );
    fixture.close().await.unwrap();
    let files = root.join("source-files").to_string_lossy().into_owned();
    let state = fvoci_server::app::build_state(common::config_with(&[
        ("DATABASE_URL", &app_url),
        ("FVOCI_STORAGE_DIR", &files),
    ]))
    .unwrap();
    // tests/collab.rs의 연결 방식과 T6의 동일 계정 세션을 사용한다.
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: record["subject"].as_str().unwrap().to_owned(),
        email: None,
        email_verified: false,
        display_name: "실측 사용자".to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (id, token) = start_session(&state.db, state.config.session_ttl, &identity, None)
        .await
        .unwrap();
    let user = TestUser {
        id,
        cookie: format!("{SESSION_COOKIE}={token}"),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = common::app(&state);
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut sockets = Vec::with_capacity(count);
    assert!(
        docs.iter().all(|id| !state.collab.has_room(*id)),
        "baseline has zero rooms"
    );
    let baseline = memory();
    let opened_at = Instant::now();
    println!(
        "room measure start {} rooms={count} baseline={baseline}",
        time::OffsetDateTime::now_utc()
    );
    for doc in &docs {
        let mut request = format!(
            "ws://{addr}/api/documents/{}/collab?expected_user={}",
            doc.0, user.id.0
        )
        .into_client_request()
        .unwrap();
        request
            .headers_mut()
            .insert("cookie", user.cookie.parse().unwrap());
        request
            .headers_mut()
            .insert("origin", common::PUBLIC_URL.parse().unwrap());
        let (mut ws, _) = tokio::time::timeout(WAIT, tokio_tungstenite::connect_async(request))
            .await
            .unwrap()
            .unwrap();
        ws.send(WsMessage::Binary(
            Message::Sync(SyncMessage::SyncStep1(Default::default()))
                .encode_v1()
                .into(),
        ))
        .await
        .unwrap();
        tokio::time::timeout(WAIT, async {
            loop {
                let message = ws.next().await.unwrap().unwrap();
                assert!(
                    !matches!(message, WsMessage::Close(_)),
                    "room closed during load: {doc:?}"
                );
                if let WsMessage::Binary(bytes) = message {
                    let mut decoder = DecoderV1::from(bytes.as_ref());
                    if MessageReader::new(&mut decoder)
                        .any(|m| matches!(m.unwrap(), Message::Sync(SyncMessage::SyncStep2(_))))
                    {
                        break;
                    }
                }
            }
        })
        .await
        .unwrap();
        assert!(state.collab.has_room(*doc), "loaded room {doc:?}");
        sockets.push(ws);
    }
    assert!(
        docs.iter().all(|id| state.collab.has_room(*id)),
        "all N rooms remain open"
    );
    let opened = memory();
    let open_secs = opened_at.elapsed().as_secs_f64();
    println!("opened rooms={count} memory={opened} seconds={open_secs:.3}");
    let closing_at = Instant::now();
    for mut ws in sockets {
        ws.close(None).await.unwrap();
    }
    tokio::time::timeout(WAIT, async {
        while docs.iter().any(|id| state.collab.has_room(*id)) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    // 방 목록 제거 바로 뒤 run()의 지역 상태가 drop될 기회를 준다.
    tokio::time::sleep(Duration::from_millis(100)).await;
    let closed = memory();
    let close_secs = closing_at.elapsed().as_secs_f64();
    let delta = i64::try_from(opened["rss_kib"].as_u64().unwrap()).unwrap()
        - i64::try_from(baseline["rss_kib"].as_u64().unwrap()).unwrap();
    let result = json!({
        "rooms": count, "source_db": database, "stored_collab_states": saved,
        "pending_collab_states": pending, "baseline": baseline, "opened": opened, "closed": closed,
        "rss_delta_kib": delta, "rss_per_room_kib": delta as f64 / count as f64,
        "open_secs": open_secs, "close_secs": close_secs, "measure_secs": started.elapsed().as_secs_f64(),
    });
    println!(
        "room measure end {} {result}",
        time::OffsetDateTime::now_utc()
    );
    std::fs::write(
        root.join("room-memory.json"),
        serde_json::to_vec_pretty(&result).unwrap(),
    )
    .unwrap();
    server.abort();
    assert!(
        server.await.unwrap_err().is_cancelled(),
        "stop measurement listener"
    );
    state.db.close().await;
}
