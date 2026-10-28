use crate::common::{self, TestUser};
use crate::sql_support::{World, fixture, one, rows};
use axum::http::{Method, StatusCode};
use serde_json::json;

async fn join(w: &World, user: &TestUser) {
    common::join_organization(w.org, user, "admin").await;
    // 실제 HTTP가 필요한 id 개수를 할당하는 경로까지 검증한다.
    let reply = common::send(
        &common::app(&w.state),
        Method::POST,
        &format!(
            "/api/organizations/{}/workspaces/{}/join-as-admin",
            w.org, w.ws
        ),
        Some(user),
        None,
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.json);
}

async fn demote(w: &World, user: &TestUser) {
    let reply = common::send(
        &common::app(&w.state),
        Method::PUT,
        &format!("/api/organizations/{}/members/{}", w.org, user.id.0),
        Some(&w.owner),
        Some(json!({"role":"member"})),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.json);
}

#[tokio::test]
async fn demotion_removes_self_join_only_and_cleans_shares_and_assignments() {
    let w = World::new().await;
    let email = common::unique_email("admin-outside");
    let user = common::login_with_email(&w.state, "admin outside", &email, true).await;
    join(&w, &user).await;
    // 반복 참여해도 참여 전의 "행 없음"을 잃지 않는다.
    join(&w, &user).await;
    let doc = w.doc().await;
    let task = w.task().await;
    w.share(doc, &user, "read").await;
    fixture(
        "INSERT INTO fvoci.task_assignees(task_id,user_id,assigned_by) VALUES($1,$2,$3)",
        &[task, user.id.0, w.owner.id.0],
    )
    .await;
    demote(&w, &user).await;
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, user.id.0]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1 AND grantee_user_id=$2",
            &[doc, user.id.0]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT user_id FROM fvoci.task_assignees WHERE task_id=$1 AND user_id=$2",
            &[task, user.id.0]
        )
        .await
        .is_empty()
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[doc]
        )
        .await["grade"],
        "none"
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_access('organization',$1) AS grade",
            &[w.org]
        )
        .await["grade"],
        "read"
    );
}

async fn restored(role: &str) {
    let w = World::new().await;
    let user = w.member(role).await;
    join(&w, &user).await;
    join(&w, &user).await;
    demote(&w, &user).await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, user.id.0]
        )
        .await["role"],
        role
    );
}

#[tokio::test]
async fn demotion_restores_member_before_self_join() {
    restored("member").await;
}

#[tokio::test]
async fn demotion_restores_readonly_before_self_join() {
    restored("readonly").await;
}

#[tokio::test]
async fn demotion_preserves_a_role_changed_by_another_workspace_admin() {
    let w = World::new().await;
    let user = w.member("member").await;
    join(&w, &user).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_member_role($1,$2,'admin',@ids)",
        &[w.ws, user.id.0],
    )
    .await;
    demote(&w, &user).await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, user.id.0]
        )
        .await["role"],
        "admin"
    );
}

#[tokio::test]
async fn demotion_preserves_accepted_ownership_and_its_independent_admin_grant() {
    let w = World::new().await;
    let user = w.member("member").await;
    join(&w, &user).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_offer_ownership($1,$2,@ids)",
        &[w.ws, user.id.0],
    )
    .await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_accept_ownership($1,@ids)",
        &[w.ws],
    )
    .await;
    demote(&w, &user).await;
    let member = one(&w.state, &w.owner,
        "SELECT to_jsonb(m) AS membership FROM fvoci.workspace_members m WHERE workspace_id=$1 AND user_id=$2",
        &[w.ws,user.id.0]).await;
    assert_eq!(member["membership"]["role"], "admin");
    assert_eq!(member["membership"]["role_source"], "admin_assigned");
    assert_eq!(
        member["membership"]["previous_role"],
        serde_json::Value::Null
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT owner_id FROM fvoci.workspaces WHERE id=$1",
            &[w.ws]
        )
        .await["owner_id"],
        user.id.0.to_string()
    );
}
