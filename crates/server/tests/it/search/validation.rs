#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "HTTP integration assertions panic when the contract differs"
)]
//! PR3 칸 검증 계약을 실제 검색 HTTP 응답으로 확인한다.

use crate::common;
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn invalid_search_fields_include_reason_field_and_limit() {
    let state = common::state();
    let app = common::app(&state);
    let user = common::login(&state, "search-validation").await;
    common::create_workspace(&app, &user, "Search validation").await;
    let organization = common::organization_for(&user).await;
    let long_query = format!("q={}", "x".repeat(201));
    let mut failures = Vec::new();
    for (query, field, reason, limit) in [
        (long_query.as_str(), "q", "too_long", Some(200)),
        ("q=x&from=2026/11/01", "from", "invalid_format", None),
        ("q=x&to=9999-12-31", "to", "out_of_range", None),
        (
            "q=x&status_category=late",
            "status_category",
            "invalid_format",
            None,
        ),
        ("q=x&kinds=nope", "kinds", "invalid_format", None),
        ("q=x&assignee=someone", "assignee", "invalid_format", None),
        ("q=x&author=someone", "author", "invalid_format", None),
        (
            "q=x&from=2026-11-01&tz=Unknown/Zone",
            "tz",
            "invalid_format",
            None,
        ),
        (
            "q=x&workspace_id=wrong",
            "workspace_id",
            "invalid_format",
            None,
        ),
        ("q=x&module_id=wrong", "module_id", "invalid_format", None),
        ("q=x&title_only=maybe", "title_only", "invalid_format", None),
        ("kinds=task", "q", "required", None),
    ] {
        let reply = common::send(
            &app,
            Method::GET,
            &format!("/api/search?organization_id={organization}&{query}"),
            Some(&user),
            None,
        )
        .await;
        if reply.status != StatusCode::BAD_REQUEST
            || reply.json["error"] != "bad_request"
            || reply.json["reason"] != reason
            || reply.json["field"] != field
            || reply.json.get("limit") != limit.map(|n| json!(n)).as_ref()
        {
            failures.push(format!("{query}: {} {:?}", reply.status, reply.json));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
