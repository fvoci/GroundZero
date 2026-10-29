use super::command_support::{assert_stale, call, rename, snapshot};
use crate::sql_support::World;
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn body_put_and_workspace_realtime_selection_are_removed() {
    let w = World::new().await;
    let doc = w.doc().await;
    let before = snapshot(&w).await;
    for id in [doc, Uuid::now_v7()] {
        let reply = call(&w, &w.owner, Method::PUT, &format!("/api/documents/{id}"), &[],
            &json!({"request_id":Uuid::now_v7(),"base_revision":1,"title":"removed","body":{"type":"doc"}})).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{}",
            reply.body
        );
    }
    let toggle = call(
        &w,
        &w.owner,
        Method::PUT,
        &format!("/api/workspaces/{}/realtime", w.ws),
        &[],
        &json!({"enabled":false}),
    )
    .await;
    assert_eq!(toggle.status, StatusCode::NOT_FOUND, "{}", toggle.body);
    let read = call(
        &w,
        &w.owner,
        Method::GET,
        &format!("/api/documents/{doc}"),
        &[],
        &json!(null),
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert!(read.body.get("realtime").is_none(), "{}", read.body);
    assert_eq!(snapshot(&w).await, before);
}

#[tokio::test]
async fn title_ignores_if_match_and_uses_base_revision_without_write_etags() {
    let w = World::new().await;
    let doc = w.doc().await;
    let uri = format!("/api/documents/{doc}");
    let request = rename("first title", 1, Uuid::now_v7());
    let saved = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[("if-match", "\"unrelated\"")],
        &request,
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{}", saved.body);
    assert!(saved.headers.get("etag").is_none());
    let before = snapshot(&w).await;
    let retry = call(&w, &w.owner, Method::PATCH, &uri, &[], &request).await;
    assert_eq!(retry.status, StatusCode::OK);
    assert_eq!(retry.body, saved.body);
    assert_eq!(snapshot(&w).await, before);
    let stale = call(
        &w,
        &w.owner,
        Method::PATCH,
        &uri,
        &[("if-match", "*")],
        &rename("next title", 1, Uuid::now_v7()),
    )
    .await;
    assert_stale(&stale, json!(["title"]));
    assert!(stale.headers.get("etag").is_none());
    assert!(stale.body.get("etag").is_none());
    assert_eq!(stale.body["current"], saved.body);
    assert_eq!(snapshot(&w).await, before);
    let restored = call(
        &w,
        &w.owner,
        Method::POST,
        &format!("{uri}/revisions/1/restore"),
        &[],
        &json!({"request_id":Uuid::now_v7(),"base_revision":saved.body["revision"]}),
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK, "{}", restored.body);
    assert!(restored.headers.get("etag").is_none());
    let read = call(&w, &w.owner, Method::GET, &uri, &[], &json!(null)).await;
    assert!(read.headers.get("etag").is_some());
}
