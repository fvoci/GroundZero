#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
use crate::sql_support;

use sql_support::*;
use uuid::Uuid;

#[tokio::test]
async fn comment_first_holds_shared_item_lock_until_archive_can_commit() {
    let w = World::new().await;
    let task = w.task().await;
    let mut first = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(first.conn(), COMMENT, &[Uuid::now_v7(), task])
        .await
        .unwrap();
    let holder = pid(first.conn()).await;
    let mut archive = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(archive.conn()).await;
    let item = w.item;
    let pending = tokio::spawn(async move {
        on(
            archive.conn(),
            "SELECT fvoci.app_archive_item($1,true,@ids)",
            &[item],
        )
        .await
        .unwrap();
        archive.commit().await.unwrap();
    });
    blocked(holder, waiter).await;
    assert!(!pending.is_finished());
    first.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !one(
            &w.state,
            &w.owner,
            "SELECT archived_at FROM fvoci.items WHERE id=$1",
            &[item]
        )
        .await["archived_at"]
            .is_null()
    );
}

#[tokio::test]
async fn archive_first_rechecks_add_edit_resolve_but_allows_own_delete() {
    let w = World::new().await;
    let task = w.task().await;
    let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let mut archive = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        archive.conn(),
        "SELECT fvoci.app_archive_item($1,true,@ids)",
        &[w.item],
    )
    .await
    .unwrap();
    let holder = pid(archive.conn()).await;
    let mut add = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(add.conn()).await;
    let pending =
        tokio::spawn(async move { on(add.conn(), COMMENT, &[Uuid::now_v7(), task]).await });
    blocked(holder, waiter).await;
    archive.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    code(&result.unwrap_err(), "55000");
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_edit_comment($1,'edited')",
        &[comment],
        "55000",
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_resolve_comment($1,true)",
        &[comment],
        "55000",
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_comment($1,@ids)",
        &[comment],
    )
    .await;
    // D-106: 본인 댓글 회수는 보관 중에도 된다. 답글이 없는 댓글은 행이 사라지고, 답글이 있으면 "지운 댓글"로 남는다.
    let left = rows(
        &w.state,
        &w.owner,
        "SELECT deleted_at FROM fvoci.comments WHERE id=$1",
        &[comment],
    )
    .await;
    assert!(
        left.is_empty() || !left[0]["deleted_at"].is_null(),
        "own comment was not withdrawn: {left:?}"
    );
}

#[tokio::test]
async fn due_notifications_wait_for_archive_and_do_not_notify_after_commit() {
    let w = World::new().await;
    let task = w.task().await;
    // 다른 시험의 기한과 겹치지 않으며 worker는 전달한 날짜를 그대로 사용한다.
    fixture(
        "UPDATE fvoci.tasks SET due_date='2091-02-03' WHERE id=$1",
        &[task],
    )
    .await;
    let mut archive = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        archive.conn(),
        "SELECT fvoci.app_archive_item($1,true,@ids)",
        &[w.item],
    )
    .await
    .unwrap();
    let holder = pid(archive.conn()).await;
    let mut worker = w.state.db.begin_anonymous().await.unwrap();
    let waiter = pid(worker.conn()).await;
    let pending = tokio::spawn(async move {
        let result = on(
            worker.conn(),
            "SELECT fvoci.app_system_due_notifications('2091-02-03',@ids)",
            &[],
        )
        .await;
        worker.commit().await.unwrap();
        result
    });
    blocked(holder, waiter).await;
    archive.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.notifications WHERE task_id=$1 AND kind='due'",
            &[task]
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn readonly_role_and_read_share_cannot_comment() {
    let w = World::new().await;
    let readonly = w.member("readonly").await;
    let reader = w.member("member").await;
    let task = w.task().await;
    denied(
        &w.state,
        &readonly,
        COMMENT,
        &[Uuid::now_v7(), task],
        "42501",
    )
    .await;
    let note = w.note().await;
    w.share(note, &reader, "read").await;
    denied(
        &w.state,
        &reader,
        "SELECT fvoci.app_add_comment($1,$2,NULL,NULL,'forbidden',@ids)",
        &[Uuid::now_v7(), note],
        "42501",
    )
    .await;
}

// #86 (1): 모듈 보관도 항목 보관처럼 댓글 쓰기와 겹칠 때 순서가 정해진다. 댓글 쓰기는 app_lock_item으로 모듈 행을 공유
// 잠금하고, app_archive_module은 그 행을 FOR UPDATE로 잠근다.
#[tokio::test]
async fn comment_first_holds_shared_module_lock_until_module_archive_can_commit() {
    let w = World::new().await;
    let task = w.task().await;
    let mut first = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(first.conn(), COMMENT, &[Uuid::now_v7(), task])
        .await
        .unwrap();
    let holder = pid(first.conn()).await;
    let mut archive = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(archive.conn()).await;
    let module = w.module;
    let pending = tokio::spawn(async move {
        on(
            archive.conn(),
            "SELECT fvoci.app_archive_module($1,true,@ids)",
            &[module],
        )
        .await
        .unwrap();
        archive.commit().await.unwrap();
    });
    blocked(holder, waiter).await;
    assert!(!pending.is_finished());
    first.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    assert!(
        !one(
            &w.state,
            &w.owner,
            "SELECT archived_at FROM fvoci.modules WHERE id=$1",
            &[module]
        )
        .await["archived_at"]
            .is_null()
    );
    // 댓글은 보관보다 먼저 커밋되어 남아 있다.
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.comments WHERE task_id=$1",
            &[task]
        )
        .await["n"],
        1
    );
}

#[tokio::test]
async fn module_archive_first_rechecks_add_edit_resolve_but_allows_own_delete() {
    let w = World::new().await;
    let task = w.task().await;
    let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let mut archive = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        archive.conn(),
        "SELECT fvoci.app_archive_module($1,true,@ids)",
        &[w.module],
    )
    .await
    .unwrap();
    let holder = pid(archive.conn()).await;
    let mut add = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(add.conn()).await;
    let pending =
        tokio::spawn(async move { on(add.conn(), COMMENT, &[Uuid::now_v7(), task]).await });
    blocked(holder, waiter).await;
    assert!(!pending.is_finished());
    archive.commit().await.unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), pending)
        .await
        .unwrap()
        .unwrap();
    code(&result.unwrap_err(), "55000");
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_edit_comment($1,'edited')",
        &[comment],
        "55000",
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_resolve_comment($1,true)",
        &[comment],
        "55000",
    )
    .await;
    // D-106: 본인 댓글 회수는 보관 중에도 된다.
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_comment($1,@ids)",
        &[comment],
    )
    .await;
}
