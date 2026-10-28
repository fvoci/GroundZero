#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration fixtures and lock observations must fail loudly"
)]
//! #186: 공간을 쥔 제거와 문서 저장·조직 제거의 다음 잠금을 직접 정한다.

use crate::common;
use crate::sql_support::*;
use axum::http::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

async fn workspace_holder(w: &World) -> (fvoci_db::UserTx, i32) {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        tx.conn(),
        "SELECT id FROM fvoci.workspaces WHERE id=$1 FOR NO KEY UPDATE",
        &[w.ws],
    )
    .await
    .unwrap();
    let holder = pid(tx.conn()).await;
    (tx, holder)
}

async fn has_waiter(holder: i32) {
    let mut observer = common::fixture_conn().await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
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
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the saver did not wait for the workspace holder");
}

#[tokio::test]
async fn saving_waits_for_workspace_removal_before_locking_the_document() {
    let w = World::new().await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    w.share(doc, &member, "read").await;
    let (mut removal, holder) = workspace_holder(&w).await;
    let app = common::app(&w.state);
    let owner = w.owner.clone();
    let saving = tokio::spawn(async move {
        common::send(
            &app,
            Method::PUT,
            &format!("/api/documents/{doc}"),
            Some(&owner),
            Some(json!({
                "request_id": Uuid::now_v7(), "base_revision": 1, "title": "saved during removal",
                "body": {"type":"doc","content":[]}
            })),
        )
        .await
    });
    // 이전 구현은 여기서 문서 UPDATE를 쥔 채 공간 SHARE를 기다린다. 새 구현은 공간부터 기다린다.
    has_waiter(holder).await;
    let removed = on(
        removal.conn(),
        "SELECT fvoci.app_remove_member($1,$2,@ids)",
        &[w.ws, member.id.0],
    )
    .await;
    if removed.is_ok() {
        removal.commit().await.unwrap();
    } else {
        drop(removal);
    }
    let saved = tokio::time::timeout(std::time::Duration::from_secs(10), saving)
        .await
        .unwrap()
        .unwrap();
    assert!(removed.is_ok(), "workspace removal failed: {removed:?}");
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1 AND grantee_user_id=$2",
            &[doc, member.id.0]
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn organization_removal_waits_for_workspace_removal_before_locking_shared_documents() {
    let w = World::new().await;
    let member = w.member("member").await;
    let doc = w.doc().await;
    w.share(doc, &member, "read").await;
    let (mut workspace, holder) = workspace_holder(&w).await;
    let mut organization = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(organization.conn()).await;
    let (org, user) = (w.org, member.id.0);
    let removing = tokio::spawn(async move {
        let result = on(
            organization.conn(),
            "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
            &[org, user],
        )
        .await;
        if result.is_ok() {
            organization.commit().await.unwrap();
        }
        result
    });
    blocked(holder, waiter).await;
    let removed = on(
        workspace.conn(),
        "SELECT fvoci.app_remove_member($1,$2,@ids)",
        &[w.ws, user],
    )
    .await;
    if removed.is_ok() {
        workspace.commit().await.unwrap();
    } else {
        drop(workspace);
    }
    let organization = tokio::time::timeout(std::time::Duration::from_secs(10), removing)
        .await
        .unwrap()
        .unwrap();
    assert!(removed.is_ok(), "workspace removal failed: {removed:?}");
    assert!(
        organization.is_ok(),
        "organization removal failed: {organization:?}"
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1",
            &[doc]
        )
        .await
        .is_empty()
    );
    assert!(rows(&w.state, &w.owner, "SELECT user_id FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2", &[org,user]).await.is_empty());
}
