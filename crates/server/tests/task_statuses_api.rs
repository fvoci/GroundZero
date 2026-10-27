#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 항목별 상태 정의 API (D-116): 목록·생성·수정·삭제와 소유 권한·보관 규칙.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

/// 모듈 키는 워크스페이스 안에서 유일해야 하므로 부를 때마다 다른 키를 준다.
async fn item(app: &Router, admin: &TestUser, ws: &str, key: &str) -> String {
    let module = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
    let created = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let id = created.json["id"].as_str().unwrap().to_owned();
    // 공개 항목: 멤버·읽기 전용이 본다.
    let opened = send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(opened.status, StatusCode::OK, "{:?}", opened.json);
    id
}

fn names(list: &Value) -> Vec<(String, String)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_owned(),
                s["category"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[tokio::test]
async fn statuses_are_defined_per_item_by_its_owner_and_tasks_follow_them() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "상태").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let it = item(&app, &admin, &ws, "STS").await;
    let path = format!("/api/items/{it}/statuses");

    // 기본 세 상태(D-116).
    let list = send(&app, Method::GET, &path, Some(&reader), None).await;
    assert_eq!(list.status, StatusCode::OK, "{:?}", list.json);
    assert_eq!(
        names(&list.json),
        [("할 일", "todo"), ("진행 중", "doing"), ("완료", "done")]
            .map(|(n, c)| (n.to_owned(), c.to_owned()))
    );
    assert_eq!(
        send(&app, Method::GET, &path, Some(&outsider), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );

    // 만들기는 항목 소유 권한(관리자·만든 사람). 멤버는 403, 읽기 전용도 403.
    let body = json!({ "name": "review", "color": "#F59E0B", "category": "doing" });
    for user in [&member, &reader] {
        assert_eq!(
            send(&app, Method::POST, &path, Some(user), Some(body.clone()))
                .await
                .status,
            StatusCode::FORBIDDEN
        );
    }
    let created = send(&app, Method::POST, &path, Some(&admin), Some(body.clone())).await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    assert_eq!(created.json["name"], "review");
    assert_eq!(created.json["color"], "#f59e0b");
    assert_eq!(created.json["task_count"], 0);
    let review = created.json["id"].as_str().unwrap().to_owned();
    // 상태 수정·삭제도 같은 소유 권한이 필요하다. 기대 사용자 헤더가 맞아도
    // 일반 멤버와 읽기 전용 멤버에게 권한을 부여하지 않는다.
    for user in [&member, &reader] {
        let changed = send(
            &app,
            Method::PUT,
            &format!("{path}/{review}"),
            Some(user),
            Some(json!({ "category": "done" })),
        )
        .await;
        assert_eq!(changed.status, StatusCode::FORBIDDEN, "{:?}", changed.json);
        let removed = send(
            &app,
            Method::DELETE,
            &format!("{path}/{review}"),
            Some(user),
            None,
        )
        .await;
        assert_eq!(removed.status, StatusCode::FORBIDDEN, "{:?}", removed.json);
    }
    let unchanged = send(&app, Method::GET, &path, Some(&admin), None).await;
    assert_eq!(unchanged.status, StatusCode::OK, "{:?}", unchanged.json);
    assert_eq!(
        unchanged
            .json
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == review),
        Some(&created.json)
    );
    // 같은 이름은 409, 모르는 범주는 본문 역직렬화 거절(422, 서버의 기존 규칙).
    assert_eq!(
        send(&app, Method::POST, &path, Some(&admin), Some(body.clone()))
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(
            &app,
            Method::POST,
            &path,
            Some(&admin),
            Some(json!({ "name": "x", "category": "later" }))
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // 색은 빈 문자열 또는 #rrggbb(표의 CHECK): 토큰 이름은 400.
    assert_eq!(
        send(
            &app,
            Method::POST,
            &path,
            Some(&admin),
            Some(json!({ "name": "x", "category": "todo", "color": "amber" }))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );

    // 업무는 상태 이름으로 들어가고 응답에 범주가 붙는다.
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{it}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "검토할 것", "status": "review" })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    assert_eq!(task.json["status"], "review");
    assert_eq!(task.json["status_category"], "doing");

    // 범주를 done으로 바꾸면 그 열의 업무가 완료된다.
    let updated = send(
        &app,
        Method::PUT,
        &format!("{path}/{review}"),
        Some(&admin),
        Some(json!({ "category": "done" })),
    )
    .await;
    assert_eq!(updated.status, StatusCode::OK, "{:?}", updated.json);
    assert_eq!(updated.json["category"], "done");
    let detail = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{}", task.json["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    assert!(detail.json["completed_at"].is_string(), "{:?}", detail.json);

    // 업무가 있는 열은 옮길 열 없이 지울 수 없고(400), 옮기면 지워진다.
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("{path}/{review}"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let todo = list.json[0]["id"].as_str().unwrap().to_owned();
    let deleted = send(
        &app,
        Method::DELETE,
        &format!("{path}/{review}?move_to={todo}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(deleted.status, StatusCode::OK, "{:?}", deleted.json);
    assert_eq!(deleted.json["moved"], 1);
    let after = send(&app, Method::GET, &path, Some(&admin), None).await;
    assert_eq!(after.json.as_array().unwrap().len(), 3);
    assert_eq!(after.json[0]["task_count"], 1);

    // 다른 항목의 상태 id를 이 항목 경로로 고치려 하면 404.
    let other = item(&app, &admin, &ws, "OTH").await;
    let other_status = send(
        &app,
        Method::GET,
        &format!("/api/items/{other}/statuses"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &format!("{path}/{}", other_status.json[0]["id"].as_str().unwrap()),
            Some(&admin),
            Some(json!({ "name": "stolen" }))
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // 보관된 항목에서는 누구에게나 409가 먼저다.
    let archived = send(
        &app,
        Method::PUT,
        &format!("/api/items/{it}/archived"),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(archived.status, StatusCode::OK, "{:?}", archived.json);
    for user in [&admin, &reader] {
        assert_eq!(
            send(
                &app,
                Method::POST,
                &path,
                Some(user),
                Some(json!({ "name": "later", "category": "todo" }))
            )
            .await
            .status,
            StatusCode::CONFLICT
        );
    }
}

/// D-116: 상태 없이 만든 업무는 항목의 기본 상태(첫 상태)로 들어간다. 기본 상태의 이름이 바뀌어도 같다(옛 기본값
/// "todo"라는 이름을 서버가 끼워 넣지 않는다).
#[tokio::test]
async fn a_task_without_a_status_takes_the_item_default_even_after_renaming() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기본 상태").await;
    let it = item(&app, &admin, &ws, "DEF").await;
    let path = format!("/api/items/{it}/statuses");
    let list = send(&app, Method::GET, &path, Some(&admin), None).await;
    assert_eq!(list.status, StatusCode::OK, "{:?}", list.json);
    let todo = list.json[0]["id"].as_str().unwrap().to_owned();
    let renamed = send(
        &app,
        Method::PUT,
        &format!("{path}/{todo}"),
        Some(&admin),
        Some(json!({ "name": "할 일" })),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.json);
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{it}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "상태 없이" })),
    )
    .await;
    assert_eq!(task.status, StatusCode::CREATED, "{:?}", task.json);
    assert_eq!(task.json["status"], "할 일");
    assert_eq!(task.json["status_category"], "todo");
}
