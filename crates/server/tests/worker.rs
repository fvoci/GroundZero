#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 백그라운드 작업 (O-19): 만료된 휴지통 비우기, 반복 업무 회차 만들기, 주기 작업 한 번만.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, create_doc, create_workspace, fixture_sql, login, new_request_id, send};
use fvoci_server::worker;
use serde_json::{Value, json};

async fn trashed_doc(app: &Router, user: &TestUser, ws: &str, title: &str) -> (Value, String) {
    let doc = create_doc(app, user, ws, title).await;
    let r = send(
        app,
        Method::DELETE,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(user),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    (doc, r.json["batch_id"].as_str().unwrap().to_owned())
}

#[tokio::test]
async fn the_worker_purges_only_expired_trash_and_fills_repeating_tasks() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "작업").await;
    let (old, old_batch) = trashed_doc(&app, &admin, &ws, "오래된 문서").await;
    let (_fresh, _) = trashed_doc(&app, &admin, &ws, "새 문서").await;
    fixture_sql("UPDATE fvoci.trash_batches SET deleted_at = now() - interval '31 days' WHERE id = $1::uuid", &old_batch).await;

    // 반복 업무: 매일. 내일 회차를 지운 것처럼 만들고 작업이 다시 만드는지 본다.
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "W", "key": "WRK" }),
        ),
    )
    .await
    .json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "점검" })),
    )
    .await
    .json;
    let t = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "백업 확인" })),
    )
    .await
    .json;
    let today: String = {
        let mut tx = state.db.begin_as(admin.id).await.unwrap();
        sqlx::query_scalar("SELECT ((now() AT TIME ZONE 'Asia/Seoul')::date)::text")
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    let r = send(&app, Method::PATCH, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "base_version": t["version"], "title": "백업 확인", "description": "", "status": "할 일",
                              "priority": "none", "start_date": null, "due_date": today, "estimate_minutes": null,
                              "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK);
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}/recurrence", t["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "rule": {"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null}, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let series = r.json["id"].as_str().unwrap().to_owned();
    fixture_sql("DELETE FROM fvoci.tasks WHERE series_id = $1::uuid AND occurrence_date > (now() AT TIME ZONE 'Asia/Seoul')::date", &series).await;

    // 작업은 DB 전체를 본다. 같은 DB를 쓰는 다른 시험이 남긴 묶음이나, 한국 자정을 넘기면 앞선 시험들의 매일 반복도
    // 함께 처리되므로 전체 수가 아니라 이 시험의 묶음·반복만 센다(전에는 자정에 돈 CI에서 회차 2로 실패했다).
    let report = worker::run_all_now(&state.db, None, None).await.unwrap();
    assert!(report.purged >= 1, "{report:?}");
    assert!(report.occurrences >= 1, "{report:?}");
    // 다시 돌려도 이 시험의 일은 더 없다(아래에서 회차 1개·만료 묶음만 지워졌음을 본다).
    worker::run_all_now(&state.db, None, None).await.unwrap();

    let entries = send(
        &app,
        Method::GET,
        &format!("/api/trash-batches?workspace_id={ws}"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let titles: Vec<&str> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["새 문서"]);
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    // 두 번 돌렸어도 만료 묶음은 한 번만 영구 삭제됐다(다시 지우면 감사 기록이 둘이 된다).
    let purges: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action = 'purge' AND target_id = $1::uuid",
    )
    .bind(old["id"].as_str().unwrap())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(purges, 1);
    let purge: (Option<uuid::Uuid>, bool) = sqlx::query_as(
        "SELECT actor, (detail->>'system')::boolean FROM audit_log WHERE action = 'purge' AND target_id = $1::uuid",
    )
    .bind(old["id"].as_str().unwrap())
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(purge, (None, true));
    let made: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_log WHERE action = 'create' AND detail->>'series' = $1 AND detail ? 'system'",
    )
    .bind(&series)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(made, 1);
    drop(tx);

    // 주기 작업은 간격 안에서 한 번만 돈다(여러 프로세스가 떠 있어도).
    let (a, b) = tokio::join!(
        worker::run_once(&state.db, None, None),
        worker::run_once(&state.db, None, None)
    );
    let _ = (a, b);
    assert_eq!(
        worker::run_once(&state.db, None, None).await,
        worker::TickReport::default()
    );
}

/// 같은 순간에 지울 목록에 오른 저장소 객체(버전 여럿인 파일, 파일 여럿을 한 번에 영구 삭제)는 모두 저장소에서 지워진다.
/// 지울 목록을 빈 표로 분석해 둔 뒤(오래 쓴 설치에서 흔한 통계)에는 꺼내기의 LIMIT이 행마다 다시 돌아 한 번에 여럿을
/// 목록에서 뺐고, 작업은 그중 하나만 지워 나머지 객체가 영구히 남았다. DB 전체의 대기열을 보므로 새 DB에서 시험한다.
#[tokio::test]
async fn storage_cleanup_deletes_every_object_queued_together() {
    use sqlx::Connection as _;
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin = sqlx::PgConnection::connect(&admin_url).await.unwrap();
    let fresh = common::fresh_database(&mut admin, "storage_claim").await;
    let dir = std::env::temp_dir().join(format!("fvoci-claim-{}", uuid::Uuid::now_v7().simple()));
    let dir_text = dir.to_str().unwrap().to_owned();
    let state = fvoci_server::app::build_state(common::config_with(&[
        ("DATABASE_URL", &fresh.app_url),
        ("FVOCI_STORAGE_DIR", &dir_text),
    ]))
    .unwrap();

    let keys: Vec<String> = (0..3)
        .map(|i| format!("v1/claim-{}/{i}", uuid::Uuid::now_v7().simple()))
        .collect();
    for key in &keys {
        let upload = state.storage.begin(key).await.unwrap();
        let etag = state
            .storage
            .put_part(key, &upload, 1, bytes::Bytes::from_static(b"x"))
            .await
            .unwrap();
        let part = fvoci_server::storage::Part {
            number: 1,
            etag,
            size: 1,
        };
        state.storage.complete(key, &upload, &[part]).await.unwrap();
        assert!(state.storage.digest(key).await.is_ok());
    }

    let owner_url = common::with_database(
        &std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`"),
        &fresh.name,
    );
    let mut owner = sqlx::PgConnection::connect(&owner_url).await.unwrap();
    // 한때 행이 있다가 비워진 표의 통계(오래 쓴 설치에서 흔하다: 한 쪽, 행 0).
    sqlx::raw_sql(
        "INSERT INTO fvoci.storage_deletions (storage_key) VALUES ('v1/claim-warmup');
         DELETE FROM fvoci.storage_deletions;",
    )
    .execute(&mut owner)
    .await
    .unwrap();
    sqlx::raw_sql("ANALYZE fvoci.storage_deletions")
        .execute(&mut owner)
        .await
        .unwrap();
    // 한 트랜잭션으로 올린다(영구 삭제처럼 queued_at이 같다).
    let mut tx = owner.begin().await.unwrap();
    for key in &keys {
        sqlx::query("INSERT INTO fvoci.storage_deletions (storage_key) VALUES ($1)")
            .bind(key)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();

    let report = worker::run_all_now(&state.db, None, Some(&state.storage))
        .await
        .unwrap();
    assert_eq!(report.cleaned, 3, "{report:?}");
    for key in &keys {
        assert!(
            state.storage.digest(key).await.is_err(),
            "{key} is still stored"
        );
    }
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM fvoci.storage_deletions")
        .fetch_one(&mut owner)
        .await
        .unwrap();
    assert_eq!(left, 0);
    std::fs::remove_dir_all(&dir).ok();
}

/// 감사 S12: 색인 대기열의 꺼냄은 2분짜리 시간 점유뿐이라, 첨부를 읽는 등으로 점유 시간을 넘긴 작업 프로세스가 그 사이
/// 더 새 변경(세대)을 다른 작업 프로세스가 먼저 반영한 뒤 옛 내용을 보내 덮었다. 보내기 직전에 "세대가 그대로이고 그 뒤
/// 아무도 다시 꺼내지 않은 행"만 골라 점유를 늘린다(`app_system_search_recheck`, 시도 번호로 점유를 구별). 세대가
/// 바뀌었거나 다른 작업 프로세스가 다시 꺼낸 행은 고르지 않고, 시간만 지난 행은 고른다.
#[tokio::test]
async fn indexing_rechecks_the_claim_and_generation_before_sending() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "색인 재확인").await;
    let doc = create_doc(&app, &admin, &ws, "기밀 문단이 있던 문서").await;
    let id: uuid::Uuid = doc["id"].as_str().unwrap().parse().unwrap();
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&url).await.unwrap();
    // 이 시험의 행만 다룬다: 작업 프로세스가 꺼낸 것처럼 점유하고(시도 번호가 오름) 그때의 세대와 시도 번호를 기억한다.
    let claim = |minutes: i32| {
        let owner = owner.clone();
        async move {
            sqlx::query_as::<_, (time::OffsetDateTime, i32)>(
                "UPDATE fvoci.search_queue SET claimed_until = now() + make_interval(mins => $2), task_uid = NULL,
                        task_queued_at = NULL, attempts = attempts + 1
                 WHERE kind = 'document' AND target_id = $1 RETURNING queued_at, attempts",
            )
            .bind(id)
            .bind(minutes)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };
    let recheck = |(queued, attempts): (time::OffsetDateTime, i32)| {
        let db = state.db.clone();
        async move {
            let mut tx = db.begin_anonymous().await.unwrap();
            let held: Vec<(String, uuid::Uuid)> = sqlx::query_as(
                "SELECT kind, target_id FROM app_system_search_recheck($1, $2, $3, $4)",
            )
            .bind(vec!["document"])
            .bind(vec![id])
            .bind(vec![queued])
            .bind(vec![attempts])
            .fetch_all(tx.conn())
            .await
            .unwrap();
            tx.commit().await.unwrap();
            held
        }
    };
    let mine = vec![("document".to_owned(), id)];

    // 세대가 그대로이고 아무도 다시 꺼내지 않았으면 고르고 점유를 늘린다.
    let first = claim(2).await;
    assert_eq!(recheck(first).await, mine);
    // 그 사이 사용자가 고쳐 다시 대기열에 오르면(세대가 바뀌고 시도 번호가 0으로) 옛 세대로는 고르지 않는다(옛 내용을
    // 보내지 않는다).
    let newer: time::OffsetDateTime = sqlx::query_scalar(
        "UPDATE fvoci.search_queue SET queued_at = queued_at + interval '1 second', attempts = 0
         WHERE kind = 'document' AND target_id = $1 RETURNING queued_at",
    )
    .bind(id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert!(recheck(first).await.is_empty());
    // 리뷰: 보내지 않기로 한 그 행의 이 꺼냄 점유는 바로 푼다(남은 점유 시간 동안 새 세대를 아무도 꺼내지 못해 검색 반영이
    // 늦었다). drain은 재확인 뒤 따로 이 함수를 부른다.
    {
        let mut tx = state.db.begin_anonymous().await.unwrap();
        sqlx::query("SELECT app_system_search_release_stale($1, $2, $3, $4)")
            .bind(vec!["document"])
            .bind(vec![id])
            .bind(vec![first.0])
            .bind(vec![first.1])
            .execute(tx.conn())
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let lease: Option<time::OffsetDateTime> = sqlx::query_scalar(
        "SELECT claimed_until FROM fvoci.search_queue WHERE kind = 'document' AND target_id = $1",
    )
    .bind(id)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(
        lease, None,
        "the stale claim kept the newer generation locked"
    );

    // 리뷰: 점유 시각만 지나고 아무도 다시 꺼내지 않았으면 그대로 보낸다(첨부가 많아 준비가 2분을 넘긴 묶음이 매번 버려져
    // 시도 횟수만 올라 영영 꺼내지 않게 되지 않도록).
    let second = claim(2).await;
    assert_eq!(second.0, newer);
    sqlx::query(
        "UPDATE fvoci.search_queue SET claimed_until = now() - interval '1 second'
         WHERE kind = 'document' AND target_id = $1",
    )
    .bind(id)
    .execute(&owner)
    .await
    .unwrap();
    assert_eq!(
        recheck(second).await,
        mine,
        "an expired but untouched claim was dropped"
    );
    // 리뷰: 같은 세대를 다른 작업 프로세스가 다시 꺼냈으면(시도 번호가 바뀜) 앞 작업 프로세스는 보내지 않는다(두 번
    // 보내지 않음).
    let third = claim(2).await;
    assert_eq!(third.0, newer);
    assert!(
        recheck(second).await.is_empty(),
        "the earlier claim passed after another worker claimed the row again"
    );
    assert_eq!(recheck(third).await, mine);
    owner.close().await;
}
