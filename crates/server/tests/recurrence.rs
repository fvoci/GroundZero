#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 반복 업무·반복 일정 (D-31, D-50). 회차는 쓰기 요청에서만 만들고, 읽기는 예정 회차를 계산만 한다.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};
use time::Date;

/// 열을 쥔 채 DB 전체의 반복 회차 맞추기를 돌리는 시험끼리는 차례로 돈다. 함께 돌면 한쪽 작업이 앞 항목들의 열을 쥐고
/// 다른 쪽이 쥔 열에서 기다리고, 다른 쪽 작업(잠금 시간 300ms)은 그 항목마다 시간 초과를 기다려 반복이 쌓일수록
/// 길어졌다(W9: 항목 58개에서 17초). 그동안 열 순서 시험은 자기 열에서 기다리는 작업을 보지 못하고 실패했다.
/// 그 작업은 규칙이 있는 모든 반복을 맞추므로(회차 상한까지 따라잡기 포함) 반복을 만드는 다른 시험과도 겹치지 않게 한다.
/// 작업을 돌리는 시험은 `write`, 나머지 시험은 `read`를 쥔다(#158). 예: 따라잡기 상한 시험이 회차 51개를 세기 전에 작업이
/// 그 반복을 더 따라잡으면 수가 달라진다.
static LANE_TICK: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());

/// 상한(`app_system_recurrence_tick_ids`)을 본 뒤 다른 시험이 반복을 만들거나 회차를 지우면 뒤 반복의 몫이 모자라
/// 그 반복이 FV001 경고로 건너뛴다(작업 프로세스는 다음 주기에 다시 한다, D-88). 시험은 회차 맞추기를 한 번만 부르므로
/// id를 넉넉히 준다(남는 id는 버린다). 한 반복이 한 번에 쓰는 몫은 회차 50개 × 3 = 150개다(체크리스트 줄 없음).
const SPARE_TICK_IDS: usize = 10_000;

/// 감사 행(action, target_type, workspace_id, organization_id, owner_user_id, detail) — 어긋난 검사의 진단 출력용.
type AuditRow = (
    String,
    String,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Option<uuid::Uuid>,
    Value,
);

async fn module_item(app: &Router, admin: &TestUser, ws: &str, key: &str) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": key })),
    )
    .await;
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "base_version": 1, "title": key, "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    i.json
}

async fn dated_task(app: &Router, user: &TestUser, item: &Value, title: &str, due: &str) -> Value {
    let t = send(
        app,
        Method::POST,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await
    .json;
    let r = send(app, Method::PUT, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(user),
                 Some(json!({ "base_version": t["version"], "title": title, "description": "", "status": "할 일",
                              "priority": "none", "start_date": null, "due_date": due, "estimate_minutes": null,
                              "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

/// 반복 규칙을 처음 정한다(기준 규칙 없음).
async fn repeat(app: &Router, user: &TestUser, task: &Value, rule: Value) -> (StatusCode, Value) {
    repeat_from(app, user, task, rule, Value::Null).await
}

/// 반복 규칙을 바꾼다. `base`는 편집을 시작할 때 본 규칙이다(D-125: 지금 규칙이 기준과도 새 규칙과도 다르면 409).
async fn repeat_from(
    app: &Router,
    user: &TestUser,
    task: &Value,
    rule: Value,
    base: Value,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/tasks/{}/recurrence", task["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "rule": rule, "timezone": "Asia/Seoul", "base": base })),
    )
    .await;
    (r.status, r.json)
}

async fn tasks(app: &Router, user: &TestUser, item: &Value) -> Vec<Value> {
    send(
        app,
        Method::GET,
        &format!("/api/items/{}/tasks", item["id"].as_str().unwrap()),
        Some(user),
        None,
    )
    .await
    .json
    .as_array()
    .unwrap()
    .clone()
}

async fn seoul_today(state: &fvoci_server::http::AppState, user: &TestUser) -> Date {
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Seoul')::date")
        .fetch_one(tx.conn())
        .await
        .unwrap()
}

fn day(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

#[tokio::test]
async fn rules_expand_with_skips_counts_and_limits() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let admin = login(&state, "admin").await;
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let cases = [
        // 매달 31일: 31일이 없는 달은 건너뛴다.
        (
            json!({"freq":"monthly","interval":1,"weekdays":[],"until":null,"count":null}),
            "2026-01-31",
            "2026-01-01",
            "2026-06-30",
            vec!["2026-01-31", "2026-03-31", "2026-05-31"],
        ),
        // 2월 29일 매년: 윤년만.
        (
            json!({"freq":"yearly","interval":1,"weekdays":[],"until":null,"count":null}),
            "2024-02-29",
            "2024-01-01",
            "2032-12-31",
            vec!["2024-02-29", "2028-02-29", "2032-02-29"],
        ),
        // 격주 월·목, 횟수 4(첫 회차 포함).
        (
            json!({"freq":"weekly","interval":2,"weekdays":[1,4],"until":null,"count":4}),
            "2026-10-05",
            "2026-10-01",
            "2026-12-31",
            vec!["2026-10-05", "2026-10-08", "2026-10-19", "2026-10-22"],
        ),
        // 기준일(월)이 규칙 요일(화·목)이 아니어도 첫 회차이고 횟수에 든다.
        (
            json!({"freq":"weekly","interval":1,"weekdays":[2,4],"until":null,"count":3}),
            "2026-10-05",
            "2026-10-01",
            "2026-12-31",
            vec!["2026-10-05", "2026-10-06", "2026-10-08"],
        ),
        // 먼 기간도 회차 단위로만 계산한다(횟수 없는 매일 규칙의 2199년 한 주).
        (
            json!({"freq":"daily","interval":7,"weekdays":[],"until":null,"count":null}),
            "1900-01-01",
            "2199-12-25",
            "2199-12-31",
            vec!["2199-12-30"],
        ),
        // 사흘마다, 끝 날짜까지.
        (
            json!({"freq":"daily","interval":3,"weekdays":[],"until":"2026-10-10","count":null}),
            "2026-10-01",
            "2026-10-03",
            "2026-12-31",
            vec!["2026-10-04", "2026-10-07", "2026-10-10"],
        ),
    ];
    for (rule, anchor, from, to, want) in cases {
        let rule = rule.to_string();
        let got: Vec<Date> = sqlx::query_scalar(
            "SELECT occurrence FROM app_recurrence_dates($1::jsonb, $2::date, $3::date, $4::date)",
        )
        .bind(&rule)
        .bind(anchor)
        .bind(from)
        .bind(to)
        .fetch_all(tx.conn())
        .await
        .unwrap();
        assert_eq!(got.into_iter().map(day).collect::<Vec<_>>(), want, "{rule}");
    }
}

/// #76: 반복 끝이 첫 회차보다 앞서면 400 `bad_request`에 칸(`until`)과 안정된 이유를 싣는다. 업무 반복과 일정이 같은 서버
/// 오류 대응을 쓴다(일정 쪽은 `moving_a_repeating_event_past_its_until_is_refused`).
#[tokio::test]
async fn a_task_rule_ending_before_its_first_occurrence_names_the_until_field() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반복 끝").await;
    let item = module_item(&app, &admin, &ws, "UNT").await;
    let root = dated_task(&app, &admin, &item, "월간 정산", "2026-11-10").await;
    let early =
        json!({"freq":"weekly","interval":1,"weekdays":[],"until":"2026-11-01","count":null});
    let (status, body) = repeat(&app, &admin, &root, early).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"], "bad_request");
    assert_eq!(body["reason"], "until_before_first_occurrence");
    assert_eq!(body["field"], "until");
    assert_eq!(
        body["message"],
        "until must not be before the first occurrence"
    );
}

#[tokio::test]
async fn repeating_tasks_keep_open_occurrences_and_follow_the_original_schedule() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반복").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let item = module_item(&app, &admin, &ws, "REP").await;
    let today = seoul_today(&state, &admin).await;
    let anchor = today - time::Duration::days(3);
    let root = dated_task(&app, &admin, &item, "주간 보고", &day(anchor)).await;

    let daily = json!({"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null});
    let (status, detail) = repeat(&app, &admin, &root, daily.clone()).await;
    assert_eq!(status, StatusCode::OK, "{detail:?}");
    assert_eq!(detail["series"]["rule"]["freq"], "daily");
    // 기준일~오늘(4개)과 다음 회차 하나.
    let all = tasks(&app, &admin, &item).await;
    let mut dates: Vec<String> = all
        .iter()
        .map(|t| t["due_date"].as_str().unwrap().to_owned())
        .collect();
    dates.sort();
    let want: Vec<String> = (0..=4)
        .map(|i| day(anchor + time::Duration::days(i)))
        .collect();
    assert_eq!(dates, want);

    // 회차 하나를 끝내도 다른 미완료 회차는 남고, 다시 맞춰도 새로 생기지 않는다(같은 요청 반복·동시 요청).
    let first_open = all
        .iter()
        .find(|t| t["occurrence_date"] == json!(day(anchor + time::Duration::days(1))))
        .unwrap();
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{}/position", first_open["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "status": "완료", "before_id": null })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let sync = |app: Router, user: TestUser, item: Value| async move {
        send(
            &app,
            Method::POST,
            &format!(
                "/api/items/{}/recurrences/sync",
                item["id"].as_str().unwrap()
            ),
            Some(&user),
            None,
        )
        .await
    };
    let (a, b) = tokio::join!(
        sync(app.clone(), admin.clone(), item.clone()),
        sync(app.clone(), admin.clone(), item.clone())
    );
    assert_eq!(
        (a.status, b.status),
        (StatusCode::OK, StatusCode::OK),
        "{:?} {:?}",
        a.json,
        b.json
    );
    assert_eq!(
        a.json["created"].as_i64().unwrap() + b.json["created"].as_i64().unwrap(),
        0
    );
    assert_eq!(tasks(&app, &admin, &item).await.len(), 5);

    // 독자는 회차를 만들 수 없다.
    assert_eq!(
        sync(app.clone(), reader.clone(), item.clone()).await.status,
        StatusCode::FORBIDDEN
    );

    // 틀(첫 회차)을 고쳐도 이미 만든 회차는 그대로다.
    let fresh = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{}", root["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json;
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}", root["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "base_version": fresh["version"], "title": "월간 보고", "description": "", "status": "할 일",
                              "priority": "none", "start_date": null, "due_date": fresh["due_date"], "estimate_minutes": null,
                              "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK);
    let titles: Vec<String> = tasks(&app, &admin, &item)
        .await
        .iter()
        .map(|t| t["title"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        titles.iter().filter(|t| *t == "주간 보고").count(),
        4,
        "{titles:?}"
    );

    // 읽기(달력)는 예정 회차를 보여 주되 아무것도 만들지 않는다.
    let audit_before: i64 = {
        let mut tx = state.db.begin_as(admin.id).await.unwrap();
        sqlx::query_scalar("SELECT count(*) FROM audit_log")
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    let from = today + time::Duration::days(5);
    let cal = send(
        &app,
        Method::GET,
        &format!(
            "/api/workspaces/{ws}/calendar?from={}&to={}&tz=Asia/Seoul",
            day(from),
            day(from + time::Duration::days(2))
        ),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(cal.status, StatusCode::OK, "{:?}", cal.json);
    assert_eq!(cal.json["projected_tasks"].as_array().unwrap().len(), 3);
    assert_eq!(cal.json["projected_tasks"][0]["title"], "월간 보고");
    assert_eq!(tasks(&app, &admin, &item).await.len(), 5);
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let audit_after: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    // 어긋나면 무엇이 쓰였는지 보인다(읽기가 쓰지 않았음을 확인하는 시험이라 원인이 바로 보여야 한다).
    let latest: Vec<AuditRow> = sqlx::query_as(
            "SELECT action, target_type, workspace_id, organization_id, owner_user_id, detail FROM audit_log
             ORDER BY created_at DESC, id DESC LIMIT 4",
        )
        .fetch_all(tx.conn())
        .await
        .unwrap();
    assert_eq!(
        audit_before, audit_after,
        "a calendar read must not add audit rows; latest rows: {latest:?}"
    );
    drop(tx);

    // 멈추면 이미 만든 회차는 남고 예정도 사라진다.
    let (status, stopped) = repeat_from(&app, &admin, &root, Value::Null, daily.clone()).await;
    assert_eq!(status, StatusCode::OK, "{stopped:?}");
    assert_eq!(stopped["series"]["rule"], Value::Null);
    let cal = send(
        &app,
        Method::GET,
        &format!(
            "/api/workspaces/{ws}/calendar?from={}&to={}&tz=Asia/Seoul",
            day(from),
            day(from + time::Duration::days(2))
        ),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(cal.json["projected_tasks"], json!([]));
    assert_eq!(tasks(&app, &admin, &item).await.len(), 5);
}

#[tokio::test]
async fn catch_up_is_capped_per_request_and_archived_items_refuse() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "따라잡기").await;
    let item = module_item(&app, &admin, &ws, "CAP").await;
    let today = seoul_today(&state, &admin).await;
    let root = dated_task(
        &app,
        &admin,
        &item,
        "점검",
        &day(today - time::Duration::days(119)),
    )
    .await;
    let (status, _) = repeat(
        &app,
        &admin,
        &root,
        json!({"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(tasks(&app, &admin, &item).await.len(), 51);
    let uri = format!(
        "/api/items/{}/recurrences/sync",
        item["id"].as_str().unwrap()
    );
    assert_eq!(
        send(&app, Method::POST, &uri, Some(&admin), None)
            .await
            .json["created"],
        50
    );
    // 남은 20개(기준일 포함 120일 + 다음 하나 = 121개 중).
    assert_eq!(
        send(&app, Method::POST, &uri, Some(&admin), None)
            .await
            .json["created"],
        20
    );
    assert_eq!(
        send(&app, Method::POST, &uri, Some(&admin), None)
            .await
            .json["created"],
        0
    );

    let (status, _) = repeat(
        &app,
        &admin,
        &root,
        json!({"freq":"daily","interval":0,"weekdays":[],"until":null,"count":null}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/archived", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(
        send(&app, Method::POST, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn repeating_events_keep_their_wall_clock_across_daylight_saving() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반복 일정").await;
    let item = module_item(&app, &admin, &ws, "EVR").await;
    let r = send(&app, Method::POST, &format!("/api/items/{}/events", item["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "request_id": new_request_id(), "kind": "meeting", "title": "뉴욕 회의", "description": "",
                              "all_day": false, "start_date": null, "end_date": null,
                              "starts_at": "2026-10-26T09:00:00-04:00", "ends_at": "2026-10-26T10:00:00-04:00",
                              "timezone": "America/New_York" }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let event = r.json;
    let uri = format!("/api/events/{}", event["id"].as_str().unwrap());
    let set = send(&app, Method::PUT, &format!("{uri}/recurrence"), Some(&admin),
                   Some(json!({ "rule": {"freq":"weekly","interval":1,"weekdays":[],"until":null,"count":null} }))).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.json);
    assert_eq!(set.json["recurrence"]["freq"], "weekly");

    let cal = |app: Router, admin: TestUser, ws: String| async move {
        send(
            &app,
            Method::GET,
            &format!(
                "/api/workspaces/{ws}/calendar?from=2026-10-26&to=2026-11-15&tz=America/New_York"
            ),
            Some(&admin),
            None,
        )
        .await
        .json
    };
    let starts = |c: &Value| -> Vec<String> {
        c["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["starts_at"].as_str().unwrap().to_owned())
            .collect()
    };
    let c = cal(app.clone(), admin.clone(), ws.clone()).await;
    // 2026-11-01 여름시간 끝: 09:00 EDT = 13:00Z, 09:00 EST = 14:00Z.
    assert_eq!(
        starts(&c),
        [
            "2026-10-26T13:00:00Z",
            "2026-11-02T14:00:00Z",
            "2026-11-09T14:00:00Z"
        ]
    );
    assert_eq!(c["events"].as_array().unwrap().len(), 1);

    let r = send(
        &app,
        Method::POST,
        &format!("{uri}/exclusions"),
        Some(&admin),
        Some(json!({ "date": "2026-11-02" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let c = cal(app.clone(), admin.clone(), ws.clone()).await;
    assert_eq!(starts(&c), ["2026-10-26T13:00:00Z", "2026-11-09T14:00:00Z"]);
}

#[tokio::test]
async fn changing_a_rule_applies_from_today_and_the_template_stays_top_level() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "규칙 변경").await;
    let item = module_item(&app, &admin, &ws, "CHG").await;
    let today = seoul_today(&state, &admin).await;
    let root = dated_task(
        &app,
        &admin,
        &item,
        "정리",
        &day(today - time::Duration::days(28)),
    )
    .await;
    let weekly = json!({"freq":"weekly","interval":1,"weekdays":[],"until":null,"count":null});
    assert_eq!(
        repeat(&app, &admin, &root, weekly.clone()).await.0,
        StatusCode::OK
    );
    // 4주 전부터 매주: 기준일 포함 5개 + 다음 하나.
    let before = tasks(&app, &admin, &item).await.len();
    assert_eq!(before, 6);
    // 매일로 바꾸면 지난 28일을 채우지 않고 오늘부터만(오늘·내일, 이미 있는 날은 그대로).
    let daily = json!({"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null});
    assert_eq!(
        repeat_from(&app, &admin, &root, daily.clone(), weekly)
            .await
            .0,
        StatusCode::OK
    );
    let after = tasks(&app, &admin, &item).await;
    let new_dates: Vec<String> = after
        .iter()
        .map(|t| t["occurrence_date"].as_str().unwrap().to_owned())
        .filter(|d| *d >= day(today))
        .collect();
    assert!(after.len() <= before + 2, "{} -> {}", before, after.len());
    assert!(
        new_dates.contains(&day(today))
            && new_dates.contains(&day(today + time::Duration::days(1))),
        "{new_dates:?}"
    );

    // 반복이 켜진 첫 회차는 하위 업무가 되거나 날짜를 모두 잃을 수 없다.
    let parent = dated_task(&app, &admin, &item, "상위", &day(today)).await;
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{}/parent", root["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "parent_id": parent["id"] })),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    let fresh = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{}", root["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json;
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}", root["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "base_version": fresh["version"], "title": "정리", "description": "", "status": "할 일",
                              "priority": "none", "start_date": null, "due_date": null, "estimate_minutes": null,
                              "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    // 끝 날짜가 첫 회차보다 앞서면 400, 터무니없는 날짜도 400.
    let early =
        json!({"freq":"daily","interval":1,"weekdays":[],"until":"2000-01-01","count":null});
    assert_eq!(
        repeat_from(&app, &admin, &root, early, daily.clone())
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let cal = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/calendar?from=9999-12-01&to=9999-12-31&tz=UTC"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(cal.status, StatusCode::BAD_REQUEST);
}

/// 반복 일정의 DST(D-81): 일정 시간대에 없는 현지 시각의 회차는 건너뛰고(건너뜀으로 표시), 두 번 있는 시각은 앞선
/// 시각에 한 번만. 보는 사람의 시간대가 달라도 회차 시점은 같다.
#[tokio::test]
async fn weekly_events_skip_missing_local_times_and_take_the_first_of_repeated_ones() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "여름시간").await;
    let item = module_item(&app, &admin, &ws, "DST").await;
    let weekly = |title: &'static str, starts: &'static str, ends: &'static str| {
        let app = app.clone();
        let admin = admin.clone();
        let item = item["id"].as_str().unwrap().to_owned();
        async move {
            let r = send(&app, Method::POST, &format!("/api/items/{item}/events"), Some(&admin),
                Some(json!({ "request_id": new_request_id(), "kind": "meeting", "title": title, "description": "",
                             "all_day": false, "start_date": null, "end_date": null,
                             "starts_at": starts, "ends_at": ends, "timezone": "America/New_York" }))).await;
            assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
            let id = r.json["id"].as_str().unwrap().to_owned();
            let set = send(&app, Method::PUT, &format!("/api/events/{id}/recurrence"), Some(&admin),
                Some(json!({ "rule": {"freq":"weekly","interval":1,"weekdays":[],"until":null,"count":null} }))).await;
            assert_eq!(set.status, StatusCode::OK, "{:?}", set.json);
            id
        }
    };
    // 일요일 02:30(뉴욕). 2026-03-08 02:00에 03:00으로 건너뛴다.
    let spring = weekly(
        "새벽 점검",
        "2026-03-01T02:30:00-05:00",
        "2026-03-01T03:00:00-05:00",
    )
    .await;
    // 일요일 01:30(뉴욕). 2026-11-01 02:00에 01:00으로 돌아가 01:30이 두 번 있다.
    let fall = weekly(
        "새벽 백업",
        "2026-10-25T01:30:00-04:00",
        "2026-10-25T02:00:00-04:00",
    )
    .await;

    let occurrences = |from: &'static str, to: &'static str, tz: &'static str, id: String| {
        let app = app.clone();
        let admin = admin.clone();
        let ws = ws.clone();
        async move {
            let r = send(
                &app,
                Method::GET,
                &format!("/api/workspaces/{ws}/calendar?from={from}&to={to}&tz={tz}"),
                Some(&admin),
                None,
            )
            .await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|o| o["event_id"] == id.as_str())
                .map(|o| {
                    (
                        o["occurrence"].as_str().unwrap().to_owned(),
                        o["starts_at"].as_str().map(str::to_owned),
                        o["skipped"].as_bool().unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        }
    };
    let march = occurrences(
        "2026-03-01",
        "2026-03-21",
        "America/New_York",
        spring.clone(),
    )
    .await;
    assert_eq!(
        march,
        vec![
            (
                "2026-03-01".to_owned(),
                Some("2026-03-01T07:30:00Z".to_owned()),
                false
            ),
            ("2026-03-08".to_owned(), None, true),
            (
                "2026-03-15".to_owned(),
                Some("2026-03-15T06:30:00Z".to_owned()),
                false
            ),
        ]
    );
    // 보는 사람의 시간대가 서울이어도 회차 시점은 같다(반복 기준은 일정의 시간대).
    let seen_from_seoul = occurrences("2026-03-01", "2026-03-21", "Asia/Seoul", spring).await;
    assert_eq!(
        seen_from_seoul
            .iter()
            .map(|o| o.1.clone())
            .collect::<Vec<_>>(),
        vec![
            Some("2026-03-01T07:30:00Z".to_owned()),
            None,
            Some("2026-03-15T06:30:00Z".to_owned())
        ]
    );
    let november = occurrences("2026-10-25", "2026-11-08", "America/New_York", fall).await;
    assert_eq!(
        november,
        vec![
            (
                "2026-10-25".to_owned(),
                Some("2026-10-25T05:30:00Z".to_owned()),
                false
            ),
            // 01:30 EDT(앞선 시각) 한 번만.
            (
                "2026-11-01".to_owned(),
                Some("2026-11-01T05:30:00Z".to_owned()),
                false
            ),
            (
                "2026-11-08".to_owned(),
                Some("2026-11-08T06:30:00Z".to_owned()),
                false
            ),
        ]
    );
}

/// 감사 S18: 반복 일정의 시작을 규칙의 끝(`until`) 뒤로 옮기면 회차가 하나도 없어 달력에서 사라졌다. 규칙을 정할 때와
/// 같이 400으로 받지 않는다. 시간대만 바꿔 첫 회차 날짜가 밀리는 경우도 같다.
#[tokio::test]
async fn moving_a_repeating_event_past_its_until_is_refused() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반복 끝").await;
    let item = module_item(&app, &admin, &ws, "EVU").await;
    let create = |body: Value| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/items/{}/events", item["id"].as_str().unwrap());
        async move {
            let mut body = body;
            body["request_id"] = json!(new_request_id());
            let r = send(&app, Method::POST, &uri, Some(&admin), Some(body)).await;
            assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
            r.json
        }
    };
    let set_rule = |event: &Value, until: &str| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/events/{}/recurrence", event["id"].as_str().unwrap());
        let rule = json!({ "rule": {"freq":"weekly","interval":1,"weekdays":[],"until":until,"count":null} });
        async move {
            let r = send(&app, Method::PUT, &uri, Some(&admin), Some(rule)).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json
        }
    };

    // 종일 일정: 11-02부터 매주, 11-30까지. 시작을 12-07로 옮기면 400이고 일정은 그대로 달력에 남는다.
    let all_day = create(
        json!({ "kind": "meeting", "title": "주간 점검", "description": "", "all_day": true,
        "start_date": "2026-11-02", "end_date": "2026-11-02", "starts_at": null, "ends_at": null,
        "timezone": "Asia/Seoul" }),
    )
    .await;
    let all_day = set_rule(&all_day, "2026-11-30").await;
    let moved = send(&app, Method::PUT, &format!("/api/events/{}", all_day["id"].as_str().unwrap()), Some(&admin),
        Some(json!({ "base_version": all_day["version"], "kind": "meeting", "title": "주간 점검", "description": "",
            "all_day": true, "start_date": "2026-12-07", "end_date": "2026-12-07", "starts_at": null,
            "ends_at": null, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(moved.status, StatusCode::BAD_REQUEST, "{:?}", moved.json);
    // #76: 웹(apps/web/src/lib/api/errors.ts의 untilBeforeStart)이 이 경우를 `error`와 `message`의 고정 값으로 알아본다.
    assert_eq!(moved.json["error"], "bad_request");
    assert_eq!(moved.json["reason"], "until_before_first_occurrence");
    assert_eq!(moved.json["field"], "until");
    assert_eq!(
        moved.json["message"],
        "until must not be before the first occurrence"
    );
    let cal = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/calendar?from=2026-11-01&to=2026-11-30&tz=Asia/Seoul"),
        Some(&admin),
        None,
    )
    .await
    .json;
    let dates: Vec<&str> = cal["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["event_id"] == all_day["id"])
        .map(|o| o["start_date"].as_str().unwrap())
        .collect();
    assert_eq!(
        dates,
        [
            "2026-11-02",
            "2026-11-09",
            "2026-11-16",
            "2026-11-23",
            "2026-11-30"
        ]
    );

    // 시각 일정: 서울 11-02 23:30에 한 번(끝 = 첫 회차 날짜). 시각은 두고 시간대만 키리티마티(UTC+14)로 바꾸면 그곳의
    // 날짜는 11-03이라 끝보다 뒤다.
    let timed = create(
        json!({ "kind": "meeting", "title": "늦은 통화", "description": "", "all_day": false,
        "start_date": null, "end_date": null, "starts_at": "2026-11-02T23:30:00+09:00",
        "ends_at": "2026-11-03T00:00:00+09:00", "timezone": "Asia/Seoul" }),
    )
    .await;
    let timed = set_rule(&timed, "2026-11-02").await;
    let zone = send(&app, Method::PUT, &format!("/api/events/{}", timed["id"].as_str().unwrap()), Some(&admin),
        Some(json!({ "base_version": timed["version"], "kind": "meeting", "title": "늦은 통화", "description": "",
            "all_day": false, "start_date": null, "end_date": null, "starts_at": "2026-11-02T23:30:00+09:00",
            "ends_at": "2026-11-03T00:00:00+09:00", "timezone": "Pacific/Kiritimati" }))).await;
    assert_eq!(zone.status, StatusCode::BAD_REQUEST, "{:?}", zone.json);
    assert_eq!(zone.json["error"], "bad_request");
    assert_eq!(zone.json["reason"], "until_before_first_occurrence");
    assert_eq!(
        zone.json["message"],
        "until must not be before the first occurrence"
    );

    // 끝 안에서 옮기는 것은 그대로 된다.
    let inside = send(&app, Method::PUT, &format!("/api/events/{}", all_day["id"].as_str().unwrap()), Some(&admin),
        Some(json!({ "base_version": all_day["version"], "kind": "meeting", "title": "주간 점검", "description": "",
            "all_day": true, "start_date": "2026-11-03", "end_date": "2026-11-03", "starts_at": null,
            "ends_at": null, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(inside.status, StatusCode::OK, "{:?}", inside.json);
}

/// 감사 W21: 일정 반복 정하기가 기준 없이 저장해, 화면을 연 뒤 다른 사람이 바꾼 규칙(수요일 추가)을 옛 규칙으로 말없이
/// 덮었다. 편집을 시작한 규칙(`base`)이 지금 규칙과 다르면 409 `rule_conflict`(최신 일정)이고 아무것도 바꾸지 않는다.
/// 규칙만 보므로 그사이 제목만 바뀐 것은 충돌이 아니고, 이미 보낸 규칙과 같으면 같은 요청의 재시도다.
#[tokio::test]
async fn event_rule_saves_from_a_stale_rule_conflict() {
    let _tick = LANE_TICK.read().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "규칙 기준").await;
    let item = module_item(&app, &admin, &ws, "EVB").await;
    let r = send(&app, Method::POST, &format!("/api/items/{}/events", item["id"].as_str().unwrap()), Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "meeting", "title": "주간 회의", "description": "",
                     "all_day": true, "start_date": "2026-11-02", "end_date": "2026-11-02", "starts_at": null,
                     "ends_at": null, "timezone": "Asia/Seoul" }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let uri = format!("/api/events/{}", r.json["id"].as_str().unwrap());
    let rule_uri = format!("{uri}/recurrence");
    let monday = json!({"freq":"weekly","interval":1,"weekdays":[1],"until":null,"count":null});
    let mon_wed = json!({"freq":"weekly","interval":1,"weekdays":[1,3],"until":null,"count":null});
    let ten_mondays = json!({"freq":"weekly","interval":1,"weekdays":[1],"until":null,"count":10});
    let put = |rule: Value, base: Value| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = rule_uri.clone();
        async move {
            send(
                &app,
                Method::PUT,
                &uri,
                Some(&admin),
                Some(json!({ "rule": rule, "base": base })),
            )
            .await
        }
    };

    // 반복이 없던 일정에 매주 월요일(기준 없음).
    let set = put(monday.clone(), Value::Null).await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.json);
    // 다른 사람이 수요일을 더한다(기준 = 월요일).
    let other = put(mon_wed.clone(), monday.clone()).await;
    assert_eq!(other.status, StatusCode::OK, "{:?}", other.json);
    // 월요일만 보던 화면이 "10회"로 저장하면 충돌이고, 수요일은 남는다.
    let stale = put(ten_mondays.clone(), monday.clone()).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    assert_eq!(stale.json["error"], "rule_conflict");
    assert_eq!(
        stale.json["current"]["recurrence"]["weekdays"],
        json!([1, 3])
    );
    // 반복이 있는데 없던 것으로 알고 보내도 충돌이다.
    let blind = put(ten_mondays.clone(), Value::Null).await;
    assert_eq!(blind.status, StatusCode::CONFLICT, "{:?}", blind.json);
    // 지금 규칙이 이미 보낸 규칙과 같으면(같은 요청 재시도) 성공이다.
    let retry = put(mon_wed.clone(), monday.clone()).await;
    assert_eq!(retry.status, StatusCode::OK, "{:?}", retry.json);
    // 리뷰: 재시도는 바꿀 것이 없어 판을 다시 올리지 않는다(올리면 응답을 잃고 다시 보낸 화면이 판이 두 번 오른 것으로
    // 보고 일정 입력의 기준을 놓쳐, 제 변경과 부딪히는 409를 받았다).
    assert_eq!(retry.json["version"], other.json["version"]);

    // 제목만 바뀐 것은 규칙 충돌이 아니다.
    let renamed = send(&app, Method::PUT, &uri, Some(&admin), Some(json!({
        "base_version": retry.json["version"], "kind": "meeting", "title": "주간 회의(이름 바꿈)", "description": "",
        "all_day": true, "start_date": "2026-11-02", "end_date": "2026-11-02", "starts_at": null, "ends_at": null,
        "timezone": "Asia/Seoul" }))).await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.json);
    let after_rename = put(ten_mondays.clone(), mon_wed.clone()).await;
    assert_eq!(
        after_rename.status,
        StatusCode::OK,
        "{:?}",
        after_rename.json
    );
    assert_eq!(after_rename.json["recurrence"]["count"], 10);
    // 멈추기도 같은 기준을 따른다.
    let stop = put(Value::Null, ten_mondays.clone()).await;
    assert_eq!(stop.status, StatusCode::OK, "{:?}", stop.json);
    assert!(stop.json["recurrence"].is_null());
}

/// 감사 S3: 반복 회차 맞추기가 반복마다 그 뿌리 업무의 열을 잠그고 끝까지 쥐어, 같은 항목에서 할 일 → 완료 순으로 잡았다.
/// 칸반 이동·항목 동기화는 완료 → 진행 중 → 할 일 순으로 잡으므로 서로 기다리다 교착이 났고, 작업 쪽 반복의 회차가 다음
/// 주기까지 빠졌다. 작업은 이제 항목의 세 열을 같은 순서로 먼저 잡는다.
#[tokio::test]
async fn the_recurrence_tick_takes_lane_locks_in_the_same_order_as_board_moves() {
    let _serial = LANE_TICK.write().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "열 잠금 순서").await;
    let item = module_item(&app, &admin, &ws, "LKO").await;
    let item_id = item["id"].as_str().unwrap().to_owned();
    let today = seoul_today(&state, &admin).await;
    let daily = json!({"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null});
    // 뿌리가 할 일인 반복을 먼저(더 작은 id), 뿌리가 완료인 반복을 나중에 만든다.
    let todo_root = dated_task(&app, &admin, &item, "할 일 뿌리", &day(today)).await;
    let (status, todo_rule) = repeat(&app, &admin, &todo_root, daily.clone()).await;
    assert_eq!(status, StatusCode::OK, "{todo_rule:?}");
    let done_root = dated_task(&app, &admin, &item, "완료 뿌리", &day(today)).await;
    let (status, done_rule) = repeat(&app, &admin, &done_root, daily.clone()).await;
    assert_eq!(status, StatusCode::OK, "{done_rule:?}");
    let current = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{}", done_root["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await
    .json;
    let finished = send(&app, Method::PUT, &format!("/api/tasks/{}", done_root["id"].as_str().unwrap()), Some(&admin),
        Some(json!({ "base_version": current["version"], "title": "완료 뿌리", "description": "", "status": "완료",
                     "priority": "none", "start_date": null, "due_date": day(today), "estimate_minutes": null,
                     "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(finished.status, StatusCode::OK, "{:?}", finished.json);
    let done_series = done_rule["series"]["id"].as_str().unwrap().to_owned();
    for series in [
        todo_rule["series"]["id"].as_str().unwrap(),
        done_series.as_str(),
    ] {
        common::fixture_sql(
            "DELETE FROM fvoci.tasks WHERE series_id = $1::uuid AND occurrence_date > (now() AT TIME ZONE 'Asia/Seoul')::date",
            series,
        )
        .await;
    }

    // 사용자 쪽: 칸반 이동과 같은 순서(열 id 순, D-116)로 첫 열을 잡고 마지막 열을 기다린다(완료 업무를 할 일로 옮기는 중).
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&url).await.unwrap();
    let lanes: Vec<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM fvoci.task_statuses WHERE item_id = $1::uuid ORDER BY id",
    )
    .bind(&item_id)
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(lanes.len(), 3, "default statuses");
    let mut t1 = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *t1)
        .await
        .unwrap();
    let lane = "SELECT pg_advisory_xact_lock(hashtextextended('fvoci-lane:' || $1::text, 0))";
    sqlx::query(lane)
        .bind(lanes[0])
        .execute(&mut *t1)
        .await
        .unwrap();
    let tick = tokio::spawn({
        let db = state.db.clone();
        async move { fvoci_server::worker::run_all_now(&db, None, None).await }
    });
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&owner)
        .await
        .unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(blocked, "the tick never waited on the first lane");
    let last = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        sqlx::query(lane).bind(lanes[2]).execute(&mut *t1),
    )
    .await
    .expect("the user side waited too long");
    assert!(last.is_ok(), "{last:?}");
    t1.commit().await.unwrap();
    tick.await.unwrap().unwrap();

    // 두 반복 모두 다음 회차가 만들어졌다(교착으로 건너뛰지 않았다, 리뷰: 할 일 뿌리 반복도 확인).
    let made = |series: String| {
        let owner = owner.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM fvoci.tasks WHERE series_id = $1::uuid AND deleted_at IS NULL
                   AND occurrence_date > (now() AT TIME ZONE 'Asia/Seoul')::date",
            )
            .bind(series)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };
    let done_made = made(done_series.clone()).await;
    let todo_made = made(todo_rule["series"]["id"].as_str().unwrap().to_owned()).await;
    owner.close().await;
    assert!(done_made >= 1, "the done-rooted series lost its occurrence");
    assert!(todo_made >= 1, "the todo-rooted series lost its occurrence");
}

/// 리뷰(감사 S3 후속): 열 잠금을 반복별 예외 블록 밖에서 잡게 바꾼 뒤로, 한 항목의 열 잠금이 실패하면(교착 상대로
/// 골라짐, 잠금 시간 초과) 반복 회차 맞추기 전체가 실패해 다른 항목의 회차까지 다음 주기로 밀렸다. 전처럼 그 항목의
/// 반복만 경고하고 건너뛴다.
#[tokio::test]
async fn a_lane_lock_failure_skips_only_that_item_in_the_recurrence_tick() {
    let _serial = LANE_TICK.write().await;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "열 잠금 실패").await;
    let today = seoul_today(&state, &admin).await;
    let daily = json!({"freq":"daily","interval":1,"weekdays":[],"until":null,"count":null});
    let mut series = Vec::new();
    let mut items = Vec::new();
    // 앞 항목(더 작은 id)의 잠금이 실패해도 뒤 항목은 맞춘다.
    for key in ["LKA", "LKB"] {
        let item = module_item(&app, &admin, &ws, key).await;
        let root = dated_task(&app, &admin, &item, "매일", &day(today)).await;
        let (status, rule) = repeat(&app, &admin, &root, daily.clone()).await;
        assert_eq!(status, StatusCode::OK, "{rule:?}");
        series.push(rule["series"]["id"].as_str().unwrap().to_owned());
        items.push(item["id"].as_str().unwrap().to_owned());
    }

    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&url).await.unwrap();
    // 다른 연결이 앞 항목의 첫 열(열 id 순, 뿌리 업무의 할 일 열)을 쥐고 있다.
    let first_lane: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM fvoci.task_statuses WHERE item_id = $1::uuid ORDER BY id LIMIT 1",
    )
    .bind(&items[0])
    .fetch_one(&owner)
    .await
    .unwrap();
    let mut holder = owner.acquire().await.unwrap();
    sqlx::query("SELECT pg_advisory_lock(hashtextextended('fvoci-lane:' || $1::text, 0))")
        .bind(first_lane)
        .execute(&mut *holder)
        .await
        .unwrap();
    // 잠근 뒤에 미래 회차를 지운다. 같은 바이너리의 다른 시험이 회차 맞추기를 돌려도 앞 항목은 이 잠금에서 기다리고(지우기
    // 전에 다시 만들지 못함), 아래에서 세는 동안에도 그렇다(리뷰).
    for id in &series {
        common::fixture_sql(
            "DELETE FROM fvoci.tasks WHERE series_id = $1::uuid AND occurrence_date > (now() AT TIME ZONE 'Asia/Seoul')::date",
            id,
        )
        .await;
    }
    // 작업 프로세스처럼(#95): 기한이 된 계열을 모으고 계열마다 트랜잭션 하나에서 맞춘다. 잠금 시간 초과(55P03)는 그 계열만
    // 건너뛰고 다음 계열을 계속한다. 이 시험의 두 계열만 본다(공유 DB의 다른 계열은 다른 시험의 것).
    let due: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT fvoci.app_system_recurrence_due()")
        .fetch_all(&owner)
        .await
        .unwrap();
    let mut outcomes: Vec<Result<i32, Option<String>>> = Vec::new();
    for id in &series {
        let id: uuid::Uuid = id.parse().unwrap();
        assert!(due.contains(&id), "{id} is not due: {due:?}");
        let mut tx = owner.begin().await.unwrap();
        sqlx::query("SET LOCAL lock_timeout = '300ms'")
            .execute(&mut *tx)
            .await
            .unwrap();
        // id는 작업 프로세스처럼 서버가 만든다(Supabase의 Postgres에는 `uuidv7()`이 없다). 상한 뒤의 경합에 대비해 넉넉히.
        let need: i32 = sqlx::query_scalar("SELECT fvoci.app_system_sync_task_series_ids($1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        let made = sqlx::query_scalar::<_, i32>("SELECT fvoci.app_system_sync_task_series($1, $2)")
            .bind(id)
            .bind(fvoci_db::new_ids(
                usize::try_from(need).unwrap_or_default() + SPARE_TICK_IDS,
            ))
            .fetch_one(&mut *tx)
            .await;
        match made {
            Ok(n) => {
                tx.commit().await.unwrap();
                outcomes.push(Ok(n));
            }
            Err(e) => outcomes.push(Err(e
                .as_database_error()
                .and_then(|d| d.code().map(|c| c.into_owned())))),
        }
    }

    // 잠금을 풀기 전에 센다(풀면 다른 시험의 회차 맞추기가 앞 항목을 채울 수 있다).
    let made = |id: String| {
        let owner = owner.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM fvoci.tasks WHERE series_id = $1::uuid AND deleted_at IS NULL
                   AND occurrence_date > (now() AT TIME ZONE 'Asia/Seoul')::date",
            )
            .bind(id)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };
    let skipped = made(series[0].clone()).await;
    let synced = made(series[1].clone()).await;
    sqlx::query("SELECT pg_advisory_unlock(hashtextextended('fvoci-lane:' || $1::text, 0))")
        .bind(first_lane)
        .execute(&mut *holder)
        .await
        .unwrap();
    drop(holder);
    owner.close().await;
    assert_eq!(
        outcomes[0],
        Err(Some("55P03".to_owned())),
        "the locked series must stop at lock_timeout: {outcomes:?}"
    );
    assert!(outcomes[1].is_ok(), "{outcomes:?}");
    assert_eq!(skipped, 0, "the locked item was synced");
    assert!(synced >= 1, "the other item lost its occurrences");
}
