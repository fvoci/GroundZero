#![expect(
    clippy::unwrap_used,
    reason = "integration setup and contract assertions must fail the test"
)]

use super::command_support::{HEADER, call, rename, snapshot};
use crate::common;
use crate::sql_support::{self, World};
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn every_document_command_checks_a_sent_expected_user_before_any_write() {
    let w = World::new().await;
    let doc = w.doc().await;
    let member = w.member("member").await;
    let share = w.share(doc, &member, "read").await;
    let email = sql_support::one(
        &w.state,
        &w.owner,
        "SELECT email FROM fvoci.users WHERE id=$1",
        &[member.id.0],
    )
    .await["email"]
        .clone();
    let link = call(
        &w,
        &w.owner,
        Method::POST,
        &format!("/api/documents/{doc}/public-links"),
        &[],
        &json!({}),
    )
    .await;
    assert_eq!(link.status, StatusCode::CREATED, "{}", link.body);
    let link = link.body["id"].as_str().unwrap();
    let commands = [
        (
            Method::POST,
            format!("/api/workspaces/{}/wiki/documents", w.ws),
            json!({"request_id":Uuid::now_v7(),"title":"new wiki"}),
        ),
        (
            Method::POST,
            format!("/api/items/{}/documents", w.item),
            json!({"request_id":Uuid::now_v7(),"title":"new item document"}),
        ),
        (
            Method::POST,
            "/api/me/notes".to_owned(),
            json!({"request_id":Uuid::now_v7(),"title":"new note"}),
        ),
        (
            Method::PUT,
            format!("/api/documents/{doc}"),
            json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"body save","body":common::doc_body("new")}),
        ),
        (
            Method::PATCH,
            format!("/api/documents/{doc}"),
            rename("title", 1, Uuid::now_v7()),
        ),
        (
            Method::PATCH,
            format!("/api/documents/{doc}/settings"),
            json!({"allow_copy":false}),
        ),
        (
            Method::POST,
            format!("/api/documents/{doc}/shares"),
            json!({"organization_id":w.org,"email":email,"permission":"read"}),
        ),
        (
            Method::DELETE,
            format!("/api/document-shares/{share}"),
            json!(null),
        ),
        (
            Method::POST,
            format!("/api/documents/{doc}/copy"),
            json!({"request_id":Uuid::now_v7(),"title":"copy"}),
        ),
        (
            Method::POST,
            format!("/api/documents/{doc}/move-plan"),
            json!({"target_workspace_id":w.ws}),
        ),
        (
            Method::PUT,
            format!("/api/documents/{doc}/placement"),
            json!({"target_workspace_id":w.ws,"acknowledged":{"changes":[]}}),
        ),
        (
            Method::PUT,
            format!("/api/documents/{doc}/parent"),
            json!({"parent_id":null}),
        ),
        (
            Method::PUT,
            format!("/api/documents/{doc}/template"),
            json!({"template":true}),
        ),
        (
            Method::POST,
            format!("/api/documents/{doc}/revisions/1/restore"),
            json!({"request_id":Uuid::now_v7(),"base_revision":1}),
        ),
        (
            Method::POST,
            format!("/api/documents/{doc}/revisions/1/export"),
            json!({"format":"markdown"}),
        ),
        (
            Method::POST,
            format!("/api/documents/{doc}/public-links"),
            json!({}),
        ),
        (
            Method::DELETE,
            format!("/api/public-links/{link}"),
            json!(null),
        ),
        (
            Method::POST,
            "/api/references/resolve".to_owned(),
            json!({"refs":[]}),
        ),
    ];
    let wrong = member.id.0.to_string();
    let mut failures = Vec::new();
    for (method, uri, body) in commands {
        for (expected, status, error) in [
            (wrong.as_str(), StatusCode::CONFLICT, "account_changed"),
            ("not-a-user-id", StatusCode::BAD_REQUEST, "bad_request"),
        ] {
            let before = snapshot(&w).await;
            let reply = call(
                &w,
                &w.owner,
                method.clone(),
                &uri,
                &[(HEADER, expected)],
                &body,
            )
            .await;
            if reply.status != status || reply.body["error"] != error {
                failures.push(format!("{method} {uri}: {} {}", reply.status, reply.body));
            }
            if snapshot(&w).await != before {
                failures.push(format!("{method} {uri}: rejected identity changed document rows, revisions, audit, shares or links"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn only_note_creation_and_body_put_require_the_expected_user_header() {
    let w = World::new().await;
    let doc = w.doc().await;
    for (method, uri, body) in [
        (
            Method::POST,
            "/api/me/notes".to_owned(),
            json!({"request_id":Uuid::now_v7(),"title":"note"}),
        ),
        (
            Method::PUT,
            format!("/api/documents/{doc}"),
            json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"save","body":common::doc_body("save")}),
        ),
    ] {
        let before = snapshot(&w).await;
        let reply = call(&w, &w.owner, method, &uri, &[], &body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{}", reply.body);
        assert_eq!(snapshot(&w).await, before);
    }
    let reply = call(
        &w,
        &w.owner,
        Method::PATCH,
        &format!("/api/documents/{doc}"),
        &[],
        &rename("optional header", 1, Uuid::now_v7()),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
}
