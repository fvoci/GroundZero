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
    // 이미 참여한 공간의 반복 참여는 행을 바꾸지 않고 실제 참여 함수의 조직 SHARE를 유지한다.
    on(
        joining.conn(),
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
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

/// CR201-B: 공유가 조직 멤버 잠금을 먼저 잡으면 ID 산정도 그 커밋 뒤 공유를 세어야 한다.
#[tokio::test]
async fn demotion_counts_a_share_committed_while_waiting_for_the_organization_member() {
    let w = World::new().await;
    let email = common::unique_email("sharing-admin");
    let user = common::login_with_email(&w.state, "sharing organization admin", &email, true).await;
    common::join_organization(w.org, &user, "admin").await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    let first = w.doc().await;
    let second = w.doc().await;
    w.share(first, &user, "read").await;
    let mut sharing = w.state.db.begin_as(w.owner.id).await.unwrap();
    // 기존 공유를 갱신해 건수는 유지하고 실제 공유 함수의 조직 멤버 SHARE를 잡아 둔다.
    let share = "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'read',$3,@ids)";
    on(sharing.conn(), share, &[first, user.id.0, w.org])
        .await
        .unwrap();
    let holder = pid(sharing.conn()).await;
    let mut demotion = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(demotion.conn()).await;
    let (organization, member) = (w.org, user.id.0);
    let pending = tokio::spawn(async move {
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
    on(sharing.conn(), share, &[second, member, organization])
        .await
        .unwrap();
    sharing.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_ok(), "CR201-B demotion failed: {result:?}");
    assert!(rows(&w.state, &w.owner,
        "SELECT id FROM fvoci.document_shares WHERE document_id IN ($1,$2) AND grantee_user_id=$3",
        &[first, second, member]).await.is_empty());
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, member]
        )
        .await
        .is_empty()
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2",
            &[organization, member]
        )
        .await["role"],
        "member"
    );
}

/// D-88: 권한 없는 ID 산정은 잠긴 행을 기다리지 않고 0, 실제 명령은 기존 거절이다.
#[tokio::test]
async fn unprivileged_demotion_bounds_return_zero_without_waiting_for_locks() {
    let w = World::new().await;
    let member = w.member("member").await;
    let outsider = common::login(&w.state, "unrelated organization member").await;
    let mut holder = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        holder.conn(),
        "SELECT fvoci.app_set_organization_member_ids($1,$2,'member')",
        &[w.org, member.id.0],
    )
    .await
    .unwrap();
    for (actor, expected) in [(&member, "42501"), (&outsider, "P0002")] {
        let mut call = w.state.db.begin_as(actor.id).await.unwrap();
        let bound = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            sqlx::query_scalar::<_, i32>(
                "SELECT fvoci.app_set_organization_member_ids($1,$2,'member')",
            )
            .bind(w.org)
            .bind(member.id.0)
            .fetch_one(call.conn()),
        )
        .await
        .expect("D-88 unprivileged bounds waited for a locked organization or member");
        assert!(
            bound.is_ok(),
            "D-88 unprivileged bounds must return zero: {bound:?}"
        );
        assert_eq!(bound.unwrap(), 0);
        let command = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            sqlx::query("SELECT fvoci.app_set_organization_member($1,$2,'member',$3)")
                .bind(w.org)
                .bind(member.id.0)
                .bind(Vec::<uuid::Uuid>::new())
                .execute(call.conn()),
        )
        .await
        .expect("unprivileged command waited for the organization lock");
        code(&command.unwrap_err(), expected);
    }
    holder.commit().await.unwrap();
}

/// D-88: 권한 선검사를 지난 뒤 강등되어도 상한은 예외 대신 0을 반환한다.
#[tokio::test]
async fn demotion_bounds_return_zero_when_authority_is_lost_while_waiting() {
    let w = World::new().await;
    let actor = w.member("member").await;
    common::join_organization(w.org, &actor, "admin").await;
    let mut holder = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        holder.conn(),
        "SELECT fvoci.app_set_organization_member($1,$2,'member',@ids)",
        &[w.org, actor.id.0],
    )
    .await
    .unwrap();
    let holder_pid = pid(holder.conn()).await;
    let mut call = w.state.db.begin_as(actor.id).await.unwrap();
    let waiter = pid(call.conn()).await;
    let (organization, member) = (w.org, w.owner.id.0);
    let pending = tokio::spawn(async move {
        let result = sqlx::query_scalar::<_, i32>(
            "SELECT fvoci.app_set_organization_member_ids($1,$2,'member')",
        )
        .bind(organization)
        .bind(member)
        .fetch_one(call.conn())
        .await;
        if result.is_ok() {
            call.commit().await.unwrap();
        }
        result
    });
    blocked(holder_pid, waiter).await;
    holder.commit().await.unwrap();
    let bound = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(
        bound.is_ok(),
        "D-88 lost authority must return zero: {bound:?}"
    );
    assert_eq!(bound.unwrap(), 0);
    let command = attempt(
        &w.state,
        &actor,
        "SELECT fvoci.app_set_organization_member($1,$2,'member',@ids)",
        &[organization, member],
    )
    .await;
    code(&command.unwrap_err(), "42501");
}

/// SR-201-4: 담당 추가가 멤버 잠금을 먼저 잡으면 ID 산정도 그 커밋 뒤 담당을 세어야 한다.
#[tokio::test]
async fn demotion_counts_an_assignment_committed_while_waiting_for_the_workspace_member() {
    let w = World::new().await;
    let user = common::login(&w.state, "assigned organization admin").await;
    common::join_organization(w.org, &user, "admin").await;
    rows(
        &w.state,
        &user,
        "SELECT fvoci.app_join_workspace_as_admin($1,@ids)",
        &[w.ws],
    )
    .await;
    let mut assigning = w.state.db.begin_as(w.owner.id).await.unwrap();
    let task = on(
        assigning.conn(),
        "SELECT fvoci.app_create_task($1,$2,'SR-201-4 assigned task',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",
        &[w.item, uuid::Uuid::now_v7(), user.id.0],
    )
    .await
    .unwrap();
    let task = crate::sql_support::id(&task[0]);
    let holder = pid(assigning.conn()).await;
    let mut demotion = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(demotion.conn()).await;
    let (organization, member) = (w.org, user.id.0);
    let pending = tokio::spawn(async move {
        // 처리기와 같은 두 문장이다. 실제 집계 수만큼 전달해야 FV001 재현을 가리지 않는다.
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
    assigning.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(result.is_ok(), "SR-201-4 demotion failed: {result:?}");
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT title FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await["title"],
        "SR-201-4 assigned task"
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT user_id FROM fvoci.task_assignees WHERE task_id=$1 AND user_id=$2",
            &[task, member]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, member]
        )
        .await
        .is_empty()
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2",
            &[organization, member]
        )
        .await["role"],
        "member"
    );
}
