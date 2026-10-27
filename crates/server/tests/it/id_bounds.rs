#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
use crate::common;
use crate::sql_support;

use serde_json::{Value, json};
use sql_support::*;
use uuid::Uuid;

// Read whole rows, including revisions, counters, membership, audit and activity. Admin observation
// also sees deleted/private rows; every predicate is limited to this test's organization/owner.
async fn snapshot(w: &World) -> Value {
    let mut conn = common::fixture_conn().await;
    let scope: Vec<String> = sqlx::query_scalar(
        "SELECT $1::uuid::text UNION SELECT $2::uuid::text
         UNION SELECT id::text FROM fvoci.workspaces WHERE organization_id=$1
         UNION SELECT i.id::text FROM fvoci.items i JOIN fvoci.workspaces w ON w.id=i.workspace_id WHERE w.organization_id=$1
         UNION SELECT d.id::text FROM fvoci.documents d LEFT JOIN fvoci.workspaces w ON w.id=d.workspace_id WHERE w.organization_id=$1 OR d.owner_user_id=$2
         UNION SELECT t.id::text FROM fvoci.tasks t JOIN fvoci.workspaces w ON w.id=t.workspace_id WHERE w.organization_id=$1")
        .bind(w.org).bind(w.owner.id.0).fetch_all(&mut conn).await.unwrap();
    let mut result = serde_json::Map::new();
    for table in [
        "workspaces",
        "organization_members",
        "workspace_members",
        "items",
        "tasks",
        "task_statuses",
        "task_assignees",
        "task_dependencies",
        "checklist_items",
        "events",
        "documents",
        "document_revisions",
        "document_shares",
        "document_links",
        "files",
        "uploads",
        "trash_batches",
        "audit_log",
        "activity",
        "notifications",
        "numbered_targets",
        "reference_counters",
        "course_episodes",
        "study_units",
        "item_members",
        "search_queue",
    ] {
        let rows: Value = sqlx::query_scalar(&format!(
            "SELECT coalesce(jsonb_agg(r ORDER BY r::text),'[]'::jsonb) FROM
             (SELECT to_jsonb(t) r FROM fvoci.{table} t) s WHERE EXISTS
             (SELECT 1 FROM jsonb_each_text(r) e WHERE e.key IN
              ('id','organization_id','workspace_id','item_id','document_id','task_id','owner_user_id','target_id')
              AND e.value=ANY($1))"))
            .bind(&scope).fetch_one(&mut conn).await.unwrap();
        result.insert(table.to_owned(), rows);
    }
    Value::Object(result)
}

async fn exact_call(
    w: &World,
    sql: &str,
    args: &[Uuid],
    count: usize,
) -> Result<Vec<Value>, sqlx::Error> {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let sql = format!(
        "SELECT to_jsonb(r) FROM ({}) r",
        sql.replace("@ids", &format!("${}::uuid[]", args.len() + 1))
    );
    let mut query = sqlx::query_scalar(&sql);
    for arg in args {
        query = query.bind(arg);
    }
    let result = query
        .bind(fvoci_db::new_ids(count))
        .fetch_all(tx.conn())
        .await;
    match result {
        Ok(rows) => {
            tx.commit().await.unwrap();
            Ok(rows)
        }
        Err(error) => {
            // Explicitly close the failed transaction before observing from another connection.
            sqlx::query("ROLLBACK").execute(tx.conn()).await.unwrap();
            drop(tx);
            Err(error)
        }
    }
}

async fn boundary(w: &World, sql: &str, args: &[Uuid], count: usize) -> Vec<Value> {
    let before = snapshot(w).await;
    code(
        &exact_call(w, sql, args, count - 1).await.unwrap_err(),
        "FV001",
    );
    assert_eq!(snapshot(w).await, before, "partial mutation after {sql}");
    let result = exact_call(w, sql, args, count).await.unwrap();
    assert_ne!(snapshot(w).await, before, "exact count must commit {sql}");
    result
}

async fn bound(w: &World, sql: &str, args: &[Uuid]) -> usize {
    usize::try_from(
        one(&w.state, &w.owner, sql, args).await["n"]
            .as_i64()
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn create_task_needs_seven_ids_for_two_assignees() {
    let w = World::new().await;
    let a = w.member("member").await;
    let b = w.member("member").await;
    let result = boundary(&w,
        "SELECT fvoci.app_create_task($1,$2,'bounded task',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3,$4],false,@ids) AS id",
        &[w.item, Uuid::now_v7(), a.id.0, b.id.0], 7).await;
    let task = id(&result[0]);
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.task_assignees WHERE task_id=$1",
            &[task]
        )
        .await["n"],
        2
    );
}

#[tokio::test]
async fn create_item_needs_six_ids() {
    let w = World::new().await;
    boundary(
        &w,
        "SELECT fvoci.app_create_item($1,$2,'bounded item','',@ids) AS id",
        &[w.module, Uuid::now_v7()],
        6,
    )
    .await;
}

#[tokio::test]
async fn create_event_needs_three_ids() {
    let w = World::new().await;
    boundary(&w,
        "SELECT fvoci.app_create_event($1,$2,'event','bounded event','',true,current_date,current_date,NULL,NULL,'UTC',@ids) AS id",
        &[w.item,Uuid::now_v7()], 3).await;
}

#[tokio::test]
async fn move_bound_includes_two_children_and_one_revoked_share() {
    let w = World::new().await;
    let doc = w.note().await;
    let children = [w.note().await, w.note().await];
    for child in children {
        fixture(
            "UPDATE fvoci.documents SET parent_id=$1 WHERE id=$2",
            &[doc, child],
        )
        .await;
    }
    let recipient = common::login_with_email(
        &w.state,
        "share recipient",
        &common::unique_email("bound"),
        true,
    )
    .await;
    let other_org = common::organization_for(&recipient).await;
    common::join_organization(other_org, &w.owner, "member").await;
    made(&w.state,&w.owner,
        "SELECT fvoci.app_share_document($1,(SELECT email FROM fvoci.users WHERE id=$2),'read',$3,@ids) AS id",
        &[doc,recipient.id.0,other_org]).await;
    // A workspace-to-workspace share in the same organization would not be revoked: *_ids is
    // an upper bound. Moving a personal note with a different-organization share consumes it.
    let n = bound(&w, "SELECT fvoci.app_move_document_ids($1) AS n", &[doc]).await;
    assert_eq!(n, 6);
    boundary(
        &w,
        "SELECT * FROM fvoci.app_move_document($1,$2,NULL,@ids)",
        &[doc, w.ws],
        n,
    )
    .await;
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
    for child in children {
        assert!(
            one(
                &w.state,
                &w.owner,
                "SELECT parent_id FROM fvoci.documents WHERE id=$1",
                &[child]
            )
            .await["parent_id"]
                .is_null()
        );
    }
}

async fn member_fixture(w: &World) -> common::TestUser {
    let member = w.member("member").await;
    for _ in 0..2 {
        w.share(w.doc().await, &member, "read").await;
    }
    made(&w.state,&w.owner,
        "SELECT fvoci.app_create_task($1,$2,'assigned',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",
        &[w.item,Uuid::now_v7(),member.id.0]).await;
    member
}

#[tokio::test]
async fn remove_member_bound_covers_two_shares_and_one_assignment() {
    let w = World::new().await;
    let member = member_fixture(&w).await;
    let args = [w.ws, member.id.0];
    let n = bound(&w, "SELECT fvoci.app_remove_member_ids($1,$2) AS n", &args).await;
    assert_eq!(n, 5);
    boundary(&w, "SELECT fvoci.app_remove_member($1,$2,@ids)", &args, n).await;
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT user_id FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
            &args
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn remove_organization_member_bound_covers_shares_assignment_and_membership() {
    let w = World::new().await;
    let member = member_fixture(&w).await;
    let args = [w.org, member.id.0];
    let n = bound(
        &w,
        "SELECT fvoci.app_remove_organization_member_ids($1,$2) AS n",
        &args,
    )
    .await;
    assert_eq!(n, 6);
    boundary(
        &w,
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &args,
        n,
    )
    .await;
    assert!(rows(&w.state,&w.owner,"SELECT user_id FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2",&args).await.is_empty());
}

#[tokio::test]
async fn copy_item_consumes_its_computed_bound() {
    let w = World::new().await;
    w.task().await;
    w.doc().await;
    let n = bound(&w, "SELECT fvoci.app_copy_item_ids($1) AS n", &[w.item]).await;
    assert_eq!(n, 14); // create 6 + copied statuses 3 + task 2 + document 2 + audit 1
    let copied = boundary(
        &w,
        "SELECT * FROM fvoci.app_copy_item($1,$2,$3,'bounded copy',false,@ids)",
        &[w.item, w.module, Uuid::now_v7()],
        n,
    )
    .await;
    assert_eq!(copied[0]["skipped_count"], json!(0));
}

#[tokio::test]
async fn restore_batch_needs_one_id_and_zero_leaves_it_trashed() {
    let w = World::new().await;
    let doc = w.doc().await;
    let batch = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_trash_document($1,@ids) AS id",
        &[doc],
    )
    .await;
    boundary(&w, "SELECT fvoci.app_restore_batch($1,@ids)", &[batch], 1).await;
    assert!(
        one(
            &w.state,
            &w.owner,
            "SELECT deleted_at FROM fvoci.documents WHERE id=$1",
            &[doc]
        )
        .await["deleted_at"]
            .is_null()
    );
}
