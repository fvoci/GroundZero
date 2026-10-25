#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 회의·발표·행사 일정과 워크스페이스 달력 (D-31, D-50, D-57).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn module_item(app: &Router, admin: &TestUser, ws: &str, key: &str, public: bool) -> Value {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": key })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    if public {
        let r = send(
            app,
            Method::PUT,
            &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
            Some(admin),
            Some(json!({ "base_version": 1, "title": key, "description": "", "visibility": "workspace" })),
        )
        .await;
        assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    }
    i.json
}

fn timed(title: &str, starts: &str, ends: &str, tz: &str) -> Value {
    json!({ "request_id": new_request_id(), "kind": "meeting", "title": title, "description": "",
            "all_day": false, "start_date": null, "end_date": null, "starts_at": starts, "ends_at": ends,
            "timezone": tz })
}

async fn create(app: &Router, user: &TestUser, item: &Value, body: Value) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!("/api/items/{}/events", item["id"].as_str().unwrap()),
        Some(user),
        Some(body),
    )
    .await;
    (r.status, r.json)
}

async fn calendar(
    app: &Router,
    user: &TestUser,
    ws: &str,
    from: &str,
    to: &str,
    tz: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::GET,
        &format!("/api/workspaces/{ws}/calendar?from={from}&to={to}&tz={tz}"),
        Some(user),
        None,
    )
    .await;
    (r.status, r.json)
}

#[tokio::test]
async fn events_follow_item_access_keep_their_zone_and_guard_versions() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "일정").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let item = module_item(&app, &admin, &ws, "EVT", true).await;

    let body = timed(
        "주간 회의",
        "2026-11-02T10:00:00+09:00",
        "2026-11-02T11:00:00+09:00",
        "Asia/Seoul",
    );
    let (status, event) = create(&app, &admin, &item, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{event:?}");
    assert_eq!(event["starts_at"], "2026-11-02T01:00:00Z");
    assert_eq!(event["timezone"], "Asia/Seoul");
    // 같은 요청 id로 다시 보내면 같은 일정.
    let (_, again) = create(&app, &admin, &item, body).await;
    assert_eq!(again["id"], event["id"]);

    let bad_zone = timed(
        "x",
        "2026-11-02T10:00:00+09:00",
        "2026-11-02T11:00:00+09:00",
        "Mars/Olympus",
    );
    assert_eq!(
        create(&app, &admin, &item, bad_zone).await.0,
        StatusCode::BAD_REQUEST
    );
    let backwards = timed(
        "x",
        "2026-11-02T10:00:00+09:00",
        "2026-11-02T09:00:00+09:00",
        "Asia/Seoul",
    );
    assert_eq!(
        create(&app, &admin, &item, backwards).await.0,
        StatusCode::BAD_REQUEST
    );
    let all_day = json!({ "request_id": new_request_id(), "kind": "event", "title": "학회", "description": "",
        "all_day": true, "start_date": "2026-11-05", "end_date": "2026-11-07", "starts_at": null, "ends_at": null,
        "timezone": "Asia/Seoul" });
    let (status, conf) = create(&app, &admin, &item, all_day.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{conf:?}");
    assert_eq!(
        (conf["start_date"].clone(), conf["end_date"].clone()),
        (json!("2026-11-05"), json!("2026-11-07"))
    );
    assert_eq!(
        create(&app, &reader, &item, all_day).await.0,
        StatusCode::FORBIDDEN
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/items/{}/events", item["id"].as_str().unwrap()),
        Some(&outsider),
        None,
    )
    .await;
    assert_eq!(list.status, StatusCode::NOT_FOUND);

    // 버전 충돌과 날짜 변경 감사.
    let uri = format!("/api/events/{}", event["id"].as_str().unwrap());
    let edit = |version: &Value, starts: &str| {
        json!({ "base_version": version, "kind": "meeting", "title": "주간 회의",
        "description": "", "all_day": false, "start_date": null, "end_date": null, "starts_at": starts,
        "ends_at": "2026-11-02T12:00:00+09:00", "timezone": "Asia/Seoul" })
    };
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(edit(&event["version"], "2026-11-02T10:30:00+09:00")),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(edit(&event["version"], "2026-11-02T09:00:00+09:00")),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(r.json["current"]["starts_at"], "2026-11-02T01:30:00Z");

    // 지우기: 독자는 못 하고, 지운 일정은 없는 것과 같다.
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&reader), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(&app, Method::DELETE, &uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(event["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(actions, ["create", "reschedule", "delete"]);
}

#[tokio::test]
async fn the_calendar_shows_visible_tasks_and_events_by_the_viewers_days() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "달력").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;
    let public = module_item(&app, &admin, &ws, "PUB", true).await;
    let private = module_item(&app, &admin, &ws, "PRV", false).await;

    let t = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/tasks", public["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "보고서" })),
    )
    .await
    .json;
    let r = send(&app, Method::PUT, &format!("/api/tasks/{}", t["id"].as_str().unwrap()), Some(&admin),
                 Some(json!({ "base_version": t["version"], "title": "보고서", "description": "", "status": "todo",
                              "priority": "none", "start_date": "2026-10-28", "due_date": "2026-11-03",
                              "estimate_minutes": null, "tags": [], "assignee_ids": [], "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    // 서울 11월 1일 00:30 = UTC 10월 31일 15:30.
    let (status, _) = create(
        &app,
        &admin,
        &public,
        timed(
            "새벽 점검",
            "2026-11-01T00:30:00+09:00",
            "2026-11-01T01:00:00+09:00",
            "Asia/Seoul",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = create(
        &app,
        &admin,
        &private,
        timed(
            "비공개 회의",
            "2026-11-01T10:00:00+09:00",
            "2026-11-01T11:00:00+09:00",
            "Asia/Seoul",
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let titles = |cal: &Value| -> Vec<String> {
        let mut out: Vec<String> = cal["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .chain(cal["events"].as_array().unwrap().iter())
            .map(|v| v["title"].as_str().unwrap().to_owned())
            .collect();
        out.sort();
        out
    };
    let (status, seoul_nov1) =
        calendar(&app, &writer, &ws, "2026-11-01", "2026-11-01", "Asia/Seoul").await;
    assert_eq!(status, StatusCode::OK, "{seoul_nov1:?}");
    assert_eq!(titles(&seoul_nov1), ["보고서", "새벽 점검"]);
    assert_eq!(seoul_nov1["items"].as_array().unwrap().len(), 1);
    let (_, seoul_oct31) =
        calendar(&app, &writer, &ws, "2026-10-31", "2026-10-31", "Asia/Seoul").await;
    assert_eq!(titles(&seoul_oct31), ["보고서"]);
    let (_, utc_oct31) = calendar(&app, &writer, &ws, "2026-10-31", "2026-10-31", "UTC").await;
    assert_eq!(titles(&utc_oct31), ["보고서", "새벽 점검"]);
    let (_, admin_nov1) =
        calendar(&app, &admin, &ws, "2026-11-01", "2026-11-01", "Asia/Seoul").await;
    assert_eq!(titles(&admin_nov1), ["보고서", "비공개 회의", "새벽 점검"]);
    let (_, later) = calendar(&app, &admin, &ws, "2026-11-10", "2026-11-30", "Asia/Seoul").await;
    assert_eq!(titles(&later), Vec::<String>::new());

    assert_eq!(
        calendar(&app, &writer, &ws, "2026-01-01", "2026-12-31", "UTC")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        calendar(&app, &writer, &ws, "2026-11-01", "2026-11-01", "Nowhere")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        calendar(&app, &outsider, &ws, "2026-11-01", "2026-11-01", "UTC")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

/// #101: 달력의 날 경계도 그 시간대에서 그날 0시가 처음 오는 시점이다(시간 기록·검색과 같은 식, 감사 S21). 날짜는
/// tzdata의 America/Havana 2026년 전환(`zdump -v -c 2026,2027 America/Havana`)에서 골랐다.
/// - 2026-11-01: 01:00 CDT → 00:00 CST로 0시가 두 번(04:00Z, 05:00Z) 온다. 첫 0시 직후 일정은 그날에 든다.
///   `날짜::timestamp AT TIME ZONE`만 쓰면 뒤의 0시(05:00Z)에 시작해 이 일정을 전날로 보냈다.
/// - 2026-03-08: 00:00 CST → 01:00 CDT로 0시가 없다. 그날은 건너뛴 직후(01:00 CDT = 05:00Z)에 시작하고, 직전
///   (전날 23:30 CST)은 전날이다.
#[tokio::test]
async fn calendar_days_start_at_the_first_midnight_in_the_viewers_zone() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "아바나 달력").await;
    let item = module_item(&app, &admin, &ws, "HAV", true).await;
    for (title, starts, ends) in [
        (
            "가을 전날 밤",
            "2026-10-31T23:30:00-04:00",
            "2026-10-31T23:45:00-04:00",
        ),
        (
            "첫 0시 뒤",
            "2026-11-01T00:30:00-04:00",
            "2026-11-01T00:45:00-04:00",
        ),
        (
            "봄 전날 밤",
            "2026-03-07T23:30:00-05:00",
            "2026-03-07T23:45:00-05:00",
        ),
        (
            "건너뛴 직후",
            "2026-03-08T01:00:00-04:00",
            "2026-03-08T01:15:00-04:00",
        ),
    ] {
        let (status, event) = create(
            &app,
            &admin,
            &item,
            timed(title, starts, ends, "America/Havana"),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{event:?}");
    }
    let titles = async |day: &str| -> Vec<String> {
        let (status, cal) = calendar(&app, &admin, &ws, day, day, "America/Havana").await;
        assert_eq!(status, StatusCode::OK, "{cal:?}");
        cal["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["title"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(titles("2026-10-31").await, ["가을 전날 밤"]);
    assert_eq!(titles("2026-11-01").await, ["첫 0시 뒤"]);
    assert_eq!(titles("2026-03-07").await, ["봄 전날 밤"]);
    assert_eq!(titles("2026-03-08").await, ["건너뛴 직후"]);
}

/// 워크스페이스·모듈·항목 범위의 같은 달력과 날짜 출처(D-80): 강의·스터디 계획일(내 계획일이 있으면 그것, 본인만),
/// 투고 제출 예정일·수정 마감일, 산출물 마감일. 볼 수 없는 항목은 나오지 않고, 다른 워크스페이스·범위 밖은 404.
#[tokio::test]
async fn calendar_scopes_and_date_sources_follow_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "달력 범위").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let other_ws = create_workspace(&app, &admin, "다른 곳").await;
    let made = |kind: &'static str, key: &'static str, public: bool| {
        let app = app.clone();
        let admin = admin.clone();
        let ws = ws.clone();
        async move {
            let m = send(&app, Method::POST, &format!("/api/workspaces/{ws}/modules"), Some(&admin),
                Some(json!({ "request_id": new_request_id(), "kind": kind, "name": key, "key": key }))).await;
            assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
            let module = m.json["id"].as_str().unwrap().to_owned();
            let i = send(
                &app,
                Method::POST,
                &format!("/api/modules/{module}/items"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "title": key })),
            )
            .await;
            let item = i.json["id"].as_str().unwrap().to_owned();
            if public {
                let r = send(
                    &app,
                    Method::PUT,
                    &format!("/api/items/{item}"),
                    Some(&admin),
                    Some(json!({ "base_version": 1, "title": key, "description": "", "visibility": "workspace" })),
                )
                .await;
                assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            }
            (module, item)
        }
    };
    let (course_module, course) = made("course", "LEC", true).await;
    let (_, study) = made("study", "STU", true).await;
    let (_, research) = made("research", "RES", true).await;
    let (_, project) = made("project", "PRJ", false).await;

    let r = send(&app, Method::POST, &format!("/api/items/{course}/episodes"), Some(&admin),
        Some(json!({ "episodes": [
            { "request_id": new_request_id(), "title": "벡터", "duration_seconds": 600, "planned_on": "2026-11-05" },
            { "request_id": new_request_id(), "title": "행렬", "duration_seconds": 600, "planned_on": "2026-11-06" }
        ]}))).await;
    assert_eq!(r.json["added"], 2, "{:?}", r.json);
    let course_view = send(
        &app,
        Method::GET,
        &format!("/api/items/{course}/course"),
        Some(&writer),
        None,
    )
    .await;
    let second = course_view.json["episodes"][1]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // 작성자만 둘째 회차를 11-10으로 옮긴다(개인 계획일).
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/episodes/{second}/progress"),
        Some(&writer),
        Some(json!({ "status": "not_started", "planned_on": "2026-11-10" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
    let r = send(&app, Method::POST, &format!("/api/items/{study}/study/units"), Some(&admin),
        Some(json!({ "units": [{ "request_id": new_request_id(), "title": "1장", "unit": "pages", "amount": 10, "planned_on": "2026-11-07" }] }))).await;
    assert!(r.status.is_success(), "{:?}", r.json);
    let r = send(&app, Method::POST, &format!("/api/items/{research}/records"), Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "submission", "title": "학회 투고", "occurred_on": "2026-11-08",
                     "body": "", "fields": { "status": "submitted", "revision_due": "2026-11-12" } }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let r = send(&app, Method::POST, &format!("/api/items/{project}/records"), Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "deliverable", "title": "설계서", "occurred_on": "2026-11-09",
                     "body": "", "fields": { "status": "done" } }))).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);

    let dates = |user: &TestUser, extra: String| {
        let app = app.clone();
        let user = user.clone();
        let ws = ws.clone();
        async move {
            let r = send(&app, Method::GET,
                &format!("/api/workspaces/{ws}/calendar?from=2026-11-01&to=2026-11-30&tz=Asia/Seoul{extra}"),
                Some(&user), None).await;
            (r.status, r.json)
        }
    };
    let summary = |v: &Value| -> Vec<(String, String, String, bool, bool)> {
        v["dates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| {
                (
                    d["kind"].as_str().unwrap().to_owned(),
                    d["title"].as_str().unwrap().to_owned(),
                    d["date"].as_str().unwrap().to_owned(),
                    d["personal"].as_bool().unwrap(),
                    d["done"].as_bool().unwrap(),
                )
            })
            .collect()
    };
    let s = |a: &str, b: &str, c: &str, p: bool, d: bool| {
        (a.to_owned(), b.to_owned(), c.to_owned(), p, d)
    };
    let (status, all) = dates(&admin, String::new()).await;
    assert_eq!(status, StatusCode::OK, "{all:?}");
    assert_eq!(
        summary(&all),
        vec![
            s("episode", "1. 벡터", "2026-11-05", false, false),
            s("episode", "2. 행렬", "2026-11-06", false, false),
            s("study_unit", "1. 1장", "2026-11-07", false, false),
            s("submission", "학회 투고", "2026-11-08", false, false),
            s("deliverable", "설계서", "2026-11-09", false, true),
            s("revision_due", "학회 투고", "2026-11-12", false, false),
        ]
    );
    // 작성자: 내 계획일로 옮긴 회차는 11-10(개인), 비공개 프로젝트의 산출물은 보이지 않는다.
    let (_, mine) = dates(&writer, String::new()).await;
    assert_eq!(
        summary(&mine),
        vec![
            s("episode", "1. 벡터", "2026-11-05", false, false),
            s("study_unit", "1. 1장", "2026-11-07", false, false),
            s("submission", "학회 투고", "2026-11-08", false, false),
            s("episode", "2. 행렬", "2026-11-10", true, false),
            s("revision_due", "학회 투고", "2026-11-12", false, false),
        ]
    );
    // 모듈 범위: 강의 모듈의 회차만. 항목 범위: 그 항목만.
    let (_, by_module) = dates(&writer, format!("&module_id={course_module}")).await;
    assert_eq!(
        summary(&by_module)
            .iter()
            .map(|d| d.0.as_str())
            .collect::<Vec<_>>(),
        ["episode", "episode"]
    );
    let (_, by_item) = dates(&admin, format!("&item_id={research}")).await;
    assert_eq!(
        summary(&by_item)
            .iter()
            .map(|d| d.0.as_str())
            .collect::<Vec<_>>(),
        ["submission", "revision_due"]
    );
    assert_eq!(by_item["items"].as_array().unwrap().len(), 1);
    // 볼 수 없는 항목·범위 밖·다른 워크스페이스는 404.
    assert_eq!(
        dates(&writer, format!("&item_id={project}")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        dates(
            &admin,
            format!("&module_id={course_module}&item_id={research}")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let r = send(&app, Method::GET,
        &format!("/api/workspaces/{other_ws}/calendar?from=2026-11-01&to=2026-11-30&tz=Asia/Seoul&module_id={course_module}"),
        Some(&admin), None).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
}
