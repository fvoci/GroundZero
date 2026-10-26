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
async fn default_statuses_and_owner_only_customization() {
    let w = World::new().await;
    let statuses = rows(
        &w.state,
        &w.owner,
        "SELECT name,category FROM fvoci.app_task_statuses($1)",
        &[w.item],
    )
    .await;
    assert_eq!(
        statuses,
        vec![
            serde_json::json!({"name":"할 일","category":"todo"}),
            serde_json::json!({"name":"진행 중","category":"doing"}),
            serde_json::json!({"name":"완료","category":"done"})
        ]
    );
    let member = w.member("member").await;
    denied(
        &w.state,
        &member,
        "SELECT fvoci.app_create_task_status($1,'review','','doing',@ids)",
        &[w.item],
        "42501",
    )
    .await;
    let custom = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_task_status($1,'review','','doing',@ids) AS id",
        &[w.item],
    )
    .await;
    let task = made(
        &w.state,
        &w.owner,
        &CREATE_TASK.replace("'할 일'", "'review'"),
        &[w.item, Uuid::now_v7()],
    )
    .await;
    assert!(
        one(
            &w.state,
            &w.owner,
            "SELECT completed_at FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await["completed_at"]
            .is_null()
    );
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_update_task_status($1,NULL,NULL,'done',NULL,@ids)",
        &[custom],
    )
    .await;
    assert!(
        !one(
            &w.state,
            &w.owner,
            "SELECT completed_at FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await["completed_at"]
            .is_null()
    );
    fixture(
        "DELETE FROM fvoci.search_queue WHERE kind='task' AND target_id=$1",
        &[task],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_update_task_status($1,'accepted',NULL,NULL,NULL,@ids)",
        &[custom],
    )
    .await;
    // 대기열은 앱 SELECT 권한이 없으므로 시스템의 공개 claim 계약으로 관찰하고 롤백한다.
    let mut tx = w.state.db.begin_anonymous().await.unwrap();
    let queued = on(
        tx.conn(),
        "SELECT * FROM fvoci.app_system_search_payload(ARRAY['task'],ARRAY[$1])",
        &[task],
    )
    .await
    .unwrap();
    assert_eq!(queued[0]["status"], "accepted");
    drop(tx);
    // 준비 시 비웠던 큐의 재삽입은 앱 worker claim으로 확인한다.
    let mut tx = w.state.db.begin_anonymous().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let claimed = on(
                tx.conn(),
                "SELECT * FROM fvoci.app_system_search_claim(500)",
                &[],
            )
            .await
            .unwrap();
            assert!(!claimed.is_empty(), "renamed task was not enqueued");
            if claimed.iter().any(|r| r["target_id"] == task.to_string()) {
                break;
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn deleting_status_moves_tasks_and_preserves_one_column() {
    let w = World::new().await;
    let task = w.task().await;
    let statuses = rows(
        &w.state,
        &w.owner,
        "SELECT id,name FROM fvoci.app_task_statuses($1)",
        &[w.item],
    )
    .await;
    let todo = id(&statuses[0]);
    let doing = id(&statuses[1]);
    let done = id(&statuses[2]);
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_task_status($1,NULL,@ids)",
        &[todo],
        "22023",
    )
    .await;
    let before = one(
        &w.state,
        &w.owner,
        "SELECT move_version FROM fvoci.tasks WHERE id=$1",
        &[task],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT fvoci.app_delete_task_status($1,$2,@ids) AS n",
            &[todo, doing]
        )
        .await["n"],
        1
    );
    let after = one(
        &w.state,
        &w.owner,
        "SELECT move_version,status_id FROM fvoci.tasks WHERE id=$1",
        &[task],
    )
    .await;
    assert_eq!(after["status_id"], doing.to_string());
    assert!(after["move_version"].as_i64().unwrap() > before["move_version"].as_i64().unwrap());
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_task_status($1,NULL,@ids)",
        &[done],
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_task_status($1,NULL,@ids)",
        &[doing],
        "55000",
    )
    .await;
}

#[tokio::test]
async fn task_mutators_reject_unknown_names_and_accept_custom_names() {
    let w = World::new().await;
    let task = w.task().await;
    denied(
        &w.state,
        &w.owner,
        &CREATE_TASK.replace("'할 일'", "'missing'"),
        &[w.item, Uuid::now_v7()],
        "22023",
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        &UPDATE_TASK.replace("'할 일'", "'missing'"),
        &[task],
        "22023",
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_move_task($1,'missing',NULL,NULL,@ids)",
        &[task],
        "22023",
    )
    .await;
    let status = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_task_status($1,'review','','doing',@ids) AS id",
        &[w.item],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        &UPDATE_TASK.replace("'할 일'", "'review'"),
        &[task],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_move_task($1,'할 일',NULL,NULL,@ids)",
        &[task],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_move_task($1,'review',NULL,NULL,@ids)",
        &[task],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT status_id FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await["status_id"],
        status.to_string()
    );
}

#[tokio::test]
async fn copies_preserve_status_definitions_and_reset_tasks_and_recurrences() {
    let w = World::new().await;
    made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_task_status($1,'review','#0000ff','doing',@ids) AS id",
        &[w.item],
    )
    .await;
    let task = made(
        &w.state,
        &w.owner,
        &CREATE_TASK.replace("'할 일'", "'review'"),
        &[w.item, Uuid::now_v7()],
    )
    .await;
    for reset in [false, true] {
        let copy = one(
            &w.state,
            &w.owner,
            &format!("SELECT * FROM fvoci.app_copy_item($1,$2,$3,'copy',{reset},@ids)"),
            &[w.item, w.module, Uuid::now_v7()],
        )
        .await;
        let copied = Uuid::parse_str(copy["copied_item"].as_str().unwrap()).unwrap();
        let defs = "SELECT name,color,category,position FROM fvoci.app_task_statuses($1)";
        assert_eq!(
            rows(&w.state, &w.owner, defs, &[copied]).await,
            rows(&w.state, &w.owner, defs, &[w.item]).await
        );
        assert_eq!(one(&w.state,&w.owner,"SELECT s.name FROM fvoci.tasks t JOIN fvoci.task_statuses s ON s.id=t.status_id WHERE t.item_id=$1",&[copied]).await["name"],if reset {"할 일"} else {"review"});
    }
    fixture(
        "UPDATE fvoci.tasks SET due_date=current_date WHERE id=$1",
        &[task],
    )
    .await;
    let series = one(&w.state,&w.owner,"SELECT * FROM fvoci.app_set_task_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[]}','UTC',NULL,@ids)",&[task]).await;
    let series = Uuid::parse_str(series["series_id"].as_str().unwrap()).unwrap();
    system(
        &w.state,
        "SELECT fvoci.app_system_sync_task_series($1,@ids)",
        &[series],
    )
    .await
    .unwrap();
    let generated = rows(&w.state,&w.owner,"SELECT s.name FROM fvoci.tasks t JOIN fvoci.task_statuses s ON s.id=t.status_id WHERE t.series_id=$1 AND t.id<>$2",&[series,task]).await;
    assert!(!generated.is_empty());
    assert!(generated.iter().all(|r| r["name"] == "할 일"));
}
