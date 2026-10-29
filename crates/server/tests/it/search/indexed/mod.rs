//! 실제 색인을 준비하는 HTTP 시나리오. 대기열 전체를 비우므로 별도 검색 실행 파일에서만 등록한다.

use std::time::Duration;

use crate::common;
use crate::common::{
    TestUser, add_member, config_with, create_doc, create_workspace, doc_body, login,
    new_request_id, send,
};
use axum::Router;
use axum::http::{Method, StatusCode};
use fvoci_server::http::AppState;
use fvoci_server::search::{SearchClient, drain, prepare};
use serde_json::{Value, json};
use uuid::Uuid;

/// 공유 시험 DB의 검색 대기열. 닿지 않는 검색 서비스로 주기 작업을 돌리는 시험은 대기열 행을 DB 전체에서 꺼내(2분 동안
/// 다시 꺼내지 않음) 보내지 못한다. 그동안 다른 시험의 행이 색인되지 않아 기다리던 시험이 시간 초과로 실패할 수 있다. 그
/// 시험은 `write`, 나머지 시험은 `read`를 쥔다(#158).
static SHARED_QUEUE: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

mod access;
mod dates;
mod filters;
mod lifecycle;
mod shares;

/// Meilisearch를 쓰는 상태. `cargo xtask test`가 테스트용 주소와 키를 넣는다.
async fn search_state() -> AppState {
    let url = std::env::var("FVOCI_TEST_SEARCH_URL").expect("run via `cargo xtask test`");
    let key = std::env::var("FVOCI_TEST_SEARCH_KEY").expect("run via `cargo xtask test`");
    let state = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_SEARCH_URL", &url),
        ("FVOCI_SEARCH_KEY", &key),
    ]))
    .unwrap();
    prepare(&state.db, state.search.as_ref().unwrap())
        .await
        .unwrap();
    state
}

/// 테스트마다 겹치지 않는 검색어.
fn token() -> String {
    format!("zq{}", &Uuid::now_v7().simple().to_string()[20..])
}

fn client(state: &AppState) -> &SearchClient {
    state.search.as_ref().unwrap()
}

// D-111·D-112: 검색 대기열 전용 DB에서는 조직도 같은 DB에 준비한다.

async fn search(app: &Router, user: &TestUser, q: &str) -> Value {
    let r = send(
        app,
        Method::GET,
        &format!(
            "/api/search?q={q}&organization_id={}",
            common::organization_for(user).await
        ),
        Some(user),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

fn titles(results: &Value) -> Vec<String> {
    let mut t: Vec<String> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["title"].as_str().unwrap().to_owned())
        .collect();
    t.sort();
    t
}

/// 대기열을 비우고 기대한 결과가 나올 때까지 기다린다(다른 테스트가 같은 대기열을 함께 비운다).
async fn settle(state: &AppState, app: &Router, user: &TestUser, q: &str, want: &[&str]) -> Value {
    let mut want: Vec<String> = want.iter().map(|s| (*s).to_owned()).collect();
    want.sort();
    let mut last = Value::Null;
    for _ in 0..100 {
        while drain(&state.db, client(state), Some(&state.storage))
            .await
            .unwrap()
            > 0
        {}
        last = search(app, user, q).await;
        if titles(&last) == want {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(titles(&last), want, "search for {q} never settled");
    last
}

/// 색인에 그 대상이 있는지 직접 본다(접근 거르기 없이). 다른 테스트가 같은 대기열을 함께 비우므로 기대한 상태가
/// 될 때까지 기다린 뒤 마지막 상태를 돌려준다.
async fn indexed_eventually(state: &AppState, q: &str, id: &str, want: bool) -> bool {
    let mut found = !want;
    for _ in 0..100 {
        while drain(&state.db, client(state), Some(&state.storage))
            .await
            .unwrap()
            > 0
        {}
        found = !client(state)
            .query(q, &format!("id = \"{id}\""), 0, 5, false)
            .await
            .unwrap()
            .is_empty();
        if found == want {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    found
}

async fn private_item(app: &Router, admin: &TestUser, ws: &str, key: &str) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": key })),
    )
    .await
    .json
}

async fn upload_bytes(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    name: &str,
    content_type: &str,
    bytes: &[u8],
) {
    use sha2::{Digest, Sha256};
    let up = send(
        app,
        Method::POST,
        "/api/uploads",
        Some(user),
        Some(json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                     "name": name, "size": bytes.len(), "content_type": content_type })),
    )
    .await;
    assert_eq!(up.status, StatusCode::CREATED, "{:?}", up.json);
    let id = up.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(bytes));
    let (status, _, _) = common::send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(user),
        &[("x-content-sha256", hash.as_str())],
        bytes.to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
}

fn docx(text: &str) -> Vec<u8> {
    use std::io::Write;
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("word/document.xml", options).unwrap();
    write!(
        zip,
        "<w:document><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
    )
    .unwrap();
    zip.finish().unwrap();
    out.into_inner()
}

/// 표준 글꼴(Helvetica) 한 쪽짜리 PDF(D-73). 상호 참조 표의 위치를 직접 계산한다.
fn pdf(text: &str) -> Vec<u8> {
    let stream = format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// 오늘(DB의 UTC 날짜)과 앞뒤 하루. 자정 무렵에 돌아도 흔들리지 않게 범위로 쓴다.
async fn around_today(state: &AppState, user: &TestUser) -> (String, String) {
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let (from, to): (time::Date, time::Date) =
        sqlx::query_as("SELECT current_date - 1, current_date + 1")
            .fetch_one(tx.conn())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    (from.to_string(), to.to_string())
}

/// 대체(DB) 경로의 상태: 검색 서비스 없이 제목만 DB에서 찾는다.
fn plain_app() -> Router {
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    common::app(&plain)
}

/// 같은 질의를 색인 경로(반영될 때까지 기다림)와 대체 경로에서 보고 둘 다 `want`인지 확인한다.
async fn both_paths(
    state: &AppState,
    app: &Router,
    plain: &Router,
    user: &TestUser,
    query: &str,
    want: &[&str],
) {
    settle(state, app, user, query, want).await;
    let mut want: Vec<String> = want.iter().map(|s| (*s).to_owned()).collect();
    want.sort();
    assert_eq!(
        titles(&search(plain, user, query).await),
        want,
        "fallback path for {query}"
    );
}

/// 고친 때를 정해 둔 문서(제목을 다시 써 색인한다).
async fn doc_changed_at(app: &Router, user: &TestUser, ws: &str, title: &str, at: &str) {
    let doc = create_doc(app, user, ws, title).await;
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    sqlx::query(
        "UPDATE fvoci.documents SET title = title, updated_at = $2::timestamptz WHERE id = $1::uuid",
    )
    .bind(doc["id"].as_str().unwrap())
    .bind(at)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
}
