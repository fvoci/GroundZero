use crate::common;
use crate::sql_support::{World, attempt, blocked, code, on, one, pid, rows, workspace};

/// SR-201-2: 참여가 조직 잠금을 먼저 잡으면 ID 산정도 그 커밋 뒤 상태를 세어야 한다.
#[tokio::test]
async fn demotion_counts_a_self_join_committed_while_waiting_for_the_organization() {
    let w = World::new().await;
    let user = common::login(&w.state, "joining organization admin").await;
    common::join_organization(w.org, &user, "admin").await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    let second = workspace(&w.state, &w.owner, w.org).await;
    let mut joining = w.state.db.begin_as(user.id).await.unwrap();
    on(
        joining.conn(),
        "SELECT id FROM fvoci.organizations WHERE id=$1 FOR SHARE",
        &[w.org],
    )
    .await
    .unwrap();
    let holder = pid(joining.conn()).await;
    let mut demotion = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(demotion.conn()).await;
    let (organization, member) = (w.org, user.id.0);
    let pending = tokio::spawn(async move {
        // HTTP 처리기와 같은 두 문장·트랜잭션이다. 고정 상한 @ids를 쓰면 결함을 가린다.
        let count: i32 =
            sqlx::query_scalar("SELECT fvoci.app_set_organization_member_ids($1,$2,'member')")
                .bind(organization)
                .bind(member)
                .fetch_one(demotion.conn())
                .await?;
        sqlx::query("SELECT fvoci.app_set_organization_member($1,$2,'member',$3)")
            .bind(organization)
            .bind(member)
            .bind(fvoci_db::new_ids(usize::try_from(count).unwrap()))
            .execute(demotion.conn())
            .await?;
        demotion.commit().await
    });
    blocked(holder, waiter).await;
    on(
        joining.conn(),
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[second],
    )
    .await
    .unwrap();
    joining.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_ok(), "SR-201-2 demotion failed: {result:?}");
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2",
            &[organization, member],
        )
        .await["role"],
        "member"
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT workspace_id FROM fvoci.workspace_members WHERE workspace_id IN ($1,$2) AND user_id=$3",
            &[w.ws, second, member],
        )
        .await
        .is_empty()
    );
}

/// SR-201-1: 자기 제안·수락은 다른 관리자가 준 권한처럼 강등 회수를 피할 수 없다.
#[tokio::test]
async fn self_offered_ownership_cannot_bypass_admin_join_recovery() {
    let w = World::new().await;
    let user = common::login(&w.state, "self offering organization admin").await;
    common::join_organization(w.org, &user, "admin").await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    let offer = attempt(
        &w.state,
        &user,
        "SELECT fvoci.app_offer_ownership($1,$2,@ids)",
        &[w.ws, user.id.0],
    )
    .await;
    let error = offer.expect_err("SR-201-1 self-offered ownership must be rejected");
    code(&error, "22023");
    assert_eq!(
        error.as_database_error().unwrap().message(),
        "the recipient must be another member"
    );
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_organization_member($1,$2,'member',@ids)",
        &[w.org, user.id.0],
    )
    .await;
    assert_ne!(
        one(
            &w.state,
            &w.owner,
            "SELECT owner_id FROM fvoci.workspaces WHERE id=$1",
            &[w.ws],
        )
        .await["owner_id"],
        user.id.0.to_string(),
        "SR-201-1 self-offered ownership survived organization admin demotion"
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, user.id.0],
        )
        .await
        .is_empty()
    );
}
