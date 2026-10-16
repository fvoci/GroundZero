#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! `/healthz` 통합 테스트. `DATABASE_URL`이 없으면 실패한다(건너뛰지 않는다).

mod common;

use axum::http::{Method, StatusCode};
use fvoci_server::app::build_state;

#[tokio::test]
async fn healthz_reports_ok_with_database() {
    let state = common::state();
    let r = common::send(&common::app(&state), Method::GET, "/healthz", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["status"], "ok");
    assert_eq!(r.json["database"], "ok");
}

#[tokio::test]
async fn healthz_reports_degraded_without_database() {
    let cfg = common::config_with(&[("DATABASE_URL", "postgres://nobody@127.0.0.1:1/none")]);
    let state = build_state(cfg).unwrap();
    let r = common::send(&common::app(&state), Method::GET, "/healthz", None, None).await;
    assert_eq!(r.status, StatusCode::SERVICE_UNAVAILABLE);
}
