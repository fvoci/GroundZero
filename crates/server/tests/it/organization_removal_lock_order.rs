#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 조직에서 빠지는 일과 그 사람에게 주는 공유가 멤버 행에서 줄 선다(D-114). 어느 쪽이 먼저든 빠진 사람의 공유는 남지 않는다.

use crate::common::{self, TestUser};
use crate::sql_support;

use sql_support::*;
use sqlx::Connection;
use uuid::Uuid;

const SHARE: &str = "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'write',$3,@ids) AS id";
const REMOVE: &str = "SELECT fvoci.app_remove_organization_member($1,$2,@ids)";

async fn shares_of(w: &World, doc: Uuid, member: &TestUser) -> usize {
    rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.document_shares WHERE document_id=$1 AND grantee_user_id=$2",
        &[doc, member.id.0],
    )
    .await
    .len()
}

/// SR1-3: 공유가 먼저 멤버 행을 쥐면 조직 제거는 그 공유가 커밋될 때까지 기다렸다가 함께 거둔다.
#[tokio::test]
async fn share_first_is_revoked_by_the_waiting_organization_removal() {
    let w = World::new().await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    let mut share = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(share.conn(), SHARE, &[doc, member.id.0, w.org])
        .await
        .unwrap();
    let holder = pid(share.conn()).await;
    let mut remove = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(remove.conn()).await;
    let (org, user) = (w.org, member.id.0);
    let pending = tokio::spawn(async move {
        on(remove.conn(), REMOVE, &[org, user]).await.unwrap();
        remove.commit().await.unwrap();
    });
    blocked(holder, waiter).await;
    share.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(shares_of(&w, doc, &member).await, 0);
}

/// 조직 제거가 먼저면 공유는 멤버 행을 기다렸다가 받는 사람이 조직에 없다며 거절된다.
#[tokio::test]
async fn organization_removal_first_rejects_the_waiting_share() {
    let w = World::new().await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    let mut remove = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(remove.conn(), REMOVE, &[w.org, member.id.0])
        .await
        .unwrap();
    let holder = pid(remove.conn()).await;
    let mut share = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(share.conn()).await;
    let (org, user) = (w.org, member.id.0);
    let pending = tokio::spawn(async move {
        let result = on(share.conn(), SHARE, &[doc, user, org]).await;
        if result.is_ok() {
            share.commit().await.unwrap();
        }
        result
    });
    blocked(holder, waiter).await;
    remove.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    code(&result.unwrap_err(), "22023");
    assert_eq!(shares_of(&w, doc, &member).await, 0);
}

type Call = tokio::task::JoinHandle<Result<Vec<serde_json::Value>, sqlx::Error>>;

/// 호출을 뒤에서 돌린다. 성공하면 커밋하고, 실패하면 트랜잭션을 버린다.
fn spawn_call(mut tx: fvoci_db::UserTx, sql: &'static str, args: Vec<Uuid>) -> Call {
    tokio::spawn(async move {
        let result = on(tx.conn(), sql, &args).await;
        if result.is_ok() {
            tx.commit().await.unwrap();
        }
        result
    })
}

async fn joined(call: Call) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    tokio::time::timeout(std::time::Duration::from_secs(10), call)
        .await
        .expect("the call did not finish")
        .unwrap()
}

/// #177 SR1-3 후속(CR-11, D-114): 조직 제거가 멤버 행을 쥔 채 앞 문서를 기다리는 동안, 이미 공유한 뒤 문서를 그 사람에게
/// 다시 공유한다. 공유가 문서를 먼저 잠그면 제거(멤버 행 → 문서)와 엇갈려 교착(40P01)이 된다. 공유도 멤버 행을 먼저
/// 잠그므로 제거 뒤에 줄 서고, 제거가 끝나면 받는 사람이 조직에 없어 거절된다.
#[tokio::test]
async fn resharing_during_organization_removal_waits_on_the_member_row_without_deadlock() {
    let w = World::new().await;
    let member = w.member("member").await;
    // 제거는 그 사람이 받은 공유의 문서를 id 순서로 잠근다. 앞 문서를 쥐어 제거를 두 잠금 사이에 세운다.
    let first = w.doc().await;
    let doc = w.doc().await;
    w.share(first, &member, "read").await;
    w.share(doc, &member, "read").await;
    let mut holder = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        holder.conn(),
        "SELECT id FROM fvoci.documents WHERE id=$1 FOR UPDATE",
        &[first],
    )
    .await
    .unwrap();
    let holder_pid = pid(holder.conn()).await;
    let mut remove = w.state.db.begin_as(w.owner.id).await.unwrap();
    let remove_pid = pid(remove.conn()).await;
    let removing = spawn_call(remove, REMOVE, vec![w.org, member.id.0]);
    blocked(holder_pid, remove_pid).await;
    let mut share = w.state.db.begin_as(w.owner.id).await.unwrap();
    let share_pid = pid(share.conn()).await;
    let sharing = spawn_call(share, SHARE, vec![doc, member.id.0, w.org]);
    blocked(remove_pid, share_pid).await;
    holder.commit().await.unwrap();
    let removed = joined(removing).await;
    let shared = joined(sharing).await;
    assert!(removed.is_ok(), "{removed:?}");
    code(&shared.unwrap_err(), "22023");
    assert_eq!(shares_of(&w, first, &member).await, 0);
    assert_eq!(shares_of(&w, doc, &member).await, 0);
}

/// 공유는 문서를 잠그지 않고 읽어 조직을 정한 뒤 멤버 행을 기다린다. 그사이 문서가 옮겨지면 문서를 잠근 뒤 자리를 다시
/// 보고, 다르면 이동과 같은 55000(409)으로 거절한다(받는 사람은 다시 열어 새 자리에서 공유한다).
#[tokio::test]
async fn a_document_moved_while_the_share_waits_is_refused() {
    let w = World::new().await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    let target = workspace(&w.state, &w.owner, w.org).await;
    let mut holder = common::fixture_conn().await;
    let mut held = holder.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(w.org)
    .bind(member.id.0)
    .execute(&mut *held)
    .await
    .unwrap();
    let holder_pid = pid(&mut *held).await;
    let mut share = w.state.db.begin_as(w.owner.id).await.unwrap();
    let share_pid = pid(share.conn()).await;
    let sharing = spawn_call(share, SHARE, vec![doc, member.id.0, w.org]);
    blocked(holder_pid, share_pid).await;
    // 공유가 문서를 쥐고 있지 않아 이동이 끝난다.
    let mut mover = w.state.db.begin_as(w.owner.id).await.unwrap();
    let moved = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        on(
            mover.conn(),
            "SELECT * FROM fvoci.app_move_document($1,$2,NULL,@ids)",
            &[doc, target],
        ),
    )
    .await
    .expect("the move waited for the share's document lock");
    assert!(moved.is_ok(), "{moved:?}");
    mover.commit().await.unwrap();
    held.commit().await.unwrap();
    code(&joined(sharing).await.unwrap_err(), "55000");
    assert_eq!(shares_of(&w, doc, &member).await, 0);
}

/// 호출이 끝났거나 다른 세션을 기다리는 상태가 될 때까지 조건으로 기다린다.
async fn finished_or_blocked(call: &Call, waiter: i32) {
    let url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via cargo xtask test");
    let mut observer = sqlx::PgConnection::connect(&url).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT cardinality(pg_blocking_pids($1)) > 0")
                .bind(waiter)
                .fetch_one(&mut observer)
                .await
                .unwrap();
            if waiting || call.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the call neither finished nor waited for a lock");
}

/// #177 SR2 참고-1(D-49·D-114): 개인 노트 소유자를 조직에서 빼는 일과 그 소유자가 노트를 공유하는 일이 겹쳐도 그 조직이
/// 적힌 공유가 남지 않는다. 공유는 받는 사람과 소유자의 멤버 행을 id 순서로 잠그므로 제거와 줄 선다.
#[tokio::test]
async fn removing_the_note_owner_while_they_share_leaves_no_share_in_that_organization() {
    let w = World::new().await;
    let owner = common::login_with_email(
        &w.state,
        "note owner",
        &common::unique_email("note-owner"),
        true,
    )
    .await;
    common::join_organization(w.org, &owner, "member").await;
    let member = w.member("member").await;
    let note = made(&w.state, &owner, "SELECT fvoci.app_create_personal_note($1,'SQL note',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id", &[Uuid::now_v7()]).await;
    // 받는 사람 멤버 행을 쥐어 공유를 조직 계산 뒤·공유 기록 전에 세운다.
    let mut holder = common::fixture_conn().await;
    let mut held = holder.begin().await.unwrap();
    sqlx::query(
        "SELECT 1 FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2 FOR UPDATE",
    )
    .bind(w.org)
    .bind(member.id.0)
    .execute(&mut *held)
    .await
    .unwrap();
    let holder_pid = pid(&mut *held).await;
    let mut share = w.state.db.begin_as(owner.id).await.unwrap();
    let share_pid = pid(share.conn()).await;
    let sharing = spawn_call(share, SHARE, vec![note, member.id.0, w.org]);
    blocked(holder_pid, share_pid).await;
    let mut remove = w.state.db.begin_as(w.owner.id).await.unwrap();
    let remove_pid = pid(remove.conn()).await;
    let removing = spawn_call(remove, REMOVE, vec![w.org, owner.id.0]);
    finished_or_blocked(&removing, remove_pid).await;
    held.commit().await.unwrap();
    let removed = joined(removing).await;
    assert!(removed.is_ok(), "{removed:?}");
    if let Err(err) = joined(sharing).await {
        code(&err, "22023");
    }
    let mut observer = common::fixture_conn().await;
    let left: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.document_shares WHERE document_id=$1 AND organization_id=$2",
    )
    .bind(note)
    .bind(w.org)
    .fetch_one(&mut observer)
    .await
    .unwrap();
    assert_eq!(left, 0);
}
