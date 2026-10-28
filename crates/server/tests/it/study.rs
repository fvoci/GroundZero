#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 스터디 계획 (D-36): 계획 단위는 함께, 진행은 사람마다(본인만 읽음), 밀린 단위는 사용자가 정한 날짜로 재배치,
//! 함께 하는 사람의 집계는 지금 볼 수 있는 사람만.

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn module(app: &Router, admin: &TestUser, ws: &str, kind: &str, key: &str) -> String {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": kind, "name": key, "key": key })),
    )
    .await;
    m.json["id"].as_str().unwrap().to_owned()
}

async fn open_item(app: &Router, admin: &TestUser, module: &str, title: &str) -> String {
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    let id = i.json["id"].as_str().unwrap().to_owned();
    send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "base_version": 1, "title": title, "description": "", "visibility": "workspace" })),
    )
    .await;
    id
}

async fn plan(app: &Router, user: &TestUser, item: &str) -> Value {
    let r = send(
        app,
        Method::GET,
        &format!("/api/items/{item}/study"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

#[tokio::test]
async fn study_units_are_shared_and_progress_is_personal() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "스터디").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let study = module(&app, &admin, &ws, "study", "STU").await;
    let lecture = module(&app, &admin, &ws, "course", "LEC").await;
    let item = open_item(&app, &admin, &study, "통계 스터디").await;
    let course = open_item(&app, &admin, &lecture, "강좌").await;
    let units_uri = format!("/api/items/{item}/study/units");

    let first = new_request_id();
    let body = json!({ "units": [
        { "request_id": first, "title": "1장 기술통계", "chapter": "1부", "unit": "pages", "amount": 30, "planned_on": "2026-10-10" },
        { "request_id": new_request_id(), "title": "연습문제", "unit": "problems", "amount": 50, "planned_on": "2026-10-12" },
        { "request_id": new_request_id(), "title": "복습 강의", "unit": "minutes", "amount": 60 }
    ]});
    let added = send(
        &app,
        Method::POST,
        &units_uri,
        Some(&writer),
        Some(body.clone()),
    )
    .await;
    assert_eq!(added.json["added"], 3, "{:?}", added.json);
    assert_eq!(
        send(&app, Method::POST, &units_uri, Some(&writer), Some(body))
            .await
            .json["added"],
        0
    );
    let bad =
        json!({ "units": [{ "request_id": new_request_id(), "title": "x", "unit": "chapters" }] });
    assert_eq!(
        send(&app, Method::POST, &units_uri, Some(&writer), Some(bad))
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    let reader_add = json!({ "units": [{ "request_id": new_request_id(), "title": "x" }] });
    assert_eq!(
        send(
            &app,
            Method::POST,
            &units_uri,
            Some(&reader),
            Some(reader_add)
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/items/{item}/study"),
            Some(&outsider),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/items/{course}/study"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // 진행은 사람마다. 멤버 이상만 자기 진행을 남긴다. 완료한 날은 사용자의 오늘.
    let p = plan(&app, &reader, &item).await;
    assert_eq!(p["can_write"], false);
    let units = p["units"].as_array().unwrap().clone();
    assert_eq!(units.len(), 3);
    let today = time::OffsetDateTime::now_utc().date();
    // 밀린 단위의 계획일은 오늘 기준(참여자 집계의 today는 서버 날짜에서 하루 안이어야 한다).
    let two_days_ago = today - time::Duration::days(2);
    let yesterday = today - time::Duration::days(1);
    let progress =
        |unit: &Value| format!("/api/study-units/{}/progress", unit["id"].as_str().unwrap());
    // 읽기 전용은 자기 진도도 남기지 못한다(개정 2026-10-26 ⑧, D-20).
    let refused = send(
        &app,
        Method::PUT,
        &progress(&units[0]),
        Some(&reader),
        Some(json!({ "status": "done", "done_amount": 30, "today": today.to_string() })),
    )
    .await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{:?}", refused.json);
    let done = send(
        &app,
        Method::PUT,
        &progress(&units[0]),
        Some(&writer),
        Some(json!({
        "status": "done", "done_amount": 30, "note": "어려웠음", "today": today.to_string() })),
    )
    .await;
    assert_eq!(done.status, StatusCode::NO_CONTENT, "{:?}", done.json);
    let mine = plan(&app, &writer, &item).await;
    assert_eq!(mine["units"][0]["mine"]["status"], "done");
    assert_eq!(mine["units"][0]["mine"]["completed_on"], today.to_string());
    assert_eq!(mine["units"][0]["mine"]["note"], "어려웠음");
    let theirs = plan(&app, &admin, &item).await;
    assert_eq!(theirs["units"][0]["mine"]["status"], "not_started");
    assert_eq!(theirs["units"][0]["mine"]["note"], "");

    // 밀린 단위를 사용자가 정한 날짜로 재배치(내 계획일만, 상태는 그대로).
    let refused_plan = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}/study/plan"),
        Some(&reader),
        Some(json!({ "entries": [{ "unit_id": units[1]["id"], "planned_on": yesterday.to_string() }] })),
    )
    .await;
    assert_eq!(
        refused_plan.status,
        StatusCode::FORBIDDEN,
        "{:?}",
        refused_plan.json
    );
    assert!(plan(&app, &reader, &item).await["units"][1]["mine"]["planned_on"].is_null());
    let replan = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}/study/plan"),
        Some(&writer),
        Some(json!({
        "entries": [
            { "unit_id": units[1]["id"], "planned_on": two_days_ago.to_string() },
            { "unit_id": units[2]["id"], "planned_on": yesterday.to_string() }
        ] })),
    )
    .await;
    assert_eq!(replan.json["planned"], 2, "{:?}", replan.json);
    let mine = plan(&app, &writer, &item).await;
    assert_eq!(
        mine["units"][1]["mine"]["planned_on"],
        two_days_ago.to_string()
    );
    assert_eq!(mine["units"][1]["mine"]["status"], "not_started");
    assert_eq!(mine["units"][0]["mine"]["status"], "done");
    let foreign = send(
        &app,
        Method::PUT,
        &format!("/api/items/{course}/study/plan"),
        Some(&writer),
        Some(json!({
        "entries": [{ "unit_id": units[1]["id"], "planned_on": "2026-10-20" }] })),
    )
    .await;
    assert_eq!(foreign.status, StatusCode::NOT_FOUND);

    // 함께 하는 사람: 진행을 남긴 사람의 집계만, 메모 없이. 볼 수 없게 되면 빠진다.
    let participants = |user: TestUser| {
        let app = app.clone();
        let item = item.clone();
        async move {
            send(
                &app,
                Method::GET,
                &format!("/api/items/{item}/study/participants?today={today}"),
                Some(&user),
                None,
            )
            .await
            .json
        }
    };
    let list = participants(writer.clone()).await;
    assert_eq!(list[0]["display_name"], "member", "{list:?}");
    assert_eq!(list[0]["done"], 1);
    assert_eq!(list[0]["done_amount"], 30);
    assert_eq!(list[0]["overdue"], 2);
    assert!(list[0].get("note").is_none());
    // 감사 S9: 서버 날짜에서 하루 넘게 떨어진 today는 400(today를 바꿔 가며 물어 밀린 수의 계단에서 다른 사람의
    // 개인 계획일을 알아내지 못하게).
    for far in [
        today + time::Duration::days(30),
        today - time::Duration::days(30),
    ] {
        let r = send(
            &app,
            Method::GET,
            &format!("/api/items/{item}/study/participants?today={far}"),
            Some(&writer),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    }
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}"),
        Some(&admin),
        Some(json!({ "base_version": 2, "title": "통계 스터디", "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(participants(admin.clone()).await, json!([]));

    // 스터디 단위를 시간 기록 대상으로(D-32). 볼 수 없는 사람은 404.
    let timer = send(
        &app,
        Method::POST,
        "/api/me/timer",
        Some(&admin),
        Some(json!({
        "request_id": new_request_id(), "kind": "study", "memo": "",
        "target": { "kind": "study_unit", "id": units[1]["id"] } })),
    )
    .await;
    assert_eq!(timer.status, StatusCode::CREATED, "{:?}", timer.json);
    let targets = send(
        &app,
        Method::GET,
        &format!(
            "/api/time-entry-targets?entry_id={}",
            timer.json["id"].as_str().unwrap()
        ),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(targets.status, StatusCode::OK, "{:?}", targets.json);
    assert_eq!(targets.json[0]["label"], "STU-1 2. 연습문제");
    let refused = send(
        &app,
        Method::POST,
        "/api/me/timer",
        Some(&outsider),
        Some(json!({
        "request_id": new_request_id(), "kind": "study", "memo": "",
        "target": { "kind": "study_unit", "id": units[1]["id"] } })),
    )
    .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{:?}", refused.json);

    // 단위를 지우면 모든 사람의 그 진행도 지워진다.
    let unit_uri = format!("/api/study-units/{}", units[0]["id"].as_str().unwrap());
    assert_eq!(
        send(&app, Method::DELETE, &unit_uri, Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        plan(&app, &admin, &item).await["units"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

// 두 사람이 같은 판의 공통 단위를 보고 고치면 먼저 저장한 내용이 남고 뒤의 저장은 409로 아무것도 바꾸지 않는다. 동시에
// 보내도 하나만 바뀐다. 다시 읽은 판으로 보내면 바뀐다.
#[tokio::test]
async fn two_writers_on_the_same_unit_version_do_not_overwrite() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "스터디 동시 수정").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let study = module(&app, &admin, &ws, "study", "STV").await;
    let item = open_item(&app, &admin, &study, "통계 스터디").await;
    let added = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/study/units"),
        Some(&admin),
        Some(json!({ "units": [
            { "request_id": new_request_id(), "title": "1장", "unit": "pages", "amount": 30 },
            { "request_id": new_request_id(), "title": "2장", "unit": "pages", "amount": 20 }
        ]})),
    )
    .await;
    assert_eq!(added.json["added"], 2, "{:?}", added.json);
    let units = plan(&app, &admin, &item).await["units"].clone();
    let (seen, other) = (units[0].clone(), units[1].clone());
    let uri = format!("/api/study-units/{}", seen["id"].as_str().unwrap());
    let base = seen["version"].clone();

    let first = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "version": base, "number": 1, "title": "1장 기술통계", "unit": "pages", "amount": 40 })),
    )
    .await;
    assert_eq!(first.status, StatusCode::NO_CONTENT, "{:?}", first.json);
    let second = send(
        &app,
        Method::PUT,
        &uri,
        Some(&writer),
        Some(json!({ "version": base, "number": 1, "title": "1장", "unit": "problems", "amount": 10 })),
    )
    .await;
    assert_eq!(second.status, StatusCode::CONFLICT, "{:?}", second.json);
    // 판 충돌은 다시 보내라는 재시도가 아니라 다시 읽으라는 뜻이다(감사 C32, #77): 409 `invalid_state`와 SQL의 설명 그대로.
    assert_eq!(second.json["error"], "invalid_state");
    assert_eq!(second.json["message"], "the unit changed; reload it");
    let now = plan(&app, &writer, &item).await["units"][0].clone();
    assert_eq!(now["title"], "1장 기술통계");
    assert_eq!(now["unit"], "pages");
    assert_eq!(now["amount"], 40);
    assert_ne!(now["version"], base);

    let retry = send(
        &app,
        Method::PUT,
        &uri,
        Some(&writer),
        Some(json!({ "version": now["version"], "number": 1, "title": "1장", "unit": "problems", "amount": 10 })),
    )
    .await;
    assert_eq!(retry.status, StatusCode::NO_CONTENT, "{:?}", retry.json);
    assert_eq!(plan(&app, &admin, &item).await["units"][0]["amount"], 10);

    let other_uri = format!("/api/study-units/{}", other["id"].as_str().unwrap());
    let body = |amount: i32| json!({ "version": other["version"], "number": 2, "title": "2장", "unit": "pages", "amount": amount });
    let (a, b) = tokio::join!(
        send(&app, Method::PUT, &other_uri, Some(&admin), Some(body(21))),
        send(&app, Method::PUT, &other_uri, Some(&writer), Some(body(22)))
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::NO_CONTENT, StatusCode::CONFLICT]);
    let winner = if a.status == StatusCode::NO_CONTENT {
        21
    } else {
        22
    };
    assert_eq!(
        plan(&app, &admin, &item).await["units"][1]["amount"],
        winner
    );
}

/// D-113(개정 ⑯)·D-20(⑧): 본인 진도 저장도 쓰기다. 보관된 항목이나 보관된 모듈의 항목에서는 멤버의 스터디 단위 진도
/// 저장이 409 `invalid_state`("the item is archived", 웹의 보관 문구)이고, 열린 항목에서는 저장된다.
#[tokio::test]
async fn unit_progress_is_refused_under_an_archived_item_or_module() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "스터디 진도 보관").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let open_module = module(&app, &admin, &ws, "study", "OPN").await;
    let archived_module = module(&app, &admin, &ws, "study", "AMD").await;
    let open = open_item(&app, &admin, &open_module, "열린 스터디").await;
    let archived_item = open_item(&app, &admin, &open_module, "보관된 스터디").await;
    let in_archived_module =
        open_item(&app, &admin, &archived_module, "보관된 모듈의 스터디").await;
    let mut units = Vec::new();
    for item in [&open, &archived_item, &in_archived_module] {
        let added = send(
            &app,
            Method::POST,
            &format!("/api/items/{item}/study/units"),
            Some(&admin),
            Some(json!({ "units": [{ "request_id": new_request_id(), "title": "1장", "unit": "pages", "amount": 10 }] })),
        )
        .await;
        assert_eq!(added.json["added"], 1, "{:?}", added.json);
        units.push(plan(&app, &member, item).await["units"][0].clone());
    }
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/items/{archived_item}/archived"),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{archived_module}/archived"),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let today = time::OffsetDateTime::now_utc().date().to_string();
    for (item, unit, archived) in [
        (&open, &units[0], false),
        (&archived_item, &units[1], true),
        (&in_archived_module, &units[2], true),
    ] {
        let saved = send(
            &app,
            Method::PUT,
            &format!("/api/study-units/{}/progress", unit["id"].as_str().unwrap()),
            Some(&member),
            Some(json!({ "status": "done", "done_amount": 10, "today": today })),
        )
        .await;
        // 밀린 단위의 내 계획일 재배치도 본인 진도 행을 쓴다(`app_plan_units`).
        let planned = send(
            &app,
            Method::PUT,
            &format!("/api/items/{item}/study/plan"),
            Some(&member),
            Some(json!({ "entries": [{ "unit_id": unit["id"], "planned_on": today }] })),
        )
        .await;
        if archived {
            assert_eq!(
                (saved.status, planned.status),
                (StatusCode::CONFLICT, StatusCode::CONFLICT),
                "{unit}: progress {:?}, plan {:?}",
                saved.json,
                planned.json
            );
            for r in [&saved, &planned] {
                assert_eq!(r.json["error"], "invalid_state", "{:?}", r.json);
                assert_eq!(r.json["message"], "the item is archived", "{:?}", r.json);
            }
        } else {
            assert_eq!(
                saved.status,
                StatusCode::NO_CONTENT,
                "{unit}: {:?}",
                saved.json
            );
            assert_eq!(planned.status, StatusCode::OK, "{unit}: {:?}", planned.json);
        }
    }
}
