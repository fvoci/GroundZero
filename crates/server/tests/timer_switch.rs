#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
mod common;
mod sql_support;

use sql_support::*;
use uuid::Uuid;

const START: &str = "SELECT * FROM fvoci.app_start_timer($1,'work','','task',$2,@ids)";

#[tokio::test]
async fn timer_switch_retry_and_inaccessible_target_are_atomic() {
    let w = World::new().await;
    let a = w.task().await;
    let b = w.task().await;
    let foreign = World::new().await;
    let hidden = foreign.task().await;
    let first = one(&w.state, &w.owner, START, &[Uuid::now_v7(), a]).await;
    let old = Uuid::parse_str(first["entry_id"].as_str().unwrap()).unwrap();
    assert!(first["stopped_entry_id"].is_null());
    let request = Uuid::now_v7();
    let second = one(&w.state, &w.owner, START, &[request, b]).await;
    assert_eq!(second["stopped_entry_id"], old.to_string());
    let new = Uuid::parse_str(second["entry_id"].as_str().unwrap()).unwrap();
    let saved = one(
        &w.state,
        &w.owner,
        "SELECT ended_at FROM fvoci.time_entries WHERE id=$1",
        &[old],
    )
    .await;
    assert!(!saved["ended_at"].is_null());
    let third = one(&w.state, &w.owner, START, &[Uuid::now_v7(), a]).await;
    let retry = one(&w.state, &w.owner, START, &[request, b]).await;
    assert_eq!(retry["entry_id"], new.to_string());
    assert!(retry["stopped_entry_id"].is_null());
    denied(
        &w.state,
        &w.owner,
        START,
        &[Uuid::now_v7(), hidden],
        "P0002",
    )
    .await;
    let running = rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.time_entries WHERE user_id=$1 AND ended_at IS NULL",
        &[w.owner.id.0],
    )
    .await;
    assert_eq!(running.len(), 1);
    assert_eq!(running[0]["id"], third["entry_id"]);
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT count(*) AS n FROM fvoci.time_entries WHERE user_id=$1",
            &[w.owner.id.0]
        )
        .await["n"],
        3
    );
}

#[tokio::test]
async fn concurrent_starts_leave_one_running_and_paused_time_is_saved() {
    let w = World::new().await;
    let task = w.task().await;
    let req_a = [Uuid::now_v7(), task];
    let req_b = [Uuid::now_v7(), w.task().await];
    let (a, b) = tokio::join!(
        attempt(&w.state, &w.owner, START, &req_a),
        attempt(&w.state, &w.owner, START, &req_b)
    );
    a.unwrap();
    b.unwrap();
    let running = rows(
        &w.state,
        &w.owner,
        "SELECT id FROM fvoci.time_entries WHERE user_id=$1 AND ended_at IS NULL",
        &[w.owner.id.0],
    )
    .await;
    assert_eq!(running.len(), 1);
    let old = id(&running[0]);
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_timer_action($1,'pause')",
        &[old],
    )
    .await;
    fixture("UPDATE fvoci.time_entries SET started_at=now()-interval '2 minutes',paused_at=now()-interval '1 minute',paused_seconds=7 WHERE id=$1",&[old]).await;
    let next = one(&w.state, &w.owner, START, &[Uuid::now_v7(), task]).await;
    assert_eq!(next["stopped_entry_id"], old.to_string());
    let saved = one(
        &w.state,
        &w.owner,
        "SELECT paused_seconds,paused_at,ended_at,paused_seconds=7+floor(extract(epoch FROM ended_at-started_at-interval '1 minute')) AS exact_pause FROM fvoci.time_entries WHERE id=$1",
        &[old],
    )
    .await;
    assert!(saved["paused_seconds"].as_i64().unwrap() >= 67);
    assert_eq!(saved["exact_pause"], true);
    assert!(saved["paused_at"].is_null());
    assert!(!saved["ended_at"].is_null());
}

#[tokio::test]
async fn time_rows_and_aggregates_are_personal_even_for_admins() {
    let w = World::new().await;
    let member = w.member("member").await;
    let task = w.task().await;
    let add = "SELECT fvoci.app_add_time_entry($1,'2026-01-01 00:00Z','2026-01-01 00:02Z',0,'work','private memo','task',$2,@ids) AS id";
    let entry = made(&w.state, &member, add, &[Uuid::now_v7(), task]).await;
    let own = made(
        &w.state,
        &w.owner,
        &add.replace("00:02Z", "00:01Z"),
        &[Uuid::now_v7(), task],
    )
    .await;
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT * FROM fvoci.time_entries WHERE id=$1",
            &[entry]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &member,
            "SELECT * FROM fvoci.time_entries WHERE id=$1",
            &[own]
        )
        .await
        .is_empty()
    );
    for (user, seconds) in [(&w.owner, 60), (&member, 120)] {
        assert_eq!(
            one(
                &w.state,
                user,
                "SELECT fvoci.app_task_time($1) AS n",
                &[task]
            )
            .await["n"],
            seconds
        );
        assert_eq!(
            one(
                &w.state,
                user,
                "SELECT * FROM fvoci.app_item_time($1)",
                &[w.item]
            )
            .await["mine_seconds"],
            seconds
        );
    }
}
