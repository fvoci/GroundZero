//! `/healthz` 통합 테스트. `DATABASE_URL`이 없으면 실패한다(건너뛰지 않는다).
//! `cargo xtask test`가 Compose를 띄우고 이 변수를 넣는다.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fvoci_server::config::{Config, DatabaseUrl};
use fvoci_server::http::{AppState, router};
use http_body_util::BodyExt;
use tower::ServiceExt;

/// `DATABASE_URL`이 없으면 Err. 테스트 본문에서 unwrap해 실패시킨다.
fn database_url() -> Result<DatabaseUrl, fvoci_server::error::AppError> {
    Config::from_lookup(|k| std::env::var(k).ok()).map(|cfg| cfg.database_url)
}

#[tokio::test]
async fn healthz_reports_ok_with_database() {
    let url = database_url().unwrap(); // DATABASE_URL must be set; run via `cargo xtask test`
    let pool = fvoci_server::db::connect_lazy(&url).unwrap();
    let app = router(AppState { pool });
    let res = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert_eq!(json["database"], "ok");
}

#[tokio::test]
async fn healthz_reports_degraded_without_database() {
    let bad = Config::from_lookup(|k| {
        (k == "DATABASE_URL").then(|| "postgres://nobody@127.0.0.1:1/none".to_owned())
    })
    .unwrap();
    let pool = fvoci_server::db::connect_lazy(&bad.database_url).unwrap();
    let app = router(AppState { pool });
    let res = app
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
}
