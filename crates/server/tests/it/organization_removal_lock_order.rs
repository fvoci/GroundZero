#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 조직에서 빠지는 일과 그 사람에게 주는 공유가 멤버 행에서 줄 선다(D-114). 어느 쪽이 먼저든 빠진 사람의 공유는 남지 않는다.

use crate::common::TestUser;
use crate::sql_support;

use sql_support::*;
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
