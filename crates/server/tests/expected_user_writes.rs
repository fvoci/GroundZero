#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
mod common;
mod sql_support;

use axum::http::{Method, StatusCode};
use common::TestUser;
use serde_json::{Value, json};
use sql_support::*;
use uuid::Uuid;

const HEADER: &str = "x-fvoci-expected-user";

async fn raw(
    w: &World,
    user: Option<&TestUser>,
    method: Method,
    uri: &str,
    expected: Option<Uuid>,
    body: &Value,
) -> (StatusCode, Value) {
    let expected = expected.map(|id| id.to_string());
    let mut headers = vec![("content-type", "application/json")];
    if let Some(ref value) = expected {
        headers.push((HEADER, value));
    }
    let (status, _, bytes) = common::send_raw(
        &common::app(&w.state),
        method,
        uri,
        user,
        &headers,
        serde_json::to_vec(body).unwrap(),
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn snapshot(w: &World) -> Value {
    let mut conn = common::fixture_conn().await;
    let mut result = serde_json::Map::new();
    for (table, predicate) in [
        ("documents", "workspace_id=$1 OR owner_user_id=$2"),
        (
            "document_revisions",
            "document_id IN (SELECT id FROM fvoci.documents WHERE workspace_id=$1 OR owner_user_id=$2)",
        ),
        (
            "document_mentions",
            "document_id IN (SELECT id FROM fvoci.documents WHERE workspace_id=$1 OR owner_user_id=$2)",
        ),
        (
            "document_links",
            "document_id IN (SELECT id FROM fvoci.documents WHERE workspace_id=$1 OR owner_user_id=$2)",
        ),
        ("uploads", "workspace_id=$1 OR user_id=$2"),
        ("files", "workspace_id=$1 OR owner_user_id=$2"),
        ("audit_log", "workspace_id=$1 OR actor=$2"),
        ("activity", "workspace_id=$1 OR actor=$2"),
        (
            "notifications",
            "document_id IN (SELECT id FROM fvoci.documents WHERE workspace_id=$1 OR owner_user_id=$2)",
        ),
        ("reference_counters", "workspace_id=$1 OR workspace_id=$2"),
        ("numbered_targets", "workspace_id=$1 OR scope_id=$2"),
    ] {
        let rows: Value = sqlx::query_scalar(&format!("SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM fvoci.{table} t WHERE {predicate}"))
            .bind(w.ws).bind(w.owner.id.0).fetch_one(&mut conn).await.unwrap();
        result.insert(table.to_owned(), rows);
    }
    Value::Object(result)
}

fn save() -> Value {
    json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"changed","body":common::doc_body("changed")})
}

#[tokio::test]
async fn required_expected_user_is_checked_before_every_write_and_audit() {
    let w = World::new().await;
    let doc = w.doc().await;
    let other = w.member("member").await;
    let requests = [
        (Method::PUT, format!("/api/documents/{doc}"), save()),
        (
            Method::POST,
            "/api/me/notes".to_owned(),
            json!({"request_id":Uuid::now_v7(),"title":"new note","body":common::doc_body("note")}),
        ),
        (
            Method::POST,
            "/api/uploads".to_owned(),
            json!({"request_id":Uuid::now_v7(),"target":{"kind":"document","id":doc},"name":"expected.txt","content_type":"text/plain","size":10}),
        ),
    ];
    for (method, uri, body) in requests {
        let before = snapshot(&w).await;
        for (expected, status, error) in [
            (None, StatusCode::BAD_REQUEST, "bad_request"),
            (Some(other.id.0), StatusCode::CONFLICT, "account_changed"),
        ] {
            let (actual, response) =
                raw(&w, Some(&w.owner), method.clone(), &uri, expected, &body).await;
            assert_eq!(actual, status, "{method} {uri}: {response}");
            assert_eq!(response["error"], error);
            assert_eq!(
                snapshot(&w).await,
                before,
                "{method} {uri}: a rejected request wrote rows or audit"
            );
        }
        let (actual, response) = raw(
            &w,
            Some(&w.owner),
            method.clone(),
            &uri,
            Some(w.owner.id.0),
            &body,
        )
        .await;
        assert!(
            actual.is_success(),
            "positive control {method} {uri}: {actual} {response}"
        );
        assert_ne!(snapshot(&w).await, before);
    }
    // Organization settings and workspace limits PUT routes are covered by
    // organizations_api::settings_validate_patch_preserve_null_and_require_expected_user.
}

#[tokio::test]
async fn expected_user_is_not_authentication_or_permission() {
    let w = World::new().await;
    let doc = w.doc().await;
    let reader = w.member("readonly").await;
    let outsider = common::login(&w.state, "expected-user outsider").await;
    let uri = format!("/api/documents/{doc}");
    let before = snapshot(&w).await;
    for (session, expected, status) in [
        (None, w.owner.id.0, StatusCode::UNAUTHORIZED),
        (Some(&reader), reader.id.0, StatusCode::FORBIDDEN),
        (Some(&outsider), outsider.id.0, StatusCode::NOT_FOUND),
        (Some(&outsider), w.owner.id.0, StatusCode::CONFLICT),
    ] {
        let (actual, response) = raw(&w, session, Method::PUT, &uri, Some(expected), &save()).await;
        assert_eq!(actual, status, "{response}");
        assert_eq!(snapshot(&w).await, before);
    }
}
