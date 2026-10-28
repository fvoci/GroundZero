//! DB에 닿기 전의 검색 검증도 실제 오류 응답 계약을 지킨다.

use super::*;
use axum::response::IntoResponse;
use fvoci_core::secret::Secret;
use serde_json::{Value, json};

async fn body(error: AppError) -> Value {
    let response = error.into_response();
    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn query_length_error_has_reason_field_and_limit() {
    let state = SearchState {
        db: fvoci_db::Db::connect_lazy(&Secret::new("postgres://127.0.0.1:1/unused".into()))
            .unwrap(),
        search: None,
    };
    let query = serde_json::from_value(json!({"q": "🦀".repeat(201)})).unwrap();
    let error = search(
        State(state),
        OrgScope {
            user: UserId(Uuid::from_u128(1)),
            organization: OrganizationId(Uuid::from_u128(2)),
        },
        Query(query),
    )
    .await
    .unwrap_err();
    let response = body(error).await;
    assert_eq!(response["reason"], "too_long");
    assert_eq!(response["field"], "q");
    assert_eq!(response["limit"], 200);
}

#[tokio::test]
async fn date_errors_name_the_date_field_and_reason() {
    let me = UserId(Uuid::from_u128(1));
    for (field, value, reason) in [
        ("from", "2026/11/01", "invalid_format"),
        ("to", "9999-12-31", "out_of_range"),
    ] {
        let query = serde_json::from_value(json!({"q":"x", field:value})).unwrap();
        let response = body(filters(&query, me).unwrap_err()).await;
        assert_eq!(response["field"], field);
        assert_eq!(response["reason"], reason);
    }
}

#[tokio::test]
async fn filter_errors_name_the_rejected_filter() {
    let me = UserId(Uuid::from_u128(1));
    for field in ["status_category", "assignee", "author"] {
        let query = serde_json::from_value(json!({"q":"x", field:"unknown"})).unwrap();
        let response = body(filters(&query, me).unwrap_err()).await;
        assert_eq!(response["field"], field);
        assert_eq!(response["reason"], "invalid_format");
    }
    let response = body(parse_kinds(Some("unknown")).unwrap_err()).await;
    assert_eq!(response["field"], "kinds");
    assert_eq!(response["reason"], "invalid_format");
}
