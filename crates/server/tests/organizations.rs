#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use sqlx::Connection;
use uuid::Uuid;

#[tokio::test]
async fn admin_join_rechecks_workspace_after_concurrent_deletion()
-> Result<(), Box<dyn std::error::Error>> {
    let w = World::new().await;
    let admin = common::login(&w.state, "organization admin").await;
    common::join_organization(w.org, &admin, "admin").await;

    let mut fixture = common::fixture_conn().await;
    let mut deletion = fixture.begin().await?;
    sqlx::query("UPDATE fvoci.workspaces SET deleted_at=clock_timestamp() WHERE id=$1")
        .bind(w.ws)
        .execute(&mut *deletion)
        .await?;
    let holder = pid(&mut deletion).await;
    let mut join = w.state.db.begin_as(admin.id).await?;
    let waiter = pid(join.conn()).await;
    let ws = w.ws;
    let pending = tokio::spawn(async move {
        let result = sqlx::query("SELECT fvoci.app_join_workspace_as_admin($1,$2)")
            .bind(ws)
            .bind(ids())
            .execute(join.conn())
            .await;
        if result.is_ok() {
            join.commit().await?;
        }
        Ok::<_, fvoci_core::error::AppError>(result)
    });
    blocked(holder, waiter).await;
    deletion.commit().await?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending).await???;

    let memberships: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
    )
    .bind(w.ws)
    .bind(admin.id.0)
    .fetch_one(&mut fixture)
    .await?;
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.audit_log WHERE actor=$1 AND action='admin_join'",
    )
    .bind(admin.id.0)
    .fetch_one(&mut fixture)
    .await?;
    assert_eq!(
        (memberships, audits),
        (0, 0),
        "deleted workspace must not gain a member or join audit records"
    );
    let error = result
        .err()
        .ok_or("admin join must reject a deleted workspace")?;
    code(&error, "P0002");
    Ok(())
}

#[tokio::test]
async fn organization_creation_membership_and_last_admin_guards() {
    let state = common::state();
    let actor = common::login(&state, "ordinary").await;
    let create = "SELECT fvoci.app_create_organization($1,'organization',$2,@ids) AS id";
    denied(
        &state,
        &actor,
        create,
        &[Uuid::now_v7(), actor.id.0],
        "42501",
    )
    .await;
    common::make_server_admin(&actor).await;
    let org = made(&state, &actor, create, &[Uuid::now_v7(), actor.id.0]).await;
    let mine = rows(
        &state,
        &actor,
        "SELECT id,role FROM fvoci.app_my_organizations()",
        &[],
    )
    .await;
    assert_eq!(mine, vec![serde_json::json!({"id":org,"role":"admin"})]);
    denied(
        &state,
        &actor,
        "SELECT fvoci.app_set_organization_member($1,$2,'member',@ids)",
        &[org, actor.id.0],
        "55000",
    )
    .await;
    let stranger = common::login(&state, "stranger").await;
    assert!(
        rows(
            &state,
            &stranger,
            "SELECT * FROM fvoci.app_my_organizations()",
            &[]
        )
        .await
        .is_empty()
    );
    denied(
        &state,
        &stranger,
        "SELECT fvoci.app_create_workspace($1,$2,'no',@ids)",
        &[Uuid::now_v7(), org],
        "P0002",
    )
    .await;
    common::join_organization(org, &stranger, "member").await;
    workspace(&state, &stranger, org).await;
    denied(
        &state,
        &actor,
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &[org, stranger.id.0],
        "55000",
    )
    .await;
}

#[tokio::test]
async fn workspace_invitation_acceptance_joins_the_organization() {
    let w = World::new().await;
    let email = common::unique_email("invite");
    let invitee = common::login_with_email(&w.state, "invitee", &email, true).await;
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let ws_invite: Uuid = sqlx::query_scalar("SELECT fvoci.app_invite($1,$2,'member',3600,$3)")
        .bind(w.ws)
        .bind(&email)
        .bind(ids())
        .fetch_one(tx.conn())
        .await
        .unwrap();
    let org_invite: Uuid =
        sqlx::query_scalar("SELECT fvoci.app_invite_organization($1,$2,'member',3600,$3)")
            .bind(w.org)
            .bind(&email)
            .bind(ids())
            .fetch_one(tx.conn())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    // 개정 2026-10-26(⑬, D-112): 조직 밖 사람이 워크스페이스 초대를 수락하면 조직 멤버로 먼저 가입하고 감사로그에 남는다.
    rows(
        &w.state,
        &invitee,
        "SELECT fvoci.app_respond_invitation($1,true,@ids)",
        &[ws_invite],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &invitee,
            "SELECT role FROM fvoci.app_my_organizations() WHERE id=$1",
            &[w.org]
        )
        .await["role"],
        "member"
    );
    let joined = rows(
        &w.state,
        &w.owner,
        "SELECT detail FROM fvoci.audit_log WHERE organization_id=$1 AND actor=$2 AND action='join' AND target_type='organization_member'",
        &[w.org, invitee.id.0],
    )
    .await;
    assert_eq!(joined.len(), 1, "{joined:?}");
    assert_eq!(joined[0]["detail"]["via"], "workspace_invitation");
    assert_eq!(joined[0]["detail"]["role"], "member");
    assert_eq!(
        one(
            &w.state,
            &invitee,
            "SELECT fvoci.app_access('workspace',$1) AS grade",
            &[w.ws]
        )
        .await["grade"],
        "write"
    );
    // 이미 멤버가 된 뒤의 조직 초대 수락은 역할을 바꾸지 않는다.
    rows(
        &w.state,
        &invitee,
        "SELECT fvoci.app_respond_invitation($1,true,@ids)",
        &[org_invite],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &invitee,
            "SELECT role FROM fvoci.app_my_organizations() WHERE id=$1",
            &[w.org]
        )
        .await["role"],
        "member"
    );
}

#[tokio::test]
async fn organization_removal_matches_preview_and_revokes_all_received_rights() {
    let w = World::new().await;
    let member = w.member("member").await;
    let ws2 = workspace(&w.state, &w.owner, w.org).await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[ws2, member.id.0],
    )
    .await;
    let doc = w.doc().await;
    let note = w.note().await;
    let team_share = w.share(doc, &member, "read").await;
    let note_share = w.share(note, &member, "write").await;
    let task = made(&w.state,&w.owner,"SELECT fvoci.app_create_task($1,$2,'assigned',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",&[w.item,Uuid::now_v7(),member.id.0]).await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.app_remove_organization_member_preview($1,$2)",
            &[w.org, member.id.0]
        )
        .await,
        serde_json::json!({"workspaces":2,"shares":2,"assignments":1})
    );
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &[w.org, member.id.0],
    )
    .await;
    assert!(rows(&w.state,&w.owner,"SELECT user_id FROM fvoci.workspace_members WHERE workspace_id IN ($1,$2) AND user_id=$3",&[w.ws,ws2,member.id.0]).await.is_empty());
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE id IN ($1,$2)",
            &[team_share, note_share]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT user_id FROM fvoci.task_assignees WHERE task_id=$1 AND user_id=$2",
            &[task, member.id.0]
        )
        .await
        .is_empty()
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.app_remove_organization_member_preview($1,$2)",
            &[w.org, member.id.0]
        )
        .await,
        serde_json::json!({"workspaces":0,"shares":0,"assignments":0})
    );
    for (kind, target) in [("unshare", doc), ("unassign", task)] {
        let logs = rows(
            &w.state,
            &w.owner,
            &format!("SELECT detail FROM fvoci.activity WHERE target_id=$1 AND kind='{kind}'"),
            &[target],
        )
        .await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0]["detail"]["user"], member.id.0.to_string());
    }
    assert_eq!(
        one(
            &w.state,
            &member,
            "SELECT fvoci.app_access('document',$1) AS grade",
            &[note]
        )
        .await["grade"],
        "none"
    );
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.documents WHERE id=$1",
            &[note]
        )
        .await
        .len(),
        1
    );
}

#[tokio::test]
async fn public_link_switch_unknown_settings_effective_limits_and_retention() {
    let w = World::new().await;
    let doc = w.doc().await;
    let token = Uuid::now_v7();
    made(&w.state,&w.owner,"SELECT fvoci.app_create_public_link($1,decode(replace($2::text,'-',''),'hex'),NULL,@ids) AS id",&[doc,token]).await;
    let public = "SELECT * FROM fvoci.app_public_document(decode(replace($1::text,'-',''),'hex'))";
    assert_eq!(system(&w.state, public, &[token]).await.unwrap().len(), 1);
    rows(&w.state,&w.owner,"SELECT fvoci.app_set_organization_settings($1,'{\"public_links_enabled\":false,\"max_file_bytes\":80}',@ids)",&[w.org]).await;
    assert!(system(&w.state, public, &[token]).await.unwrap().is_empty());
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_public_link($1,decode(replace($2::text,'-',''),'hex'),NULL,@ids)",
        &[doc, Uuid::now_v7()],
        "55000",
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_organization_settings($1,'{\"unknown\":1}',@ids)",
        &[w.org],
        "22023",
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_workspace_limits($1,'{\"max_file_bytes\":40}',@ids)",
        &[w.ws],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_effective_limit($1,'max_file_bytes',100) AS n",
            &[w.ws]
        )
        .await["n"],
        40
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_effective_limit($1,'max_file_bytes',20) AS n",
            &[w.ws]
        )
        .await["n"],
        20
    );
    for (org, ws, days) in [
        ("null", "null", 30),
        ("7", "null", 7),
        ("null", "8", 8),
        ("9", "9", 9),
        ("9", "7", 7),
        ("7", "8", 7),
    ] {
        // 공간 값을 정한 뒤 조직 상한을 낮추더라도 실효값은 둘 중 최솟값이다.
        rows(&w.state,&w.owner,"SELECT fvoci.app_set_organization_settings($1,'{\"activity_retention_days\":null}',@ids)",&[w.org]).await;
        rows(&w.state,&w.owner,&format!("SELECT fvoci.app_set_workspace_limits($1,'{{\"activity_retention_days\":{ws}}}',@ids)"),&[w.ws]).await;
        rows(&w.state,&w.owner,&format!("SELECT fvoci.app_set_organization_settings($1,'{{\"activity_retention_days\":{org}}}',@ids)"),&[w.org]).await;
        // app_retention_days는 비공개 함수다. 공개 worker를 통해 경계 양쪽의 행으로 반환값을 검증한다.
        let young = Uuid::now_v7();
        let old = Uuid::now_v7();
        for (id, age) in [(young, days - 1), (old, days + 1)] {
            fixture(&format!("INSERT INTO fvoci.activity(id,organization_id,workspace_id,item_id,target_kind,target_id,actor,kind,created_at) VALUES($1,$2,$3,$4,'item',$4,$5,'create',now()-make_interval(days=>{age}))"),&[id,w.org,w.ws,w.item,w.owner.id.0]).await;
        }
        system(&w.state, "SELECT fvoci.app_system_purge_activity(30)", &[])
            .await
            .unwrap();
        let retained = rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.activity WHERE id IN ($1,$2)",
            &[young, old],
        )
        .await;
        assert!(retained.iter().any(|r| r["id"] == young.to_string()));
        assert!(!retained.iter().any(|r| r["id"] == old.to_string()));
    }
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_workspace_limits($1,'{\"activity_retention_days\":8}',@ids)",
        &[w.ws],
        "22023",
    )
    .await;
}

/// 개정 2026-10-26(⑩, D-121): 마지막으로 고른 조직은 계정 설정에 남는다. 멤버인 조직만 받고 NULL은 지운다.
#[tokio::test]
async fn last_organization_is_remembered_for_members_only() {
    let w = World::new().await;
    let other = World::new().await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_last_organization($1)",
        &[w.org],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT last_organization_id::text AS org FROM fvoci.user_settings WHERE user_id=fvoci.app_user_id()",
            &[]
        )
        .await["org"],
        w.org.to_string()
    );
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_last_organization($1)",
        &[other.org],
        "22023",
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_last_organization(NULL)",
        &[],
    )
    .await;
    assert!(
        one(
            &w.state,
            &w.owner,
            "SELECT last_organization_id::text AS org FROM fvoci.user_settings WHERE user_id=fvoci.app_user_id()",
            &[]
        )
        .await["org"]
            .is_null()
    );
}
