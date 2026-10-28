use crate::common::{self, TestUser};
use crate::sql_support::{World, fixture, one, rows};
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

async fn outside(w: &World) -> TestUser {
    let email = common::unique_email("outside member");
    let user = common::login_with_email(&w.state, "outside member", &email, true).await;
    common::join_organization(w.org, &user, "member").await;
    user
}

// 기존 write 저장값을 재현한다. 새 공유 명령이 이 값을 만들 수 있다는 가정은 하지 않는다.
async fn legacy_write(w: &World, user: &TestUser) -> Uuid {
    let doc = w.doc().await;
    let share = w.share(doc, user, "read").await;
    fixture(
        "UPDATE fvoci.document_shares SET permission='write' WHERE id=$1",
        &[share],
    )
    .await;
    doc
}

#[tokio::test]
async fn outside_member_legacy_write_is_read_without_rewriting_the_share() {
    let w = World::new().await;
    let user = outside(&w).await;
    let doc = legacy_write(&w, &user).await;
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "read"
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT permission FROM fvoci.document_shares WHERE document_id=$1",
            &[doc]
        )
        .await["permission"],
        "write"
    );
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[w.ws, user.id.0],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "write"
    );
}

async fn refused_edit(change_body: bool) {
    let w = World::new().await;
    let user = outside(&w).await;
    let doc = legacy_write(&w, &user).await;
    let app = common::app(&w.state);
    let before = common::send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}"),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(before.status, StatusCode::OK);
    let mut body = before.json["body"].clone();
    let mut title = before.json["title"].clone();
    if change_body {
        body = common::doc_body("denied outside edit");
    } else {
        title = json!("denied outside title");
    }
    let reply = common::send(
        &app, Method::PUT, &format!("/api/documents/{doc}"), Some(&user),
        Some(json!({"request_id": Uuid::now_v7(), "base_revision": before.json["revision"], "title":title, "body":body})),
    ).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{:?}", reply.json);
    let after = common::send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}"),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(after.json["title"], before.json["title"]);
    assert_eq!(after.json["body"], before.json["body"]);
}

#[tokio::test]
async fn outside_member_legacy_write_cannot_edit_body() {
    refused_edit(true).await;
}

#[tokio::test]
async fn outside_member_legacy_write_cannot_change_title() {
    refused_edit(false).await;
}

#[tokio::test]
async fn outside_member_write_share_is_a_field_error_and_read_share_succeeds() {
    let w = World::new().await;
    let user = outside(&w).await;
    let doc = w.doc().await;
    let email = one(
        &w.state,
        &w.owner,
        "SELECT email FROM fvoci.users WHERE id=$1",
        &[user.id.0],
    )
    .await;
    let app = common::app(&w.state);
    let path = format!("/api/documents/{doc}/shares");
    let denied = common::send(
        &app,
        Method::POST,
        &path,
        Some(&w.owner),
        Some(json!({
            "organization_id":w.org, "email":email["email"], "permission":"write"
        })),
    )
    .await;
    assert_eq!(denied.status, StatusCode::BAD_REQUEST, "{:?}", denied.json);
    assert_eq!(denied.json["field"], "permission");
    assert_eq!(denied.json["reason"], "out_of_range");
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1",
            &[doc]
        )
        .await
        .is_empty()
    );
    let accepted = common::send(
        &app,
        Method::POST,
        &path,
        Some(&w.owner),
        Some(json!({
            "organization_id":w.org, "email":email["email"], "permission":"read"
        })),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::CREATED, "{:?}", accepted.json);
}

#[tokio::test]
async fn member_readonly_personal_note_and_resharing_keep_their_rules() {
    let w = World::new().await;
    let member = w.member("member").await;
    let reader = w.member("readonly").await;
    let outsider = outside(&w).await;
    let doc = legacy_write(&w, &reader).await;
    w.share(doc, &member, "write").await;
    assert_eq!(
        one(
            &w.state,
            &reader,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "read"
    );
    assert_eq!(
        one(
            &w.state,
            &member,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "write"
    );
    let note = w.note().await;
    w.share(note, &outsider, "write").await;
    assert_eq!(
        one(
            &w.state,
            &outsider,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[note]
        )
        .await["grade"],
        "write"
    );
    for (actor, target) in [(&member, doc), (&outsider, note)] {
        assert_eq!(
            one(
                &w.state,
                actor,
                "SELECT fvoci.app_document_can_manage($1) AS allowed",
                &[target]
            )
            .await["allowed"],
            false
        );
        crate::sql_support::denied(&w.state, actor,
            "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'read',$3,@ids)",
            &[target, reader.id.0, w.org], "42501").await;
    }
}
