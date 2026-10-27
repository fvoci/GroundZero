#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests must panic on failure"
)]
mod common;
mod sql_support;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sql_support::*;
use uuid::Uuid;

const START: &str = "SELECT * FROM fvoci.app_start_timer($1,'work','','task',$2,@ids)";

struct RunningServer(tokio::task::JoinHandle<()>);

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn hold(w: &World) -> (fvoci_server::db::UserTx, i32) {
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || $1::uuid::text,0))",
    )
    .bind(w.owner.id.0)
    .execute(tx.conn())
    .await
    .unwrap();
    let holder = pid(tx.conn()).await;
    (tx, holder)
}

async fn assert_switched(w: &World, replies: &[Value], id_key: &str) {
    assert_ne!(replies[0][id_key], replies[1][id_key]);
    let stopped: Vec<_> = replies
        .iter()
        .filter(|r| !r["stopped_entry_id"].is_null())
        .collect();
    assert_eq!(stopped.len(), 1, "{replies:?}");
    let stopped_id = &stopped[0]["stopped_entry_id"];
    let first = replies
        .iter()
        .find(|r| r["stopped_entry_id"].is_null())
        .unwrap();
    assert_eq!(stopped_id, &first[id_key]);
    let entries = rows(&w.state, &w.owner,
        "SELECT id,ended_at IS NULL AS running,ended_at>=started_at AS ordered FROM fvoci.time_entries WHERE user_id=$1",
        &[w.owner.id.0]).await;
    assert_eq!(entries.len(), 2);
    let running: Vec<_> = entries.iter().filter(|r| r["running"] == true).collect();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0]["id"], stopped[0][id_key]);
    let ended = entries.iter().find(|r| &r["id"] == stopped_id).unwrap();
    assert_eq!(ended["ordered"], true);
}

#[tokio::test]
async fn sql_starts_really_contend_on_the_same_user_lock() {
    let w = World::new().await;
    let tasks = [w.task().await, w.task().await];
    let (holder, holder_pid) = hold(&w).await;
    let mut pending = Vec::new();
    for task in tasks {
        let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
        let waiter = pid(tx.conn()).await;
        let join = tokio::spawn(async move {
            let reply = on(tx.conn(), START, &[Uuid::now_v7(), task])
                .await
                .unwrap()
                .remove(0);
            tx.commit().await.unwrap();
            reply
        });
        blocked(holder_pid, waiter).await;
        pending.push(join);
    }
    holder.commit().await.unwrap();
    let mut replies = Vec::new();
    for join in pending {
        replies.push(
            tokio::time::timeout(std::time::Duration::from_secs(10), join)
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_switched(&w, &replies, "entry_id").await;
}

#[tokio::test]
async fn http_starts_really_contend_on_the_same_user_lock() {
    let w = World::new().await;
    let tasks = [w.task().await, w.task().await];
    let (holder, holder_pid) = hold(&w).await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let app = common::app(&w.state);
    let _server = RunningServer(tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    }));
    let mut pending = Vec::new();
    for task in tasks {
        let user = w.owner.clone();
        pending.push(tokio::spawn(async move {
            let response = reqwest::Client::new()
                .post(format!("http://{address}/api/me/timer"))
                .timeout(std::time::Duration::from_secs(30))
                .header("cookie", &user.cookie)
                .header("origin", common::PUBLIC_URL)
                .json(&json!({"request_id":Uuid::now_v7(),"kind":"work","memo":"",
                    "target":{"kind":"task","id":task}}))
                .send()
                .await
                .unwrap();
            let status = response.status();
            let json = response.json().await.unwrap();
            common::Reply { status, json }
        }));
    }
    // HTTP hides connection PIDs. Discover only waiters of this test's unique user lock,
    // then use the shared pg_blocking_pids observer for each concrete PID.
    let mut observer = common::fixture_conn().await;
    let waiters = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let pids: Vec<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid))",
            )
            .bind(holder_pid)
            .fetch_all(&mut observer)
            .await
            .unwrap();
            if pids.len() == 2 {
                break pids;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("both HTTP requests must reach the timer advisory lock");
    assert_ne!(waiters[0], waiters[1]);
    for waiter in waiters {
        blocked(holder_pid, waiter).await;
    }
    holder.commit().await.unwrap();
    let mut replies = Vec::new();
    for join in pending {
        let reply = tokio::time::timeout(std::time::Duration::from_secs(10), join)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.json);
        replies.push(reply.json);
    }
    assert_switched(&w, &replies, "id").await;
}
