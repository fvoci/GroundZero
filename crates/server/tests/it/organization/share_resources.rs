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
    // 임의의 조직원 id로 이메일을 찾는 내부 함수는 앱 역할에 열지 않는다.
    crate::sql_support::denied(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.app_document_share_recipient($1,$2,$3)",
        &[doc, outside.id.0, w.org],
        "42501",
    )
    .await;
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

#[tokio::test]
async fn share_commands_compare_a_sent_expected_user_before_changing_any_share() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let (outside, email) = outsider(&w).await;
    let doc = w.doc().await;
    let path = format!("/api/documents/{doc}/shares");
    let wrong = outside.id.0.to_string();
    let body = json!({"email":email,"organization_id":w.org,"permission":"read"});
    let (status, _, _) = common::send_raw(
        &app,
        Method::POST,
        &path,
        Some(&w.owner),
        &[
            ("content-type", "application/json"),
            ("x-fvoci-expected-user", &wrong),
        ],
        serde_json::to_vec(&body).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(
        crate::sql_support::rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1",
            &[doc]
        )
        .await
        .is_empty()
    );
    // 지금까지 선택이었던 머리는 계속 선택이다.
    let created = common::send(&app, Method::POST, &path, Some(&w.owner), Some(body)).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let share = created.json["id"].as_str().unwrap();
    let deletion = format!("/api/document-shares/{share}");
    let (status, _, _) = common::send_raw(
        &app,
        Method::DELETE,
        &deletion,
        Some(&w.owner),
        &[("x-fvoci-expected-user", &wrong)],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    let own = w.owner.id.0.to_string();
    let (status, _, _) = common::send_raw(
        &app,
        Method::DELETE,
        &deletion,
        Some(&w.owner),
        &[("x-fvoci-expected-user", &own)],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
}
