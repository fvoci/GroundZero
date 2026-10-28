#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
use crate::common;
use crate::sql_support;

use sql_support::*;
use uuid::Uuid;

#[tokio::test]
async fn episode_raw_progress_is_private_and_participants_are_aggregates() {
    progress_contract("course").await;
}

#[tokio::test]
async fn unit_raw_progress_is_private_and_participants_are_aggregates() {
    progress_contract("study").await;
}

async fn progress_contract(kind: &str) {
    let w = World::of_kind(kind).await;
    let member = w.member("member").await;
    let outsider = common::login(&w.state, "outside").await;
    let (add, table, progress, key, set, aggregate, total) = if kind == "course" {
        (
            "app_add_episodes",
            "course_episodes",
            "episode_progress",
            "episode_id",
            "SELECT fvoci.app_set_episode_progress($1,'done',NULL,20,'private note',current_date)",
            "app_course_participants",
            "done_seconds",
        )
    } else {
        (
            "app_add_study_units",
            "study_units",
            "unit_progress",
            "unit_id",
            "SELECT fvoci.app_set_unit_progress($1,'done',20,NULL,'private note',current_date)",
            "app_study_participants",
            "done_amount",
        )
    };
    rows(&w.state,&w.owner,&format!("SELECT fvoci.{add}($1,jsonb_build_array(jsonb_build_object('title','unit','request_id',$2::text,'duration_seconds',60,'amount',20)),@ids)"),&[w.item,Uuid::now_v7()]).await;
    let unit = made(
        &w.state,
        &w.owner,
        &format!("SELECT id FROM fvoci.{table} WHERE item_id=$1"),
        &[w.item],
    )
    .await;
    rows(&w.state, &member, set, &[unit]).await;
    let select = format!("SELECT * FROM fvoci.{progress} WHERE {key}=$1 AND user_id=$2");
    assert_eq!(
        rows(&w.state, &member, &select, &[unit, member.id.0])
            .await
            .len(),
        1
    );
    for actor in [&w.owner, &outsider] {
        assert!(
            rows(&w.state, actor, &select, &[unit, member.id.0])
                .await
                .is_empty()
        );
    }
    let query = format!("SELECT * FROM fvoci.{aggregate}($1,current_date)");
    for actor in [&w.owner, &member] {
        let result = rows(&w.state, actor, &query, &[w.item]).await;
        assert_eq!(result.len(), 1);
        let mut columns: Vec<_> = result[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        columns.sort_unstable();
        let mut expected = vec![
            "user_id",
            "display_name",
            "done",
            "in_progress",
            "overdue",
            total,
        ];
        expected.sort_unstable();
        assert_eq!(columns, expected);
        assert_eq!(result[0]["done"], 1);
        assert_eq!(result[0][total], if kind == "course" { 60 } else { 20 });
    }
    denied(&w.state, &outsider, &query, &[w.item], "P0002").await;
    fixture(
        "UPDATE fvoci.items SET visibility='private' WHERE id=$1",
        &[w.item],
    )
    .await;
    denied(&w.state, &member, &query, &[w.item], "P0002").await;
}
