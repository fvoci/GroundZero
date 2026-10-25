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
async fn move_and_edit_conflicts_leave_the_entire_task_unchanged() {
    let w = World::new().await;
    let task = w.task().await;
    let before = one(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.tasks WHERE id=$1",
        &[task],
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_move_task($1,'done',NULL,-1,@ids)",
        &[task],
        "FV003",
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await,
        before
    );
    let result = one(
        &w.state,
        &w.owner,
        &UPDATE_TASK.replace("$1,1,", "$1,-1,"),
        &[task],
    )
    .await;
    assert_eq!(result["outcome"], "conflict");
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.tasks WHERE id=$1",
            &[task]
        )
        .await,
        before
    );
}

#[tokio::test]
async fn create_request_and_task_recurrence_retries_are_idempotent() {
    let w = World::new().await;
    let request = Uuid::now_v7();
    let first = made(&w.state, &w.owner, CREATE_TASK, &[w.item, request]).await;
    assert_eq!(
        made(&w.state, &w.owner, CREATE_TASK, &[w.item, request]).await,
        first
    );
    fixture(
        "UPDATE fvoci.tasks SET due_date=current_date WHERE id=$1",
        &[first],
    )
    .await;
    let set = "SELECT * FROM fvoci.app_set_task_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[]}','UTC',NULL,@ids)";
    let initial = one(&w.state, &w.owner, set, &[first]).await;
    assert_eq!(initial["outcome"], "updated");
    let series = Uuid::parse_str(initial["series_id"].as_str().unwrap()).unwrap();
    let before = one(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.task_series WHERE id=$1",
        &[series],
    )
    .await;
    let conflict = set.replace("interval\":1", "interval\":2");
    assert_eq!(
        one(&w.state, &w.owner, &conflict, &[first]).await["outcome"],
        "conflict"
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.task_series WHERE id=$1",
            &[series]
        )
        .await,
        before
    );
    assert_eq!(one(&w.state, &w.owner, set, &[first]).await, initial);
}

#[tokio::test]
async fn event_recurrence_uses_the_same_base_and_retry_contract() {
    let w = World::new().await;
    let event = made(&w.state,&w.owner,"SELECT fvoci.app_create_event($1,$2,'meeting','event','',true,current_date,current_date,NULL,NULL,'UTC',@ids) AS id",&[w.item,Uuid::now_v7()]).await;
    let set = "SELECT fvoci.app_set_event_recurrence($1,'{\"freq\":\"daily\",\"interval\":1,\"weekdays\":[]}',NULL,@ids) AS outcome";
    assert_eq!(
        one(&w.state, &w.owner, set, &[event]).await["outcome"],
        "updated"
    );
    let before = one(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.events WHERE id=$1",
        &[event],
    )
    .await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            &set.replace("interval\":1", "interval\":2"),
            &[event]
        )
        .await["outcome"],
        "conflict"
    );
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.events WHERE id=$1",
            &[event]
        )
        .await,
        before
    );
    assert_eq!(
        one(&w.state, &w.owner, set, &[event]).await["outcome"],
        "updated"
    );
}
