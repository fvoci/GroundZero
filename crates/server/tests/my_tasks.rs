#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

#[tokio::test]
async fn assigned_only_buckets_organization_order_and_lost_access() {
    let w = World::new().await;
    let member = w.member("member").await;
    let unassigned = task(&w.state, &member, w.item).await;
    let cases = [
        ("NULL", "todo", "none"),
        ("'2026-10-24'", "todo", "overdue"),
        ("'2026-10-25'", "todo", "today"),
        ("'2026-10-26'", "todo", "upcoming"),
        ("'2026-10-24'", "done", "done"),
    ];
    let mut expected = Vec::new();
    for (due, status, bucket) in cases {
        let query = format!(
            "SELECT fvoci.app_create_task($1,$2,'mine',NULL,'','{status}','none',NULL,{due},NULL,'{{}}',ARRAY[$3],false,@ids) AS id"
        );
        let task = made(
            &w.state,
            &w.owner,
            &query,
            &[w.item, Uuid::now_v7(), member.id.0],
        )
        .await;
        expected.push((task, bucket));
    }
    let ws2 = workspace(&w.state, &member, w.org).await;
    let module2 = made(
        &w.state,
        &member,
        "SELECT fvoci.app_create_module($1,$2,'project','second','TWO',@ids) AS id",
        &[ws2, Uuid::now_v7()],
    )
    .await;
    let item2 = made(
        &w.state,
        &member,
        "SELECT fvoci.app_create_item($1,$2,'second','',@ids) AS id",
        &[module2, Uuid::now_v7()],
    )
    .await;
    let second = made(&w.state,&member,"SELECT fvoci.app_create_task($1,$2,'second',NULL,'','todo','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",&[item2,Uuid::now_v7(),member.id.0]).await;
    let list = rows(
        &w.state,
        &member,
        "SELECT * FROM fvoci.app_my_tasks($1,'2026-10-25')",
        &[w.org],
    )
    .await;
    assert_eq!(list.len(), 6);
    assert!(!list.iter().any(|r| r["task_id"] == unassigned.to_string()));
    for (id, bucket) in &expected {
        assert_eq!(
            list.iter()
                .find(|r| r["task_id"] == id.to_string())
                .unwrap()["bucket"],
            *bucket
        );
    }
    assert!(
        list[..5]
            .iter()
            .all(|r| r["workspace_id"] == w.ws.to_string())
    );
    assert_eq!(list[5]["task_id"], second.to_string());
    assert_eq!(
        one(
            &w.state,
            &member,
            "SELECT * FROM fvoci.app_my_task_counts($1,'2026-10-25')",
            &[w.org]
        )
        .await,
        serde_json::json!({"overdue":1,"today":1,"upcoming":1,"undated":2,"done":1})
    );
    let other = World::new().await;
    common::join_organization(other.org, &member, "member").await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[other.ws, member.id.0],
    )
    .await;
    let foreign = made(&w.state,&other.owner,"SELECT fvoci.app_create_task($1,$2,'other org',NULL,'','todo','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",&[other.item,Uuid::now_v7(),member.id.0]).await;
    assert_eq!(
        rows(
            &w.state,
            &member,
            "SELECT task_id FROM fvoci.app_my_tasks($1,'2026-10-25')",
            &[other.org]
        )
        .await,
        vec![serde_json::json!({"task_id":foreign})]
    );
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    let visible = rows(
        &w.state,
        &member,
        "SELECT task_id FROM fvoci.app_my_tasks($1,'2026-10-25')",
        &[w.org],
    )
    .await;
    assert_eq!(visible, vec![serde_json::json!({"task_id":second})]);
}
