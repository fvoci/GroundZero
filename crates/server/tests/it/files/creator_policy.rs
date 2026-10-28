//! D-115: 문서 작성자 예외는 등급을 빌려주거나 설정 권한을 넓히지 않는다.
use super::fixtures::upload_all;
use crate::common::{self, TestUser, new_request_id, send, send_raw};
use crate::sql_support::World;
use axum::Router;
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

async fn setup() -> (World, Router, TestUser, TestUser, Uuid) {
    let w = World::new().await;
    let author = w.member("member").await;
    let peer = w.member("member").await;
    let app = common::app(&w.state);
    let made = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", w.item),
        Some(&author),
        Some(json!({"request_id":new_request_id(), "title":"작성자 문서"})),
    )
    .await;
    assert_eq!(made.status, StatusCode::CREATED, "{}", made.json);
    let doc: Uuid = made.json["id"].as_str().unwrap().parse().unwrap();
    let settings = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc}/settings"),
        Some(&w.owner),
        Some(json!({"visibility":"workspace", "allow_copy":false})),
    )
    .await;
    assert_eq!(settings.status, StatusCode::OK, "{}", settings.json);
    let mut conn = common::fixture_conn().await;
    let created_by: Uuid = sqlx::query_scalar("SELECT created_by FROM fvoci.documents WHERE id=$1")
        .bind(doc)
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(created_by, author.id.0);
    (w, app, author, peer, doc)
}

#[tokio::test]
async fn a_document_creator_can_download_attachments_with_copy_disabled() {
    let (w, app, author, peer, doc) = setup().await;
    let up = upload_all(
        &app,
        &author,
        json!({"request_id":new_request_id(),
        "target":{"kind":"document","id":doc}, "name":"own.txt", "size":3,
        "content_type":"text/plain"}),
        b"own",
    )
    .await;
    let file = up["file_id"].as_str().unwrap();
    for (user, allowed) in [(&author, true), (&w.owner, true), (&peer, false)] {
        let listed = send(
            &app,
            Method::GET,
            &format!("/api/documents/{doc}/files"),
            Some(user),
            None,
        )
        .await;
        assert_eq!(listed.json[0]["can_download"], allowed);
        let (status, _, data) = send_raw(
            &app,
            Method::GET,
            &format!("/api/files/{file}/content"),
            Some(user),
            &[],
            Vec::new(),
        )
        .await;
        assert_eq!(
            status,
            if allowed {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            }
        );
        if allowed {
            assert_eq!(data, b"own");
        }
    }
    let (inline, _, _) = send_raw(
        &app,
        Method::GET,
        &format!("/api/files/{file}/content?inline=true"),
        Some(&peer),
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(inline, StatusCode::OK);
}

#[tokio::test]
async fn a_document_creator_can_export_with_copy_disabled_but_cannot_change_the_flag() {
    let (w, app, author, peer, doc) = setup().await;
    for (user, allowed) in [(&author, true), (&w.owner, true), (&peer, false)] {
        let got = send(
            &app,
            Method::GET,
            &format!("/api/documents/{doc}"),
            Some(user),
            None,
        )
        .await;
        assert_eq!(got.json["can_export"], allowed);
        let exported = send(
            &app,
            Method::POST,
            &format!("/api/documents/{doc}/revisions/1/export"),
            Some(user),
            Some(json!({"format":"markdown"})),
        )
        .await;
        assert_eq!(
            exported.status,
            if allowed {
                StatusCode::OK
            } else {
                StatusCode::FORBIDDEN
            },
            "{}",
            exported.json
        );
    }
    for user in [&author, &peer] {
        let refused = send(
            &app,
            Method::PUT,
            &format!("/api/documents/{doc}/settings"),
            Some(user),
            Some(json!({"allow_copy":true})),
        )
        .await;
        assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.json);
    }
}

#[tokio::test]
async fn a_document_creator_can_copy_with_copy_disabled() {
    let (w, app, author, peer, doc) = setup().await;
    for (user, allowed) in [(&author, true), (&w.owner, true), (&peer, false)] {
        let copied = send(&app, Method::POST, &format!("/api/documents/{doc}/copy"), Some(user),
            Some(json!({"request_id":new_request_id(), "target_workspace_id":w.ws, "title":"문서 사본"}))).await;
        assert_eq!(
            copied.status,
            if allowed {
                StatusCode::CREATED
            } else {
                StatusCode::FORBIDDEN
            },
            "{}",
            copied.json
        );
    }
}

#[tokio::test]
async fn an_item_copy_keeps_the_creators_document_with_copy_disabled() {
    let (w, app, author, peer, doc) = setup().await;
    for (user, expected) in [(&author, 0), (&peer, 1)] {
        let copied = send(&app, Method::POST, &format!("/api/items/{}/copy", w.item), Some(user),
            Some(json!({"request_id":new_request_id(), "module_id":w.module, "title":"항목 사본", "reset":false}))).await;
        assert_eq!(copied.status, StatusCode::CREATED, "{}", copied.json);
        assert_eq!(copied.json["skipped_documents"], expected);
        let target = copied.json["item_id"].as_str().unwrap();
        let docs = send(
            &app,
            Method::GET,
            &format!("/api/items/{target}/documents"),
            Some(user),
            None,
        )
        .await;
        assert_eq!(docs.status, StatusCode::OK, "{}", docs.json);
        let names: Vec<_> = docs
            .json
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["title"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            if expected == 0 {
                vec!["작성자 문서"]
            } else {
                vec![]
            }
        );
    }
    let original = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}"),
        Some(&author),
        None,
    )
    .await;
    assert_eq!(original.json["allow_copy"], false);
}
