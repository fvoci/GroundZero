#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 반복 일정의 규칙·회차·날짜 검증(D-31·D-50·D-81·D-125).
use crate::common;
use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

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

/// 반복 일정의 DST(D-81): 일정 시간대에 없는 현지 시각의 회차는 건너뛰고(건너뜀으로 표시), 두 번 있는 시각은 앞선
/// 시각에 한 번만. 보는 사람의 시간대가 달라도 회차 시점은 같다.
#[tokio::test]
async fn weekly_events_skip_missing_local_times_and_take_the_first_of_repeated_ones() {
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
    assert_eq!(stale.json["changed"], json!(["rule"]));
    assert!(stale.json["message"].is_string());
    assert!(stale.json["request_id"].is_string());
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
