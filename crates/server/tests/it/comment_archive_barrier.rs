#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration fixtures must fail loudly"
)]
//! #86: 댓글 수정·해결·재개와 보관을 같은 모듈 → 항목 → 댓글 순서로 직렬화한다.

use crate::sql_support::*;
use sqlx::Connection;
use uuid::Uuid;

const WRITES: [&str; 3] = [
    "SELECT fvoci.app_edit_comment($1,'edited')",
    "SELECT fvoci.app_resolve_comment($1,true)",
    "SELECT fvoci.app_resolve_comment($1,false)",
];

async fn overlap(module: bool, archive_first: bool) {
    for write in WRITES {
        let w = World::new().await;
        let task = w.task().await;
        let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
        // 재개가 실제 변경이 되게 한다.
        rows(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_resolve_comment($1,true)",
            &[comment],
        )
        .await;
        let (archive, target) = if module {
            ("SELECT fvoci.app_archive_module($1,true,@ids)", w.module)
        } else {
            ("SELECT fvoci.app_archive_item($1,true,@ids)", w.item)
        };
        let mut first = w.state.db.begin_as(w.owner.id).await.unwrap();
        let mut second = w.state.db.begin_as(w.owner.id).await.unwrap();
        let holder = pid(first.conn()).await;
        let waiter = pid(second.conn()).await;
        let (first_sql, first_id, next_sql, next_id) = if archive_first {
            (archive, target, write, comment)
        } else {
            (write, comment, archive, target)
        };
        on(first.conn(), first_sql, &[first_id]).await.unwrap();
        let pending = tokio::spawn(async move {
            let result = on(second.conn(), next_sql, &[next_id]).await;
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
        if archive_first {
            code(&result.unwrap_err(), "55000");
        } else {
            result.unwrap();
        }
    }
}

#[tokio::test]
async fn comment_updates_hold_item_until_archive_commits() {
    overlap(false, false).await;
}
#[tokio::test]
async fn comment_updates_hold_module_until_archive_commits() {
    overlap(true, false).await;
}
#[tokio::test]
async fn item_archive_rejects_waiting_comment_updates() {
    overlap(false, true).await;
}
#[tokio::test]
async fn module_archive_rejects_waiting_comment_updates() {
    overlap(true, true).await;
}

#[tokio::test]
async fn reply_and_edit_use_the_same_module_before_comment_lock_order() {
    let w = World::new().await;
    let task = w.task().await;
    let comment = made(&w.state, &w.owner, COMMENT, &[Uuid::now_v7(), task]).await;
    let mut fixture = crate::common::fixture_conn().await;
    let mut archive = fixture.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id',$1,true)")
        .bind(w.owner.id.0.to_string())
        .execute(&mut *archive)
        .await
        .unwrap();
    on(
        &mut archive,
        "SELECT id FROM fvoci.modules WHERE id=$1 FOR UPDATE",
        &[w.module],
    )
    .await
    .unwrap();
    let holder = pid(&mut archive).await;
    let mut edit = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(edit.conn()).await;
    let editing = tokio::spawn(async move { on(edit.conn(), WRITES[0], &[comment]).await });
    blocked(holder, waiter).await;
    let reply = on(
        &mut archive,
        "SELECT fvoci.app_add_comment($1,NULL,$2,$3,'reply',@ids)",
        &[Uuid::now_v7(), task, comment],
    )
    .await;
    if reply.is_ok() {
        on(
            &mut archive,
            "SELECT fvoci.app_archive_module($1,true,@ids)",
            &[w.module],
        )
        .await
        .unwrap();
        archive.commit().await.unwrap();
    } else {
        drop(archive);
    }
    let edited = tokio::time::timeout(std::time::Duration::from_secs(10), editing)
        .await
        .unwrap()
        .unwrap();
    assert!(reply.is_ok(), "reply/archive failed: {reply:?}");
    code(&edited.unwrap_err(), "55000");
}

async fn due_overlap(archive_first: bool) {
    let w = World::new().await;
    let task = w.task().await;
    let date = if archive_first {
        "2091-04-05"
    } else {
        "2091-05-06"
    };
    fixture(
        &format!("UPDATE fvoci.tasks SET due_date='{date}' WHERE id=$1"),
        &[task],
    )
    .await;
    let sql: &'static str = if archive_first {
        "SELECT fvoci.app_system_due_notifications('2091-04-05',@ids)"
    } else {
        "SELECT fvoci.app_system_due_notifications('2091-05-06',@ids)"
    };
    let mut archive = w.state.db.begin_as(w.owner.id).await.unwrap();
    let mut notify = w.state.db.begin_anonymous().await.unwrap();
    let archive_pid = pid(archive.conn()).await;
    let notify_pid = pid(notify.conn()).await;
    if archive_first {
        on(
            archive.conn(),
            "SELECT fvoci.app_archive_module($1,true,@ids)",
            &[w.module],
        )
        .await
        .unwrap();
        // ID 수 계산은 예약 상한이며, 보관 장벽은 실제 생성 시 app_notify가 다시 확인한다.
        on(
            notify.conn(),
            "SELECT fvoci.app_system_due_notifications_ids('2091-04-05')",
            &[],
        )
        .await
        .unwrap();
        let pending = tokio::spawn(async move {
            on(notify.conn(), sql, &[]).await.unwrap();
            notify.commit().await.unwrap();
        });
        blocked(archive_pid, notify_pid).await;
        archive.commit().await.unwrap();
        pending.await.unwrap();
    } else {
        on(notify.conn(), sql, &[]).await.unwrap();
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
        blocked(notify_pid, archive_pid).await;
        notify.commit().await.unwrap();
        pending.await.unwrap();
    }
    let notifications = rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.notifications WHERE task_id=$1 AND kind='due'",
        &[task],
    )
    .await;
    assert_eq!(notifications.len(), usize::from(!archive_first));
}

#[tokio::test]
async fn module_archive_prevents_waiting_due_notifications() {
    due_overlap(true).await;
}

#[tokio::test]
async fn due_notification_holds_module_until_archive_can_commit() {
    due_overlap(false).await;
}
