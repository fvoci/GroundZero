#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 정의자 함수가 올린 오류 코드(SQLSTATE)를 HTTP 응답으로 바꾸는 표(`AppError::from_db_function`, D-83). 실제 DB가 올린
//! 오류로 확인한다(감사 C32, fvoci/GroundZero#77).

mod common;

use axum::http::StatusCode;
use axum::response::IntoResponse;
use common::{TestUser, login};
use fvoci_server::error::AppError;
use http_body_util::BodyExt;
use serde_json::Value;

/// 앱 사용자로 그 SQLSTATE의 오류를 올리고 응답으로 바꾼다.
async fn raised(
    state: &fvoci_server::http::AppState,
    user: &TestUser,
    code: &str,
) -> (StatusCode, Value) {
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let err = sqlx::query(&format!(
        "DO $$ BEGIN RAISE EXCEPTION 'raised {code}' USING ERRCODE = '{code}'; END $$"
    ))
    .execute(tx.conn())
    .await
    .unwrap_err();
    let response = AppError::from_db_function(err).into_response();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn database_error_codes_map_to_stable_responses() {
    let state = common::state();
    let user = login(&state, "dberrors").await;

    // 정해진 코드는 그 뜻의 응답으로. 입력·상태 오류는 SQL의 설명을 그대로 준다.
    let (status, body) = raised(&state, &user, "22023").await;
    assert_eq!(
        (status, &body["error"], &body["message"]),
        (
            StatusCode::BAD_REQUEST,
            &Value::from("bad_request"),
            &Value::from("raised 22023")
        )
    );
    let (status, body) = raised(&state, &user, "55000").await;
    assert_eq!(
        (status, &body["error"], &body["message"]),
        (
            StatusCode::CONFLICT,
            &Value::from("invalid_state"),
            &Value::from("raised 55000")
        )
    );
    let (status, body) = raised(&state, &user, "P0002").await;
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::NOT_FOUND, &Value::from("not_found"))
    );
    let (status, body) = raised(&state, &user, "42501").await;
    assert_eq!(
        (status, &body["error"]),
        (StatusCode::FORBIDDEN, &Value::from("forbidden"))
    );

    // 동시 변경 재시도(교착·잠금 경합)는 409이고 다시 보내라는 일반 설명이다. 판이 지난 수정은 이 코드가 아니라 55000이다.
    for code in ["40001", "40P01"] {
        let (status, body) = raised(&state, &user, code).await;
        assert_eq!(status, StatusCode::CONFLICT, "{code}");
        assert_eq!(body["error"], "invalid_state", "{code}");
        assert_eq!(
            body["message"], "the change collided with another one; try again",
            "{code}"
        );
    }

    // 무결성 검사 위반(23514, 예: 문서 트리·리비전 단계 검사)은 정상 경로로 생기지 않는 불변식 위반이라 500이고,
    // SQL의 설명을 내보내지 않는다.
    let (status, body) = raised(&state, &user, "23514").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"], "internal");
    assert_ne!(body["message"], "raised 23514");
}
