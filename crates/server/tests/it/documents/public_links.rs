#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 비로그인 공개 링크 (D-20).

use crate::common;

use axum::http::{Method, StatusCode};
use common::{create_doc, create_workspace, doc_body, login, new_request_id, send};
use serde_json::json;

fn token_path(url: &str) -> String {
    let token = url.rsplit('/').next().unwrap();
    format!("/api/public/{token}")
}

#[tokio::test]
async fn owner_creates_expires_and_revokes_public_links() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "공개").await;
    let doc = create_doc(&app, &admin, &ws, "공개 문서").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1, "title": "공개 문서", "body": doc_body("모두에게") })),
    )
    .await;

    let created = send(
        &app,
        Method::POST,
        &format!("{uri}/public-links"),
        Some(&admin),
        Some(json!({})),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let url = created.json["url"].as_str().unwrap().to_owned();
    assert!(url.starts_with(common::PUBLIC_URL) && url.contains("/p/"));

    // 로그인 없이 읽기 전용으로 열린다.
    let public = send(&app, Method::GET, &token_path(&url), None, None).await;
    assert_eq!(public.status, StatusCode::OK);
    assert_eq!(public.json["title"], "공개 문서");
    assert_eq!(public.json["body"], doc_body("모두에게"));
    assert!(
        public.json.get("id").is_none(),
        "internal ids are not exposed"
    );

    // 틀린 토큰은 404.
    assert_eq!(
        send(&app, Method::GET, "/api/public/not-a-token", None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // 철회하면 바로 닫힌다.
    let links = send(
        &app,
        Method::GET,
        &format!("{uri}/public-links"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(links.json[0]["active"], true);
    let link_id = created.json["id"].as_str().unwrap();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/public-links/{link_id}"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::GET, &token_path(&url), None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let links = send(
        &app,
        Method::GET,
        &format!("{uri}/public-links"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(links.json[0]["active"], false);

    // 과거 만료는 거부, 곧 만료되는 링크는 만료 후 닫힌다.
    let past = send(
        &app,
        Method::POST,
        &format!("{uri}/public-links"),
        Some(&admin),
        Some(json!({ "expires_at": "2000-01-01T00:00:00Z" })),
    )
    .await;
    assert_eq!(past.status, StatusCode::BAD_REQUEST);
    let soon = (time::OffsetDateTime::now_utc() + time::Duration::seconds(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let short = send(
        &app,
        Method::POST,
        &format!("{uri}/public-links"),
        Some(&admin),
        Some(json!({ "expires_at": soon })),
    )
    .await;
    assert_eq!(short.status, StatusCode::CREATED);
    let short_path = token_path(short.json["url"].as_str().unwrap());
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert_eq!(
        send(&app, Method::GET, &short_path, None, None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn only_owner_level_can_manage_public_links() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let outsider = login(&state, "outsider").await;
    let ws = create_workspace(&app, &admin, "공개 권한").await;
    let doc = create_doc(&app, &admin, &ws, "문서").await;
    let path = format!(
        "/api/documents/{}/public-links",
        doc["id"].as_str().unwrap()
    );
    assert_eq!(
        send(&app, Method::POST, &path, Some(&outsider), Some(json!({})))
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::GET, &path, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let created = send(&app, Method::POST, &path, Some(&admin), Some(json!({}))).await;
    let link_id = created.json["id"].as_str().unwrap();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/public-links/{link_id}"),
            Some(&outsider),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}
