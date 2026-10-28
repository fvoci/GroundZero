use crate::common::{self, TestUser};
use crate::sql_support::{World, fixture};
use axum::http::{Method, StatusCode};
use serde_json::json;

async fn outsider(w: &World) -> (TestUser, String) {
    let email = common::unique_email("share-resource");
    let user = common::login_with_email(&w.state, "Outside member", &email, true).await;
    common::join_organization(w.org, &user, "member").await;
    (user, email)
}

#[tokio::test]
async fn share_resources_report_calculated_roles_and_actual_permission_caps() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let (outside, _) = outsider(&w).await;
    let readonly = w.member("readonly").await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    let legacy = w.share(doc, &outside, "read").await;
    fixture(
        "UPDATE fvoci.document_shares SET permission='write' WHERE id=$1",
        &[legacy],
    )
    .await;
    w.share(doc, &readonly, "read").await;
    w.share(doc, &member, "write").await;
    let listed = common::send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}/shares"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK);
    for (user, role, cap, stored) in [
        (&outside, "outside_member", "read", "write"),
        (&readonly, "readonly", "read", "read"),
        (&member, "member", "write", "write"),
    ] {
        let share = listed
            .json
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["grantee_user_id"] == user.id.0.to_string())
            .unwrap();
        assert_eq!(share["recipient_role"], role);
        assert_eq!(share["max_permission"], cap);
        assert_eq!(share["permission"], stored);
    }
    let note = w.note().await;
    let created = common::send(&app, Method::POST, &format!("/api/documents/{note}/shares"), Some(&w.owner), Some(json!({
        "email": crate::sql_support::one(&w.state,&w.owner,"SELECT email FROM fvoci.users WHERE id=$1",&[outside.id.0]).await["email"],
        "organization_id":w.org,"permission":"write"
    }))).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    assert_eq!(created.json["recipient_role"], serde_json::Value::Null);
    assert_eq!(created.json["max_permission"], "write");
}

#[tokio::test]
async fn share_recipient_is_a_manager_only_read_and_hides_foreign_recipients() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let (outside, email) = outsider(&w).await;
    let doc = w.doc().await;
    let path = format!(
        "/api/documents/{doc}/share-recipient?organization_id={}&email={email}",
        w.org
    );
    let reply = common::send(&app, Method::GET, &path, Some(&w.owner), None).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
    assert_eq!(reply.json["user_id"], outside.id.0.to_string());
    assert_eq!(reply.json["recipient_role"], "outside_member");
    assert_eq!(reply.json["max_permission"], "read");
    w.share(doc, &outside, "read").await;
    assert_eq!(
        common::send(&app, Method::GET, &path, Some(&outside), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let foreign_email = common::unique_email("foreign-recipient");
    common::login_with_email(&w.state, "Foreign", &foreign_email, true).await;
    let foreign_path = format!(
        "/api/documents/{doc}/share-recipient?organization_id={}&email={foreign_email}",
        w.org
    );
    assert_eq!(
        common::send(&app, Method::GET, &foreign_path, Some(&w.owner), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}
