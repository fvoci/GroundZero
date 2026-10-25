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
use fvoci_server::search::{SearchClient, drain, prepare};
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

#[tokio::test]
async fn search_returns_only_what_each_person_can_see() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
        &format!("/api/search?q={tok}&kinds=nope"),
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
    assert!(indexed_eventually(&state, &tok, id, true).await);

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
    assert!(!indexed_eventually(&state, &tok, id, false).await);

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
    assert!(indexed_eventually(&state, &tok, id, true).await);

    // 항목을 지우면 그 안의 문서·업무도 색인에서 빠지고, 되살리면 함께 돌아온다.
    let it = private_item(&app, &admin, &ws, "SRT").await;
    let item_id = it["id"].as_str().unwrap();
    let inner = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("항목 문서 {tok}") })),
    )
    .await;
    assert_eq!(inner.status, StatusCode::CREATED, "{:?}", inner.json);
    let inner_id = inner.json["id"].as_str().unwrap().to_owned();
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("항목 업무 {tok}") })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    let task_id = task.json["id"].as_str().unwrap().to_owned();
    let all = [
        format!("지울 문서 {tok}"),
        format!("항목 문서 {tok}"),
        format!("항목 업무 {tok}"),
    ];
    let all: Vec<&str> = all.iter().map(String::as_str).collect();
    settle(&state, &app, &admin, &tok, &all).await;
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let item_batch = r.json["batch_id"].as_str().unwrap().to_owned();
    settle(&state, &app, &admin, &tok, &[&format!("지울 문서 {tok}")]).await;
    assert!(!indexed_eventually(&state, &tok, &inner_id, false).await);
    assert!(!indexed_eventually(&state, &tok, &task_id, false).await);
    let r = send(
        &app,
        Method::POST,
        &format!("/api/trash/{item_batch}/restore"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    settle(&state, &app, &admin, &tok, &all).await;
    assert!(indexed_eventually(&state, &tok, &inner_id, true).await);
}

#[tokio::test]
async fn an_unreachable_search_service_falls_back_to_titles() {
    let state = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_SEARCH_URL", "http://127.0.0.1:9"),
        ("FVOCI_SEARCH_KEY", "unused-test-key"),
    ]))
    .unwrap();
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색 대신").await;
    create_doc(&app, &admin, &ws, &format!("제목만 {tok}")).await;
    assert_eq!(
        titles(&search(&app, &admin, &tok).await),
        [format!("제목만 {tok}")]
    );
    // 검색 서비스가 없어도 다른 주기 작업은 계속 돈다.
    let report =
        fvoci_server::worker::run_once(&state.db, state.search.as_ref(), Some(&state.storage))
            .await;
    assert_eq!(report.indexed, 0);
}

#[tokio::test]
async fn file_names_are_searchable_by_people_who_can_see_the_file() {
    use sha2::{Digest, Sha256};
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "파일 검색").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &admin, &ws, "자료 모음").await;
    let up = send(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                     "name": format!("{tok}-결과.csv"), "size": 3, "content_type": "text/csv" })),
    )
    .await;
    assert_eq!(up.status, StatusCode::CREATED, "{:?}", up.json);
    let id = up.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(b"a,b"));
    let (status, _, _) = common::send_raw(
        &app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(&admin),
        &[("x-content-sha256", hash.as_str())],
        b"a,b".to_vec(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = send(
        &app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    let file_id = done.json["file_id"].as_str().unwrap().to_owned();

    let found = settle(&state, &app, &admin, &tok, &[&format!("{tok}-결과.csv")]).await;
    assert_eq!(found[0]["kind"], "file");
    assert_eq!(found[0]["document_id"], doc["id"]);
    settle(&state, &app, &outsider, &tok, &[]).await;

    // 파일을 휴지통에 넣으면 빠진다.
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    settle(&state, &app, &admin, &tok, &[]).await;
    assert!(!indexed_eventually(&state, &tok, &file_id, false).await);
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

#[tokio::test]
async fn filters_narrow_results_and_attachment_text_is_searchable() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let body_tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "검색 필터").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "FLT" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    let new_task = |title: String| {
        let app = app.clone();
        let admin = admin.clone();
        let item = item["id"].as_str().unwrap().to_owned();
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/items/{item}/tasks"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "title": title })),
            )
            .await
            .json
        }
    };
    let report = new_task(format!("{tok} 보고서")).await;
    new_task(format!("{tok} 기획")).await;
    let updated = send(&app, Method::PUT, &format!("/api/tasks/{}", report["id"].as_str().unwrap()), Some(&admin), Some(json!({
        "base_version": report["version"], "title": format!("{tok} 보고서"), "description": "", "status": "done",
        "priority": "none", "start_date": null, "due_date": "2026-11-05", "estimate_minutes": null, "tags": [],
        "milestone": false, "assignee_ids": [admin.id.0] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let doc = create_doc(&app, &admin, &ws, &format!("{tok} 회의록")).await;
    upload_bytes(
        &app,
        &admin,
        &doc,
        "계획서.docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        &docx(&format!("예산 {tok} {body_tok} 확정")),
    )
    .await;
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[
            &format!("{tok} 기획"),
            &format!("{tok} 보고서"),
            &format!("{tok} 회의록"),
            "계획서.docx",
        ],
    )
    .await;

    let found = |query: String| {
        let app = app.clone();
        let admin = admin.clone();
        async move { titles(&search(&app, &admin, &query).await) }
    };
    // 업무를 고친 내용(상태·담당·기한)이 색인에 반영될 때까지 기다린다(제목만으로는 알 수 없다).
    let report = format!("{tok} 보고서");
    for filter in [
        "status=done",
        "assignee=me",
        "from=2026-11-01&to=2026-11-30",
    ] {
        settle(&state, &app, &admin, &format!("{tok}&{filter}"), &[&report]).await;
    }
    assert_eq!(
        found(format!(
            "{tok}&module_id={}",
            module["id"].as_str().unwrap()
        ))
        .await,
        [format!("{tok} 기획"), format!("{tok} 보고서")]
    );
    let r = send(
        &app,
        Method::GET,
        &format!("/api/search?q={tok}&status=late"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = send(
        &app,
        Method::GET,
        &format!("/api/search?q={tok}&from=2026/11/01"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 첨부 본문(DOCX)의 낱말로 파일을 찾는다.
    let by_body = settle(&state, &app, &admin, &body_tok, &["계획서.docx"]).await;
    assert_eq!(by_body[0]["kind"], "file");
    assert!(
        by_body[0]["snippet"]
            .as_str()
            .is_some_and(|s| s.contains("예산")),
        "{by_body:?}"
    );

    // PDF 본문의 낱말로도 찾는다(D-73). 깨진 PDF는 이름으로만 색인되고 색인 작업을 멈추지 않는다.
    let pdf_tok = token();
    upload_bytes(
        &app,
        &admin,
        &doc,
        "논문.pdf",
        "application/pdf",
        &pdf(&format!("Rainbow {pdf_tok} outcome")),
    )
    .await;
    upload_bytes(
        &app,
        &admin,
        &doc,
        "깨진.pdf",
        "application/pdf",
        b"%PDF-1.4 broken",
    )
    .await;
    let by_pdf = settle(&state, &app, &admin, &pdf_tok, &["논문.pdf"]).await;
    assert_eq!(by_pdf[0]["kind"], "file");
    settle(&state, &app, &admin, "깨진.pdf", &["깨진.pdf"]).await;

    // 검색 서비스가 없을 때도 같은 필터를 쓴다(제목 검색).
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&status=done")).await),
        [format!("{tok} 보고서")]
    );
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&assignee=me&kinds=task")).await),
        [format!("{tok} 보고서")]
    );
}

/// 작성자·제목만 거르기: 좁히기만 한다. 다른 사람이 만든 것 가운데 볼 수 없는 것(개인 노트)은 작성자로 골라도 나오지 않고,
/// 제목만이면 본문 낱말로는 찾지 않는다. 검색 서비스가 없을 때의 제목 검색도 작성자 조건을 따른다.
#[tokio::test]
async fn author_and_title_only_narrow_results_without_widening_access() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let body_tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "작성자 검색").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let mine = create_doc(&app, &admin, &ws, &format!("{tok} 관리자 문서")).await;
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", mine["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": format!("{tok} 관리자 문서"),
                     "body": doc_body(&format!("본문에만 있는 {body_tok}")) })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    create_doc(&app, &writer, &ws, &format!("{tok} 멤버 문서")).await;
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("{tok} 관리자 노트") })),
    )
    .await;
    assert_eq!(note.status, StatusCode::CREATED, "{:?}", note.json);
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[
            &format!("{tok} 관리자 노트"),
            &format!("{tok} 관리자 문서"),
            &format!("{tok} 멤버 문서"),
        ],
    )
    .await;
    settle(
        &state,
        &app,
        &admin,
        &body_tok,
        &[&format!("{tok} 관리자 문서")],
    )
    .await;

    let found = |user: TestUser, query: String| {
        let app = app.clone();
        async move { titles(&search(&app, &user, &query).await) }
    };
    assert_eq!(
        found(admin.clone(), format!("{tok}&author=me")).await,
        [format!("{tok} 관리자 노트"), format!("{tok} 관리자 문서")]
    );
    assert_eq!(
        found(admin.clone(), format!("{tok}&author={}", writer.id.0)).await,
        [format!("{tok} 멤버 문서")]
    );
    // 멤버가 관리자를 작성자로 골라도 관리자의 개인 노트는 보이지 않는다(거르기는 좁히기만).
    assert_eq!(
        found(writer.clone(), format!("{tok}&author={}", admin.id.0)).await,
        [format!("{tok} 관리자 문서")]
    );
    // 제목만: 본문 낱말로는 찾지 않고, 제목 낱말로는 그대로 찾는다.
    assert!(
        found(admin.clone(), format!("{body_tok}&title_only=true"))
            .await
            .is_empty()
    );
    assert_eq!(
        found(
            admin.clone(),
            format!("{tok}&title_only=true&author=me&kinds=document")
        )
        .await,
        [format!("{tok} 관리자 노트"), format!("{tok} 관리자 문서")]
    );
    let r = send(
        &app,
        Method::GET,
        &format!("/api/search?q={tok}&author=someone"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // 검색 서비스가 없을 때(제목 검색)도 같은 작성자 조건.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&author={}", writer.id.0)).await),
        [format!("{tok} 멤버 문서")]
    );
    assert_eq!(
        titles(&search(&plain_app, &writer, &format!("{tok}&author={}", admin.id.0)).await),
        [format!("{tok} 관리자 문서")]
    );
}

/// 항목 기록(D-39 자료 검색): 제목·메모·칸의 글자로 찾고, 그 항목을 볼 수 있는 사람에게만 보인다.
#[tokio::test]
async fn item_records_are_searchable_by_people_who_see_the_item() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let field_tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기록 검색").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "research", "name": "R", "key": "RSR" })),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "비공개 연구" })),
    )
    .await
    .json;
    let record = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/records", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "reference", "title": format!("{tok} 선행연구"),
                     "body": "핵심 요약", "fields": { "summary": format!("요약 {field_tok}") } })),
    )
    .await;
    assert_eq!(record.status, StatusCode::CREATED, "{:?}", record.json);

    let found = settle(&state, &app, &admin, &tok, &[&format!("{tok} 선행연구")]).await;
    assert_eq!(found[0]["kind"], "record");
    assert_eq!(found[0]["item_id"], item["id"]);
    settle(
        &state,
        &app,
        &admin,
        &field_tok,
        &[&format!("{tok} 선행연구")],
    )
    .await;
    // 비공개 항목이라 멤버에게는 보이지 않는다.
    settle(&state, &app, &member, &tok, &[]).await;
    // 검색 서비스가 없을 때의 제목 검색도 기록을 찾는다.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &format!("{tok}&kinds=record")).await),
        [format!("{tok} 선행연구")]
    );
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
        .with_state(fake.clone());
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
    let ws = create_workspace(&app, &admin_user, "색인 작업").await;
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
    let ws = create_workspace(&app, &user, "세대 재확인").await;
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

/// 감사 델타 SD4: 비공개 항목 안의 문서를 공유로만 보는 사람에게, Meilisearch 경로는 그 항목 id를 돌려주고 모듈로
/// 걸러도 결과를 냈다. 대체(DB) 경로는 같은 요청에 결과를 내지 않아 두 경로가 갈렸다. 문서 조회와 같이 그 항목의
/// 맥락(항목 id·모듈)은 주지 않는다. 항목을 보는 사람은 그대로 항목 id와 모듈 필터를 쓴다. 대체 경로의 모듈 필터가
/// 다른 모듈·위키 문서를 빼는지도 본다(감사 델타 SD8).
#[tokio::test]
async fn a_document_shared_out_of_a_private_item_does_not_reveal_the_item() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "공유 모듈").await;
    let (reader, email) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "SHM" }),
        ),
    )
    .await
    .json;
    let m = module["id"].as_str().unwrap().to_owned();
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{m}/items"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "비공개 항목" })),
    )
    .await
    .json;
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": format!("공유 문서 {tok}") })),
    )
    .await;
    assert_eq!(doc.status, StatusCode::CREATED, "{:?}", doc.json);
    let doc_id = doc.json["id"].as_str().unwrap().to_owned();
    // D-114: 개별 공유 행에 문서의 조직을 명시한다.
    let organization = common::organization_of_workspace(&ws).await;
    let shared = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(json!({ "email": email, "permission": "read", "organization_id": organization })),
    )
    .await;
    assert!(shared.status.is_success(), "{:?}", shared.json);
    let wiki_title = format!("위키 문서 {tok}");
    create_doc(&app, &admin, &ws, &wiki_title).await;
    let shared_title = format!("공유 문서 {tok}");
    let filtered = format!("{tok}&workspace_id={ws}&module_id={m}");
    let shared_of = |results: &Value| -> Value {
        results
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["title"] == shared_title.as_str())
            .cloned()
            .unwrap()
    };

    // Meilisearch 경로: 공유받은 사람은 문서를 찾지만 항목 id는 받지 않고, 모듈로 거르면 나오지 않는다.
    let found = settle(
        &state,
        &app,
        &reader,
        &format!("{tok}&kinds=document"),
        &[&shared_title, &wiki_title],
    )
    .await;
    assert!(shared_of(&found)["item_id"].is_null(), "{found}");
    assert!(titles(&search(&app, &reader, &filtered).await).is_empty());
    // 항목을 보는 사람은 그대로다(모듈 밖 위키 문서는 빠진다).
    let mine = settle(&state, &app, &admin, &filtered, &[&shared_title]).await;
    assert_eq!(mine[0]["item_id"], item["id"]);

    // 대체(DB) 경로도 같다.
    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    let plain_found = search(&plain_app, &reader, &format!("{tok}&kinds=document")).await;
    let mut both = [shared_title.clone(), wiki_title];
    both.sort();
    assert_eq!(titles(&plain_found), both);
    assert!(
        shared_of(&plain_found)["item_id"].is_null(),
        "{plain_found}"
    );
    assert!(titles(&search(&plain_app, &reader, &filtered).await).is_empty());
    let plain_mine = search(&plain_app, &admin, &filtered).await;
    assert_eq!(titles(&plain_mine), [shared_title.as_str()]);
    assert_eq!(plain_mine[0]["item_id"], item["id"]);
}

/// 감사 델타 SD2: 항목 정보(상태·목표 등)나 템플릿 표시만 바꾸면 `updated_at`은 바뀌어도 다시 색인되지 않아,
/// Meilisearch 경로의 항목 날짜가 옛 날짜로 남았다(대체 경로는 지금 날짜). 이제 다시 색인한다.
#[tokio::test]
async fn changing_an_item_profile_reindexes_its_day() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 날짜").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "IDY" }),
        ),
    )
    .await
    .json;
    let title = format!("{tok} 항목");
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await
    .json;
    let id = item["id"].as_str().unwrap().to_owned();
    // 지난날 고친 항목으로 만든다(제목을 다시 써 색인한다).
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    sqlx::query(
        "UPDATE fvoci.items SET title = title, updated_at = '2026-09-01T03:00:00Z' WHERE id = $1::uuid",
    )
    .bind(&id)
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    settle(
        &state,
        &app,
        &admin,
        &format!("{tok}&kinds=item&from=2026-09-01&to=2026-09-01"),
        &[&title],
    )
    .await;

    let profile = send(
        &app,
        Method::PUT,
        &format!("/api/items/{id}/profile"),
        Some(&admin),
        Some(json!({ "base_version": 1, "leader_id": null, "goal": "목표", "scope": "", "starts_on": null, "ends_on": null,
                     "status": "active", "details": {} })),
    )
    .await;
    assert_eq!(profile.status, StatusCode::NO_CONTENT, "{:?}", profile.json);
    let (from, to) = around_today(&state, &admin).await;
    settle(
        &state,
        &app,
        &admin,
        &format!("{tok}&kinds=item&from={from}&to={to}"),
        &[&title],
    )
    .await;
}

/// 감사 델타 SD8: 기록의 날짜는 발생일이 있으면 발생일, 없으면 고친 날이다. 이 규칙은 색인(payload)과 대체 SQL에 따로
/// 있는데 시험이 없어, 한쪽이 바뀌어도 시험이 통과했다. 두 경로에서 같은 날짜 필터로 확인한다.
#[tokio::test]
async fn record_dates_follow_the_occurred_day_then_the_change_day() {
    let state = search_state().await;
    let app = common::app(&state);
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기록 날짜").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "research", "name": "R", "key": "RDT" })),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "연구" })),
    )
    .await
    .json;
    let uri = format!("/api/items/{}/records", item["id"].as_str().unwrap());
    let occurred = format!("{tok} 발생일");
    let changed = format!("{tok} 고친 날");
    for (title, day) in [(&occurred, json!("2026-03-15")), (&changed, Value::Null)] {
        let r = send(
            &app,
            Method::POST,
            &uri,
            Some(&admin),
            Some(
                json!({ "request_id": new_request_id(), "kind": "reference", "title": title,
                         "occurred_on": day, "body": "", "fields": {} }),
            ),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    }
    let march = format!("{tok}&kinds=record&from=2026-03-01&to=2026-03-31");
    let (from, to) = around_today(&state, &admin).await;
    let now = format!("{tok}&kinds=record&from={from}&to={to}");
    settle(&state, &app, &admin, &march, &[&occurred]).await;
    settle(&state, &app, &admin, &now, &[&changed]).await;

    let plain = fvoci_server::app::build_state(config_with(&[("FVOCI_SEARCH_URL", "")])).unwrap();
    let plain_app = common::app(&plain);
    assert_eq!(
        titles(&search(&plain_app, &admin, &march).await),
        [occurred]
    );
    assert_eq!(titles(&search(&plain_app, &admin, &now).await), [changed]);
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

/// D-101(P-24): 실제 시각(문서의 고친 때)은 보는 사람 시간대의 그날 시작·끝과 비교한다. 서울 10-21 08:30(UTC 10-20)에
/// 고친 문서는 서울의 10-21, 뉴욕의 10-20에 나온다. 뉴욕의 DST 날(2026-03-08은 23시간, 2026-11-01은 25시간)은 그날
/// 처음·마지막 1초가 들고 바로 앞·뒤 1초는 빠진다. 대체 경로도 같다. 시간대를 보내지 않으면 저장한 시간대(D-81)를 쓰고,
/// 모르는 시간대는 400이다.
#[tokio::test]
async fn search_dates_use_the_viewers_day_boundaries() {
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let (tok, spring, fall) = (token(), token(), token());
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "시간대 경계").await;
    let morning = format!("{tok} 서울 아침");
    doc_changed_at(&app, &admin, &ws, &morning, "2026-10-20T23:30:00Z").await;
    let s = ["앞", "처음", "끝", "뒤"].map(|w| format!("{spring} {w}"));
    for (title, at) in s.iter().zip([
        "2026-03-08T04:59:59Z",
        "2026-03-08T05:00:00Z",
        "2026-03-09T03:59:59Z",
        "2026-03-09T04:00:00Z",
    ]) {
        doc_changed_at(&app, &admin, &ws, title, at).await;
    }
    let f = ["앞", "처음", "끝", "뒤"].map(|w| format!("{fall} {w}"));
    for (title, at) in f.iter().zip([
        "2026-11-01T03:59:59Z",
        "2026-11-01T04:00:00Z",
        "2026-11-02T04:59:59Z",
        "2026-11-02T05:00:00Z",
    ]) {
        doc_changed_at(&app, &admin, &ws, title, at).await;
    }
    settle(&state, &app, &admin, &spring, &[&s[0], &s[1], &s[2], &s[3]]).await;
    settle(&state, &app, &admin, &fall, &[&f[0], &f[1], &f[2], &f[3]]).await;
    settle(&state, &app, &admin, &tok, &[&morning]).await;

    let day = |q: &str, tz: &str, d: &str| format!("{q}&kinds=document&tz={tz}&from={d}&to={d}");
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "Asia/Seoul", "2026-10-21"),
        &[&morning],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "Asia/Seoul", "2026-10-20"),
        &[],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "America/New_York", "2026-10-20"),
        &[&morning],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&tok, "America/New_York", "2026-10-21"),
        &[],
    )
    .await;
    // 23시간인 날: [03-08 05:00Z, 03-09 04:00Z).
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&spring, "America/New_York", "2026-03-08"),
        &[&s[1], &s[2]],
    )
    .await;
    // 25시간인 날: [11-01 04:00Z, 11-02 05:00Z).
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day(&fall, "America/New_York", "2026-11-01"),
        &[&f[1], &f[2]],
    )
    .await;
    // 한쪽 끝만: 시작은 그날 0시부터, 끝은 그날 마지막까지.
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{spring}&kinds=document&tz=America/New_York&from=2026-03-09"),
        &[&s[3]],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{spring}&kinds=document&tz=America/New_York&to=2026-03-08"),
        &[&s[0], &s[1], &s[2]],
    )
    .await;

    // 시간대를 보내지 않으면 저장한 시간대(D-81)로 본다.
    let saved = send(
        &app,
        Method::PUT,
        "/api/me/settings",
        Some(&admin),
        Some(json!({ "time_zone": "America/New_York" })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::NO_CONTENT, "{:?}", saved.json);
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{tok}&kinds=document&from=2026-10-20&to=2026-10-20"),
        &[&morning],
    )
    .await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &format!("{tok}&kinds=document&from=2026-10-21&to=2026-10-21"),
        &[],
    )
    .await;

    // 모르는 시간대·시간대가 아닌 이름은 400(두 경로 모두).
    for tz in ["Mars/Olympus", "posix/Asia/Seoul", ""] {
        for router in [&app, &plain] {
            let r = send(
                router,
                Method::GET,
                &format!("/api/search?q={tok}&from=2026-10-21&tz={tz}"),
                Some(&admin),
                None,
            )
            .await;
            if tz.is_empty() {
                // 빈 값은 보내지 않은 것과 같다(저장한 시간대).
                assert_eq!(r.status, StatusCode::OK, "{tz}: {:?}", r.json);
            } else {
                assert_eq!(r.status, StatusCode::BAD_REQUEST, "{tz}: {:?}", r.json);
            }
        }
    }
    // 경계 시각을 나타낼 수 없는 날(다음 날이 없는 9999-12-31)은 500이 아니라 400(CodeRabbit #99).
    for router in [&app, &plain] {
        let r = send(
            router,
            Method::GET,
            &format!("/api/search?q={tok}&to=9999-12-31&tz=UTC"),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    }
}

/// D-101(P-24): 날짜만 있는 값(업무 기한, 종일 일정의 시작일, 기록의 발생일)은 시각으로 바꾸지 않아 어느 시간대에서도 같은
/// 날에 나온다. 시각 일정은 일정 자체의 시간대가 아니라 보는 사람 시간대의 날짜로 나온다(감사 델타 SD5: 서울 10-21 08:00
/// 일정은 뉴욕의 달력에서 10-20이다).
#[tokio::test]
async fn date_only_values_keep_their_day_and_timed_events_follow_the_viewer() {
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "날짜만").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "research", "name": "R", "key": "DTZ" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    let item_id = item["id"].as_str().unwrap();
    let due = format!("{tok} 기한 업무");
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": due })),
    )
    .await
    .json;
    let updated = send(&app, Method::PUT, &format!("/api/tasks/{}", task["id"].as_str().unwrap()), Some(&admin), Some(json!({
        "base_version": task["version"], "title": due, "description": "", "status": "todo",
        "priority": "none", "start_date": null, "due_date": "2026-10-21", "estimate_minutes": null, "tags": [],
        "milestone": false, "assignee_ids": [] }))).await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    let all_day = format!("{tok} 종일 일정");
    let timed = format!("{tok} 시각 일정");
    for body in [
        json!({ "request_id": new_request_id(), "kind": "meeting", "title": all_day, "description": "",
                "all_day": true, "start_date": "2026-10-21", "end_date": "2026-10-21", "starts_at": null,
                "ends_at": null, "timezone": "Asia/Seoul" }),
        // 서울 10-21 08:00 = UTC 10-20 23:00.
        json!({ "request_id": new_request_id(), "kind": "meeting", "title": timed, "description": "",
                "all_day": false, "start_date": null, "end_date": null, "starts_at": "2026-10-20T23:00:00Z",
                "ends_at": "2026-10-21T00:00:00Z", "timezone": "Asia/Seoul" }),
    ] {
        let r = send(
            &app,
            Method::POST,
            &format!("/api/items/{item_id}/events"),
            Some(&admin),
            Some(body),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    }
    let occurred = format!("{tok} 발생 기록");
    let r = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/records"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "reference", "title": occurred,
                     "occurred_on": "2026-10-21", "body": "", "fields": {} }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    settle(
        &state,
        &app,
        &admin,
        &tok,
        &[&due, &all_day, &timed, &occurred],
    )
    .await;

    let day = |tz: &str, d: &str| format!("{tok}&tz={tz}&from={d}&to={d}");
    // 시각 일정이 10-21인 시간대(서울 08:00, 키리티마티 13:00)와 10-20인 시간대(뉴욕 19:00, 파고파고 12:00).
    for tz in ["Asia/Seoul", "Pacific/Kiritimati"] {
        both_paths(
            &state,
            &app,
            &plain,
            &admin,
            &day(tz, "2026-10-21"),
            &[&due, &all_day, &timed, &occurred],
        )
        .await;
        both_paths(&state, &app, &plain, &admin, &day(tz, "2026-10-20"), &[]).await;
    }
    for tz in ["America/New_York", "Pacific/Pago_Pago"] {
        both_paths(
            &state,
            &app,
            &plain,
            &admin,
            &day(tz, "2026-10-21"),
            &[&due, &all_day, &occurred],
        )
        .await;
        both_paths(
            &state,
            &app,
            &plain,
            &admin,
            &day(tz, "2026-10-20"),
            &[&timed],
        )
        .await;
    }
}

/// D-102(P-25): 작성자는 색인 필터로 거른다. 예전에는 후보를 60개씩 5쪽(300개)만 받은 뒤 작성자로 걸러, 다른 사람이 만든
/// 더 높은 순위의 결과가 300개를 넘으면 그 작성자의 결과를 놓쳤다. 작성자 필터가 볼 수 없는 것을 드러내지 않는지도 본다
/// (최종 접근 확인은 지금처럼 서버가 한다, P-14).
#[tokio::test]
async fn an_author_filter_finds_results_beyond_the_old_candidate_window() {
    const DECOYS: usize = 320;
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "작성자 창").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (decoy, _) = add_member(&app, &state, &admin, &ws, "decoy", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let target = format!("{tok} 대상 문서");
    let target_id = create_doc(&app, &writer, &ws, &target).await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // 제목이 검색어와 꼭 같은 미끼가 대상보다 위에 온다.
    for _ in 0..DECOYS {
        create_doc(&app, &decoy, &ws, &tok).await;
    }
    // 비공개 항목 안의 관리자 업무: 멤버가 관리자를 작성자로 골라도 나오지 않는다.
    let it = private_item(&app, &admin, &ws, "AWN").await;
    let hidden = format!("{tok} 비공개 업무");
    let r = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", it["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": hidden })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let admin_doc = format!("{tok} 관리자 문서");
    create_doc(&app, &admin, &ws, &admin_doc).await;

    // 모두 색인된 뒤, 작성자 없이 받은 색인 순위에서 대상이 옛 후보 창(300개) 밖에 있는지 먼저 확인한다.
    let total = DECOYS + 3;
    let mut ranked = Vec::new();
    for _ in 0..100 {
        while drain(&state.db, client(&state), Some(&state.storage))
            .await
            .unwrap()
            > 0
        {}
        ranked = client(&state)
            .query(&tok, &format!("workspace_id = \"{ws}\""), 0, 1000, false)
            .await
            .unwrap();
        if ranked.len() == total {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(ranked.len(), total, "everything is indexed");
    let rank = ranked.iter().position(|h| h.id == target_id).unwrap();
    assert!(
        rank >= 300,
        "the target ranks {rank}, inside the old window"
    );

    let by = |author: &TestUser| format!("{tok}&author={}", author.id.0);
    assert_eq!(
        titles(&search(&app, &admin, &by(&writer)).await),
        std::slice::from_ref(&target)
    );
    assert_eq!(
        titles(&search(&plain, &admin, &by(&writer)).await),
        std::slice::from_ref(&target)
    );
    // 작성자 필터는 좁히기만 한다: 멤버는 관리자의 비공개 항목 업무를 보지 못하고, 관리자는 본다.
    for router in [&app, &plain] {
        assert_eq!(
            titles(&search(router, &reader, &by(&admin)).await),
            std::slice::from_ref(&admin_doc)
        );
        let mut mine = vec![admin_doc.clone(), hidden.clone()];
        mine.sort();
        assert_eq!(titles(&search(router, &admin, &by(&admin)).await), mine);
    }
}

/// D-101: 0시에 여름시간이 끝나 0시가 두 번 있는 날(America/Havana 2026-11-01, 01:00 CDT → 00:00 CST)은 첫 번째 0시
/// (04:00Z)에 시작한다. `날짜::timestamp AT TIME ZONE`만 쓰면 두 번째 0시(05:00Z)를 시작으로 보아 그날 첫 1시간이 앞날에
/// 들어간다(시간 기록의 날 경계와 같은 `app_local_instant`, 감사 S21).
#[tokio::test]
async fn a_day_with_a_repeated_midnight_starts_at_the_first_one() {
    let state = search_state().await;
    let app = common::app(&state);
    let plain = plain_app();
    let tok = token();
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "두 번의 0시").await;
    let before = format!("{tok} 전날 밤");
    let first_hour = format!("{tok} 첫 0시 뒤");
    // 10-31 23:30 CDT, 11-01 00:30 CDT(첫 번째).
    doc_changed_at(&app, &admin, &ws, &before, "2026-11-01T03:30:00Z").await;
    doc_changed_at(&app, &admin, &ws, &first_hour, "2026-11-01T04:30:00Z").await;
    settle(&state, &app, &admin, &tok, &[&before, &first_hour]).await;
    let day = |d: &str| format!("{tok}&kinds=document&tz=America/Havana&from={d}&to={d}");
    both_paths(&state, &app, &plain, &admin, &day("2026-10-31"), &[&before]).await;
    both_paths(
        &state,
        &app,
        &plain,
        &admin,
        &day("2026-11-01"),
        &[&first_hour],
    )
    .await;
}
