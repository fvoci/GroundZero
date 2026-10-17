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

/// 복제(D-33): 같은 종류 모듈로, 업무·하위 업무·체크리스트·선행 관계·문서 트리를 새 번호로. 새로 시작이면 상태·날짜·체크를
/// 비운다. 복제를 허용하지 않은 남의 문서는 건너뛴다(D-20). 템플릿 표시.
#[tokio::test]
async fn items_copy_with_fresh_numbers_and_templates() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 복제").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (mate, _) = add_member(&app, &state, &admin, &ws, "mate", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let p1 = module(&app, &admin, &ws, "project", "ONE").await;
    let p2 = module(&app, &admin, &ws, "project", "TWO").await;
    let research = module(&app, &admin, &ws, "research", "RES").await;
    let src = item(&app, &writer, &p1, "출시 준비").await;
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{src}/members"),
        Some(&writer),
        Some(json!({ "user_ids": [mate.id.0, reader.id.0], "group_ids": [] })),
    )
    .await;

    let task = |title: &str, parent: Option<&str>| {
        let app = app.clone();
        let writer = writer.clone();
        let src = src.clone();
        let body = json!({ "request_id": new_request_id(), "title": title, "parent_id": parent });
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/items/{src}/tasks"),
                Some(&writer),
                Some(body),
            )
            .await
            .json
        }
    };
    let a = task("설계", None).await;
    let a_id = a["id"].as_str().unwrap().to_owned();
    task("화면 설계", Some(&a_id)).await;
    let b = task("구현", None).await;
    let b_id = b["id"].as_str().unwrap().to_owned();
    let done = send(&app, Method::PUT, &format!("/api/tasks/{a_id}"), Some(&writer), Some(json!({
        "base_version": a["version"], "title": "설계", "description": "", "status": "done", "priority": "high",
        "start_date": null, "due_date": "2026-11-01", "estimate_minutes": 120, "tags": ["ux"], "milestone": false,
        "assignee_ids": [writer.id.0] }))).await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    send(
        &app,
        Method::POST,
        &format!("/api/tasks/{b_id}/checklist"),
        Some(&writer),
        Some(json!({
        "items": [{ "request_id": new_request_id(), "text": "API", "checked": true },
                  { "request_id": new_request_id(), "text": "화면" }] })),
    )
    .await;
    let dep = send(
        &app,
        Method::PUT,
        &format!("/api/tasks/{b_id}/predecessors/{a_id}"),
        Some(&writer),
        None,
    )
    .await;
    assert!(dep.status.is_success(), "{:?}", dep.json);
    let doc = |user: &TestUser, title: &str| {
        let app = app.clone();
        let user = user.clone();
        let src = src.clone();
        let body = json!({ "request_id": new_request_id(), "title": title });
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/items/{src}/documents"),
                Some(&user),
                Some(body),
            )
            .await
        }
    };
    let mine = doc(&writer, "요구사항").await;
    assert_eq!(mine.status, StatusCode::CREATED, "{:?}", mine.json);
    let theirs = doc(&mate, "남의 메모").await;
    assert_eq!(theirs.status, StatusCode::CREATED, "{:?}", theirs.json);
    // 팀 문서의 소유 권한은 관리자에게 있다. 작성자가 복제하려면 복제 허용이 필요하다(D-20).
    let allow = send(
        &app,
        Method::PUT,
        &format!(
            "/api/documents/{}/settings",
            mine.json["id"].as_str().unwrap()
        ),
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": true })),
    )
    .await;
    assert_eq!(allow.status, StatusCode::OK, "{:?}", allow.json);

    // 다른 종류 모듈로는 400, 독자는 대상 모듈에 항목을 만들 수 없다.
    let copy_uri = format!("/api/items/{src}/copy");
    let body = |module: &str, reset: bool| {
        json!({ "request_id": new_request_id(), "module_id": module,
        "title": "", "reset": reset })
    };
    assert_eq!(
        send(
            &app,
            Method::POST,
            &copy_uri,
            Some(&writer),
            Some(body(&research, true))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &copy_uri,
            Some(&reader),
            Some(body(&p2, true))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let request = body(&p2, true);
    let copied = send(
        &app,
        Method::POST,
        &copy_uri,
        Some(&writer),
        Some(request.clone()),
    )
    .await;
    assert_eq!(copied.status, StatusCode::CREATED, "{:?}", copied.json);
    assert_eq!(copied.json["skipped_documents"], 1);
    let new_id = copied.json["item_id"].as_str().unwrap().to_owned();
    // 같은 요청 id면 같은 항목.
    let again = send(&app, Method::POST, &copy_uri, Some(&writer), Some(request)).await;
    assert_eq!(again.json["item_id"], copied.json["item_id"]);

    let new_item = send(
        &app,
        Method::GET,
        &format!("/api/items/{new_id}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(new_item.json["title"], "출시 준비");
    assert_eq!(new_item.json["visibility"], "private");
    assert_eq!(new_item.json["module_id"], p2.as_str());
    // 새 항목은 비공개: 원래 항목의 멤버였던 사람도 보지 못한다.
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/items/{new_id}"),
            Some(&mate),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let tasks = send(
        &app,
        Method::GET,
        &format!("/api/items/{new_id}/tasks"),
        Some(&writer),
        None,
    )
    .await;
    let tasks = tasks.json.as_array().unwrap().clone();
    assert_eq!(tasks.len(), 3, "{tasks:?}");
    let by = |title: &str| tasks.iter().find(|t| t["title"] == title).unwrap().clone();
    let (na, na1, nb) = (by("설계"), by("화면 설계"), by("구현"));
    assert!(
        na["reference"].as_str().unwrap().starts_with("TWO-"),
        "{na:?}"
    );
    assert_eq!(na["status"], "todo");
    assert!(na["due_date"].is_null());
    assert_eq!(na["priority"], "high");
    assert_eq!(na["estimate_minutes"], 120);
    assert_eq!(na["assignees"], json!([]));
    assert_eq!(na1["parent_id"], na["id"]);
    assert_eq!(nb["predecessor_ids"], json!([na["id"]]));
    assert_eq!(nb["checklist_total"], 2);
    assert_eq!(nb["checklist_done"], 0);
    let docs = send(
        &app,
        Method::GET,
        &format!("/api/items/{new_id}/documents"),
        Some(&writer),
        None,
    )
    .await;
    let titles: Vec<&str> = docs
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["title"].as_str().unwrap())
        .collect();
    assert_eq!(titles, ["요구사항"]);

    // 그대로 복제(새로 시작이 아님)는 상태·날짜를 지킨다.
    let kept = send(
        &app,
        Method::POST,
        &copy_uri,
        Some(&writer),
        Some(body(&p1, false)),
    )
    .await;
    let kept_id = kept.json["item_id"].as_str().unwrap().to_owned();
    let kept_tasks = send(
        &app,
        Method::GET,
        &format!("/api/items/{kept_id}/tasks"),
        Some(&writer),
        None,
    )
    .await;
    let design = kept_tasks
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["title"] == "설계")
        .unwrap()
        .clone();
    assert_eq!(design["status"], "done");
    assert_eq!(design["due_date"], "2026-11-01");

    // 템플릿 표시는 쓸 수 있는 사람이, 목록에 보인다.
    let template = format!("/api/items/{src}/template");
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &template,
            Some(&reader),
            Some(json!({ "template": true }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &template,
            Some(&mate),
            Some(json!({ "template": true }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/modules/{p1}/items"),
        Some(&writer),
        None,
    )
    .await;
    let listed = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == src.as_str())
        .unwrap()
        .clone();
    assert_eq!(listed["is_template"], true);
}

/// 리뷰 반영: 그룹에서 빠지거나 그룹이 지워지면 담당·리더에서도 빠진다. 이동 미리보기와 담당자 후보는 항목 멤버를
/// 센다. 워크스페이스를 떠난 사람은 다시 초대돼도 예전 비공개 항목이 열리지 않는다.
#[tokio::test]
async fn group_changes_and_leaving_end_item_roles() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "그룹 정리").await;
    let (owner, _) = add_member(&app, &state, &admin, &ws, "owner", "writer").await;
    let (grouped, email) = add_member(&app, &state, &admin, &ws, "grouped", "writer").await;
    let m = module(&app, &admin, &ws, "project", "GRP").await;
    let it = item(&app, &owner, &m, "비공개").await;
    let item_uri = format!("/api/items/{it}");
    let members_uri = format!("/api/items/{it}/members");
    let profile_uri = format!("/api/items/{it}/profile");

    let new_group = || async {
        let g = send(
            &app,
            Method::POST,
            &format!("/api/workspaces/{ws}/groups"),
            Some(&admin),
            Some(json!({ "name": format!("그룹 {}", new_request_id()) })),
        )
        .await;
        let gid = g.json["id"].as_str().unwrap().to_owned();
        let joined = send(
            &app,
            Method::PUT,
            &format!("/api/groups/{gid}/members/{}", grouped.id.0),
            Some(&admin),
            None,
        )
        .await;
        assert_eq!(joined.status, StatusCode::NO_CONTENT, "{:?}", joined.json);
        let set = send(
            &app,
            Method::PUT,
            &members_uri,
            Some(&owner),
            Some(json!({ "user_ids": [], "group_ids": [gid] })),
        )
        .await;
        assert_eq!(set.status, StatusCode::NO_CONTENT, "{:?}", set.json);
        gid
    };
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{it}/tasks"),
        Some(&owner),
        Some(json!({ "request_id": new_request_id(), "title": "할 일" })),
    )
    .await;
    let task_uri = format!("/api/tasks/{}", task.json["id"].as_str().unwrap());
    // 리더와 담당으로 정하고, 둘 다 정해졌는지 돌려준다.
    let take_roles = || async {
        let p = send(
            &app,
            Method::PUT,
            &profile_uri,
            Some(&owner),
            Some(json!({
            "leader_id": grouped.id.0, "goal": "", "scope": "", "starts_on": null, "ends_on": null,
            "status": "active", "details": {} })),
        )
        .await;
        assert_eq!(p.status, StatusCode::NO_CONTENT, "{:?}", p.json);
        let t = send(&app, Method::GET, &task_uri, Some(&owner), None).await;
        let a = send(&app, Method::PUT, &task_uri, Some(&owner), Some(json!({
            "base_version": t.json["version"], "title": "할 일", "description": "", "status": "todo",
            "priority": "none", "start_date": null, "due_date": null, "estimate_minutes": null, "tags": [],
            "milestone": false, "assignee_ids": [grouped.id.0] }))).await;
        assert_eq!(a.status, StatusCode::OK, "{:?}", a.json);
    };
    let roles_cleared = || async {
        let p = send(&app, Method::GET, &profile_uri, Some(&owner), None).await;
        let t = send(&app, Method::GET, &task_uri, Some(&owner), None).await;
        p.json["leader"].is_null() && t.json["assignees"] == json!([])
    };

    // 그룹 멤버도 담당자 후보이고, 이동 미리보기에 "새로 보게 되는 사람"으로 나온다.
    let gid = new_group().await;
    let assignable = send(
        &app,
        Method::GET,
        &format!("/api/items/{it}/assignable"),
        Some(&owner),
        None,
    )
    .await;
    assert!(
        assignable
            .json
            .to_string()
            .contains(&grouped.id.0.to_string()),
        "{:?}",
        assignable.json
    );
    let doc = common::create_doc(&app, &owner, &ws, "옮길 문서").await;
    let doc_id = doc["id"].as_str().unwrap();
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/move/preview"),
        Some(&admin),
        Some(json!({ "target_workspace_id": ws, "target_item_id": it })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    assert!(
        !preview.json["loses"].to_string().contains("grouped"),
        "{:?}",
        preview.json
    );

    // 그룹에서 빠지면 리더·담당에서도 빠진다.
    take_roles().await;
    assert!(!roles_cleared().await);
    send(
        &app,
        Method::DELETE,
        &format!("/api/groups/{gid}/members/{}", grouped.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(roles_cleared().await);

    // 그룹이 지워져도 마찬가지.
    let gid = new_group().await;
    take_roles().await;
    let deleted = send(
        &app,
        Method::DELETE,
        &format!("/api/groups/{gid}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{:?}", deleted.json);
    assert!(roles_cleared().await);
    assert_eq!(
        status_of(&app, &grouped, Method::GET, &item_uri, None).await,
        StatusCode::NOT_FOUND
    );

    // 직접 멤버였다가 워크스페이스를 떠나면, 다시 초대돼도 비공개 항목은 열리지 않는다.
    send(
        &app,
        Method::PUT,
        &members_uri,
        Some(&owner),
        Some(json!({ "user_ids": [grouped.id.0], "group_ids": [] })),
    )
    .await;
    assert_eq!(
        status_of(&app, &grouped, Method::GET, &item_uri, None).await,
        StatusCode::OK
    );
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", grouped.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(&admin),
        Some(json!({ "email": email, "role": "writer" })),
    )
    .await;
    let accepted = send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv.json["id"].as_str().unwrap()
        ),
        Some(&grouped),
        None,
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK, "{:?}", accepted.json);
    assert_eq!(
        status_of(&app, &grouped, Method::GET, &item_uri, None).await,
        StatusCode::NOT_FOUND
    );
    let p = send(&app, Method::GET, &profile_uri, Some(&owner), None).await;
    assert_eq!(p.json["members"], json!([]), "{:?}", p.json);
}
