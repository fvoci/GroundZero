#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 강의 모듈 (D-37, D-32): 강좌 정보·회차·CSV식 일괄 등록·개인별 진도·함께 수강하는 사람의 집계.

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

async fn course(app: &Router, admin: &TestUser, ws: &str) -> String {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "course", "name": "강의", "key": "LEC" })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "선형대수" })),
    )
    .await;
    let id = i.json["id"].as_str().unwrap().to_owned();
    // 함께 수강하도록 워크스페이스에 공개한다.
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "base_version": 1, "title": "선형대수", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    id
}

async fn get(app: &Router, user: &TestUser, item: &str) -> Value {
    super::support::view(app, user, item, super::support::LearningKind::Course).await
}

async fn progress(app: &Router, user: &TestUser, episode: &Value, body: Value) -> StatusCode {
    send(
        app,
        Method::PUT,
        &format!("/api/episodes/{}/progress", episode["id"].as_str().unwrap()),
        Some(user),
        Some(body),
    )
    .await
    .status
}

#[tokio::test]
async fn course_episodes_and_personal_progress() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "강의").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let item = course(&app, &admin, &ws).await;

    // 강좌 정보(쓸 수 있는 사람).
    let details = json!({ "subject": "수학", "instructor": "김강사", "platform": "온라인", "url": "https://example.test",
                          "textbook": "교재", "starts_on": "2026-10-01", "expires_on": "2027-01-31",
                          "target_on": "2026-12-31" });
    let uri = format!("/api/items/{item}/course");
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &uri,
            Some(&reader),
            Some(details.clone())
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &uri,
            Some(&writer),
            Some(details.clone())
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // CSV 일괄 등록과 같은 길: 여러 회차를 한 번에, 같은 요청 id는 한 번만.
    let first = new_request_id();
    let rows = json!({ "episodes": [
        { "request_id": first, "title": "벡터", "chapter": "1장", "duration_seconds": 1200, "planned_on": "2026-10-10" },
        { "request_id": new_request_id(), "title": "행렬", "chapter": "1장", "duration_seconds": 1800, "planned_on": "2026-10-20" },
        { "request_id": new_request_id(), "title": "행렬식", "chapter": "2장", "duration_seconds": 600 }
    ]});
    let episodes_uri = format!("/api/items/{item}/episodes");
    let added = send(
        &app,
        Method::POST,
        &episodes_uri,
        Some(&writer),
        Some(rows.clone()),
    )
    .await;
    assert_eq!(added.json["added"], 3, "{:?}", added.json);
    let again = send(
        &app,
        Method::POST,
        &episodes_uri,
        Some(&writer),
        Some(json!({ "episodes": [
        { "request_id": first, "title": "벡터" }] })),
    )
    .await;
    assert_eq!(again.json["added"], 0);
    assert_eq!(
        send(
            &app,
            Method::POST,
            &episodes_uri,
            Some(&writer),
            Some(json!({ "episodes": [
            { "request_id": new_request_id(), "title": "  " }] }))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &episodes_uri,
            Some(&reader),
            Some(json!({ "episodes": [
            { "request_id": new_request_id(), "title": "몰래" }] }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let view = get(&app, &reader, &item).await;
    assert_eq!(view["details"]["instructor"], "김강사");
    assert_eq!(view["can_write"], false);
    let eps = view["episodes"].as_array().unwrap().clone();
    assert_eq!(
        eps.iter()
            .map(|e| e["number"].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(eps[0]["mine"]["status"], "not_started");

    // 개인 계획일은 오늘 기준(참여자 집계의 today는 서버 날짜에서 하루 안이어야 한다).
    let today = time::OffsetDateTime::now_utc().date();
    let yesterday = today - time::Duration::days(1);

    // 진도는 사람마다. 읽기 전용은 자기 진도도 남기지 못한다(개정 2026-10-26 ⑧, D-20). 다시 보아 완료해도 완료는 한 번.
    assert_eq!(
        progress(&app, &reader, &eps[0], json!({ "status": "done" })).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        progress(&app, &writer, &eps[0], json!({ "status": "done" })).await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        progress(
            &app,
            &writer,
            &eps[0],
            json!({ "status": "in_progress", "last_position_seconds": 300,
                                                        "note": "다시 보기" })
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        progress(
            &app,
            &writer,
            &eps[0],
            json!({ "status": "done", "note": "다시 보기" })
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        progress(
            &app,
            &writer,
            &eps[1],
            json!({ "status": "in_progress", "planned_on": yesterday.to_string() })
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        progress(&app, &outsider, &eps[1], json!({ "status": "done" })).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        progress(&app, &writer, &eps[1], json!({ "status": "finished" })).await,
        StatusCode::BAD_REQUEST
    );
    let mine = get(&app, &writer, &item).await;
    assert_eq!(mine["episodes"][0]["mine"]["status"], "done");
    assert_eq!(mine["episodes"][0]["mine"]["watch_count"], 2);
    assert_eq!(mine["episodes"][0]["mine"]["note"], "다시 보기");
    assert_eq!(
        mine["episodes"][1]["mine"]["planned_on"],
        yesterday.to_string()
    );
    // 다른 사람에게는 내 진도·메모가 보이지 않는다.
    let theirs = get(&app, &admin, &item).await;
    assert_eq!(theirs["episodes"][0]["mine"]["status"], "not_started");
    assert_eq!(theirs["episodes"][0]["mine"]["note"], "");

    // 함께 수강하는 사람의 진도는 집계만(완료 1, 수강 중 1, 밀린 회차: 행렬식은 계획일이 없음).
    let parts = send(
        &app,
        Method::GET,
        &format!("/api/items/{item}/course/participants?today={today}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(parts.status, StatusCode::OK, "{:?}", parts.json);
    let p = parts
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["display_name"] == "member")
        .unwrap()
        .clone();
    assert_eq!(p["done"], 1);
    assert_eq!(p["in_progress"], 1);
    assert_eq!(p["overdue"], 1);
    assert_eq!(p["done_seconds"], 1200);
    assert!(p.get("note").is_none());
    // 감사 S9: 서버 날짜에서 하루 넘게 떨어진 today는 400(개인 계획일을 알아내지 못하게).
    for far in [
        today + time::Duration::days(30),
        today - time::Duration::days(30),
    ] {
        let r = send(
            &app,
            Method::GET,
            &format!("/api/items/{item}/course/participants?today={far}"),
            Some(&writer),
            None,
        )
        .await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    }

    // 회차를 시간 기록 대상으로 고를 수 있다.
    let entry = send(
        &app,
        Method::POST,
        "/api/me/time-entries",
        Some(&reader),
        Some(json!({ "request_id": new_request_id(), "started_at": "2026-10-20T09:00:00+09:00",
                     "ended_at": "2026-10-20T09:20:00+09:00", "paused_seconds": 0, "kind": "lecture",
                     "memo": "", "target": { "kind": "episode", "id": eps[1]["id"] } })),
    )
    .await;
    assert_eq!(entry.status, StatusCode::CREATED, "{:?}", entry.json);
    assert!(
        entry.json["target"]["label"]
            .as_str()
            .unwrap()
            .contains("행렬"),
        "{:?}",
        entry.json
    );

    // 회차 고치기·지우기(쓸 수 있는 사람). 지우면 그 회차의 진도도 사라진다.
    let ep = |e: &Value| format!("/api/episodes/{}", e["id"].as_str().unwrap());
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &ep(&eps[2]),
            Some(&writer),
            Some(
                json!({ "version": eps[2]["version"], "number": 3, "title": "행렬식과 역행렬", "duration_seconds": 900 })
            )
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::DELETE, &ep(&eps[0]), Some(&reader), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &ep(&eps[0]), Some(&writer), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let after = get(&app, &reader, &item).await;
    assert_eq!(after["episodes"].as_array().unwrap().len(), 2);
    assert_eq!(after["episodes"][1]["title"], "행렬식과 역행렬");
    // 강의가 아닌 항목은 강좌가 아니다.
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "P", "key": "PRJ" }),
        ),
    )
    .await;
    let other = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "프로젝트" })),
    )
    .await;
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/items/{}/course", other.json["id"].as_str().unwrap()),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

/// 리뷰 반영: 완료한 날은 사용자의 오늘, 완료를 풀면 지움. 링크는 http·https만. 비공개로 바뀐 강좌의 집계에서
/// 볼 수 없게 된 멤버는 빠진다.
#[tokio::test]
async fn completion_dates_links_and_participant_visibility() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "강의 리뷰").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = course(&app, &admin, &ws).await;

    let bad = json!({ "subject": "", "instructor": "", "platform": "", "url": "javascript:alert(1)",
                      "textbook": "", "starts_on": null, "expires_on": null, "target_on": null });
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}/course"),
        Some(&admin),
        Some(bad),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    let episodes_uri = format!("/api/items/{item}/episodes");
    let r = send(
        &app,
        Method::POST,
        &episodes_uri,
        Some(&admin),
        Some(
            json!({ "episodes": [{ "request_id": new_request_id(), "title": "나쁜 링크",
                                    "url": "data:text/html,hi" }] }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
    let r = send(
        &app,
        Method::POST,
        &episodes_uri,
        Some(&admin),
        Some(
            json!({ "episodes": [{ "request_id": new_request_id(), "title": "벡터",
                                    "url": "https://example.test/1" }] }),
        ),
    )
    .await;
    assert_eq!(r.json["added"], 1, "{:?}", r.json);

    let episode = get(&app, &writer, &item).await["episodes"][0].clone();
    let today = time::OffsetDateTime::now_utc().date();
    let local = today.next_day().unwrap();
    let far = today.saturating_add(time::Duration::days(5));
    assert_eq!(
        progress(
            &app,
            &writer,
            &episode,
            json!({ "status": "done", "note": "", "today": far.to_string() })
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        progress(
            &app,
            &writer,
            &episode,
            json!({ "status": "done", "note": "", "today": local.to_string() })
        )
        .await,
        StatusCode::NO_CONTENT
    );
    let mine = |c: Value| c["episodes"][0]["mine"].clone();
    assert_eq!(
        mine(get(&app, &writer, &item).await)["completed_on"],
        local.to_string()
    );
    assert_eq!(
        progress(
            &app,
            &writer,
            &episode,
            json!({ "status": "in_progress", "note": "", "today": local.to_string() })
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert!(mine(get(&app, &writer, &item).await)["completed_on"].is_null());

    let participants = |user: TestUser| {
        let app = app.clone();
        let item = item.clone();
        async move {
            send(
                &app,
                Method::GET,
                &format!("/api/items/{item}/course/participants?today={today}"),
                Some(&user),
                None,
            )
            .await
            .json
        }
    };
    let names = |v: Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|p| p["display_name"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(names(participants(admin.clone()).await), ["member"]);
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}"),
        Some(&admin),
        Some(json!({ "base_version": 2, "title": "선형대수", "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert!(names(participants(admin.clone()).await).is_empty());
}

/// 강의 모듈만 보관해도 강좌는 읽기 전용이다. 조회의 편집 가능 표시가 쓰기 거부와 같아야 화면이 고치기 칸을 열지
/// 않는다(보관 해제하면 다시 고칠 수 있다).
#[tokio::test]
async fn archiving_the_course_module_makes_the_course_read_only() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "강의 보관").await;
    let m = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "course", "name": "강의", "key": "ARC" })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    let module = m.json["id"].as_str().unwrap().to_owned();
    let i = send(
        &app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "선형대수" })),
    )
    .await;
    let item = i.json["id"].as_str().unwrap().to_owned();
    assert_eq!(get(&app, &admin, &item).await["can_write"], true);

    let archive = |archived: bool| {
        let (app, admin, module) = (app.clone(), admin.clone(), module.clone());
        async move {
            send(
                &app,
                Method::PUT,
                &format!("/api/modules/{module}/archived"),
                Some(&admin),
                Some(json!({ "archived": archived })),
            )
            .await
            .status
        }
    };
    assert_eq!(archive(true).await, StatusCode::OK);
    assert_eq!(get(&app, &admin, &item).await["can_write"], false);
    let details = json!({ "subject": "수학" });
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}/course"),
        Some(&admin),
        Some(details.clone()),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);

    assert_eq!(archive(false).await, StatusCode::OK);
    assert_eq!(get(&app, &admin, &item).await["can_write"], true);
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}/course"),
        Some(&admin),
        Some(details),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT, "{:?}", r.json);
}

// 두 사람이 같은 판의 회차를 보고 고치면 먼저 저장한 내용이 남고 뒤의 저장은 409로 아무것도 바꾸지 않는다(전에는 id만으로
// 통째로 바꿔 뒤의 저장이 앞의 것을 조용히 덮었다). 동시에 보내도 하나만 바뀐다. 다시 읽은 판으로 보내면 바뀐다.
#[tokio::test]
async fn two_writers_on_the_same_episode_version_do_not_overwrite() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "강의 동시 수정").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let item = course(&app, &admin, &ws).await;
    let added = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/episodes"),
        Some(&admin),
        Some(json!({ "episodes": [
            { "request_id": new_request_id(), "title": "벡터", "duration_seconds": 600 },
            { "request_id": new_request_id(), "title": "행렬", "duration_seconds": 600 }
        ]})),
    )
    .await;
    assert_eq!(added.json["added"], 2, "{:?}", added.json);
    let episodes = get(&app, &admin, &item).await["episodes"].clone();
    let (seen, other) = (episodes[0].clone(), episodes[1].clone());
    let uri = format!("/api/episodes/{}", seen["id"].as_str().unwrap());
    let base = seen["version"].clone();

    let first = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(
            json!({ "version": base, "number": 1, "title": "벡터 공간", "duration_seconds": 600,
                     "url": "https://example.test/a" }),
        ),
    )
    .await;
    assert_eq!(first.status, StatusCode::NO_CONTENT, "{:?}", first.json);
    let second = send(
        &app,
        Method::PUT,
        &uri,
        Some(&writer),
        Some(
            json!({ "version": base, "number": 1, "title": "벡터 기초", "duration_seconds": 900 }),
        ),
    )
    .await;
    assert_eq!(second.status, StatusCode::CONFLICT, "{:?}", second.json);
    // D-131·#180: 판 충돌은 공유 회차 자원과 요청 중 다른 칸을 돌려준다.
    assert_eq!(second.json["error"], "conflict");
    assert_eq!(second.json["message"], "the episode changed; reload it");
    assert_eq!(second.json["current"]["version"], 2);
    assert_eq!(second.json["current"]["title"], "벡터 공간");
    assert!(second.json["current"]["mine"].is_null());
    assert!(
        second.json["changed"]
            .as_array()
            .unwrap()
            .contains(&json!("title"))
    );
    assert!(
        !second.json["changed"]
            .as_array()
            .unwrap()
            .contains(&json!("version"))
    );
    let now = get(&app, &writer, &item).await["episodes"][0].clone();
    assert_eq!(now["title"], "벡터 공간");
    assert_eq!(now["url"], "https://example.test/a");
    assert_eq!(now["duration_seconds"], 600);
    assert_ne!(now["version"], base);

    let retry = send(
        &app,
        Method::PUT,
        &uri,
        Some(&writer),
        Some(json!({ "version": now["version"], "number": 1, "title": "벡터 기초", "duration_seconds": 900 })),
    )
    .await;
    assert_eq!(retry.status, StatusCode::NO_CONTENT, "{:?}", retry.json);
    assert_eq!(
        get(&app, &admin, &item).await["episodes"][0]["title"],
        "벡터 기초"
    );

    // 같은 판으로 동시에: 하나만 바뀌고 다른 하나는 409.
    let other_uri = format!("/api/episodes/{}", other["id"].as_str().unwrap());
    let body = |title: &str| json!({ "version": other["version"], "number": 2, "title": title, "duration_seconds": 600 });
    let (a, b) = tokio::join!(
        send(
            &app,
            Method::PUT,
            &other_uri,
            Some(&admin),
            Some(body("행렬 A"))
        ),
        send(
            &app,
            Method::PUT,
            &other_uri,
            Some(&writer),
            Some(body("행렬 B"))
        )
    );
    let mut statuses = [a.status, b.status];
    statuses.sort();
    assert_eq!(statuses, [StatusCode::NO_CONTENT, StatusCode::CONFLICT]);
    let kept = &get(&app, &admin, &item).await["episodes"][1];
    let winner = if a.status == StatusCode::NO_CONTENT {
        "행렬 A"
    } else {
        "행렬 B"
    };
    assert_eq!(kept["title"], winner);
}

/// D-113(개정 ⑯)·D-20(⑧): 본인 진도 저장도 쓰기다. 보관된 항목이나 보관된 모듈의 항목에서는 멤버의 회차 진도 저장이
/// 409 `invalid_state`("the item is archived", 웹의 보관 문구)이고, 열린 항목에서는 저장된다.
#[tokio::test]
async fn episode_progress_is_refused_under_an_archived_item_or_module() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "강의 진도 보관").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let open_module = course_module(&app, &admin, &ws, "OPN").await;
    let archived_module = course_module(&app, &admin, &ws, "AMD").await;
    let open = course_item(&app, &admin, &open_module, "열린 강의").await;
    let archived_item = course_item(&app, &admin, &open_module, "보관된 강의").await;
    let in_archived_module =
        course_item(&app, &admin, &archived_module, "보관된 모듈의 강의").await;
    let mut episodes = Vec::new();
    for item in [&open, &archived_item, &in_archived_module] {
        let added = send(
            &app,
            Method::POST,
            &format!("/api/items/{item}/episodes"),
            Some(&admin),
            Some(json!({ "episodes": [{ "request_id": new_request_id(), "title": "1회" }] })),
        )
        .await;
        assert_eq!(added.json["added"], 1, "{:?}", added.json);
        episodes.push(get(&app, &member, item).await["episodes"][0].clone());
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
    for (episode, archived) in [
        (&episodes[0], false),
        (&episodes[1], true),
        (&episodes[2], true),
    ] {
        let r = send(
            &app,
            Method::PUT,
            &format!("/api/episodes/{}/progress", episode["id"].as_str().unwrap()),
            Some(&member),
            Some(json!({ "status": "done" })),
        )
        .await;
        if archived {
            assert_eq!(r.status, StatusCode::CONFLICT, "{episode}: {:?}", r.json);
            assert_eq!(r.json["error"], "invalid_state", "{:?}", r.json);
            assert_eq!(r.json["message"], "the item is archived", "{:?}", r.json);
        } else {
            assert_eq!(r.status, StatusCode::NO_CONTENT, "{episode}: {:?}", r.json);
        }
    }
}

async fn course_module(app: &Router, admin: &TestUser, ws: &str, key: &str) -> String {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "course", "name": key, "key": key })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    m.json["id"].as_str().unwrap().to_owned()
}

/// 워크스페이스에 공개한 강의 항목.
async fn course_item(app: &Router, admin: &TestUser, module: &str, title: &str) -> String {
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    let id = i.json["id"].as_str().unwrap().to_owned();
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "base_version": 1, "title": title, "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    id
}
