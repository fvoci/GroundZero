#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 홈: Up Next·최근 작업·진행 중인 항목·고정·즐겨찾기 (D-19).

use crate::common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};
use time::{Date, Duration};

fn day(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day())
}

async fn item(app: &Router, admin: &TestUser, ws: &str, key: &str, public: bool) -> Value {
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
    if public {
        send(
            app,
            Method::PUT,
            &format!("/api/items/{}", i.json["id"].as_str().unwrap()),
            Some(admin),
            Some(json!({ "base_version": 1, "title": key, "description": "", "visibility": "workspace" })),
        )
        .await;
    }
    i.json
}

async fn task(
    app: &Router,
    user: &TestUser,
    item: &Value,
    title: &str,
    due: Option<String>,
    assignees: Vec<String>,
    status: &str,
) -> Value {
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
                 Some(json!({ "base_version": t["version"], "title": title, "description": "", "status": status,
                              "priority": "none", "start_date": null, "due_date": due, "estimate_minutes": null,
                              "tags": [], "assignee_ids": assignees, "milestone": false }))).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

#[tokio::test]
async fn home_collects_my_upcoming_work_and_marks_only_what_i_can_see() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "홈").await;
    let (me, _) = add_member(&app, &state, &admin, &ws, "me", "member").await;
    let shared = item(&app, &admin, &ws, "PUB", true).await;
    let private = item(&app, &admin, &ws, "PRV", false).await;
    let today: Date = {
        let mut tx = state.db.begin_as(me.id).await.unwrap();
        sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Seoul')::date")
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    let mine = vec![me.id.0.to_string()];
    task(
        &app,
        &admin,
        &shared,
        "지난 마감",
        Some(day(today - Duration::days(1))),
        mine.clone(),
        "할 일",
    )
    .await;
    task(
        &app,
        &admin,
        &shared,
        "오늘 마감",
        Some(day(today)),
        mine.clone(),
        "진행 중",
    )
    .await;
    task(
        &app,
        &admin,
        &shared,
        "곧 마감",
        Some(day(today + Duration::days(3))),
        mine.clone(),
        "할 일",
    )
    .await;
    task(
        &app,
        &admin,
        &shared,
        "먼 마감",
        Some(day(today + Duration::days(10))),
        mine.clone(),
        "할 일",
    )
    .await;
    task(
        &app,
        &admin,
        &shared,
        "끝난 일",
        Some(day(today)),
        mine.clone(),
        "완료",
    )
    .await;
    task(
        &app,
        &admin,
        &shared,
        "남의 일",
        Some(day(today)),
        vec![],
        "할 일",
    )
    .await;
    task(
        &app,
        &me,
        &shared,
        "내가 만든 일",
        Some(day(today + Duration::days(1))),
        vec![],
        "할 일",
    )
    .await;

    let home = send(
        &app,
        Method::GET,
        &format!(
            "/api/me/home?tz=Asia/Seoul&organization_id={}",
            common::organization_for(&me).await
        ),
        Some(&me),
        None,
    )
    .await;
    assert_eq!(home.status, StatusCode::OK, "{:?}", home.json);
    let up: Vec<(String, String)> = home.json["up_next"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["title"].as_str().unwrap().to_owned(),
                t["bucket"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(
        up,
        [
            ("지난 마감".to_owned(), "overdue".to_owned()),
            ("오늘 마감".to_owned(), "today".to_owned()),
            ("내가 만든 일".to_owned(), "upcoming".to_owned()),
            ("곧 마감".to_owned(), "upcoming".to_owned()),
        ]
    );
    assert_eq!(
        home.json["up_next_counts"],
        json!({ "overdue": 1, "today": 1, "upcoming": 2 })
    );
    assert_eq!(home.json["recent_tasks"][0]["title"], "내가 만든 일");
    // 진행 중인 항목: 볼 수 있는 것만, 진행률은 업무 완료 비율.
    let items: Vec<(String, i64, i64)> = home.json["active_items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["title"].as_str().unwrap().to_owned(),
                i["tasks_done"].as_i64().unwrap(),
                i["tasks_total"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(items, [("PUB".to_owned(), 1, 7)]);

    // 고정·즐겨찾기: 볼 수 있는 대상만, 본인에게만.
    let pin = |user: TestUser, target: Value, on: bool| {
        let app = app.clone();
        async move {
            send(
                &app,
                Method::PUT,
                &format!("/api/me/marks/item/{}", target["id"].as_str().unwrap()),
                Some(&user),
                Some(json!({ "mark": "pin", "on": on })),
            )
            .await
        }
    };
    let r = pin(me.clone(), shared.clone(), true).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json, json!({ "pin": true, "favorite": false }));
    assert_eq!(
        pin(me.clone(), private.clone(), true).await.status,
        StatusCode::NOT_FOUND
    );
    let home = send(
        &app,
        Method::GET,
        &format!(
            "/api/me/home?tz=Asia/Seoul&organization_id={}",
            common::organization_for(&me).await
        ),
        Some(&me),
        None,
    )
    .await
    .json;
    assert_eq!(home["pins"][0]["label"], "PUB-1 PUB");
    let admin_home = send(
        &app,
        Method::GET,
        &format!(
            "/api/me/home?tz=Asia/Seoul&organization_id={}",
            common::organization_for(&admin).await
        ),
        Some(&admin),
        None,
    )
    .await
    .json;
    assert_eq!(admin_home["pins"], json!([]));

    // 항목을 볼 수 없게 되면 고정도 홈에서 사라지고, 업무도 Up Next에서 빠진다.
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}", shared["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "base_version": 2, "title": "PUB", "description": "", "visibility": "private" })),
    )
    .await;
    let home = send(
        &app,
        Method::GET,
        &format!(
            "/api/me/home?tz=Asia/Seoul&organization_id={}",
            common::organization_for(&me).await
        ),
        Some(&me),
        None,
    )
    .await
    .json;
    assert_eq!(home["pins"], json!([]));
    assert_eq!(home["up_next"], json!([]));
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!(
                "/api/me/home?tz=Nowhere/Land&organization_id={}",
                common::organization_for(&me).await
            ),
            Some(&me),
            None
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
}

/// 감사 W23: Up Next를 한 번에 50개로 잘라, 밀린 업무가 50개를 넘으면 오늘·다가오는 일이 빠지고 "기한 지남 50"이
/// 정확한 수처럼 보였다. 묶음마다 50개까지 보이고 묶음별 전체 수를 함께 준다.
#[tokio::test]
async fn up_next_keeps_today_and_upcoming_behind_a_long_overdue_backlog() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "밀린 일").await;
    let (me, _) = add_member(&app, &state, &admin, &ws, "me", "member").await;
    let shared = item(&app, &admin, &ws, "BKL", true).await;
    let today: Date = {
        let mut tx = state.db.begin_as(me.id).await.unwrap();
        sqlx::query_scalar("SELECT (now() AT TIME ZONE 'Asia/Seoul')::date")
            .fetch_one(tx.conn())
            .await
            .unwrap()
    };
    for n in 0..55 {
        let due = today - Duration::days(1 + n % 20);
        task(
            &app,
            &me,
            &shared,
            &format!("밀린 {n}"),
            Some(day(due)),
            vec![],
            "할 일",
        )
        .await;
    }
    for n in 0..3 {
        task(
            &app,
            &me,
            &shared,
            &format!("오늘 {n}"),
            Some(day(today)),
            vec![],
            "할 일",
        )
        .await;
    }
    task(
        &app,
        &me,
        &shared,
        "내일",
        Some(day(today + Duration::days(1))),
        vec![],
        "할 일",
    )
    .await;

    let home = send(
        &app,
        Method::GET,
        &format!(
            "/api/me/home?tz=Asia/Seoul&organization_id={}",
            common::organization_for(&me).await
        ),
        Some(&me),
        None,
    )
    .await;
    assert_eq!(home.status, StatusCode::OK, "{:?}", home.json);
    let buckets: Vec<&str> = home.json["up_next"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["bucket"].as_str().unwrap())
        .collect();
    let count = |b: &str| buckets.iter().filter(|x| **x == b).count();
    assert_eq!(
        (count("overdue"), count("today"), count("upcoming")),
        (50, 3, 1),
        "{buckets:?}"
    );
    assert_eq!(
        home.json["up_next_counts"],
        json!({ "overdue": 55, "today": 3, "upcoming": 1 })
    );
    // 기한 지남은 마감이 이른 것부터 50개(가장 늦게 밀린 5개 중 일부는 빠진다).
    assert_eq!(
        home.json["up_next"][0]["due_date"],
        day(today - Duration::days(20))
    );
}

/// 감사 C33: 고정·즐겨찾기 상태를 읽는 `GET /api/me/marks/{kind}/{id}`에는 시험이 없었다. 본인 표시만 보이고, 볼 수
/// 없는 대상은 모르는 id와 같이 둘 다 꺼짐이다(있는지 드러내지 않는다). 그런 대상에 켜기는 404다.
#[tokio::test]
async fn marks_read_back_only_for_their_owner() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "표시 읽기").await;
    let (me, _) = add_member(&app, &state, &admin, &ws, "me", "member").await;
    let shared = item(&app, &admin, &ws, "MRK", true).await;
    let private = item(&app, &admin, &ws, "MRP", false).await;
    let uri = |target: &Value| format!("/api/me/marks/item/{}", target["id"].as_str().unwrap());
    let set = send(
        &app,
        Method::PUT,
        &uri(&shared),
        Some(&me),
        Some(json!({ "mark": "favorite", "on": true })),
    )
    .await;
    assert_eq!(set.status, StatusCode::OK, "{:?}", set.json);
    let mine = send(&app, Method::GET, &uri(&shared), Some(&me), None).await;
    assert_eq!(mine.status, StatusCode::OK, "{:?}", mine.json);
    assert_eq!(mine.json, json!({ "pin": false, "favorite": true }));
    let theirs = send(&app, Method::GET, &uri(&shared), Some(&admin), None).await;
    assert_eq!(theirs.json, json!({ "pin": false, "favorite": false }));
    let hidden = send(&app, Method::GET, &uri(&private), Some(&me), None).await;
    assert_eq!(hidden.status, StatusCode::OK, "{:?}", hidden.json);
    assert_eq!(hidden.json, json!({ "pin": false, "favorite": false }));
    let unknown = format!("/api/me/marks/item/{}", uuid::Uuid::now_v7());
    assert_eq!(
        send(&app, Method::GET, &unknown, Some(&me), None)
            .await
            .json,
        hidden.json
    );
    let refused = send(
        &app,
        Method::PUT,
        &uri(&private),
        Some(&me),
        Some(json!({ "mark": "pin", "on": true })),
    )
    .await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND, "{:?}", refused.json);
}
