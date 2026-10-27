#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 사용자 문맥(`app.user_id`)은 트랜잭션 안에서만 산다. 커밋·롤백 뒤 같은 연결을 다시 써도(풀, Supavisor session
//! 모드) 다음 트랜잭션에 남지 않고, 문맥 없는 트랜잭션은 RLS가 막은 행을 보지 못한다.

use crate::common;

use common::{create_doc, create_workspace, login};

async fn context(tx: &mut fvoci_db::AnonTx) -> (Option<String>, Option<uuid::Uuid>, i64) {
    sqlx::query_as(
        "SELECT nullif(current_setting('app.user_id', true), ''), app_user_id(), (SELECT count(*) FROM documents)",
    )
    .fetch_one(tx.conn())
    .await
    .unwrap()
}

#[tokio::test]
async fn user_context_does_not_outlive_commit_or_rollback() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "context").await;
    let ws = create_workspace(&app, &owner, "문맥").await;
    create_doc(&app, &owner, &ws, "보이면 안 되는 문서").await;

    for finish in ["commit", "rollback"] {
        let mut tx = state.db.begin_as(owner.id).await.unwrap();
        let (seen, visible): (Option<uuid::Uuid>, i64) =
            sqlx::query_as("SELECT app_user_id(), (SELECT count(*) FROM documents)")
                .fetch_one(tx.conn())
                .await
                .unwrap();
        assert_eq!(seen, Some(owner.id.0));
        assert!(visible >= 1, "the owner sees the document");
        if finish == "commit" {
            tx.commit().await.unwrap();
        } else {
            drop(tx);
        }
        // 풀의 연결 수보다 많이 열어 같은 연결을 반드시 다시 쓰게 한다.
        for _ in 0..40 {
            let mut anon = state.db.begin_anonymous().await.unwrap();
            let (setting, user, rows) = context(&mut anon).await;
            assert_eq!(setting, None, "after {finish}: app.user_id leaked");
            assert_eq!(user, None, "after {finish}: app_user_id() leaked");
            assert_eq!(
                rows, 0,
                "after {finish}: rows visible without a user context"
            );
            anon.commit().await.unwrap();
        }
    }
}
