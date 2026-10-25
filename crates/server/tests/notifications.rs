#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

use common::TestUser;

async fn invite(w: &World, email: &str) -> Uuid {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let id = sqlx::query_scalar("SELECT fvoci.app_invite($1,$2,'member',3600,$3)")
        .bind(w.ws)
        .bind(email)
        .bind(ids())
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    id
}

async fn inbox(w: &World, user: &TestUser) -> Vec<serde_json::Value> {
    rows(
        &w.state,
        user,
        "SELECT * FROM fvoci.app_my_notifications(NULL,100,NULL,NULL)",
        &[],
    )
    .await
}

#[tokio::test]
async fn invitation_notifications_record_accept_decline_revoke_and_late_signup() {
    let w = World::new().await;
    for (action, status) in [
        ("accept", "accepted"),
        ("decline", "declined"),
        ("revoke", "revoked"),
    ] {
        let email = common::unique_email(action);
        let user = common::login_with_email(&w.state, action, &email, true).await;
        common::join_organization(w.org, &user, "member").await;
        let invitation = invite(&w, &email).await;
        let before = inbox(&w, &user).await;
        assert_eq!(before.len(), 1);
        assert_eq!(before[0]["kind"], "invite");
        assert_eq!(before[0]["invitation_id"], invitation.to_string());
        assert!(before[0]["read_at"].is_null());
        if action == "revoke" {
            rows(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_revoke_invitation($1,@ids)",
                &[invitation],
            )
            .await;
        } else {
            rows(
                &w.state,
                &user,
                &format!(
                    "SELECT fvoci.app_respond_invitation($1,{},@ids)",
                    action == "accept"
                ),
                &[invitation],
            )
            .await;
        }
        let after = inbox(&w, &user).await;
        assert_eq!(after.len(), 1);
        assert_eq!(after[0]["detail"]["status"], status);
        assert!(!after[0]["read_at"].is_null());
        assert_eq!(
            one(
                &w.state,
                &user,
                "SELECT fvoci.app_notification_unread_count(NULL) AS n",
                &[]
            )
            .await["n"],
            0
        );
    }
    let email = common::unique_email("late");
    let pending = invite(&w, &email).await;
    let late = common::login_with_email(&w.state, "late", &email, true).await;
    rows(
        &w.state,
        &late,
        "SELECT fvoci.app_notify_pending_invitations(@ids)",
        &[],
    )
    .await;
    let list = inbox(&w, &late).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["invitation_id"], pending.to_string());
    assert_eq!(
        one(
            &w.state,
            &late,
            "SELECT fvoci.app_notify_pending_invitations(@ids) AS n",
            &[]
        )
        .await["n"],
        0
    );
}

#[tokio::test]
async fn organization_keyset_unread_and_lost_access_use_the_same_visible_set() {
    let w = World::new().await;
    let email = common::unique_email("inbox");
    let user = common::login_with_email(&w.state, "inbox", &email, true).await;
    common::join_organization(w.org, &user, "member").await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[w.ws, user.id.0],
    )
    .await;
    let other = World::new().await;
    common::join_organization(other.org, &user, "member").await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[other.ws, user.id.0],
    )
    .await;
    let task = w.task().await;
    let foreign = other.task().await;
    let mut seeded = Vec::new();
    for (org, target) in [
        (w.org, task),
        (w.org, task),
        (w.org, task),
        (other.org, foreign),
    ] {
        let id = Uuid::now_v7();
        fixture("INSERT INTO fvoci.notifications(id,user_id,organization_id,kind,task_id,detail,created_at) VALUES($1,$2,$3,'due',$4,jsonb_build_object('fixture',$1::text),'2026-01-01 00:00Z')",&[id,user.id.0,org,target]).await;
        seeded.push(id);
    }
    let third = World::new().await;
    let invitation = invite(&third, &email).await;
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_notification_unread_count($1) AS n",
            &[w.org]
        )
        .await["n"],
        4
    );
    let first = rows(
        &w.state,
        &user,
        "SELECT * FROM fvoci.app_my_notifications($1,2,NULL,NULL)",
        &[w.org],
    )
    .await;
    assert_eq!(first.len(), 2);
    assert!(
        first
            .iter()
            .any(|r| r["invitation_id"] == invitation.to_string())
    );
    let cursor = id(&first[1]);
    let second = rows(&w.state,&user,"SELECT * FROM fvoci.app_my_notifications($1,2,(SELECT created_at FROM fvoci.notifications WHERE id=$2),$2)",&[w.org,cursor]).await;
    assert_eq!(second.len(), 2);
    assert!(
        first
            .iter()
            .all(|a| second.iter().all(|b| a["id"] != b["id"]))
    );
    let combined: Vec<_> = first.iter().chain(&second).collect();
    for target in &seeded[..3] {
        assert!(combined.iter().any(|r| r["id"] == target.to_string()));
    }
    assert!(combined.iter().all(|r| r["id"] != seeded[3].to_string()));
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_mark_notifications_read($1,NULL) AS n",
            &[seeded[0]]
        )
        .await["n"],
        1
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_notification_unread_count($1) AS n",
            &[w.org]
        )
        .await["n"],
        3
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_mark_notifications_read(NULL,$1) AS n",
            &[w.org]
        )
        .await["n"],
        3
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_notification_unread_count($1) AS n",
            &[other.org]
        )
        .await["n"],
        1
    );
    assert_eq!(
        one(
            &w.state,
            &user,
            "SELECT fvoci.app_mark_notifications_read(NULL,NULL) AS n",
            &[]
        )
        .await["n"],
        1
    );
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let visible = rows(
        &w.state,
        &user,
        "SELECT * FROM fvoci.app_my_notifications($1,100,NULL,NULL)",
        &[w.org],
    )
    .await;
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0]["kind"], "invite");
}
