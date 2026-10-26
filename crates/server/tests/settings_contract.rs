#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: failures must panic"
)]
//! 마지막 조직의 3상태 입력과 표시 설정의 원자적 저장.
mod common;
mod sql_support;

use axum::http::Method;
use common::send;
use serde_json::{Value, json};
use sql_support::World;

#[tokio::test]
async fn last_organization_settings_preserve_clear_and_rollback() {
    let w = World::new().await;
    let other = World::new().await;
    let app = common::app(&w.state);
    let settings = |body: Value| {
        send(
            &app,
            Method::PUT,
            "/api/me/settings",
            Some(&w.owner),
            Some(body),
        )
    };
    let me = || send(&app, Method::GET, "/api/me", Some(&w.owner), None);
    assert_eq!(me().await.json["last_organization_id"], Value::Null);
    assert_eq!(
        settings(json!({"last_organization_id":w.org,"theme":"dark"}))
            .await
            .status,
        204
    );
    assert_eq!(settings(json!({"locale":"en"})).await.status, 204);
    let saved = me().await.json;
    assert_eq!(saved["last_organization_id"], json!(w.org));
    assert_eq!(saved["theme"], "dark");
    assert_eq!(saved["locale"], "en");

    let rejected = settings(json!({"last_organization_id":other.org,"theme":"light"})).await;
    assert_eq!(rejected.status, 400, "{:?}", rejected.json);
    assert_eq!(
        me().await.json,
        saved,
        "a rejected organization rolls back display changes"
    );
    let rejected = settings(json!({"last_organization_id":null,"theme":"invalid"})).await;
    assert_eq!(rejected.status, 400, "{:?}", rejected.json);
    assert_eq!(
        me().await.json,
        saved,
        "invalid display settings preserve the organization"
    );

    assert_eq!(
        settings(json!({"last_organization_id":null})).await.status,
        204
    );
    let cleared = me().await.json;
    assert_eq!(cleared["last_organization_id"], Value::Null);
    assert_eq!(cleared["theme"], "dark");
    assert_eq!(cleared["locale"], "en");
    assert_eq!(
        settings(json!({"last_organization_id":"invalid"}))
            .await
            .status,
        422
    );
    assert_eq!(me().await.json, cleared);
}
