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
async fn activity_records_creation_comments_status_moves_and_assignments() {
    let w = World::new().await;
    let member = w.member("member").await;
    let task = w.task().await;
    let doc = w.doc().await;
    let event = made(&w.state,&w.owner,"SELECT fvoci.app_create_event($1,$2,'meeting','event','',true,current_date,current_date,NULL,NULL,'UTC',@ids) AS id",&[w.item,Uuid::now_v7()]).await;
    let comment = made(&w.state, &member, COMMENT, &[Uuid::now_v7(), task]).await;
    let update = UPDATE_TASK
        .replace("'할 일'", "'진행 중'")
        .replace("'{}','{}'", "'{}',ARRAY[$2]");
    rows(&w.state, &w.owner, &update, &[task, member.id.0]).await;
    rows(
        &w.state,
        &w.owner,
        &UPDATE_TASK
            .replace("$1,1,", "$1,2,")
            .replace("'할 일'", "'진행 중'"),
        &[task],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_move_task($1,'완료',NULL,NULL,@ids)",
        &[task],
    )
    .await;
    let feed = rows(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.app_target_activity('item',$1,200,NULL,NULL)",
        &[w.item],
    )
    .await;
    for (kind, target) in [
        ("item", w.item),
        ("task", task),
        ("document", doc),
        ("event", event),
    ] {
        let found: Vec<_> = feed
            .iter()
            .filter(|r| {
                r["kind"] == "create"
                    && r["target_kind"] == kind
                    && r["target_id"] == target.to_string()
            })
            .collect();
        assert_eq!(found.len(), 1);
        assert!(found[0]["detail"]["title"].is_string());
    }
    let get = |kind: &str| {
        feed.iter()
            .find(|r| r["kind"] == kind && r["target_id"] == task.to_string())
            .unwrap()
    };
    assert_eq!(
        get("comment")["detail"],
        serde_json::json!({"comment":comment,"reply":false})
    );
    assert_eq!(
        get("status")["detail"],
        serde_json::json!({"from":"할 일","to":"진행 중","from_category":"todo","to_category":"doing"})
    );
    assert_eq!(
        get("kanban_move")["detail"],
        serde_json::json!({"from":"진행 중","to":"완료","before":null})
    );
    for kind in ["assign", "unassign"] {
        assert_eq!(get(kind)["detail"]["user"], member.id.0.to_string());
    }
    let ws2 = workspace(&w.state, &w.owner, w.org).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT * FROM fvoci.app_move_document($1,$2,NULL,@ids)",
        &[doc, ws2],
    )
    .await;
    let moved = one(
        &w.state,
        &w.owner,
        "SELECT target_kind,detail FROM fvoci.activity WHERE target_id=$1 AND kind='move'",
        &[doc],
    )
    .await;
    assert_eq!(moved["target_kind"], "document");
    assert_eq!(moved["detail"]["from_workspace"], w.ws.to_string());
    assert_eq!(moved["detail"]["from_item"], w.item.to_string());
    let assigned = made(&w.state,&w.owner,"SELECT fvoci.app_create_task($1,$2,'assigned',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",&[w.item,Uuid::now_v7(),member.id.0]).await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_remove_member($1,$2,@ids)",
        &[w.ws, member.id.0],
    )
    .await;
    let removed = one(
        &w.state,
        &w.owner,
        "SELECT target_kind,detail FROM fvoci.activity WHERE target_id=$1 AND kind='unassign'",
        &[assigned],
    )
    .await;
    assert_eq!(removed["target_kind"], "task");
    assert_eq!(removed["detail"]["user"], member.id.0.to_string());
    assert_eq!(removed["detail"]["reason"], "remove");
}

#[tokio::test]
async fn activity_rls_private_items_and_recent_organization_scope() {
    let w = World::new().await;
    let member = w.member("member").await;
    let outsider = common::login(&w.state, "same org outside space").await;
    common::join_organization(w.org, &outsider, "member").await;
    let task = w.task().await;
    let private = made(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_create_item($1,$2,'private','',@ids) AS id",
        &[w.module, Uuid::now_v7()],
    )
    .await;
    assert!(
        rows(
            &w.state,
            &outsider,
            "SELECT * FROM fvoci.activity WHERE workspace_id=$1",
            &[w.ws]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &member,
            "SELECT * FROM fvoci.activity WHERE item_id=$1",
            &[private]
        )
        .await
        .is_empty()
    );
    let visible = rows(
        &w.state,
        &member,
        "SELECT * FROM fvoci.app_target_activity('item',$1,100,NULL,NULL)",
        &[w.item],
    )
    .await;
    assert!(visible.iter().any(|r| r["target_id"] == task.to_string()));
    let scoped = rows(
        &w.state,
        &member,
        "SELECT * FROM fvoci.app_recent_activity($1,100)",
        &[w.org],
    )
    .await;
    assert_eq!(scoped.len(), visible.len());
    let other = World::new().await;
    assert!(
        rows(
            &w.state,
            &member,
            "SELECT * FROM fvoci.app_recent_activity($1,100)",
            &[other.org]
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn activity_retention_handles_single_equal_conflicting_and_absent_settings() {
    retention(
        "activity",
        "activity_retention_days",
        "app_system_purge_activity",
    )
    .await;
}

#[tokio::test]
async fn audit_retention_handles_single_equal_conflicting_and_absent_settings() {
    retention(
        "audit_log",
        "audit_retention_days",
        "app_system_purge_audit",
    )
    .await;
}

async fn retention(table: &str, setting: &str, purge: &str) {
    // 개정 2026-10-26(⑥, D-124): 조직 값이 상한이고 실효값은 최솟값이다(org 8·ws 7 → 7). 둘 다 없으면 서버 기본값.
    // 각 삭제를 앱 역할 트랜잭션 안에서 관찰하고 롤백: 동시에 실행되는 다른 보존 시험을 건드리지 않는다.
    for (org, ws, default, deleted) in [
        ("7", "null", "NULL", true),
        ("null", "7", "NULL", true),
        ("7", "7", "NULL", true),
        ("8", "7", "NULL", true),
        ("null", "null", "NULL", false),
        ("null", "null", "7", true),
    ] {
        let w = World::new().await;
        rows(
            &w.state,
            &w.owner,
            &format!(
                "SELECT fvoci.app_set_organization_settings($1,'{{\"{setting}\":{org}}}',@ids)"
            ),
            &[w.org],
        )
        .await;
        rows(
            &w.state,
            &w.owner,
            &format!("SELECT fvoci.app_set_workspace_limits($1,'{{\"{setting}\":{ws}}}',@ids)"),
            &[w.ws],
        )
        .await;
        // 오래된 행을 전역 worker가 볼 수 없게 같은 트랜잭션에서 준비하고 지운다. 표 소유 역할(마이그레이션 역할)을 쓴다:
        // 셀프호스트 Supabase의 관리자(postgres)는 superuser가 아니라 표를 고칠 수 없고, 정리 함수는 사용자 문맥이 없는
        // 시스템 호출(app_user_id() IS NULL)이면 되므로 소유 역할로 불러도 같다.
        use sqlx::Connection;
        let fixture_url =
            std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via cargo xtask test");
        let mut fixture = sqlx::PgConnection::connect(&fixture_url).await.unwrap();
        let mut tx = fixture.begin().await.unwrap();
        sqlx::query(&format!(
            "UPDATE fvoci.{table} SET created_at=now()-interval '40 days' WHERE workspace_id=$1"
        ))
        .bind(w.ws)
        .execute(&mut *tx)
        .await
        .unwrap();
        sqlx::query(&format!("SELECT fvoci.{purge}({default})"))
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("SELECT set_config('app.user_id',$1,true)")
            .bind(w.owner.id.0.to_string())
            .execute(&mut *tx)
            .await
            .unwrap();
        let remaining = on(
            &mut tx,
            &format!("SELECT id FROM fvoci.{table} WHERE workspace_id=$1"),
            &[w.ws],
        )
        .await
        .unwrap();
        assert_eq!(
            remaining.is_empty(),
            deleted,
            "table={table}, org={org}, ws={ws}, default={default}"
        );
        tx.rollback().await.unwrap();
    }
    // 워크스페이스 값은 조직 값을 넘지 못한다(22023 → 400). 조직 값이 없으면 제한이 없다.
    let w = World::new().await;
    rows(
        &w.state,
        &w.owner,
        &format!("SELECT fvoci.app_set_workspace_limits($1,'{{\"{setting}\":30}}',@ids)"),
        &[w.ws],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        &format!("SELECT fvoci.app_set_organization_settings($1,'{{\"{setting}\":7}}',@ids)"),
        &[w.org],
    )
    .await;
    denied(
        &w.state,
        &w.owner,
        &format!("SELECT fvoci.app_set_workspace_limits($1,'{{\"{setting}\":8}}',@ids)"),
        &[w.ws],
        "22023",
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        &format!("SELECT fvoci.app_set_workspace_limits($1,'{{\"{setting}\":7}}',@ids)"),
        &[w.ws],
    )
    .await;
}
