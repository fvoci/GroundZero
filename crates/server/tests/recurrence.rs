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
        Some(json!({ "title": key, "description": "", "visibility": "workspace" })),
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
                 Some(json!({ "base_version": t["version"], "title": title, "description": "", "status": "todo",
                              "priority": "none", "start_date": null, "due_date": due, "estimate_minutes": null,
                              "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

async fn repeat(app: &Router, user: &TestUser, task: &Value, rule: Value) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/tasks/{}/recurrence", task["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "rule": rule, "timezone": "Asia/Seoul" })),
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

#[tokio::test]
async fn repeating_tasks_keep_open_occurrences_and_follow_the_original_schedule() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "반복").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
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
        Some(json!({ "status": "done", "before_id": null })),
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
                 Some(json!({ "base_version": fresh["version"], "title": "월간 보고", "description": "", "status": "todo",
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
    assert_eq!(audit_before, audit_after);
    drop(tx);

    // 멈추면 이미 만든 회차는 남고 예정도 사라진다.
    let (status, stopped) = repeat(&app, &admin, &root, Value::Null).await;
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
