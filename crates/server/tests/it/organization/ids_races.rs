//! D-88·D-114: ID 집계가 기다린 뒤 커밋된 공유·담당까지 세어 제거를 끝낸다.

use crate::common::{self, TestUser};
use crate::sql_support::{World, blocked, fixture, made, on, one, pid, rows};
use fvoci_core::error::AppError;
use fvoci_db::UserTx;
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Change {
    count: &'static str,
    command: &'static str,
}

const REMOVE_ORGANIZATION: Change = Change {
    count: "SELECT fvoci.app_remove_organization_member_ids($1,$2)",
    command: "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
};
const REMOVE_MEMBER: Change = Change {
    count: "SELECT fvoci.app_remove_member_ids($1,$2)",
    command: "SELECT fvoci.app_remove_member($1,$2,@ids)",
};
const LEAVE: Change = Change {
    count: "SELECT fvoci.app_leave_workspace_ids($1)",
    command: "SELECT fvoci.app_leave_workspace($1,@ids)",
};
const DEMOTE: Change = Change {
    count: "SELECT fvoci.app_set_member_role_ids($1,$2)",
    command: "SELECT fvoci.app_set_member_role($1,$2,'readonly',@ids)",
};
const DELETE_GROUP: Change = Change {
    count: "SELECT fvoci.app_delete_group_ids($1)",
    command: "SELECT fvoci.app_delete_group($1,@ids)",
};
const REMOVE_GROUP_MEMBER: Change = Change {
    count: "SELECT fvoci.app_set_group_member_ids($1,$2,false)",
    command: "SELECT fvoci.app_set_group_member($1,$2,false,@ids)",
};
const ADD_GROUP_MEMBER: Change = Change {
    count: "SELECT fvoci.app_set_group_member_ids($1,$2,true)",
    command: "SELECT fvoci.app_set_group_member($1,$2,true,@ids)",
};
const SHARE: &str = "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'read',$3,@ids)";
const ASSIGN_TASK: &str = "SELECT fvoci.app_create_task($1,$2,'racing task',NULL,'','할 일','none',NULL,NULL,NULL,ARRAY[$3]::uuid[],'{}',false,@ids)";
const GROUP_MEMBER: &str = "SELECT fvoci.app_set_group_member($1,$2,true,@ids)";

async fn execute(tx: &mut UserTx, change: Change, args: &[Uuid]) -> Result<(), AppError> {
    let mut query = sqlx::query_scalar::<_, i32>(change.count);
    for arg in args {
        query = query.bind(arg);
    }
    let count = query.fetch_one(tx.conn()).await?;
    let sql = change
        .command
        .replace("@ids", &format!("${}::uuid[]", args.len() + 1));
    let mut query = sqlx::query(&sql);
    for arg in args {
        query = query.bind(arg);
    }
    query
        .bind(fvoci_db::new_ids(usize::try_from(count).unwrap()))
        .execute(tx.conn())
        .await?;
    Ok(())
}

async fn pending(
    w: &World,
    actor: &TestUser,
    change: Change,
    args: Vec<Uuid>,
) -> (i32, JoinHandle<Result<(), AppError>>) {
    let mut tx = w.state.db.begin_as(actor.id).await.unwrap();
    let waiter = pid(tx.conn()).await;
    let task = tokio::spawn(async move {
        // HTTP 처리기처럼 정확한 집계값을 새 ID로 바꾸고 같은 트랜잭션에서 명령한다.
        execute(&mut tx, change, &args).await?;
        tx.commit().await
    });
    (waiter, task)
}

async fn completes(change: Change, task: JoinHandle<Result<(), AppError>>) {
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.is_ok(),
        "{} must finish after the competing commit: {result:?}",
        change.command
    );
}

#[derive(Clone, Copy)]
enum Removal {
    Organization,
    Workspace,
    Leave,
}

async fn shares_committed_while_waiting(removal: Removal) {
    let w = World::new().await;
    let member = w.member("member").await;
    let first = w.doc().await;
    let second = w.doc().await;
    let mut sharing = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(sharing.conn(), SHARE, &[first, member.id.0, w.org])
        .await
        .unwrap();
    let holder = pid(sharing.conn()).await;
    let (change, actor, args) = match removal {
        Removal::Organization => (REMOVE_ORGANIZATION, &w.owner, vec![w.org, member.id.0]),
        Removal::Workspace => (REMOVE_MEMBER, &w.owner, vec![w.ws, member.id.0]),
        Removal::Leave => (LEAVE, &member, vec![w.ws]),
    };
    let (waiter, task) = pending(&w, actor, change, args).await;
    // 수정 전에는 이미 센 명령, 수정 뒤에는 집계 함수가 이 실제 공유 트랜잭션을 기다린다.
    blocked(holder, waiter).await;
    on(sharing.conn(), SHARE, &[second, member.id.0, w.org])
        .await
        .unwrap();
    sharing.commit().await.unwrap();
    completes(change, task).await;
    assert_eq!(one(&w.state, &w.owner,
        "SELECT count(*) AS n FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
        &[w.ws, member.id.0]).await["n"], 0);
    assert_eq!(one(&w.state, &w.owner,
        "SELECT count(*) AS n FROM fvoci.document_shares WHERE grantee_user_id=$1 AND organization_id=$2",
        &[member.id.0, w.org]).await["n"], 0);
    if matches!(removal, Removal::Organization) {
        assert_eq!(one(&w.state, &w.owner,
            "SELECT count(*) AS n FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2",
            &[w.org, member.id.0]).await["n"], 0);
    }
}

#[tokio::test]
async fn organization_removal_counts_shares_committed_while_waiting() {
    shares_committed_while_waiting(Removal::Organization).await;
}

#[tokio::test]
async fn member_removal_counts_shares_committed_while_waiting() {
    shares_committed_while_waiting(Removal::Workspace).await;
}

#[tokio::test]
async fn workspace_leave_counts_shares_committed_while_waiting() {
    shares_committed_while_waiting(Removal::Leave).await;
}

#[tokio::test]
async fn member_demotion_counts_assignments_committed_while_waiting() {
    let w = World::new().await;
    let member = w.member("admin").await;
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let mut assigning = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        assigning.conn(),
        ASSIGN_TASK,
        &[w.item, Uuid::now_v7(), member.id.0],
    )
    .await
    .unwrap();
    let holder = pid(assigning.conn()).await;
    let (waiter, task) = pending(&w, &w.owner, DEMOTE, vec![w.ws, member.id.0]).await;
    blocked(holder, waiter).await;
    on(
        assigning.conn(),
        ASSIGN_TASK,
        &[w.item, Uuid::now_v7(), member.id.0],
    )
    .await
    .unwrap();
    assigning.commit().await.unwrap();
    completes(DEMOTE, task).await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT role FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, member.id.0]
        )
        .await["role"],
        "readonly"
    );
    assert_eq!(one(&w.state, &w.owner,
        "SELECT count(*) AS n FROM fvoci.task_assignees a JOIN fvoci.tasks t ON t.id=a.task_id WHERE t.item_id=$1 AND a.user_id=$2",
        &[w.item, member.id.0]).await["n"], 0);
}

async fn group_changes_count_a_new_member_and_assignment(change: Change) {
    let w = World::new().await;
    let member = w.member("member").await;
    let other = w.member("member").await;
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let group = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_group($1,'racing group',@ids) AS id",
        &[w.ws],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_set_item_members($1,'{}'::uuid[],ARRAY[$2]::uuid[],@ids)",
        &[w.item, group],
    )
    .await;
    let mut adding = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(adding.conn(), GROUP_MEMBER, &[group, other.id.0])
        .await
        .unwrap();
    let holder = pid(adding.conn()).await;
    let args = if change.command == DELETE_GROUP.command {
        vec![group]
    } else {
        vec![group, member.id.0]
    };
    let (waiter, task) = pending(&w, &w.owner, change, args).await;
    blocked(holder, waiter).await;
    on(adding.conn(), GROUP_MEMBER, &[group, member.id.0])
        .await
        .unwrap();
    on(
        adding.conn(),
        ASSIGN_TASK,
        &[w.item, Uuid::now_v7(), member.id.0],
    )
    .await
    .unwrap();
    adding.commit().await.unwrap();
    completes(change, task).await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.group_members WHERE group_id=$1 AND user_id=$2",
            &[group, member.id.0]
        )
        .await["n"],
        0
    );
    assert_eq!(one(&w.state, &w.owner,
        "SELECT count(*) AS n FROM fvoci.task_assignees a JOIN fvoci.tasks t ON t.id=a.task_id WHERE t.item_id=$1 AND a.user_id=$2",
        &[w.item, member.id.0]).await["n"], 0);
}

#[tokio::test]
async fn group_deletion_counts_members_and_assignments_committed_while_waiting() {
    group_changes_count_a_new_member_and_assignment(DELETE_GROUP).await;
}

#[tokio::test]
async fn group_removal_counts_members_and_assignments_committed_while_waiting() {
    group_changes_count_a_new_member_and_assignment(REMOVE_GROUP_MEMBER).await;
}

#[tokio::test]
async fn opposite_group_membership_commands_serialize_in_both_orders_without_deadlock() {
    for first_adds in [true, false] {
        let w = World::new().await;
        let member = w.member("member").await;
        let group = made(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_create_group($1,'opposite commands',@ids) AS id",
            &[w.ws],
        )
        .await;
        if !first_adds {
            rows(&w.state, &w.owner, GROUP_MEMBER, &[group, member.id.0]).await;
        }
        let (first, second) = if first_adds {
            (ADD_GROUP_MEMBER, REMOVE_GROUP_MEMBER)
        } else {
            (REMOVE_GROUP_MEMBER, ADD_GROUP_MEMBER)
        };
        let args = vec![group, member.id.0];
        let mut holding = w.state.db.begin_as(w.owner.id).await.unwrap();
        execute(&mut holding, first, &args).await.unwrap();
        let holder = pid(holding.conn()).await;
        let (waiter, task) = pending(&w, &w.owner, second, args).await;
        blocked(holder, waiter).await;
        holding.commit().await.unwrap();
        completes(second, task).await;
        assert_eq!(
            one(
                &w.state,
                &w.owner,
                "SELECT count(*) AS n FROM fvoci.group_members WHERE group_id=$1 AND user_id=$2",
                &[group, member.id.0]
            )
            .await["n"],
            i64::from(!first_adds)
        );
    }
}

#[tokio::test]
async fn id_count_permissions_are_checked_before_waiting_for_locked_rows() {
    let w = World::new().await;
    let reader = w.member("readonly").await;
    let outsider = common::login(&w.state, "outside ID counter").await;
    let group = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_group($1,'locked group',@ids) AS id",
        &[w.ws],
    )
    .await;
    let mut holding = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        holding.conn(),
        "SELECT fvoci.app_set_organization_member_ids($1,$2,'member')",
        &[w.org, reader.id.0],
    )
    .await
    .unwrap();
    on(
        holding.conn(),
        "SELECT fvoci.app_set_member_role($1,$2,'readonly',@ids)",
        &[w.ws, reader.id.0],
    )
    .await
    .unwrap();
    for (actor, change, args) in [
        (&reader, REMOVE_ORGANIZATION, vec![w.org, w.owner.id.0]),
        (&reader, REMOVE_MEMBER, vec![w.ws, w.owner.id.0]),
        (&reader, DEMOTE, vec![w.ws, w.owner.id.0]),
        (&reader, DELETE_GROUP, vec![group]),
        (&reader, REMOVE_GROUP_MEMBER, vec![group, reader.id.0]),
        (&reader, ADD_GROUP_MEMBER, vec![group, reader.id.0]),
        (&outsider, LEAVE, vec![w.ws]),
    ] {
        let mut denied = w.state.db.begin_as(actor.id).await.unwrap();
        let mut query = sqlx::query_scalar::<_, i32>(change.count);
        for arg in args {
            query = query.bind(arg);
        }
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            query.fetch_one(denied.conn()),
        )
        .await
        .expect("a denied ID counter must not wait for an authorized caller's row lock");
        assert_eq!(result.unwrap(), 0, "D-88: {}", change.count);
    }
    holding.commit().await.unwrap();
}
