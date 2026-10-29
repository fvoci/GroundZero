#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration fixtures and lock observations must fail loudly"
)]
//! #233: 개인 노트 쓰기도 조직 제거의 조직·멤버 잠금 뒤에서 권한을 다시 본다.

use crate::common::{self, send};
use crate::sql_support::*;
use axum::http::{Method, StatusCode};
use fvoci_core::{error::AppError, ids::DocumentId};
use fvoci_documents::documents;
use serde_json::json;
use uuid::Uuid;

/// HTTP 처리기는 별도 연결을 쓰므로 쿼리와 실제 blocking PID로 대기를 찾는다.
async fn waiting(holder: i32, query: &str) {
    use sqlx::Connection;
    let url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via cargo xtask test");
    let mut observer = sqlx::PgConnection::connect(&url).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let found: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE query LIKE '%' || $2 || '%' AND $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(holder)
            .bind(query)
            .fetch_one(&mut observer)
            .await
            .unwrap();
            if found {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("personal note writer did not wait for organization removal");
}

async fn counted_removal(w: &World, user: Uuid) -> (fvoci_db::UserTx, i32) {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        tx.conn(),
        "SELECT fvoci.app_remove_organization_member_ids($1,$2)",
        &[w.org, user],
    )
    .await
    .unwrap();
    let holder = pid(tx.conn()).await;
    (tx, holder)
}

async fn remove(w: &World, mut tx: fvoci_db::UserTx, user: Uuid) {
    on(
        tx.conn(),
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &[w.org, user],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn personal_note_checkpoint_waits_for_organization_removal_and_rechecks_access() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let (removal, holder) = counted_removal(&w, editor.id.0).await;
    let mut tx = w.state.db.begin_as(editor.id).await.unwrap();
    let waiter = pid(tx.conn()).await;
    let saving =
        tokio::spawn(async move { documents::lock_for_save(&mut tx, DocumentId(note)).await });
    blocked(holder, waiter).await;
    remove(&w, removal, editor.id.0).await;
    let result = saving.await.unwrap();
    assert!(matches!(result, Err(AppError::NotFound)), "{result:?}");
}

#[tokio::test]
async fn personal_note_title_waits_for_organization_removal_and_is_refused_after_commit() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let (removal, holder) = counted_removal(&w, editor.id.0).await;
    let app = common::app(&w.state);
    let writer = editor.clone();
    let saving = tokio::spawn(async move {
        send(
            &app,
            Method::PATCH,
            &format!("/api/documents/{note}"),
            Some(&writer),
            Some(
                json!({"request_id": Uuid::now_v7(), "base_revision": 1, "title": "revoked title"}),
            ),
        )
        .await
    });
    waiting(holder, "app_lock_document_workspace").await;
    remove(&w, removal, editor.id.0).await;
    let result = saving.await.unwrap();
    assert_eq!(result.status, StatusCode::NOT_FOUND, "{:?}", result.json);
    let unchanged = one(
        &w.state,
        &w.owner,
        "SELECT title, revision FROM fvoci.documents WHERE id=$1",
        &[note],
    )
    .await;
    assert_eq!(unchanged["title"], "SQL note");
    assert_eq!(unchanged["revision"], 1);
}

#[tokio::test]
async fn personal_note_restore_waits_for_organization_removal_and_rechecks_access() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let (removal, holder) = counted_removal(&w, editor.id.0).await;
    let mut tx = w.state.db.begin_as(editor.id).await.unwrap();
    let waiter = pid(tx.conn()).await;
    let restoring = tokio::spawn(async move {
        on(
            tx.conn(),
            "SELECT fvoci.app_restore_document_revision($1,1,$2,1,1,@ids)",
            &[note, Uuid::now_v7()],
        )
        .await
    });
    blocked(holder, waiter).await;
    remove(&w, removal, editor.id.0).await;
    code(&restoring.await.unwrap().unwrap_err(), "P0002");
}

#[tokio::test]
async fn organization_removal_locks_personal_notes_before_deleting_their_shares() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let mut held = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        held.conn(),
        "SELECT id FROM fvoci.documents WHERE id=$1 FOR UPDATE",
        &[note],
    )
    .await
    .unwrap();
    let holder = pid(held.conn()).await;
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(tx.conn()).await;
    let org = w.org;
    let removing = tokio::spawn(async move {
        on(
            tx.conn(),
            "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
            &[org, editor.id.0],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    });
    blocked(holder, waiter).await;
    held.commit().await.unwrap();
    removing.await.unwrap();
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE document_id=$1",
            &[note]
        )
        .await
        .is_empty()
    );
}
