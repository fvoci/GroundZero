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

/// HWP·HWPX 미리보기 WASM(D-76): 압축본(br)을 그대로 보내고 해시 폴더의 파일은 오래 캐시, manifest는 매번 확인.
/// 없는 파일은 SPA(index.html)가 아니라 404.
#[tokio::test]
async fn rhwp_wasm_is_served_compressed_and_cached() {
    let dir = std::env::temp_dir().join(format!("fvoci-web-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(dir.join("rhwp/abc123")).unwrap();
    std::fs::write(
        dir.join("index.html"),
        "<!doctype html><title>fvoci</title>",
    )
    .unwrap();
    std::fs::write(dir.join("rhwp/manifest.json"), r#"{"dir":"abc123"}"#).unwrap();
    std::fs::write(dir.join("rhwp/abc123/rhwp_bg.wasm"), b"\0asm-raw").unwrap();
    std::fs::write(dir.join("rhwp/abc123/rhwp_bg.wasm.br"), b"br-bytes").unwrap();
    let state = build_state(common::config_with(&[(
        "FVOCI_WEB_DIR",
        dir.to_str().unwrap(),
    )]))
    .unwrap();
    let app = common::app(&state);
    let header =
        |h: &axum::http::HeaderMap, name: &str| h.get(name).map(|v| v.to_str().unwrap().to_owned());

    let (status, headers, body) = common::send_raw(
        &app,
        Method::GET,
        "/rhwp/abc123/rhwp_bg.wasm",
        None,
        &[("accept-encoding", "br, gzip")],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"br-bytes");
    assert_eq!(header(&headers, "content-encoding").as_deref(), Some("br"));
    assert_eq!(
        header(&headers, "content-type").as_deref(),
        Some("application/wasm")
    );
    assert_eq!(
        header(&headers, "cache-control").as_deref(),
        Some("public, max-age=31536000, immutable")
    );

    let (_, headers, body) = common::send_raw(
        &app,
        Method::GET,
        "/rhwp/abc123/rhwp_bg.wasm",
        None,
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(body, b"\0asm-raw");
    assert_eq!(header(&headers, "content-encoding"), None);

    let (status, headers, _) = common::send_raw(
        &app,
        Method::GET,
        "/rhwp/manifest.json",
        None,
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        header(&headers, "cache-control").as_deref(),
        Some("no-cache")
    );

    let (status, _, body) = common::send_raw(
        &app,
        Method::GET,
        "/rhwp/missing/rhwp.js",
        None,
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!String::from_utf8_lossy(&body).contains("<title>fvoci</title>"));
    std::fs::remove_dir_all(dir).unwrap();
}
