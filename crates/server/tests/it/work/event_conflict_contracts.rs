#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
use crate::sql_support;

use sql_support::*;
use uuid::Uuid;

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
