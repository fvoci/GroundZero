#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 항목 정보와 항목 멤버 (D-33·D-34·D-36·D-38, D-20): 비공개 항목은 멤버(사람·그룹)만 워크스페이스 역할 안에서
//! 보고, 멤버는 소유 권한자만 정한다. 빠지면 바로 보지 못하고 담당·리더에서도 빠진다. 종류별 정보는 서버가 확인한다.

mod common;

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
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    m.json["id"].as_str().unwrap().to_owned()
}

async fn item(app: &Router, user: &TestUser, module: &str, title: &str) -> String {
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    i.json["id"].as_str().unwrap().to_owned()
}

async fn status_of(
    app: &Router,
    user: &TestUser,
    method: Method,
    uri: &str,
    body: Option<Value>,
) -> StatusCode {
    send(app, method, uri, Some(user), body).await.status
}

#[tokio::test]
async fn private_items_open_to_their_members_only() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 멤버").await;
    let (owner, _) = add_member(&app, &state, &admin, &ws, "owner", "writer").await;
    let (mate, _) = add_member(&app, &state, &admin, &ws, "mate", "writer").await;
    let (viewer, _) = add_member(&app, &state, &admin, &ws, "viewer", "reader").await;
    let (grouped, _) = add_member(&app, &state, &admin, &ws, "grouped", "writer").await;
    let outsider = login(&state, "outsider").await;
    let m = module(&app, &admin, &ws, "project", "PRJ").await;
    let it = item(&app, &owner, &m, "비공개 프로젝트").await;
    let item_uri = format!("/api/items/{it}");
    let members_uri = format!("/api/items/{it}/members");
    let tasks_uri = format!("/api/items/{it}/tasks");

    // 새 항목은 비공개: 만든 사람·관리자만.
    assert_eq!(
        status_of(&app, &mate, Method::GET, &item_uri, None).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(&app, &admin, Method::GET, &item_uri, None).await,
        StatusCode::OK
    );

    // 멤버는 소유 권한자만 정한다. 워크스페이스 밖 사람은 400.
    let members = json!({ "user_ids": [mate.id.0, viewer.id.0], "group_ids": [] });
    assert_eq!(
        status_of(
            &app,
            &mate,
            Method::PUT,
            &members_uri,
            Some(members.clone())
        )
        .await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(
            &app,
            &owner,
            Method::PUT,
            &members_uri,
            Some(json!({ "user_ids": [outsider.id.0], "group_ids": [] }))
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(&app, &owner, Method::PUT, &members_uri, Some(members)).await,
        StatusCode::NO_CONTENT
    );
    // 멤버는 역할 안에서: 작성자는 쓰고, 독자는 읽기만, 멤버를 정하지는 못한다.
    assert_eq!(
        status_of(&app, &mate, Method::GET, &item_uri, None).await,
        StatusCode::OK
    );
    let task = send(
        &app,
        Method::POST,
        &tasks_uri,
        Some(&mate),
        Some(json!({ "request_id": new_request_id(), "title": "함께 할 일" })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    assert_eq!(
        status_of(
            &app,
            &viewer,
            Method::POST,
            &tasks_uri,
            Some(json!({ "request_id": new_request_id(), "title": "안 됨" }))
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status_of(&app, &viewer, Method::GET, &item_uri, None).await,
        StatusCode::OK
    );
    assert_eq!(
        status_of(
            &app,
            &mate,
            Method::PUT,
            &members_uri,
            Some(json!({ "user_ids": [], "group_ids": [] }))
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // 리더는 그 항목을 볼 수 있는 사람만.
    let profile_uri = format!("/api/items/{it}/profile");
    let profile = |leader: &TestUser| {
        json!({ "leader_id": leader.id.0, "goal": "출시", "scope": "웹",
        "starts_on": "2026-10-01", "ends_on": "2026-12-31", "status": "active", "details": {} })
    };
    assert_eq!(
        status_of(
            &app,
            &owner,
            Method::PUT,
            &profile_uri,
            Some(profile(&grouped))
        )
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        status_of(
            &app,
            &owner,
            Method::PUT,
            &profile_uri,
            Some(profile(&mate))
        )
        .await,
        StatusCode::NO_CONTENT
    );
    // 업무 담당으로 지정.
    let assigned = send(&app, Method::PUT, &format!("/api/tasks/{}", task.json["id"].as_str().unwrap()), Some(&owner), Some(json!({
        "base_version": task.json["version"], "title": "함께 할 일", "description": "", "status": "todo", "priority": "none",
        "start_date": null, "due_date": null, "estimate_minutes": null, "tags": [], "milestone": false,
        "assignee_ids": [mate.id.0] }))).await;
    assert_eq!(assigned.status, StatusCode::OK, "{:?}", assigned.json);

    // 그룹으로 넣기.
    let group = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&admin),
        Some(json!({ "name": "디자인" })),
    )
    .await;
    let gid = group.json["id"].as_str().unwrap().to_owned();
    send(
        &app,
        Method::PUT,
        &format!("/api/groups/{gid}/members/{}", grouped.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        status_of(&app, &grouped, Method::GET, &item_uri, None).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        status_of(
            &app,
            &owner,
            Method::PUT,
            &members_uri,
            Some(json!({ "user_ids": [viewer.id.0], "group_ids": [gid] }))
        )
        .await,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        status_of(&app, &grouped, Method::GET, &item_uri, None).await,
        StatusCode::OK
    );

    // 빠진 사람은 바로 보지 못하고, 담당과 리더에서도 빠진다.
    assert_eq!(
        status_of(&app, &mate, Method::GET, &item_uri, None).await,
        StatusCode::NOT_FOUND
    );
    let p = send(&app, Method::GET, &profile_uri, Some(&owner), None).await;
    assert!(p.json["leader"].is_null(), "{:?}", p.json);
    assert_eq!(p.json["members"][0]["display_name"], "viewer");
    assert_eq!(p.json["groups"][0]["name"], "디자인");
    assert_eq!(p.json["can_manage_members"], true);
    let t = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{}", task.json["id"].as_str().unwrap()),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(t.json["assignees"], json!([]), "{:?}", t.json);
    let v = send(&app, Method::GET, &profile_uri, Some(&viewer), None).await;
    assert_eq!(v.json["can_edit"], false);
    assert_eq!(v.json["can_manage_members"], false);

    // 그룹에서 빠지면 보지 못한다.
    send(
        &app,
        Method::DELETE,
        &format!("/api/groups/{gid}/members/{}", grouped.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        status_of(&app, &grouped, Method::GET, &item_uri, None).await,
        StatusCode::NOT_FOUND
    );
    // 워크스페이스 밖 사람은 멤버 목록도 볼 수 없다.
    assert_eq!(
        status_of(&app, &outsider, Method::GET, &profile_uri, None).await,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn profile_fields_follow_the_module_kind() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 정보").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let research = module(&app, &admin, &ws, "research", "RES").await;
    let hobby = module(&app, &admin, &ws, "hobby", "HOB").await;
    let r = item(&app, &admin, &research, "학습 동기 연구").await;
    let h = item(&app, &admin, &hobby, "도자기").await;
    let put = |it: &str, body: Value| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/items/{it}/profile");
        async move { send(&app, Method::PUT, &uri, Some(&admin), Some(body)).await }
    };
    let base = |details: Value| {
        json!({ "leader_id": null, "goal": "", "scope": "", "starts_on": null,
        "ends_on": null, "status": "planned", "details": details })
    };

    let ok = put(&r, base(json!({ "field": "교육학", "keywords": ["동기", "동기", " 학습 "],
        "question": "무엇이 지속을 돕나", "hypothesis": "피드백", "method": "설문", "sample": "대학생 200명" }))).await;
    assert_eq!(ok.status, StatusCode::NO_CONTENT, "{:?}", ok.json);
    let got = send(
        &app,
        Method::GET,
        &format!("/api/items/{r}/profile"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(got.json["details"]["keywords"], json!(["동기", "학습"]));
    assert_eq!(got.json["status"], "planned");
    // 종류에 없는 칸, 모르는 칸, 잘못된 상태·기간은 400.
    assert_eq!(
        put(&h, base(json!({ "field": "미술" }))).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        put(&h, base(json!({ "color": "파랑" }))).await.status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        put(&h, base(json!({ "activity_type": "공예" })))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let mut bad = base(json!({}));
    bad["status"] = json!("late");
    assert_eq!(put(&h, bad).await.status, StatusCode::BAD_REQUEST);
    let mut reversed = base(json!({}));
    reversed["starts_on"] = json!("2026-12-01");
    reversed["ends_on"] = json!("2026-11-01");
    assert_eq!(put(&h, reversed).await.status, StatusCode::BAD_REQUEST);
    // 목록에 상태·기간이 보인다.
    let list = send(
        &app,
        Method::GET,
        &format!("/api/modules/{hobby}/items"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(list.json[0]["status"], "planned", "{:?}", list.json);

    // 모듈 아이콘은 관리자만.
    let icon = format!("/api/modules/{hobby}/icon");
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &icon,
            Some(&writer),
            Some(json!({ "icon": "🏺" }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &icon,
            Some(&admin),
            Some(json!({ "icon": "🏺" }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let m = send(
        &app,
        Method::GET,
        &format!("/api/modules/{hobby}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(m.json["icon"], "🏺");
}
