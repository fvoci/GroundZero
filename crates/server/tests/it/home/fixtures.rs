//! 홈 집계 HTTP 시험이 공유하는 읽기 경계.
use crate::common::{self, send};
use crate::sql_support::World;
use axum::http::{Method, StatusCode};
use serde_json::Value;

pub(super) async fn home(w: &World, user: &common::TestUser) -> Value {
    let response = send(
        &common::app(&w.state),
        Method::GET,
        &format!("/api/me/home?organization_id={}&tz=UTC", w.org),
        Some(user),
        None,
    )
    .await;
    assert_eq!(response.status, StatusCode::OK, "{:?}", response.json);
    response.json
}
