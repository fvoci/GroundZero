#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 스톱워치·개인 시간 기록 (D-08, D-32, D-50).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn start(app: &Router, user: &TestUser, body: Value) -> (StatusCode, Value) {
    let r = send(app, Method::POST, "/api/me/timer", Some(user), Some(body)).await;
    (r.status, r.json)
}

async fn act(app: &Router, user: &TestUser, entry: &Value, action: &str) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!("/api/time-entries/{}/timer", entry["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "action": action })),
    )
    .await;
    (r.status, r.json)
}

fn manual(started: &str, ended: &str, kind: &str, target: Value) -> Value {
    json!({ "request_id": new_request_id(), "started_at": started, "ended_at": ended, "paused_seconds": 0,
            "kind": kind, "memo": "", "target": target })
}

async fn task_in_public_item(app: &Router, admin: &TestUser, ws: &str) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "T", "key": "TIM" }),
        ),
    )
    .await;
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    send(
        app,
        Method::PUT,
        &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    send(
        app,
        Method::POST,
        &format!("/api/items/{}/tasks", i.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "분석" })),
    )
    .await
    .json
}

#[tokio::test]
async fn one_timer_per_user_survives_pause_and_stays_private() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "시간").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "writer").await;

    let body =
        json!({ "request_id": new_request_id(), "kind": "study", "memo": "통계", "target": null });
    let (status, timer) = start(&app, &member, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{timer:?}");
    assert_eq!(timer["ended_at"], Value::Null);
    // 같은 요청 재시도는 같은 기록, 다른 시작은 409(모든 워크스페이스 통틀어 하나).
    assert_eq!(start(&app, &member, body).await.1["id"], timer["id"]);
    let other =
        json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": null });
    assert_eq!(start(&app, &member, other).await.0, StatusCode::CONFLICT);
    let cur = send(&app, Method::GET, "/api/me/timer", Some(&member), None).await;
    assert_eq!(cur.json["id"], timer["id"]);

    let (_, paused) = act(&app, &member, &timer, "pause").await;
    assert!(paused["paused_at"].is_string());
    let (_, paused_again) = act(&app, &member, &timer, "pause").await;
    assert_eq!(
        paused_again["version"], paused["version"],
        "pause is idempotent"
    );
    let (_, resumed) = act(&app, &member, &timer, "resume").await;
    assert_eq!(resumed["paused_at"], Value::Null);
    let (status, stopped) = act(&app, &member, &timer, "stop").await;
    assert_eq!(status, StatusCode::OK);
    assert!(stopped["ended_at"].is_string());
    assert_eq!(
        send(&app, Method::GET, "/api/me/timer", Some(&member), None)
            .await
            .json,
        Value::Null
    );
    assert_eq!(
        act(&app, &member, &timer, "resume").await.0,
        StatusCode::CONFLICT
    );

    // 워크스페이스 관리자에게도 보이지 않고 고칠 수도 없다(없는 것과 같다).
    assert_eq!(
        act(&app, &admin, &timer, "stop").await.0,
        StatusCode::NOT_FOUND
    );
    let r = send(
        &app,
        Method::GET,
        &format!(
            "/api/time-entries/{}/revisions",
            timer["id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let seen: i64 = sqlx::query_scalar("SELECT count(*) FROM time_entries")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(seen, 0);
    let audited: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE target_id = $1::uuid")
            .bind(timer["id"].as_str().unwrap())
            .fetch_one(tx.conn())
            .await
            .unwrap();
    assert_eq!(
        audited, 0,
        "personal time is not in the workspace audit log"
    );
    drop(tx);

    // 동시에 두 타이머를 시작해도 하나만 된다.
    let go = |app: Router, user: TestUser| async move {
        start(
            &app,
            &user,
            json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": null }),
        )
        .await
        .0
    };
    let (a, b) = tokio::join!(
        go(app.clone(), member.clone()),
        go(app.clone(), member.clone())
    );
    assert_eq!(
        [a, b].iter().filter(|s| **s == StatusCode::CREATED).count(),
        1,
        "{a} {b}"
    );
}

#[tokio::test]
async fn entries_are_edited_with_history_and_summed_in_the_viewers_days() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "집계").await;
    let (owner_of_private, _) = add_member(&app, &state, &admin, &ws, "other", "writer").await;
    let task = task_in_public_item(&app, &admin, &ws).await;

    // 서울 10-20 23:30 시작(UTC 14:30) 1시간 공부, 10-21 09:00 시작 30분 강의, 10-26 휴식 15분.
    let a = send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&admin),
        Some(manual(
            "2026-10-20T23:30:00+09:00",
            "2026-10-21T00:30:00+09:00",
            "study",
            json!({ "kind": "task", "id": task["id"] }),
        )),
    )
    .await;
    assert_eq!(a.status, StatusCode::CREATED, "{:?}", a.json);
    assert_eq!(a.json["seconds"], 3600);
    assert_eq!(a.json["target"]["label"], "TIM-2 분석");
    send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&admin),
        Some(manual(
            "2026-10-21T09:00:00+09:00",
            "2026-10-21T09:30:00+09:00",
            "lecture",
            Value::Null,
        )),
    )
    .await;
    send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&admin),
        Some(manual(
            "2026-10-26T12:00:00+09:00",
            "2026-10-26T12:15:00+09:00",
            "rest",
            Value::Null,
        )),
    )
    .await;
    let bad = manual(
        "2026-10-21T10:00:00+09:00",
        "2026-10-21T09:00:00+09:00",
        "work",
        Value::Null,
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(&admin),
            Some(bad)
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // 볼 수 없는 대상에는 연결할 수 없다(없는 것과 같다).
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&owner_of_private),
        Some(json!({ "request_id": new_request_id(), "title": "비밀" })),
    )
    .await;
    assert!(note.status.is_success(), "{:?}", note.json);
    let hidden = manual(
        "2026-10-21T10:00:00+09:00",
        "2026-10-21T11:00:00+09:00",
        "work",
        json!({ "kind": "document", "id": note.json["id"] }),
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(&admin),
            Some(hidden)
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    let summary = |group: &'static str, tz: &'static str| {
        let app = app.clone();
        let admin = admin.clone();
        async move {
            send(
                &app,
                Method::GET,
                &format!(
                    "/api/me/time-summary?from=2026-10-19&to=2026-11-01&tz={tz}&group={group}"
                ),
                Some(&admin),
                None,
            )
            .await
            .json
        }
    };
    let rows = |v: &Value| -> Vec<(String, String, i64)> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["period"].as_str().unwrap().to_owned(),
                    r["kind"].as_str().unwrap().to_owned(),
                    r["seconds"].as_i64().unwrap(),
                )
            })
            .collect()
    };
    assert_eq!(
        rows(&summary("day", "Asia/Seoul").await),
        [
            ("2026-10-20".to_owned(), "study".to_owned(), 3600),
            ("2026-10-21".to_owned(), "lecture".to_owned(), 1800),
            ("2026-10-26".to_owned(), "rest".to_owned(), 900),
        ]
    );
    // UTC로 보면 공부는 10-20, 강의는 10-21 00:00 UTC(서울 09:00)로 같은 날이 아니다.
    assert_eq!(rows(&summary("day", "UTC").await)[1].0, "2026-10-21");
    assert_eq!(
        rows(&summary("week", "Asia/Seoul").await),
        [
            ("2026-10-19".to_owned(), "lecture".to_owned(), 1800),
            ("2026-10-19".to_owned(), "study".to_owned(), 3600),
            ("2026-10-26".to_owned(), "rest".to_owned(), 900),
        ]
    );

    // 고치기는 기준 버전으로 막고 이전 값을 수정 이력에 남긴다.
    let uri = format!("/api/time-entries/{}", a.json["id"].as_str().unwrap());
    let edit = |version: &Value, memo: &str| {
        json!({ "base_version": version, "started_at": "2026-10-20T23:00:00+09:00",
        "ended_at": "2026-10-21T00:30:00+09:00", "paused_seconds": 600, "kind": "study", "memo": memo,
        "target": { "kind": "task", "id": task["id"] } })
    };
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(edit(&a.json["version"], "1차")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["seconds"], 90 * 60 - 600);
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(edit(&a.json["version"], "늦은 고치기")),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let history = send(
        &app,
        Method::GET,
        &format!("{uri}/revisions"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(history.json.as_array().unwrap().len(), 1);
    assert_eq!(history.json[0]["started_at"], "2026-10-20T14:30:00Z");

    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let list = send(
        &app,
        Method::GET,
        "/api/me/time-entries?from=2026-10-19&to=2026-11-01&tz=Asia/Seoul",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(list.json.as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn task_totals_include_only_people_who_chose_to_share() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "공유").await;
    let (sharer, _) = add_member(&app, &state, &admin, &ws, "sharer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let task = task_in_public_item(&app, &admin, &ws).await;
    let on_task =
        |s: &str, e: &str| manual(s, e, "work", json!({ "kind": "task", "id": task["id"] }));
    send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&admin),
        Some(on_task("2026-10-21T09:00:00Z", "2026-10-21T10:00:00Z")),
    )
    .await;
    send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&sharer),
        Some(on_task("2026-10-21T09:00:00Z", "2026-10-21T09:30:00Z")),
    )
    .await;
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/time-sharing"),
        Some(&sharer),
        Some(json!({ "enabled": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);

    let time = |user: TestUser| {
        let app = app.clone();
        let uri = format!("/api/tasks/{}/time", task["id"].as_str().unwrap());
        async move { send(&app, Method::GET, &uri, Some(&user), None).await }
    };
    let mine = time(admin.clone()).await.json;
    assert_eq!(
        (
            mine["mine_seconds"].as_i64(),
            mine["shared_seconds"].as_i64(),
            mine["shared_people"].as_i64()
        ),
        (Some(3600), Some(1800), Some(1))
    );
    let seen_by_reader = time(reader.clone()).await.json;
    assert_eq!(
        (
            seen_by_reader["mine_seconds"].as_i64(),
            seen_by_reader["shared_seconds"].as_i64()
        ),
        (Some(0), Some(1800))
    );
    assert_eq!(time(outsider.clone()).await.status, StatusCode::NOT_FOUND);

    // 진행 중인 남의 타이머는 공유 합계에 들어가지 않는다(시작·멈춤 시각이 드러나지 않게).
    let r = send(&app, Method::POST, "/api/me/timer", Some(&sharer),
                 Some(json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": { "kind": "task", "id": task["id"] } }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(time(reader.clone()).await.json["shared_seconds"], 1800);

    let r = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/time-sharing"),
        Some(&sharer),
        Some(json!({ "enabled": false })),
    )
    .await;
    assert_eq!(r.json["enabled"], false);
    assert_eq!(time(reader).await.json["shared_seconds"], 0);
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &format!("/api/workspaces/{ws}/time-sharing"),
            Some(&outsider),
            Some(json!({ "enabled": true }))
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // 워크스페이스에서 빠지면 공유 선택도 사라진다(다시 들어와도 묻지 않고 공유되지 않음).
    send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/time-sharing"),
        Some(&sharer),
        Some(json!({ "enabled": true })),
    )
    .await;
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", sharer.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
    let mut tx = state.db.begin_as(sharer.id).await.unwrap();
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM time_sharing")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    assert_eq!(left, 0);
}
