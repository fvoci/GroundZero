use super::command_support::{HEADER, assert_stale, call, rename, snapshot};
use crate::common;
use crate::sql_support::{self, World};
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn stale_title_returns_current_changes_and_request_id_without_writes() {
    let w = World::new().await;
    let doc = w.doc().await;
    let uri = format!("/api/documents/{doc}");
    let saved = common::live::save(&common::app(&w.state), &w.owner, &uri, &json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"server title","body":common::doc_body("server body")})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.json);
    let before = snapshot(&w).await;
    let stale = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[("x-request-id", "document-title-stale")],
        &rename("my title", 1, Uuid::now_v7()),
    )
    .await;
    assert_stale(&stale, json!(["title"]));
    assert_eq!(stale.body["current"], saved.json);
    assert_eq!(snapshot(&w).await, before);
    let unchanged = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[],
        &rename(" server title ", 1, Uuid::now_v7()),
    )
    .await;
    assert_stale(&unchanged, json!([]));
    assert_eq!(snapshot(&w).await, before);
}

#[tokio::test]
async fn stale_restore_returns_sorted_content_fields_and_request_id_without_writes() {
    let w = World::new().await;
    let doc = w.doc().await;
    let uri = format!("/api/documents/{doc}");
    let saved = common::live::save(&common::app(&w.state), &w.owner, &uri, &json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"server title","body":common::doc_body("server body")})).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.json);
    let before = snapshot(&w).await;
    let restored = call(
        &w,
        &w.owner,
        Method::POST,
        &format!("{uri}/revisions/1/restore"),
        &[("x-request-id", "document-restore-stale")],
        &json!({"request_id":Uuid::now_v7(),"base_revision":1}),
    )
    .await;
    assert_stale(&restored, json!(["body", "title"]));
    assert_eq!(snapshot(&w).await, before);
}

#[tokio::test]
async fn document_limits_report_reason_field_and_limit_without_writes() {
    let mut w = World::new().await;
    let doc = w.doc().await;
    let reference_doc = w.doc().await;
    let reference = json!({"type":"reference","attrs":{"kind":"item","id":w.item}});
    for (id, body) in [
        (doc, common::doc_body(&"x".repeat(100_001))),
        (
            reference_doc,
            json!({"type":"doc","content":[reference.clone(),reference.clone(),reference]}),
        ),
    ] {
        let uri = format!("/api/documents/{id}");
        let read = call(&w, &w.owner, Method::GET, &uri, &[], &json!(null)).await;
        let saved = common::live::save(
            &common::app(&w.state),
            &w.owner,
            &uri,
            &json!({"request_id":Uuid::now_v7(),"title":read.body["title"],"body":body}),
        )
        .await;
        assert_eq!(saved.json["revision"], 2);
    }
    let mut config = (*w.state.config).clone();
    config.limits.max_document_bytes = 100_000;
    config.limits.max_references_per_document = 2;
    config.limits.max_title_chars = 16;
    w.state.config = std::sync::Arc::new(config);
    let expected = w.owner.id.0.to_string();
    let cases = [
        (
            Method::POST,
            format!("/api/documents/{doc}/revisions/2/restore"),
            json!({"request_id":Uuid::now_v7(),"base_revision":2}),
            "body",
            "too_long",
            100_000,
        ),
        (
            Method::POST,
            format!("/api/documents/{reference_doc}/revisions/2/restore"),
            json!({"request_id":Uuid::now_v7(),"base_revision":2}),
            "body",
            "too_many",
            2,
        ),
        (
            Method::GET,
            format!("/api/reference-candidates?q={}", "x".repeat(17)),
            json!(null),
            "q",
            "too_long",
            16,
        ),
        (
            Method::POST,
            "/api/references/resolve".to_owned(),
            json!({"refs":vec![json!({"kind":"item","id":w.item});101]}),
            "refs",
            "too_many",
            100,
        ),
    ];
    let mut failures = Vec::new();
    for (method, uri, body, field, reason, limit) in cases {
        let before = snapshot(&w).await;
        let reply = call(
            &w,
            &w.owner,
            method.clone(),
            &uri,
            &[(HEADER, &expected)],
            &body,
        )
        .await;
        if reply.status != StatusCode::BAD_REQUEST
            || reply.body["field"] != field
            || reply.body["reason"] != reason
            || reply.body["limit"] != limit
        {
            failures.push(format!("{method} {uri}: {} {}", reply.status, reply.body));
        }
        assert_eq!(snapshot(&w).await, before);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn new_document_command_routes_succeed_and_only_replaced_routes_are_removed() {
    let w = World::new().await;
    let doc = w.doc().await;
    let uri = format!("/api/documents/{doc}");
    let before = call(&w, &w.owner, Method::GET, &uri, &[], &json!(null))
        .await
        .body;
    let renamed = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[],
        &rename("new title", 1, Uuid::now_v7()),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.body["body"], before["body"]);
    let settings = call(
        &w,
        &w.owner,
        Method::PATCH,
        &format!("{uri}/settings"),
        &[],
        &json!({"allow_copy":false,"previous_allow_copy":false}),
    )
    .await;
    assert_eq!(settings.status, StatusCode::OK, "{}", settings.body);
    let target = sql_support::workspace(&w.state, &w.owner, w.org).await;
    let plan = call(
        &w,
        &w.owner,
        Method::POST,
        &format!("{uri}/move-plan"),
        &[],
        &json!({"target_workspace_id":target}),
    )
    .await;
    assert_eq!(plan.status, StatusCode::OK, "{}", plan.body);
    let moved = call(
        &w,
        &w.owner,
        Method::PUT,
        &format!("{uri}/placement"),
        &[],
        &json!({"target_workspace_id":target,"acknowledged":{"changes":plan.body["changes"]}}),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{}", moved.body);
    for (method, path, status) in [
        (Method::PUT, "settings", StatusCode::METHOD_NOT_ALLOWED),
        (Method::POST, "move", StatusCode::NOT_FOUND),
        (Method::POST, "move/preview", StatusCode::NOT_FOUND),
    ] {
        let reply = call(
            &w,
            &w.owner,
            method,
            &format!("{uri}/{path}"),
            &[],
            &json!({}),
        )
        .await;
        assert_eq!(reply.status, status, "{path}: {}", reply.body);
    }
    let expected = w.owner.id.0.to_string();
    let retained = call(
        &w,
        &w.owner,
        Method::PUT,
        &uri,
        &[(HEADER, &expected)],
        &json!({"request_id":Uuid::now_v7(),"base_revision":renamed.body["revision"],
            "title":renamed.body["title"],"body":renamed.body["body"]}),
    )
    .await;
    assert_eq!(
        retained.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "{}",
        retained.body
    );
}

#[tokio::test]
async fn a_title_retry_after_write_access_is_revoked_is_rejected_before_replay() {
    let w = World::new().await;
    let doc = w.doc().await;
    let writer = w.member("member").await;
    w.share(doc, &writer, "write").await;
    let uri = format!("/api/documents/{doc}");
    let request = rename("writer title", 1, Uuid::now_v7());
    let saved = call(&w, &writer, Method::PATCH, &uri, &[], &request).await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    sql_support::fixture(
        "UPDATE fvoci.workspace_members SET role='readonly' WHERE workspace_id=$1 AND user_id=$2",
        &[w.ws, writer.id.0],
    )
    .await;
    let before = snapshot(&w).await;
    let retry = call(&w, &writer, Method::PATCH, &uri, &[], &request).await;
    assert_eq!(retry.status, StatusCode::FORBIDDEN, "{}", retry.body);
    assert_eq!(snapshot(&w).await, before);
}
