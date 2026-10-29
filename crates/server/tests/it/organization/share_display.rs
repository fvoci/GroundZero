use crate::common;
use crate::sql_support::{World, fixture, one, rows};
use axum::http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
async fn shared_list_uses_effective_permissions_without_rewriting_shares() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let readonly = w.member("member").await;
    let outside = w.member("member").await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    let note = w.note().await;
    for user in [&readonly, &outside] {
        w.share(doc, user, "write").await;
        w.share(note, user, "write").await;
    }
    w.share(doc, &member, "read").await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_member_role($1,$2,'readonly',@ids)",
        &[w.ws, readonly.id.0],
    )
    .await;
    // 기존 공유를 가진 공간 밖 조직원 fixture. 실제 제거 명령은 공유도 철회한다.
    fixture(
        "DELETE FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
        &[w.ws, outside.id.0],
    )
    .await;
    let mut permissions = Vec::new();
    for user in [&readonly, &outside, &member] {
        let reply = common::send(
            &app,
            Method::GET,
            &format!("/api/me/shared?organization_id={}", w.org),
            Some(user),
            None,
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
        let shared = reply.json.as_array().unwrap();
        permissions.push(
            shared.iter().find(|s| s["id"] == doc.to_string()).unwrap()["permission"].clone(),
        );
        if user.id != member.id {
            assert_eq!(
                shared.iter().find(|s| s["id"] == note.to_string()).unwrap()["permission"],
                "write",
                "personal notes must keep their write shares"
            );
            assert_eq!(
                one(
                    &w.state,
                    &w.owner,
                    "SELECT permission FROM fvoci.document_shares WHERE document_id=$1 AND grantee_user_id=$2",
                    &[doc, user.id.0],
                )
                .await["permission"],
                "write",
                "the effective permission must not rewrite the saved share"
            );
        }
    }
    assert_eq!(
        permissions,
        vec![json!("read"), json!("read"), json!("read")],
        "SR-201-7: readonly and outside recipients are capped; a member's read share stays read"
    );
}

#[tokio::test]
async fn share_recipient_and_list_identify_workspace_owners() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let owner = w.member("admin").await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_offer_ownership($1,$2,@ids)",
        &[w.ws, owner.id.0],
    )
    .await;
    rows(
        &w.state,
        &owner,
        "SELECT fvoci.app_accept_ownership($1,@ids)",
        &[w.ws],
    )
    .await;
    let email = one(
        &w.state,
        &w.owner,
        "SELECT email FROM fvoci.users WHERE id=$1",
        &[owner.id.0],
    )
    .await["email"]
        .as_str()
        .unwrap()
        .to_owned();
    let doc = w.doc().await;
    let recipient = common::send(
        &app,
        Method::GET,
        &format!(
            "/api/documents/{doc}/share-recipient?organization_id={}&email={email}",
            w.org
        ),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(recipient.status, StatusCode::OK, "{:?}", recipient.json);
    let created = common::send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc}/shares"),
        Some(&w.owner),
        Some(json!({"email":email,"organization_id":w.org,"permission":"read"})),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let listed = common::send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}/shares"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK, "{:?}", listed.json);
    let share = &listed.json.as_array().unwrap()[0];
    for resource in [&recipient.json, &created.json, share] {
        assert_eq!(resource["max_permission"], "write");
    }
    assert_eq!(
        share["permission"], "read",
        "ownership must not raise a saved read share"
    );
    assert_eq!(
        [
            &recipient.json["recipient_role"],
            &created.json["recipient_role"],
            &share["recipient_role"],
        ],
        [&json!("owner"), &json!("owner"), &json!("owner")],
        "SR-201-8: lookup, creation and listing must identify the workspace owner"
    );
}
