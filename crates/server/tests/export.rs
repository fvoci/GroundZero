#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 데이터 반출 (D-39): 워크스페이스(관리자)와 내 개인 자료를 ZIP으로, 볼 수 있는 것만.

mod common;

use std::io::Read;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, doc_body, login, new_request_id, send,
    send_raw,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

async fn upload(app: &Router, user: &TestUser, doc: &Value, name: &str, bytes: &[u8]) {
    let up = send(
        app,
        Method::POST,
        "/api/uploads",
        Some(user),
        Some(json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                     "name": name, "size": bytes.len(), "content_type": "text/plain" })),
    )
    .await;
    assert_eq!(up.status, StatusCode::CREATED, "{:?}", up.json);
    let id = up.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(bytes));
    send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(user),
        &[("x-content-sha256", hash.as_str())],
        bytes.to_vec(),
    )
    .await;
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

/// ZIP의 (이름 → 내용).
fn unzip(bytes: Vec<u8>) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut out = Vec::new();
            file.read_to_end(&mut out).unwrap();
            (file.name().unwrap().into_owned(), out)
        })
        .collect()
}

#[tokio::test]
async fn workspace_and_personal_exports_contain_only_what_the_person_can_see() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반출할 공간").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &writer, &ws, "회의록").await;
    send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&writer),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": 1, "title": "회의록",
                     "body": doc_body("반출될 안건") }),
        ),
    )
    .await;
    upload(&app, &writer, &doc, "첨부.txt", b"attached").await;
    // 작성자의 개인 노트와 첨부는 워크스페이스 반출에 들어가지 않는다.
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "내 비밀 노트" })),
    )
    .await
    .json;
    upload(&app, &writer, &note, "비밀.txt", b"secret").await;

    let uri = format!("/api/workspaces/{ws}/export");
    assert_eq!(
        send_raw(&app, Method::GET, &uri, Some(&writer), &[], vec![])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send_raw(&app, Method::GET, &uri, Some(&outsider), &[], vec![])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let (status, headers, bytes) =
        send_raw(&app, Method::GET, &uri, Some(&admin), &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/zip");
    let entries = unzip(bytes);
    let json: Value =
        serde_json::from_slice(&entries.iter().find(|(n, _)| n == "export.json").unwrap().1)
            .unwrap();
    assert_eq!(json["workspace"]["name"], "반출할 공간");
    assert_eq!(json["documents"][0]["body"], doc_body("반출될 안건"));
    assert_eq!(json["files"][0]["name"], "첨부.txt");
    assert!(
        json["audit_log"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["action"] == "upload")
    );
    let text = entries
        .iter()
        .find(|(n, _)| n.starts_with("documents/"))
        .unwrap();
    assert!(String::from_utf8_lossy(&text.1).contains("반출될 안건"));
    assert!(
        entries
            .iter()
            .any(|(n, b)| n.starts_with("files/") && n.ends_with("첨부.txt") && b == b"attached")
    );
    let all = String::from_utf8_lossy(&serde_json::to_vec(&json).unwrap()).into_owned();
    assert!(!all.contains("내 비밀 노트") && !entries.iter().any(|(_, b)| b == b"secret"));

    // 내 개인 자료: 내 노트·첨부만.
    let (status, _, bytes) = send_raw(
        &app,
        Method::GET,
        "/api/me/export",
        Some(&writer),
        &[],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entries = unzip(bytes);
    let json: Value =
        serde_json::from_slice(&entries.iter().find(|(n, _)| n == "export.json").unwrap().1)
            .unwrap();
    assert_eq!(json["notes"][0]["title"], "내 비밀 노트");
    assert_eq!(json["notes"].as_array().unwrap().len(), 1);
    assert!(entries.iter().any(|(_, b)| b == b"secret"));
    assert!(!entries.iter().any(|(_, b)| b == b"attached"));
    let (_, _, theirs) = send_raw(
        &app,
        Method::GET,
        "/api/me/export",
        Some(&admin),
        &[],
        vec![],
    )
    .await;
    assert!(
        !unzip(theirs).iter().any(|(_, b)| b == b"secret"),
        "admins do not get others' notes"
    );
}
