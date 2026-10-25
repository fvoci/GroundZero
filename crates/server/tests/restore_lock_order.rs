#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

async fn setup() -> (World, common::TestUser, Uuid, Uuid, Uuid, Uuid) {
    let w = World::new().await;
    let member = w.member("member").await;
    // 항목과 무관한 비공개 위키 문서: 쓰기 공유가 유일한 복원 권한이다.
    let doc = made(&w.state,&w.owner,"SELECT fvoci.app_create_wiki_document($1,$2,'private',1,'{\"type\":\"doc\",\"content\":[]}',NULL,20,@ids) AS id",&[w.ws,Uuid::now_v7()]).await;
    fixture(
        "UPDATE fvoci.documents SET visibility='private' WHERE id=$1",
        &[doc],
    )
    .await;
    let file = attachment(&w, doc).await;
    let share = w.share(doc, &member, "write").await;
    let batch = made(
        &w.state,
        &member,
        "SELECT fvoci.app_trash_file($1,@ids) AS id",
        &[file],
    )
    .await;
    (w, member, doc, file, share, batch)
}

#[tokio::test]
async fn share_revocation_first_blocks_restore_and_rechecks_deleter_access() {
    let (w, member, doc, file, share, batch) = setup().await;
    let mut revoke = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        revoke.conn(),
        "SELECT fvoci.app_unshare_document($1,@ids)",
        &[share],
    )
    .await
    .unwrap();
    let holder = pid(revoke.conn()).await;
    let mut restore = w.state.db.begin_as(member.id).await.unwrap();
    let waiter = pid(restore.conn()).await;
    let pending = tokio::spawn(async move {
        let result = on(
            restore.conn(),
            "SELECT fvoci.app_restore_batch($1,@ids)",
            &[batch],
        )
        .await;
        if result.is_ok() {
            restore.commit().await.unwrap();
        }
        result
    });
    blocked(holder, waiter).await;
    revoke.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    code(&result.unwrap_err(), "P0002");
    w.share(doc, &member, "write").await;
    rows(
        &w.state,
        &member,
        "SELECT fvoci.app_restore_batch($1,@ids)",
        &[batch],
    )
    .await;
    assert_eq!(
        rows(
            &w.state,
            &member,
            "SELECT id FROM fvoci.files WHERE id=$1",
            &[file]
        )
        .await
        .len(),
        1
    );
}

#[tokio::test]
async fn restore_first_serializes_with_share_revocation_without_deadlock() {
    let (w, member, _doc, file, share, batch) = setup().await;
    let mut restore = w.state.db.begin_as(member.id).await.unwrap();
    on(
        restore.conn(),
        "SELECT fvoci.app_restore_batch($1,@ids)",
        &[batch],
    )
    .await
    .unwrap();
    let holder = pid(restore.conn()).await;
    let mut revoke = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(revoke.conn()).await;
    let pending = tokio::spawn(async move {
        on(
            revoke.conn(),
            "SELECT fvoci.app_unshare_document($1,@ids)",
            &[share],
        )
        .await
        .unwrap();
        revoke.commit().await.unwrap();
    });
    blocked(holder, waiter).await;
    restore.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.files WHERE id=$1",
            &[file]
        )
        .await
        .len(),
        1
    );
    assert!(
        rows(
            &w.state,
            &member,
            "SELECT id FROM fvoci.files WHERE id=$1",
            &[file]
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn public_link_revocation_and_restore_serialize_in_both_orders() {
    for restore_first in [false, true] {
        let (w, member, doc, file, _share, batch) = setup().await;
        let link = made(&w.state,&w.owner,"SELECT fvoci.app_create_public_link($1,decode(replace($2::text,'-',''),'hex'),NULL,@ids) AS id",&[doc,Uuid::now_v7()]).await;
        let restore = w.state.db.begin_as(member.id).await.unwrap();
        let revoke = w.state.db.begin_as(w.owner.id).await.unwrap();
        let (mut first, mut second, first_sql, second_sql, first_id, second_id) = if restore_first {
            (
                restore,
                revoke,
                "SELECT fvoci.app_restore_batch($1,@ids)",
                "SELECT fvoci.app_revoke_public_link($1,@ids)",
                batch,
                link,
            )
        } else {
            // 두 UserTx는 같은 형식이므로 순서만 바꾼다.
            (
                revoke,
                restore,
                "SELECT fvoci.app_revoke_public_link($1,@ids)",
                "SELECT fvoci.app_restore_batch($1,@ids)",
                link,
                batch,
            )
        };
        on(first.conn(), first_sql, &[first_id]).await.unwrap();
        let holder = pid(first.conn()).await;
        let waiter = pid(second.conn()).await;
        let pending = tokio::spawn(async move {
            on(second.conn(), second_sql, &[second_id]).await.unwrap();
            second.commit().await.unwrap();
        });
        blocked(holder, waiter).await;
        first.commit().await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), pending)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            rows(
                &w.state,
                &member,
                "SELECT id FROM fvoci.files WHERE id=$1",
                &[file]
            )
            .await
            .len(),
            1
        );
    }
}

#[tokio::test]
async fn document_trash_and_attachment_restore_obey_document_first_in_both_orders() {
    for restore_first in [false, true] {
        let (w, member, doc, file, _share, batch) = setup().await;
        let restore = w.state.db.begin_as(member.id).await.unwrap();
        let trash = w.state.db.begin_as(w.owner.id).await.unwrap();
        let (mut first, mut second, first_sql, second_sql, first_id, second_id) = if restore_first {
            (
                restore,
                trash,
                "SELECT fvoci.app_restore_batch($1,@ids)",
                "SELECT fvoci.app_trash_document($1,@ids)",
                batch,
                doc,
            )
        } else {
            (
                trash,
                restore,
                "SELECT fvoci.app_trash_document($1,@ids)",
                "SELECT fvoci.app_restore_batch($1,@ids)",
                doc,
                batch,
            )
        };
        on(first.conn(), first_sql, &[first_id]).await.unwrap();
        let holder = pid(first.conn()).await;
        let waiter = pid(second.conn()).await;
        let pending = tokio::spawn(async move {
            let result = on(second.conn(), second_sql, &[second_id]).await;
            if result.is_ok() {
                second.commit().await.unwrap();
            }
            result
        });
        blocked(holder, waiter).await;
        first.commit().await.unwrap();
        let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
            .await
            .unwrap()
            .unwrap();
        if restore_first {
            result.unwrap();
        } else {
            let error = result.unwrap_err();
            code(&error, "55000");
            assert_eq!(
                error.as_database_error().unwrap().message(),
                "restore the document first"
            );
        }
        assert!(
            rows(
                &w.state,
                &member,
                "SELECT id FROM fvoci.files WHERE id=$1",
                &[file]
            )
            .await
            .is_empty()
        );
    }
}
