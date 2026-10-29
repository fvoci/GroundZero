//! SR-243-2: 준비 전체 시한으로 응답 없는 방·DB 대기에서 범위 guard를 해제한다.

use super::*;
use sqlx::{Connection, PgConnection};
use std::time::Duration;

pub(super) async fn blocked_backend(observer: &mut PgConnection, holder: i32) -> i32 {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let blocked = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) LIMIT 1",
            )
            .bind(holder)
            .fetch_optional(&mut *observer)
            .await
            .unwrap();
            if let Some(pid) = blocked {
                return pid;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the preparation fixture never reached its row lock")
}

fn assert_retry(reply: &common::Reply) {
    assert_eq!(reply.status, StatusCode::CONFLICT, "{:?}", reply.json);
    assert!(
        reply.json["message"]
            .as_str()
            .unwrap()
            .contains("try again"),
        "timeout must explain how to continue: {:?}",
        reply.json
    );
}

#[tokio::test]
async fn unresponsive_actor_times_out_without_removal_and_releases_organization_hold() {
    let f = Fixture::with_recheck("1").await;
    let mut writer = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(writer.edit_and_wait(PENDING).await);
    let other = common::create_doc(&f.app, &f.owner, &f.workspace, "guard probe").await;
    let before = contracts::stored(&f).await;
    let mut conn = common::fixture_conn().await;
    let mut lock = conn.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT document_id FROM fvoci.document_collab_states WHERE document_id=$1::uuid FOR UPDATE")
        .bind(&f.document).execute(&mut *lock).await.unwrap();
    // 다음 상태 저장에서 actor를 멈춰 PrepareOrganization에 응답하지 못하게 한다.
    let editing = tokio::spawn(async move {
        let saved = writer.edit_and_wait("actor 대기 동안 보존할 편집").await;
        (writer, saved)
    });
    let mut observer = common::fixture_conn().await;
    blocked_backend(&mut observer, holder).await;
    let org = common::organization_of_workspace(&f.workspace).await;
    let blocked = tokio::time::timeout(
        Duration::from_secs(3),
        send(
            &f.app,
            Method::DELETE,
            &format!("/api/organizations/{org}/members/{}", f.writer.id.0),
            Some(&f.owner),
            None,
        ),
    )
    .await
    .expect("unresponsive actor did not return a timed 409");
    assert_retry(&blocked);
    assert_eq!(
        contracts::stored(&f).await,
        before,
        "409 changed acknowledged state"
    );
    assert_eq!(f.current().await["revision"], 1);
    // 같은 조직·공간의 다른 문서는 원래 actor가 멈춘 동안에도 참가할 수 있다.
    let mut probe = Client::connect(f.addr, other["id"].as_str().unwrap(), &f.writer).await;
    probe.close().await;
    lock.rollback().await.unwrap();
    let (mut writer, saved) = editing.await.unwrap();
    assert!(saved, "an edit waiting behind the stalled store was lost");
    assert!(writer.edit_and_wait("409 뒤 원래 방에서 계속한 편집").await);
    assert_eq!(
        f.current().await["revision"],
        1,
        "timed-out command later checkpointed"
    );
    writer.close().await;
    f.stop().await;
}

#[tokio::test]
async fn locked_closed_pending_times_out_rolls_back_and_releases_workspace_hold() {
    let f = Fixture::with_recheck("1").await;
    contracts::closed_pending(&f).await;
    let before = contracts::stored(&f).await;
    let mut conn = common::fixture_conn().await;
    let mut lock = conn.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM fvoci.documents WHERE id=$1::uuid FOR UPDATE")
        .bind(&f.document)
        .execute(&mut *lock)
        .await
        .unwrap();
    let deleting = tokio::spawn({
        let (app, owner, workspace) = (f.app.clone(), f.owner.clone(), f.workspace.clone());
        async move {
            send(
                &app,
                Method::DELETE,
                &format!("/api/workspaces/{workspace}"),
                Some(&owner),
                Some(json!({"name": "방 수명 재현"})),
            )
            .await
        }
    });
    let mut observer = common::fixture_conn().await;
    let waiting = blocked_backend(&mut observer, holder).await;
    let blocked = tokio::time::timeout(Duration::from_secs(3), deleting)
        .await
        .expect("locked pending preparation did not return a timed 409")
        .unwrap();
    assert_retry(&blocked);
    assert_eq!(contracts::stored(&f).await, before);
    assert_eq!(f.current().await["revision"], 1);
    let mut rejoined = Client::connect(f.addr, &f.document, &f.writer).await;
    assert_eq!(rejoined.body(), common::doc_body(PENDING));
    lock.rollback().await.unwrap();
    // 다른 역할의 pg_stat_activity 세부 정보에 기대지 않고, 취소된 쿼리 뒤에 같은 행을 잠근다.
    tokio::time::timeout(
        Duration::from_secs(5),
        sqlx::query("SELECT id FROM fvoci.documents WHERE id=$1::uuid FOR UPDATE")
            .bind(&f.document)
            .execute(&mut observer),
    )
    .await
    .expect("the timed-out preparation transaction did not release its row lock")
    .unwrap();
    assert_ne!(
        waiting, holder,
        "the fixture did not observe the blocked preparation"
    );
    assert_eq!(
        f.current().await["revision"],
        1,
        "cancelled checkpoint committed later"
    );
    assert_eq!(contracts::stored(&f).await, before);
    assert!(rejoined.edit_and_wait("시한 뒤 초안을 이어 쓴 편집").await);
    rejoined.close().await;
    f.stop().await;
}
