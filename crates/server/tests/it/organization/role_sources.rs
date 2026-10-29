use crate::common::{self, TestUser};
use crate::sql_support::{World, code, one, rows};
use serde_json::Value;

async fn membership(w: &World, user: &TestUser) -> Value {
    one(&w.state, &w.owner,
        "SELECT to_jsonb(m) AS membership FROM fvoci.workspace_members m WHERE workspace_id=$1 AND user_id=$2",
        &[w.ws, user.id.0]).await["membership"].clone()
}

#[tokio::test]
async fn role_sources_record_creation_invitation_assignment_and_original_self_join_state() {
    let w = World::new().await;
    assert_eq!(
        membership(&w, &w.owner).await["role_source"],
        "workspace_created"
    );
    let (user, _) = common::add_member(
        &common::app(&w.state),
        &w.state,
        &w.owner,
        &w.ws.to_string(),
        "invited",
        "readonly",
    )
    .await;
    assert_eq!(
        membership(&w, &user).await["role_source"],
        "invitation_accepted"
    );
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_member_role($1,$2,'member',@ids)",
        &[w.ws, user.id.0],
    )
    .await;
    assert_eq!(membership(&w, &user).await["role_source"], "admin_assigned");
    common::join_organization(w.org, &user, "admin").await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    // 자기 자신에게 같은 역할을 지정해도 독립 부여가 되지는 않는다.
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_set_member_role($1,$2,'admin',@ids)",
        &[w.ws, user.id.0],
    )
    .await;
    let joined = membership(&w, &user).await;
    assert_eq!(joined["role_source"], "admin_join");
    assert_eq!(joined["previous_role"], "member");
    assert_eq!(joined["previous_role_source"], "admin_assigned");
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_member_role($1,$2,'readonly',@ids)",
        &[w.ws, user.id.0],
    )
    .await;
    let assigned = membership(&w, &user).await;
    assert_eq!(assigned["role_source"], "admin_assigned");
    assert_eq!(assigned["previous_role"], Value::Null);
    assert_eq!(assigned["previous_role_source"], Value::Null);
}

#[tokio::test]
async fn previous_role_and_source_are_paired_and_only_allowed_for_admin_self_join() {
    let w = World::new().await;
    let user = w.member("member").await;
    let mut conn = common::fixture_conn().await;
    for (source, role, previous_source) in [
        ("admin_join", Some("member"), None),
        ("admin_join", None, Some("admin_assigned")),
        (
            "admin_assigned",
            Some("member"),
            Some("invitation_accepted"),
        ),
        (
            "invitation_accepted",
            Some("member"),
            Some("admin_assigned"),
        ),
        ("admin_join", Some("outside_member"), Some("admin_assigned")),
        ("admin_join", Some("member"), Some("unknown")),
    ] {
        let error = sqlx::query("UPDATE fvoci.workspace_members SET role_source=$1,previous_role=$2,previous_role_source=$3 WHERE workspace_id=$4 AND user_id=$5")
            .bind(source).bind(role).bind(previous_source).bind(w.ws).bind(user.id.0)
            .execute(&mut conn).await.unwrap_err();
        code(&error, "23514");
    }
}
