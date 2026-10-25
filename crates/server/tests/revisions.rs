#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 리비전 기록 보기와 복원 (D-27, D-29, D-57, D-58).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, doc_body, login, login_with_email,
    new_request_id, send, unique_email,
};
use serde_json::{Value, json};

async fn save(app: &Router, user: &TestUser, doc: &Value, text: &str) -> Value {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(user),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": doc["revision"],
                     "title": doc["title"], "body": doc_body(text) }),
        ),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

async fn restore(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    revision: i64,
    base: &Value,
    request_id: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!(
            "/api/documents/{}/revisions/{revision}/restore",
            doc["id"].as_str().unwrap()
        ),
        Some(user),
        Some(json!({ "request_id": request_id, "base_revision": base })),
    )
    .await;
    (r.status, r.json)
}

async fn get(app: &Router, user: &TestUser, uri: &str) -> (StatusCode, Value) {
    let mut r = send(app, Method::GET, uri, Some(user), None).await;
    if uri == "/api/me/notifications" {
        // D-118: 초대 알림을 제외하고 복원으로 언급 알림이 늘지 않는지 검증한다.
        r.json
            .as_array_mut()
            .unwrap()
            .retain(|row| row["kind"] != "invite");
    }
    (r.status, r.json)
}

#[tokio::test]
async fn history_is_listed_and_restore_adds_a_revision_without_erasing_history() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기록").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &admin, &ws, "계획").await;
    let v2 = save(&app, &admin, &doc, "첫 안").await;
    let v3 = save(&app, &admin, &v2, "둘째 안").await;
    let v4 = save(&app, &admin, &v3, "셋째 안").await;
    let id = doc["id"].as_str().unwrap();
    let list_uri = format!("/api/documents/{id}/revisions");

    let (status, list) = get(&app, &reader, &list_uri).await;
    assert_eq!(status, StatusCode::OK);
    let revs: Vec<i64> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["revision"].as_i64().unwrap())
        .collect();
    assert_eq!(revs, vec![4, 3, 2, 1]);
    assert_eq!(list[0]["author_name"], "admin");
    assert!(list[0].get("body").is_none(), "the list carries no bodies");
    let (_, page) = get(&app, &reader, &format!("{list_uri}?before=3")).await;
    assert_eq!(page.as_array().unwrap().len(), 2);
    let (_, old) = get(&app, &reader, &format!("{list_uri}/2")).await;
    assert_eq!(old["body"], doc_body("첫 안"));
    assert_eq!(
        get(&app, &outsider, &list_uri).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &outsider, &format!("{list_uri}/2")).await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &reader, &format!("{list_uri}/99")).await.0,
        StatusCode::NOT_FOUND
    );

    // 복원: 새 리비전 5가 리비전 2의 내용이 되고, 1~4는 그대로 남는다.
    let rid = new_request_id();
    let (status, restored) = restore(&app, &admin, &doc, 2, &v4["revision"], &rid).await;
    assert_eq!(status, StatusCode::OK, "{restored:?}");
    assert_eq!(restored["revision"], 5);
    assert_eq!(restored["body"], doc_body("첫 안"));
    let (_, list) = get(&app, &admin, &list_uri).await;
    assert_eq!(list.as_array().unwrap().len(), 5);
    assert_eq!(list[0]["restored_from"], 2);
    // 같은 요청 재시도는 같은 결과(새 리비전을 또 만들지 않음).
    let (status, again) = restore(&app, &admin, &doc, 2, &v4["revision"], &rid).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(again["revision"], 5);
    // 이미 같은 내용이면 리비전을 늘리지 않는다.
    let (status, same) = restore(&app, &admin, &doc, 2, &json!(5), &new_request_id()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(same["revision"], 5);

    // 감사 기록에 남는다 (D-57).
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let detail: Value = sqlx::query_scalar(
        "SELECT detail FROM audit_log WHERE target_id = $1::uuid AND action = 'restore_revision'",
    )
    .bind(id)
    .fetch_one(tx.conn())
    .await
    .unwrap();
    assert_eq!(detail, json!({ "from": 2, "to": 5 }));
}

#[tokio::test]
async fn restore_follows_save_conflict_and_permission_rules() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "복원 규칙").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let doc = create_doc(&app, &admin, &ws, "문서").await;
    let v2 = save(&app, &admin, &doc, "하나").await;
    let v3 = save(&app, &admin, &v2, "둘").await;

    // 독자는 복원할 수 없다. 없는 리비전은 404.
    assert_eq!(
        restore(&app, &reader, &doc, 1, &v3["revision"], &new_request_id())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        restore(&app, &writer, &doc, 42, &v3["revision"], &new_request_id())
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // 보고 있던 리비전이 지났으면 409와 서버 최신본(초안은 클라이언트가 보존).
    let (status, body) = restore(&app, &writer, &doc, 1, &v2["revision"], &new_request_id()).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["current"]["revision"], 3);

    // 복원 뒤 다른 저장이 있었으면, 같은 요청의 재시도는 성공으로 보지 않고 409.
    let rid = new_request_id();
    let (status, restored) = restore(&app, &writer, &doc, 2, &v3["revision"], &rid).await;
    assert_eq!(status, StatusCode::OK);
    let after = save(&app, &admin, &restored, "그 뒤 저장").await;
    let (status, body) = restore(&app, &writer, &doc, 2, &v3["revision"], &rid).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["current"]["revision"], after["revision"]);
}

/// 검토 반영: 볼 수 없는 문서는 잠그기 전에 거부하고, 읽기 공유·보관·남의 노트·다른 워크스페이스는 복원할 수 없다.
/// 저장과 복원은 request_id를 서로 재사용할 수 없고, 복원은 언급 알림을 만들지 않는다.
#[tokio::test]
async fn restore_is_denied_without_write_access_and_request_ids_do_not_cross() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "복원 경계").await;
    let other_ws = create_workspace(&app, &admin, "다른 곳").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let stranger = login(&state, "stranger").await;

    // 남의 개인 노트, 다른 사람 워크스페이스의 문서: 404.
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "노트" })),
    )
    .await
    .json;
    let note2 = save(&app, &admin, &note, "노트 내용").await;
    assert_eq!(
        restore(
            &app,
            &stranger,
            &note,
            1,
            &note2["revision"],
            &new_request_id()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let foreign = create_doc(&app, &admin, &other_ws, "남의 공간 문서").await;
    let foreign2 = save(&app, &admin, &foreign, "내용").await;
    assert_eq!(
        restore(
            &app,
            &member,
            &foreign,
            1,
            &foreign2["revision"],
            &new_request_id()
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // 읽기 공유만 받은 사람: 403.
    let email = unique_email("guest");
    let guest = login_with_email(&state, "guest", &email, true).await;
    // D-114: 공간 멤버가 아닌 공유 대상도 같은 조직에 속한다.
    let organization = common::organization_of_workspace(&ws).await;
    common::join_organization(organization, &guest, "member").await;
    let doc = create_doc(&app, &admin, &ws, "공유 문서").await;
    let v2 = save(&app, &admin, &doc, "언급 전").await;
    send(
        &app,
        Method::POST,
        &format!("/api/documents/{}/shares", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "email": email, "permission": "read", "organization_id": organization })),
    )
    .await;
    assert_eq!(
        restore(&app, &guest, &doc, 1, &v2["revision"], &new_request_id())
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    // 저장에 쓴 request_id로 복원하거나, 복원에 쓴 request_id로 저장하면 400(엉뚱한 성공 없음).
    let save_rid = new_request_id();
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(
            json!({ "request_id": save_rid, "base_revision": v2["revision"], "title": doc["title"],
                                  "body": doc_body("셋째") }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK);
    assert_eq!(
        restore(&app, &admin, &doc, 1, &saved.json["revision"], &save_rid)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let restore_rid = new_request_id();
    let (status, restored) =
        restore(&app, &admin, &doc, 2, &saved.json["revision"], &restore_rid).await;
    assert_eq!(status, StatusCode::OK);
    let reuse = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(
            json!({ "request_id": restore_rid, "base_revision": restored["revision"],
                                  "title": doc["title"], "body": doc_body("넷째") }),
        ),
    )
    .await;
    assert_eq!(reuse.status, StatusCode::BAD_REQUEST);
    // 같은 request_id로 다른 리비전을 복원하는 것도 재시도가 아니다.
    assert_eq!(
        restore(&app, &admin, &doc, 1, &restored["revision"], &restore_rid)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );

    // 언급이 든 리비전을 복원해도 복원한 사람 이름으로 알림이 가지 않는다.
    let mention = json!({ "type": "doc", "content": [{ "type": "paragraph", "content": [
        { "type": "reference", "attrs": { "kind": "user", "id": member.id.0.to_string() } } ] }] });
    let with_mention = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": restored["revision"],
                                         "title": doc["title"], "body": mention }),
        ),
    )
    .await
    .json;
    let mention_rev = with_mention["revision"].as_i64().unwrap();
    let cleared = save(&app, &admin, &with_mention, "언급 지움").await;
    let before = get(&app, &member, "/api/me/notifications")
        .await
        .1
        .as_array()
        .unwrap()
        .len();
    let (status, _) = restore(
        &app,
        &member,
        &doc,
        mention_rev,
        &cleared["revision"],
        &new_request_id(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        get(&app, &member, "/api/me/notifications")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        before
    );

    // D-113: 보관 아래 쓰기는 소유자도 409이며 보관 해제 전에는 복원하지 못한다.
    let m = send(&app, Method::POST, &format!("/api/workspaces/{ws}/modules"), Some(&admin),
                 Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "개발", "key": "DEV" }))).await.json;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await
    .json;
    let item_doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목 문서" })),
    )
    .await
    .json;
    let item_doc2 = save(&app, &admin, &item_doc, "내용").await;
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}/archived", item["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(
        restore(
            &app,
            &admin,
            &item_doc,
            1,
            &item_doc2["revision"],
            &new_request_id()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
}
