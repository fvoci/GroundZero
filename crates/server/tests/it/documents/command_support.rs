#![expect(
    clippy::unwrap_used,
    reason = "integration fixture setup and response decoding must fail the test"
)]

use crate::common::{self, TestUser};
use crate::sql_support::World;
use axum::http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};

pub(super) const HEADER: &str = "x-fvoci-expected-user";

pub(super) struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

pub(super) async fn call(
    w: &World,
    user: &TestUser,
    method: Method,
    uri: &str,
    headers: &[(&str, &str)],
    body: &Value,
) -> Reply {
    let mut all = vec![("content-type", "application/json")];
    all.extend_from_slice(headers);
    let (status, headers, bytes) = common::send_raw(
        &common::app(&w.state),
        method,
        uri,
        Some(user),
        &all,
        serde_json::to_vec(body).unwrap(),
    )
    .await;
    Reply {
        status,
        headers,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

pub(super) async fn snapshot(w: &World) -> Value {
    let mut conn = common::fixture_conn().await;
    let mut out = serde_json::Map::new();
    let actors = "SELECT user_id FROM fvoci.organization_members WHERE organization_id=$3";
    let docs = format!(
        "SELECT id FROM fvoci.documents WHERE workspace_id=$1 OR owner_user_id=$2 OR created_by IN ({actors})"
    );
    for (table, predicate) in [
        (
            "documents",
            format!("workspace_id=$1 OR owner_user_id=$2 OR created_by IN ({actors})"),
        ),
        ("document_revisions", format!("document_id IN ({docs})")),
        ("document_shares", format!("document_id IN ({docs})")),
        ("document_links", format!("document_id IN ({docs})")),
        ("document_mentions", format!("document_id IN ({docs})")),
        ("public_links", format!("document_id IN ({docs})")),
        (
            "audit_log",
            format!("workspace_id=$1 OR actor=$2 OR actor IN ({actors})"),
        ),
        (
            "activity",
            format!("workspace_id=$1 OR actor=$2 OR actor IN ({actors})"),
        ),
    ] {
        let rows: Value = sqlx::query_scalar(&format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text), '[]'::jsonb)
             FROM fvoci.{table} t WHERE {predicate}"
        ))
        .bind(w.ws)
        .bind(w.owner.id.0)
        .bind(w.org)
        .fetch_one(&mut conn)
        .await
        .unwrap();
        out.insert(table.to_owned(), rows);
    }
    Value::Object(out)
}

pub(super) fn rename(title: &str, base: i32, request_id: uuid::Uuid) -> Value {
    json!({"request_id":request_id,"base_revision":base,"title":title})
}

pub(super) fn assert_stale(reply: &Reply, changed: Value) {
    assert_eq!(reply.status, StatusCode::CONFLICT, "{}", reply.body);
    assert_eq!(reply.body["error"], "conflict");
    assert!(reply.body["message"].is_string(), "{}", reply.body);
    assert_eq!(
        reply.body["request_id"].as_str(),
        reply
            .headers
            .get("x-request-id")
            .and_then(|v| v.to_str().ok()),
        "{}",
        reply.body
    );
    assert!(reply.body["current"]["id"].is_string(), "{}", reply.body);
    assert_eq!(reply.body["changed"], changed);
    assert!(
        !reply.body["changed"]
            .as_array()
            .unwrap()
            .contains(&json!("base_revision")),
        "{}",
        reply.body
    );
}
