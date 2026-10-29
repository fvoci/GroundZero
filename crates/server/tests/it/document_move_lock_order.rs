#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration fixtures and lock observations must fail loudly"
)]
//! #223: ID 집계와 회수 사이에 공유 노트가 공간으로 들어와 FV001을 만들 수 없다.

use crate::common::{self, send};
use crate::sql_support::*;
use axum::http::{Method, StatusCode};
use fvoci_core::error::AppError;
use serde_json::json;
use tokio::task::JoinHandle;
use uuid::Uuid;

#[derive(Clone, Copy)]
enum Change {
    OrganizationRemoval,
    WorkspaceLeave,
    MemberRemoval,
    AdminDemotion,
}

impl Change {
    fn sql(self) -> (&'static str, &'static str) {
        match self {
            Self::OrganizationRemoval => (
                "SELECT fvoci.app_remove_organization_member_ids($1,$2)",
                "SELECT fvoci.app_remove_organization_member($1,$2,$3)",
            ),
            Self::WorkspaceLeave => (
                "SELECT fvoci.app_leave_workspace_ids($1)",
                "SELECT fvoci.app_leave_workspace($1,$2)",
            ),
            Self::MemberRemoval => (
                "SELECT fvoci.app_remove_member_ids($1,$2)",
                "SELECT fvoci.app_remove_member($1,$2,$3)",
            ),
            Self::AdminDemotion => (
                "SELECT fvoci.app_set_organization_member_ids($1,$2,'member')",
                "SELECT fvoci.app_set_organization_member($1,$2,'member',$3)",
            ),
        }
    }
}

/// baseline에서는 이동이 먼저 끝나고 명령이 FV001, 수정 뒤에는 이동이 공간 잠금을 기다린다.
async fn move_finished_or_waiting(holder: i32, moving: &JoinHandle<Result<(), AppError>>) {
    let mut observer = common::fixture_conn().await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiter: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) LIMIT 1",
            )
            .bind(holder)
            .fetch_optional(&mut observer)
            .await
            .unwrap();
            if let Some(waiter) = waiter {
                blocked(holder, waiter).await;
                break;
            }
            if moving.is_finished() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("move neither completed nor waited for the counted removal");
}

async fn serialized(change: Change, http: bool) {
    let w = World::new().await;
    let member = w.member("admin").await;
    if matches!(change, Change::AdminDemotion) {
        fixture("UPDATE fvoci.organization_members SET role='admin' WHERE organization_id=$1 AND user_id=$2", &[w.org, member.id.0]).await;
        fixture("UPDATE fvoci.workspace_members SET role_source='admin_join', previous_role=NULL, previous_role_source=NULL WHERE workspace_id=$1 AND user_id=$2", &[w.ws, member.id.0]).await;
    }
    let note = w.note().await;
    w.share(note, &member, "write").await;
    let app = common::app(&w.state);
    let plan = send(
        &app,
        Method::POST,
        &format!("/api/documents/{note}/move-plan"),
        Some(&w.owner),
        Some(json!({"target_workspace_id": w.ws})),
    )
    .await;
    assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
    let (count_sql, command_sql) = change.sql();
    let actor = if matches!(change, Change::WorkspaceLeave) {
        &member
    } else {
        &w.owner
    };
    let scope = if matches!(change, Change::OrganizationRemoval | Change::AdminDemotion) {
        w.org
    } else {
        w.ws
    };
    let args = if matches!(change, Change::WorkspaceLeave) {
        vec![scope]
    } else {
        vec![scope, member.id.0]
    };
    let mut removal = w.state.db.begin_as(actor.id).await.unwrap();
    let mut count = sqlx::query_scalar::<_, i32>(count_sql);
    for arg in &args {
        count = count.bind(arg);
    }
    let count = count.fetch_one(removal.conn()).await.unwrap();
    let holder = pid(removal.conn()).await;
    let (db, owner, ws) = (w.state.db.clone(), w.owner.clone(), w.ws);
    let moving = tokio::spawn(async move {
        if http {
            let reply = send(
                &app,
                Method::PUT,
                &format!("/api/documents/{note}/placement"),
                Some(&owner),
                Some(json!({"target_workspace_id": ws, "acknowledged": plan.json})),
            )
            .await;
            assert!(
                matches!(reply.status, StatusCode::OK | StatusCode::CONFLICT),
                "{:?}",
                reply.json
            );
            Ok(())
        } else {
            let mut tx = db.begin_as(owner.id).await?;
            on(
                tx.conn(),
                "SELECT * FROM fvoci.app_move_document($1,$2,NULL,@ids)",
                &[note, ws],
            )
            .await?;
            tx.commit().await
        }
    });
    move_finished_or_waiting(holder, &moving).await;
    let mut command = sqlx::query(command_sql);
    for arg in &args {
        command = command.bind(arg);
    }
    let removed = command
        .bind(fvoci_db::new_ids(usize::try_from(count).unwrap()))
        .execute(removal.conn())
        .await;
    if removed.is_ok() {
        removal.commit().await.unwrap();
    } else {
        drop(removal);
    }
    let moved = tokio::time::timeout(std::time::Duration::from_secs(10), moving)
        .await
        .unwrap()
        .unwrap();
    assert!(
        removed.is_ok(),
        "counted revocation must commit without FV001: {removed:?}"
    );
    assert!(moved.is_ok(), "{moved:?}");
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT user_id FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &[w.ws, member.id.0]
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn sql_move_serializes_with_organization_removal_id_count() {
    serialized(Change::OrganizationRemoval, false).await;
}
#[tokio::test]
async fn http_move_serializes_with_organization_removal_id_count() {
    serialized(Change::OrganizationRemoval, true).await;
}
#[tokio::test]
async fn sql_move_serializes_with_workspace_leave_id_count() {
    serialized(Change::WorkspaceLeave, false).await;
}
#[tokio::test]
async fn http_move_serializes_with_workspace_leave_id_count() {
    serialized(Change::WorkspaceLeave, true).await;
}
#[tokio::test]
async fn sql_move_serializes_with_member_removal_id_count() {
    serialized(Change::MemberRemoval, false).await;
}
#[tokio::test]
async fn http_move_serializes_with_member_removal_id_count() {
    serialized(Change::MemberRemoval, true).await;
}
#[tokio::test]
async fn sql_move_serializes_with_admin_demotion_id_count() {
    serialized(Change::AdminDemotion, false).await;
}
#[tokio::test]
async fn http_move_serializes_with_admin_demotion_id_count() {
    serialized(Change::AdminDemotion, true).await;
}

async fn open_upload(w: &World, doc: Uuid) -> Uuid {
    let upload = made(&w.state, &w.owner,
        "SELECT fvoci.app_create_upload($1,NULL,'document',$2,'moving.txt','','text/plain',10,10,3600,1000,100000,@ids) AS id",
        &[Uuid::now_v7(), doc]).await;
    fixture(
        "UPDATE fvoci.uploads SET state='completing' WHERE id=$1",
        &[upload],
    )
    .await;
    upload
}

async fn finish_upload(tx: &mut fvoci_db::UserTx, upload: Uuid) {
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        on(
            tx.conn(),
            "SELECT fvoci.app_finish_upload($1,repeat('a',64),100000,@ids)",
            &[upload],
        ),
    )
    .await
    .expect("upload completion and document move deadlocked");
    assert!(result.is_ok(), "{result:?}");
}

#[tokio::test]
async fn sql_move_waits_for_tree_before_locking_uploads() {
    let w = World::new().await;
    let note = w.note().await;
    let upload = open_upload(&w, note).await;
    let mut tree = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        tree.conn(),
        "SELECT pg_advisory_xact_lock(hashtextextended('fvoci-tree:' || $1::uuid::text || '::',0))",
        &[w.ws],
    )
    .await
    .unwrap();
    let holder = pid(tree.conn()).await;
    let mut mover = w.state.db.begin_as(w.owner.id).await.unwrap();
    let waiter = pid(mover.conn()).await;
    let ws = w.ws;
    let moving = tokio::spawn(async move {
        let result = on(
            mover.conn(),
            "SELECT * FROM fvoci.app_move_document($1,$2,NULL,@ids)",
            &[note, ws],
        )
        .await;
        if result.is_ok() {
            mover.commit().await.unwrap();
        }
        result
    });
    blocked(holder, waiter).await;
    finish_upload(&mut tree, upload).await;
    tree.commit().await.unwrap();
    let result = moving.await.unwrap();
    assert!(result.is_ok(), "{result:?}");
    let file = one(
        &w.state,
        &w.owner,
        "SELECT workspace_id FROM fvoci.files WHERE document_id=$1",
        &[note],
    )
    .await;
    assert_eq!(file["workspace_id"], json!(w.ws));
}

#[tokio::test]
async fn http_move_waits_for_uploads_before_locking_the_document() {
    let w = World::new().await;
    let note = w.note().await;
    let upload = open_upload(&w, note).await;
    let app = common::app(&w.state);
    let plan = send(
        &app,
        Method::POST,
        &format!("/api/documents/{note}/move-plan"),
        Some(&w.owner),
        Some(json!({"target_workspace_id": w.ws})),
    )
    .await;
    assert_eq!(plan.status, StatusCode::OK, "{:?}", plan.json);
    let mut finishing = w.state.db.begin_as(w.owner.id).await.unwrap();
    let mut fixture = common::fixture_conn().await;
    let claim: time::OffsetDateTime =
        sqlx::query_scalar("SELECT state_changed_at FROM fvoci.uploads WHERE id=$1")
            .bind(upload)
            .fetch_one(&mut fixture)
            .await
            .unwrap();
    let held: bool = sqlx::query_scalar("SELECT fvoci.app_upload_claim_held($1,$2)")
        .bind(upload)
        .bind(claim)
        .fetch_one(finishing.conn())
        .await
        .unwrap();
    assert!(held);
    let holder = pid(finishing.conn()).await;
    let (owner, ws) = (w.owner.clone(), w.ws);
    let moving = tokio::spawn(async move {
        let reply = send(
            &app,
            Method::PUT,
            &format!("/api/documents/{note}/placement"),
            Some(&owner),
            Some(json!({"target_workspace_id": ws, "acknowledged": plan.json})),
        )
        .await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
        Ok(())
    });
    move_finished_or_waiting(holder, &moving).await;
    finish_upload(&mut finishing, upload).await;
    finishing.commit().await.unwrap();
    moving.await.unwrap().unwrap();
    let file = one(
        &w.state,
        &w.owner,
        "SELECT workspace_id FROM fvoci.files WHERE document_id=$1",
        &[note],
    )
    .await;
    assert_eq!(file["workspace_id"], json!(w.ws));
}
