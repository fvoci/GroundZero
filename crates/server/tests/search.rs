#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 통합 검색 (D-39): 볼 수 있는 것만 나오고, 지우면 색인에서 빠지고, 되살리면 돌아온다.

mod common;

use std::time::Duration;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, config_with, create_doc, create_workspace, doc_body, login,
    new_request_id, send,
};
use fvoci_server::http::AppState;
use fvoci_server::search::{SearchClient, drain};
use serde_json::{Value, json};
use uuid::Uuid;

/// Meilisearch를 쓰는 상태. `cargo xtask test`가 테스트용 주소와 키를 넣는다.
async fn search_state() -> AppState {
    let url = std::env::var("FVOCI_TEST_SEARCH_URL").expect("run via `cargo xtask test`");
    let key = std::env::var("FVOCI_TEST_SEARCH_KEY").expect("run via `cargo xtask test`");
    let state = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_SEARCH_URL", &url),
        ("FVOCI_SEARCH_KEY", &key),
    ]))
    .unwrap();
    state.search.as_ref().unwrap().ensure_index().await.unwrap();
    state
}

/// 테스트마다 겹치지 않는 검색어.
fn token() -> String {
    format!("zq{}", &Uuid::now_v7().simple().to_string()[20..])
}

fn client(state: &AppState) -> &SearchClient {
    state.search.as_ref().unwrap()
}

async fn search(app: &Router, user: &TestUser, q: &str) -> Value {
    let r = send(
        app,
        Method::GET,
        &format!("/api/search?q={q}"),
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
        while drain(&state.db, client(state)).await.unwrap() > 0 {}
        last = search(app, user, q).await;
        if titles(&last) == want {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(titles(&last), want, "search for {q} never settled");
    last
}

/// 색인에 그 대상이 있는지 직접 본다(접근 거르기 없이).
async fn indexed(state: &AppState, q: &str, id: &str) -> bool {
    !client(state)
        .query(q, &format!("id = \"{id}\""), 5)
        .await
        .unwrap()
        .is_empty()
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

#[tokio::test]
async fn search_returns_only_what_each_person_can_see() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let outsider = login(&state, "outsider").await;
    let other_ws = create_workspace(&app, &outsider, "남의 공간").await;

    let doc = create_doc(&app, &admin, &ws, &format!("회의록 {tok}")).await;
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": format!("회의록 {tok}"),
                     "body": doc_body(&format!("안건 정리와 {tok}body 결정")) })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    create_doc(&app, &outsider, &other_ws, &format!("남의 문서 {tok}")).await;
    let it = private_item(&app, &admin, &ws, "SRC").await;
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("비공개 업무 {tok}") })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": format!("내 노트 {tok}") })),
    )
    .await;
    assert_eq!(note.status, StatusCode::CREATED, "{:?}", note.json);

    // 작성자: 워크스페이스 문서와 자기 노트. 남의 공간, 비공개 항목의 업무는 없다.
    let mine = settle(
        &state,
        &app,
        &writer,
        &tok,
        &[&format!("내 노트 {tok}"), &format!("회의록 {tok}")],
    )
    .await;
    let found = mine
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "document" && r["id"] == doc["id"])
        .unwrap();
    assert_eq!(found["reference"], doc["reference"]);
    // 관리자: 문서와 비공개 업무. 남의 개인 노트는 관리자에게도 없다.
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[&format!("비공개 업무 {tok}"), &format!("회의록 {tok}")],
    )
    .await;
    settle(
        &state,
        &app,
        &outsider,
        &tok,
        &[&format!("남의 문서 {tok}")],
    )
    .await;

    // 본문으로도 찾고, 찾은 곳 근처 글자를 보여 준다.
    let by_body = settle(
        &state,
        &app,
        &writer,
        &format!("{tok}body"),
        &[&format!("회의록 {tok}")],
    )
    .await;
    assert!(
        by_body[0]["snippet"]
            .as_str()
            .unwrap()
            .contains(&format!("{tok}body")),
        "{by_body:?}"
    );

    // 다른 워크스페이스로 좁히면 멤버가 아니므로 없는 것과 같다.
    let r = send(
        &app,
        Method::GET,
        &format!("/api/search?q={tok}&workspace_id={other_ws}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let r = send(
        &app,
        Method::GET,
        &format!("/api/search?q={tok}&kinds=file"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = send(
        &app,
        Method::GET,
        &format!("/api/search?q={tok}&kinds=task"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(titles(&r.json), [format!("비공개 업무 {tok}")]);

    // 멤버에서 빠지면 색인을 다시 만들지 않아도 바로 안 보인다.
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", writer.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);
    assert_eq!(
        titles(&search(&app, &writer, &tok).await),
        [format!("내 노트 {tok}")]
    );

    // Meilisearch가 없으면 제목만 DB에서 찾고 같은 규칙으로 거른다.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &tok).await),
        [format!("비공개 업무 {tok}"), format!("회의록 {tok}")]
    );
    assert_eq!(
        titles(&search(&plain_app, &writer, &tok).await),
        [format!("내 노트 {tok}")]
    );
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}body")).await),
        Vec::<String>::new()
    );
}

#[tokio::test]
async fn trashed_content_leaves_the_index_and_returns_on_restore() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색 휴지통").await;
    let doc = create_doc(&app, &admin, &ws, &format!("지울 문서 {tok}")).await;
    let id = doc["id"].as_str().unwrap();
    settle(&state, &app, &admin, &tok, &[&format!("지울 문서 {tok}")]).await;
    assert!(indexed(&state, &tok, id).await);

    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let batch = r.json["batch_id"].as_str().unwrap().to_owned();
    // 거르기 덕분에 색인 반영 전에도 안 보이고, 반영 뒤에는 색인에서도 빠진다.
    assert_eq!(
        titles(&search(&app, &admin, &tok).await),
        Vec::<String>::new()
    );
    settle(&state, &app, &admin, &tok, &[]).await;
    assert!(!indexed(&state, &tok, id).await);

    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash/{batch}/restore"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    settle(&state, &app, &admin, &tok, &[&format!("지울 문서 {tok}")]).await;
    assert!(indexed(&state, &tok, id).await);
}
