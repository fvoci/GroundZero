//! 파일 HTTP 시험 공통 준비. DB 전체 작업은 포함하지 않는다.
use crate::common::{TestUser, config_with, send, send_raw};
use axum::Router;
use axum::http::{Method, StatusCode};
use fvoci_server::http::AppState;
use serde_json::Value;
use sha2::{Digest, Sha256};
pub(super) fn state_with(overrides: &[(&str, &str)]) -> AppState {
    let mut all = vec![("FVOCI_UPLOAD_CHUNK_BYTES", "5242880")];
    all.extend_from_slice(overrides);
    fvoci_server::app::build_state(config_with(&all)).unwrap()
}

pub(super) fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) async fn start(app: &Router, user: &TestUser, body: Value) -> (StatusCode, Value) {
    let r = send(app, Method::POST, "/api/uploads", Some(user), Some(body)).await;
    (r.status, r.json)
}

pub(super) async fn part(
    app: &Router,
    user: &TestUser,
    upload: &str,
    n: usize,
    bytes: &[u8],
    hash: &str,
) -> StatusCode {
    send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{upload}/parts/{n}"),
        Some(user),
        &[
            ("x-content-sha256", hash),
            ("content-type", "application/octet-stream"),
        ],
        bytes.to_vec(),
    )
    .await
    .0
}

/// 조각으로 나눠 모두 올리고 마친다.
pub(super) async fn upload_all(app: &Router, user: &TestUser, body: Value, data: &[u8]) -> Value {
    let (status, up) = start(app, user, body).await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap();
    let chunk = usize::try_from(up["chunk_bytes"].as_i64().unwrap()).unwrap();
    for (i, piece) in data.chunks(chunk).enumerate() {
        assert_eq!(
            part(app, user, id, i + 1, piece, &sha(piece)).await,
            StatusCode::NO_CONTENT
        );
    }
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(done.json["state"], "done");
    done.json
}
