//! 명령 실패 뒤 범위 guard 해제.

use super::*;
use sqlx::Connection;

#[tokio::test]
async fn failed_workspace_command_releases_the_room_barrier() {
    let f = Fixture::new().await;
    let mut writer = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(writer.edit_and_wait(PENDING).await);
    let failed = send(
        &f.app,
        Method::DELETE,
        &format!("/api/workspaces/{}", f.workspace),
        Some(&f.owner),
        Some(json!({"name": "wrong confirmation"})),
    )
    .await;
    assert_eq!(failed.status, StatusCode::BAD_REQUEST, "{:?}", failed.json);
    let mut rejoined = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(rejoined.edit_and_wait("실패 뒤 이어 쓴 편집").await);
    writer.close().await;
    rejoined.close().await;
    f.stop().await;
}

async fn delete_waiting_after_checkpoint(cancel: bool) {
    let f = Fixture::new().await;
    let mut old = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(old.edit_and_wait(PENDING).await);
    let invitation = send(
        &f.app,
        Method::POST,
        &format!("/api/workspaces/{}/invitations", f.workspace),
        Some(&f.owner),
        Some(json!({"email": common::unique_email("waiting"), "role": "member"})),
    )
    .await;
    assert_eq!(
        invitation.status,
        StatusCode::CREATED,
        "{:?}",
        invitation.json
    );
    let mut conn = common::fixture_conn().await;
    let mut lock = conn.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM fvoci.invitations WHERE id=$1::uuid FOR UPDATE")
        .bind(invitation.json["id"].as_str().unwrap())
        .execute(&mut *lock)
        .await
        .unwrap();
    let deleting = tokio::spawn({
        let (app, owner, ws) = (f.app.clone(), f.owner.clone(), f.workspace.clone());
        async move {
            send(
                &app,
                Method::DELETE,
                &format!("/api/workspaces/{ws}"),
                Some(&owner),
                Some(json!({"name": "방 수명 재현"})),
            )
            .await
        }
    });
    let mut observer = common::fixture_conn().await;
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(holder)
            .fetch_one(&mut observer)
            .await
            .unwrap();
            if waiting {
                break;
            }
            assert!(
                !deleting.is_finished(),
                "delete ended before its commit barrier"
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("delete did not reach the post-checkpoint lock");
    assert_eq!(
        f.current().await["revision"],
        2,
        "the wait was before the checkpoint"
    );
    assert!(!old.edit_and_wait("커밋 사이에 들어온 편집").await);
    assert_eq!(Client::refused(f.addr, &f.document, &f.writer).await, 4503);
    if cancel {
        deleting.abort();
        assert!(matches!(deleting.await, Err(error) if error.is_cancelled()));
        lock.rollback().await.unwrap();
        // 취소된 SQL의 트랜잭션 롤백까지 기다려 연결의 새 쓰기와 경합하지 않게 한다.
        sqlx::query("SELECT id FROM fvoci.workspaces WHERE id=$1::uuid FOR UPDATE")
            .bind(&f.workspace)
            .execute(&mut observer)
            .await
            .unwrap();
    } else {
        lock.rollback().await.unwrap();
        let deleted = deleting.await.unwrap();
        assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);
        let restored = send(
            &f.app,
            Method::POST,
            &format!("/api/me/deleted-workspaces/{}/restore", f.workspace),
            Some(&f.owner),
            None,
        )
        .await;
        assert_eq!(
            restored.status,
            StatusCode::NO_CONTENT,
            "{:?}",
            restored.json
        );
    }
    let mut fresh = Client::connect(f.addr, &f.document, &f.writer).await;
    assert_eq!(fresh.body(), common::doc_body(PENDING));
    assert!(fresh.edit_and_wait("guard 해제 뒤 이어 쓴 편집").await);
    fresh.close().await;
    f.stop().await;
}

#[tokio::test]
async fn edits_between_checkpoint_and_commit_are_refused_and_guard_is_released() {
    delete_waiting_after_checkpoint(false).await;
}

#[tokio::test]
async fn cancelled_workspace_command_rolls_back_and_releases_the_guard() {
    delete_waiting_after_checkpoint(true).await;
}
