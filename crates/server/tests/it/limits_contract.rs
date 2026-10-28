#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! #79: 웹 CSV가 사용할 입력 한도와 실제 HTTP 행 등록의 계약.

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, config_with, create_workspace, login, new_request_id, send};
use serde_json::json;

async fn item(app: &Router, user: &TestUser, ws: &str, kind: &str) -> String {
    let module = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "kind": kind, "name": kind, "key": "LIM" })),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{}", module.json);
    let module = module.json["id"].as_str().unwrap();
    let created = send(
        app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": "한도 시험" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.json);
    created.json["id"].as_str().unwrap().to_owned()
}

async fn task_csv_title_limit(max: usize) {
    let state = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_MAX_TITLE_CHARS", &max.to_string()),
        ("FVOCI_MAX_DOCUMENT_BYTES", "123456"),
        ("FVOCI_MAX_REFERENCES_PER_DOCUMENT", "17"),
        ("FVOCI_MAX_DOCUMENT_DEPTH", "7"),
    ]))
    .unwrap();
    let app = common::app(&state);
    let user = login(&state, "limits").await;
    let me = send(&app, Method::GET, "/api/me", Some(&user), None).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.json);
    assert_eq!(
        me.json["limits"],
        json!({
            "max_title_chars": max,
            "max_document_bytes": 123456,
            "max_references_per_document": 17,
            "max_document_depth": 7,
        })
    );
    assert_eq!(
        send(&app, Method::GET, "/api/me", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );

    let ws = create_workspace(&app, &user, "CSV 한도").await;
    let item = item(&app, &user, &ws, "project").await;
    let uri = format!("/api/items/{item}/tasks");
    let mut accepted = Vec::new();
    // CSV 전용 엔드포인트는 없다. TaskCsvImport.svelte처럼 각 행을 create_task로 보낸다.
    // 한글·비 BMP 문자도 바이트나 UTF-16 길이가 아니라 코드 포인트로 세어야 한다.
    for (length, glyph) in [(max - 1, "가"), (max, "🦀"), (max + 1, "나"), (200, "다")] {
        let title = glyph.repeat(length);
        let row = send(
            &app,
            Method::POST,
            &uri,
            Some(&user),
            Some(json!({ "request_id": new_request_id(), "title": title })),
        )
        .await;
        if length <= max {
            assert_eq!(row.status, StatusCode::CREATED, "{}", row.json);
            assert_eq!(row.json["title"], title);
            accepted.push(title);
        } else {
            assert_eq!(row.status, StatusCode::BAD_REQUEST, "{}", row.json);
            assert_eq!(row.json["error"], "bad_request");
            // 한도 계약(#79): 거절된 칸과 이유, 설정된 상한을 본문에 싣는다.
            assert_eq!(row.json["reason"], "too_long");
            assert_eq!(row.json["field"], "title");
            assert_eq!(row.json["limit"], max);
        }
    }
    let saved = send(&app, Method::GET, &uri, Some(&user), None).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.json);
    let mut titles: Vec<String> = saved
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["title"].as_str().unwrap().to_owned())
        .collect();
    titles.sort();
    accepted.sort();
    assert_eq!(titles, accepted, "거절된 CSV 행은 저장되지 않는다");
}

#[tokio::test]
async fn task_csv_rows_follow_a_title_limit_above_the_default() {
    task_csv_title_limit(300).await;
}

#[tokio::test]
async fn task_csv_rows_follow_a_title_limit_below_the_default() {
    task_csv_title_limit(50).await;
}

#[tokio::test]
async fn course_csv_title_limit_is_fixed_at_300_independent_of_config() {
    for max in [50, 400] {
        let state = fvoci_server::app::build_state(config_with(&[(
            "FVOCI_MAX_TITLE_CHARS",
            &max.to_string(),
        )]))
        .unwrap();
        let app = common::app(&state);
        let user = login(&state, "course-limits").await;
        let ws = create_workspace(&app, &user, "강의 CSV 한도").await;
        let item = item(&app, &user, &ws, "course").await;
        for (length, expected) in [(300, StatusCode::OK), (301, StatusCode::BAD_REQUEST)] {
            let row = send(
                &app,
                Method::POST,
                &format!("/api/items/{item}/episodes"),
                Some(&user),
                Some(json!({ "episodes": [{ "request_id": new_request_id(), "title": "가".repeat(length) }] })),
            ).await;
            assert_eq!(
                row.status, expected,
                "max={max}, length={length}: {}",
                row.json
            );
            if expected == StatusCode::OK {
                assert_eq!(row.json, json!({ "added": 1 }));
            }
        }
        let saved = send(
            &app,
            Method::GET,
            &format!("/api/items/{item}/course"),
            Some(&user),
            None,
        )
        .await;
        assert_eq!(saved.status, StatusCode::OK, "{}", saved.json);
        assert_eq!(saved.json["episodes"].as_array().unwrap().len(), 1);
        assert_eq!(saved.json["episodes"][0]["title"], "가".repeat(300));
    }
}
