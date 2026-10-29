#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 스톱워치·개인 시간 기록 (D-08, D-32, D-50).

use crate::common;

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
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
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
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;

    let body =
        json!({ "request_id": new_request_id(), "kind": "study", "memo": "통계", "target": null });
    let (status, timer) = start(&app, &member, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{timer:?}");
    assert_eq!(timer["ended_at"], Value::Null);
    // 같은 요청 재시도는 같은 기록이고 아무것도 멈추지 않는다. 다른 시작은 409가 아니라 실행 중 기록을 서버가 멈추고 새로
    // 시작한다(D-117, 모든 워크스페이스 통틀어 하나).
    let retry = start(&app, &member, body).await.1;
    assert_eq!(retry["id"], timer["id"]);
    assert_eq!(retry["stopped_entry_id"], Value::Null, "{retry:?}");
    let other =
        json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": null });
    let (status, switched) = start(&app, &member, other).await;
    assert_eq!(status, StatusCode::CREATED, "{switched:?}");
    assert_ne!(switched["id"], timer["id"]);
    assert_eq!(switched["stopped_entry_id"], timer["id"]);
    let cur = send(&app, Method::GET, "/api/me/timer", Some(&member), None).await;
    assert_eq!(cur.json["id"], switched["id"]);
    {
        let mut tx = state.db.begin_as(member.id).await.unwrap();
        let ended: bool =
            sqlx::query_scalar("SELECT ended_at IS NOT NULL FROM time_entries WHERE id = $1::uuid")
                .bind(timer["id"].as_str().unwrap())
                .fetch_one(tx.conn())
                .await
                .unwrap();
        assert!(ended, "the switch did not stop the first timer");
    }
    // 멈춘 첫 기록은 이어갈 수 없다. 이후는 새 기록으로 계속한다.
    assert_eq!(
        act(&app, &member, &timer, "resume").await.0,
        StatusCode::CONFLICT
    );
    let timer = switched;

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

    // 동시에 두 타이머를 시작하면 둘 다 받아들이되(뒤의 시작이 앞의 것을 멈춘다, D-117) 실행 중 기록은 결국 하나다.
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
    assert_eq!((a, b), (StatusCode::CREATED, StatusCode::CREATED));
    let mut tx = state.db.begin_as(member.id).await.unwrap();
    let running: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM time_entries WHERE user_id = app_user_id() AND ended_at IS NULL AND deleted_at IS NULL",
    )
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(running, 1, "exactly one timer keeps running");
}

#[tokio::test]
async fn entries_are_edited_with_history_and_summed_in_the_viewers_days() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "집계").await;
    let (owner_of_private, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;
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
    assert_eq!(a.json["target"], json!({"kind":"task", "id":task["id"]}));
    let targets = send(
        &app,
        Method::GET,
        &format!(
            "/api/time-entry-targets?entry_id={}",
            a.json["id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(targets.json[0]["label"], "TIM-2 분석");
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
            // 서울 23:30~00:30 공부는 자정에서 나눠 센다(D-81). 기록은 하나 그대로다.
            ("2026-10-20".to_owned(), "study".to_owned(), 1800),
            ("2026-10-21".to_owned(), "lecture".to_owned(), 1800),
            ("2026-10-21".to_owned(), "study".to_owned(), 1800),
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
    // 한 주를 일요일부터 보면 주의 첫날이 바뀐다(표시 설정, 기록은 그대로).
    let sunday = send(
        &app,
        Method::GET,
        "/api/me/time-summary?from=2026-10-19&to=2026-11-01&tz=Asia/Seoul&group=week&week_start=sun",
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        rows(&sunday.json),
        [
            ("2026-10-18".to_owned(), "lecture".to_owned(), 1800),
            ("2026-10-18".to_owned(), "study".to_owned(), 3600),
            ("2026-10-25".to_owned(), "rest".to_owned(), 900),
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
    // 일시정지가 있는 기록을 자정에서 나눠도 합은 기록의 초와 같다(경과 비율: 60분·30분 → 3200초·1600초).
    let split = rows(&summary("day", "Asia/Seoul").await);
    let study: Vec<i64> = split
        .iter()
        .filter(|r| r.1 == "study")
        .map(|r| r.2)
        .collect();
    assert_eq!(study, [3200, 1600]);
    assert_eq!(study.iter().sum::<i64>(), 90 * 60 - 600);
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

/// 업무 시간 합계는 사람마다 자기 기록만 본다(D-117: 합계 공유 없음). 볼 수 없는 업무는 404.
#[tokio::test]
async fn task_totals_show_only_the_viewers_own_entries() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "합계").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let task = task_in_public_item(&app, &admin, &ws).await;
    let on_task =
        |s: &str, e: &str| manual(s, e, "work", json!({ "kind": "task", "id": task["id"] }));
    for (user, from, to) in [
        (&admin, "2026-10-21T09:00:00Z", "2026-10-21T10:00:00Z"),
        (&other, "2026-10-21T09:00:00Z", "2026-10-21T09:30:00Z"),
    ] {
        let r = send(
            &app,
            Method::POST,
            "/api/me/time-entries",
            Some(user),
            Some(on_task(from, to)),
        )
        .await;
        assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    }
    // 진행 중인 타이머도 남의 합계에 들어가지 않는다(시작 시각이 드러나지 않게).
    let r = send(&app, Method::POST, "/api/me/timer", Some(&other),
                 Some(json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": { "kind": "task", "id": task["id"] } }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let time = |user: TestUser| {
        let app = app.clone();
        let uri = format!("/api/tasks/{}/time", task["id"].as_str().unwrap());
        async move { send(&app, Method::GET, &uri, Some(&user), None).await }
    };
    let mine = time(admin.clone()).await;
    assert_eq!(mine.status, StatusCode::OK, "{:?}", mine.json);
    assert_eq!(mine.json["mine_seconds"], 3600);
    assert!(mine.json.get("shared_seconds").is_none(), "{:?}", mine.json);
    assert_eq!(time(other.clone()).await.json["mine_seconds"], 1800);
    assert_eq!(time(reader.clone()).await.json["mine_seconds"], 0);
    assert_eq!(time(outsider.clone()).await.status, StatusCode::NOT_FOUND);
}

/// 항목의 시간 합계(D-78·D-117): 항목과 그 아래 업무의 기록을 종류별로, 내 기록만. 취미 종류.
#[tokio::test]
async fn item_totals_group_entries_under_the_item_by_kind() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 시간").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let task = task_in_public_item(&app, &admin, &ws).await;
    let item = task["item_id"].as_str().unwrap().to_owned();
    let post = |user: TestUser, body: Value| {
        let app = app.clone();
        async move {
            let r = send(
                &app,
                Method::POST,
                "/api/me/time-entries",
                Some(&user),
                Some(body),
            )
            .await;
            assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
        }
    };
    let on_task = json!({ "kind": "task", "id": task["id"] });
    let on_item = json!({ "kind": "item", "id": item });
    post(
        admin.clone(),
        manual(
            "2026-10-21T09:00:00Z",
            "2026-10-21T10:00:00Z",
            "work",
            on_task.clone(),
        ),
    )
    .await;
    post(
        admin.clone(),
        manual(
            "2026-10-21T11:00:00Z",
            "2026-10-21T11:30:00Z",
            "hobby",
            on_item.clone(),
        ),
    )
    .await;
    post(
        admin.clone(),
        manual(
            "2026-10-21T12:00:00Z",
            "2026-10-21T12:20:00Z",
            "study",
            on_item.clone(),
        ),
    )
    .await;
    // 대상 없는 기록은 어느 항목에도 들지 않는다.
    post(
        admin.clone(),
        manual(
            "2026-10-21T13:00:00Z",
            "2026-10-21T14:00:00Z",
            "work",
            Value::Null,
        ),
    )
    .await;
    post(
        other.clone(),
        manual(
            "2026-10-21T09:00:00Z",
            "2026-10-21T09:30:00Z",
            "work",
            on_task,
        ),
    )
    .await;

    let uri = format!("/api/items/{item}/time");
    let kinds = |v: &Value| -> Vec<(String, i64)> {
        v["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| {
                (
                    k["kind"].as_str().unwrap().to_owned(),
                    k["mine_seconds"].as_i64().unwrap(),
                )
            })
            .collect()
    };
    let mine = send(&app, Method::GET, &uri, Some(&admin), None).await;
    assert_eq!(mine.status, StatusCode::OK, "{:?}", mine.json);
    assert_eq!(mine.json["estimate_minutes"], 0);
    assert_eq!(
        kinds(&mine.json),
        vec![
            ("hobby".to_owned(), 1800),
            ("study".to_owned(), 1200),
            ("work".to_owned(), 3600),
        ]
    );
    // 남의 기록은 종류도 드러나지 않는다(값이 0인 줄도 없다, D-32). 다른 멤버는 자기 기록만 본다.
    assert_eq!(
        kinds(&send(&app, Method::GET, &uri, Some(&other), None).await.json),
        vec![("work".to_owned(), 1800)]
    );
    let seen_by_reader = send(&app, Method::GET, &uri, Some(&reader), None).await;
    assert_eq!(
        seen_by_reader.status,
        StatusCode::OK,
        "{:?}",
        seen_by_reader.json
    );
    assert_eq!(kinds(&seen_by_reader.json), vec![]);
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

/// 감사 델타 SD3 (P-27): 진행 중 타이머의 시작을 옮겨 누적 멈춤보다 짧은 구간을 만들면 거절한다. 전에는 받아서 길이가 0이
/// 됐고, 그 상태로 멈추면 멈춤이 구간보다 긴 끝난 기록이 남아 메모만 고쳐도 400이 났다. 그렇게 이미 남은 기록도 메모는
/// 고칠 수 있고, 시각·멈춤을 고칠 때는 지금처럼 범위를 본다.
#[tokio::test]
async fn a_running_timer_keeps_its_pauses_inside_its_span() {
    use time::format_description::well_known::Rfc3339;
    use time::{Duration, OffsetDateTime};
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "멈춤 범위").await;
    let (me, _) = add_member(&app, &state, &admin, &ws, "pauser", "member").await;
    let body =
        json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": null });
    let (status, timer) = start(&app, &me, body).await;
    assert_eq!(status, StatusCode::CREATED, "{timer:?}");
    let id = timer["id"].as_str().unwrap().to_owned();
    // 두 시간 전에 시작해 한 시간을 멈췄다가 다시 시작한 타이머로 만든다.
    act(&app, &me, &timer, "pause").await;
    common::fixture_sql(
        "UPDATE fvoci.time_entries SET started_at = now() - interval '2 hours', paused_at = now() - interval '1 hour'
         WHERE id = $1::uuid",
        &id,
    )
    .await;
    let (_, running) = act(&app, &me, &timer, "resume").await;
    assert!(
        running["paused_seconds"].as_i64().unwrap() >= 3600,
        "{running}"
    );
    let at = |v: &Value| OffsetDateTime::parse(v.as_str().unwrap(), &Rfc3339).unwrap();
    let text = |t: OffsetDateTime| json!(t.format(&Rfc3339).unwrap());
    let uri = format!("/api/time-entries/{id}");
    let put = |cur: &Value, started: Value, memo: &str| {
        let body = json!({ "base_version": cur["version"], "started_at": started, "ended_at": cur["ended_at"],
                           "paused_seconds": cur["paused_seconds"], "kind": "work", "memo": memo, "target": null });
        send(&app, Method::PUT, &uri, Some(&me), Some(body))
    };

    // 시작을 한 시간 반 뒤로 옮기면 남는 구간(30분)이 멈춤(1시간)보다 짧다.
    let started = at(&running["started_at"]);
    let late = put(&running, text(started + Duration::minutes(90)), "").await;
    assert_eq!(late.status, StatusCode::BAD_REQUEST, "{:?}", late.json);
    // 남는 구간이 멈춤보다 길면 된다.
    let ok = put(&running, text(started + Duration::minutes(30)), "").await;
    assert_eq!(ok.status, StatusCode::OK, "{:?}", ok.json);
    assert!(
        ok.json["seconds"].as_i64().unwrap() >= 29 * 60,
        "{:?}",
        ok.json
    );

    // 멈춰 있으면 멈춘 때까지의 구간과 비교한다(지금 멈춘 시간은 아직 누적에 없다).
    let (_, paused) = act(&app, &me, &timer, "pause").await;
    let paused_at = at(&paused["paused_at"]);
    let short = put(&paused, text(paused_at - Duration::minutes(50)), "").await;
    assert_eq!(short.status, StatusCode::BAD_REQUEST, "{:?}", short.json);
    let fits = put(&paused, text(paused_at - Duration::minutes(70)), "").await;
    assert_eq!(fits.status, StatusCode::OK, "{:?}", fits.json);

    // 고치기 전 서버가 받아 이미 저장된, 멈춤이 구간보다 긴 끝난 기록.
    act(&app, &me, &timer, "stop").await;
    common::fixture_sql(
        "UPDATE fvoci.time_entries SET paused_seconds = floor(extract(epoch FROM ended_at - started_at))::integer + 600
         WHERE id = $1::uuid",
        &id,
    )
    .await;
    let (_, stopped) = act(&app, &me, &timer, "stop").await;
    // 시각·멈춤을 그대로 두고 메모만 고치면 된다(데이터는 바꾸지 않는다).
    let memo = put(&stopped, stopped["started_at"].clone(), "메모").await;
    assert_eq!(memo.status, StatusCode::OK, "{:?}", memo.json);
    assert_eq!(memo.json["paused_seconds"], stopped["paused_seconds"]);
    // 시각을 고치면 범위를 본다.
    let moved = put(
        &memo.json,
        text(at(&stopped["started_at"]) + Duration::seconds(1)),
        "메모",
    )
    .await;
    assert_eq!(moved.status, StatusCode::BAD_REQUEST, "{:?}", moved.json);

    // 고치기 전 서버가 받아 이미 어긋난 진행 중 타이머(10분 전 시작, 멈춤 1시간). 시작을 그대로 두면 메모는 고칠 수 있고,
    // 시작을 옮길 때만 범위를 본다.
    let body =
        json!({ "request_id": new_request_id(), "kind": "work", "memo": "", "target": null });
    let (status, second) = start(&app, &me, body).await;
    assert_eq!(status, StatusCode::CREATED, "{second:?}");
    common::fixture_sql(
        "UPDATE fvoci.time_entries SET started_at = now() - interval '10 minutes', paused_seconds = 3600
         WHERE id = $1::uuid",
        second["id"].as_str().unwrap(),
    )
    .await;
    let running = send(&app, Method::GET, "/api/me/timer", Some(&me), None)
        .await
        .json;
    assert_eq!(running["paused_seconds"], 3600, "{running}");
    let second_uri = format!("/api/time-entries/{}", second["id"].as_str().unwrap());
    let put_second = |cur: &Value, started: Value| {
        let body = json!({ "base_version": cur["version"], "started_at": started, "ended_at": null,
                           "paused_seconds": cur["paused_seconds"], "kind": "work", "memo": "메모", "target": null });
        send(&app, Method::PUT, &second_uri, Some(&me), Some(body))
    };
    let kept = put_second(&running, running["started_at"].clone()).await;
    assert_eq!(kept.status, StatusCode::OK, "{:?}", kept.json);
    let started = at(&running["started_at"]);
    let still_short = put_second(&kept.json, text(started - Duration::minutes(5))).await;
    assert_eq!(
        still_short.status,
        StatusCode::BAD_REQUEST,
        "{:?}",
        still_short.json
    );
    let long_enough = put_second(&kept.json, text(started - Duration::hours(2))).await;
    assert_eq!(long_enough.status, StatusCode::OK, "{:?}", long_enough.json);
}

/// 감사 S21·S20: 0시가 두 번 오는 날(아바나 2025-11-02, 05:00Z에 01:00 → 00:00)의 앞쪽 0시~1시 기록이 집계에서
/// 빠지거나 줄었고, 목록은 그 기록을 전날에 두었다. 하루는 그날 0시가 처음 오는 시점부터다. 또 아주 긴 기록도 조회
/// 기간의 날만큼만 나눠 같은 값을 낸다.
#[tokio::test]
async fn days_start_at_the_first_midnight_and_long_entries_split_within_the_range() {
    let state = common::state();
    let app = common::app(&state);
    let user = login(&state, "havana").await;
    let add = |started: &str, ended: &str| {
        let app = app.clone();
        let user = user.clone();
        let body = manual(started, ended, "study", Value::Null);
        async move {
            let r = send(
                &app,
                Method::POST,
                "/api/me/time-entries",
                Some(&user),
                Some(body),
            )
            .await;
            assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
            r.json
        }
    };
    let get = |uri: String| {
        let app = app.clone();
        let user = user.clone();
        async move {
            let r = send(&app, Method::GET, &uri, Some(&user), None).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json
        }
    };
    let rows = |v: &Value| -> Vec<(String, i64)> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["period"].as_str().unwrap().to_owned(),
                    r["seconds"].as_i64().unwrap(),
                )
            })
            .collect()
    };

    // 앞쪽 00:30~00:50(CDT) 20분과, 앞쪽 00:30부터 뒤쪽 01:00(CST)까지 90분.
    add("2025-11-02T04:30:00Z", "2025-11-02T04:50:00Z").await;
    add("2025-11-02T04:30:00Z", "2025-11-02T06:00:00Z").await;
    let day = get(
        "/api/me/time-summary?from=2025-10-27&to=2025-11-02&tz=America/Havana&group=day".into(),
    )
    .await;
    assert_eq!(rows(&day), [("2025-11-02".to_owned(), 1200 + 5400)]);
    let listed =
        get("/api/me/time-entries?from=2025-11-02&to=2025-11-02&tz=America/Havana".into()).await;
    assert_eq!(listed.as_array().unwrap().len(), 2, "{listed:?}");
    let before =
        get("/api/me/time-entries?from=2025-11-01&to=2025-11-01&tz=America/Havana".into()).await;
    assert!(before.as_array().unwrap().is_empty(), "{before:?}");

    // 0001년부터 9999년까지의 기록: 어느 기간을 물어도 그 기간의 날만큼(UTC 하루 86400초)만 나눈다.
    add("0001-01-01T00:00:00Z", "9999-12-31T23:59:59Z").await;
    let week =
        get("/api/me/time-summary?from=2026-03-02&to=2026-03-08&tz=UTC&group=day".into()).await;
    let week = rows(&week);
    assert_eq!(week.len(), 7, "{week:?}");
    assert!(week.iter().all(|(_, s)| *s == 86_400), "{week:?}");
    assert_eq!(week[0].0, "2026-03-02");
}
