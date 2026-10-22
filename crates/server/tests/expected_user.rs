#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 저장 요청의 기대 사용자(`x-fvoci-expected-user`, #60·이슈 #88). 편집을 시작한 사용자와 세션의 사용자가 다르면
//! 아무것도 바꾸지 않고 409 `account_changed`. 머리는 비교에만 쓰고 권한을 주지 않는다(사용자·권한은 세션 쿠키로만).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, doc_body, login, new_request_id, send,
    send_raw,
};
use serde_json::{Value, json};

const HEADER: &str = "x-fvoci-expected-user";

/// 머리를 그대로(자동으로 붙이지 않고) 정해 보낸다.
async fn raw(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    expected: &[&str],
    body: &Value,
) -> (StatusCode, Value) {
    let mut headers = vec![("content-type", "application/json")];
    for value in expected {
        headers.push((HEADER, value));
    }
    let (status, _, bytes) = send_raw(
        app,
        method,
        uri,
        user,
        &headers,
        serde_json::to_vec(body).unwrap(),
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn save_body(doc: &Value, text: &str) -> Value {
    json!({
        "request_id": new_request_id(),
        "base_revision": doc["revision"],
        "title": "바꾼 제목",
        "body": doc_body(text),
    })
}

/// 문서의 지금 리비전·제목과 리비전 기록 수.
async fn snapshot(app: &Router, user: &TestUser, id: &str) -> (i64, String, usize) {
    let doc = send(
        app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(doc.status, StatusCode::OK, "{:?}", doc.json);
    let revisions = send(
        app,
        Method::GET,
        &format!("/api/documents/{id}/revisions"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(revisions.status, StatusCode::OK, "{:?}", revisions.json);
    (
        doc.json["revision"].as_i64().unwrap(),
        doc.json["title"].as_str().unwrap().to_owned(),
        revisions.json.as_array().map_or(0, Vec::len),
    )
}

#[tokio::test]
async fn a_save_goes_through_only_for_the_user_who_started_the_edit() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "기대 사용자").await;
    let doc = create_doc(&app, &alice, &ws, "처음 제목").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/documents/{id}");
    let (other, _) = add_member(&app, &state, &alice, &ws, "bob", "writer").await;
    let before = snapshot(&app, &alice, &id).await;

    // 다른 사용자(쓰기 권한이 있는 멤버라도)로 시작한 편집이 alice의 세션으로 나가면 바꾸지 않는다.
    let other_id = other.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[&other_id],
        &save_body(&doc, "다른 계정의 편집"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
    assert_eq!(json["error"], "account_changed");
    assert!(json.get("current").is_none(), "{json:?}");
    assert_eq!(snapshot(&app, &alice, &id).await, before);

    // 머리가 없거나 형식이 틀리거나 둘이면 400(머리 없는 옛 요청을 받는 길은 두지 않는다, D-92).
    for expected in [vec![], vec!["not-a-uuid"], vec![""]] {
        let (status, json) = raw(
            &app,
            Method::PUT,
            &uri,
            Some(&alice),
            &expected,
            &save_body(&doc, "머리 없음"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{expected:?} {json:?}");
        assert_eq!(json["error"], "bad_request");
    }
    let alice_id = alice.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[&alice_id, &alice_id],
        &save_body(&doc, "머리 둘"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{json:?}");
    assert_eq!(snapshot(&app, &alice, &id).await, before);

    // 세션이 없으면 머리가 맞아도 401(머리는 신원이 아니다).
    let (status, _) = raw(
        &app,
        Method::PUT,
        &uri,
        None,
        &[&alice_id],
        &save_body(&doc, "세션 없음"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(snapshot(&app, &alice, &id).await, before);

    // 같은 사용자면 저장한다.
    let (status, json) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        &[&alice_id],
        &save_body(&doc, "내 편집"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    assert_eq!(json["revision"], before.0 + 1);
    assert_eq!(json["title"], "바꾼 제목");
}

#[tokio::test]
async fn the_expected_user_grants_nothing() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "권한 없음").await;
    let doc = create_doc(&app, &alice, &ws, "alice의 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let uri = format!("/api/documents/{id}");
    let (reader, _) = add_member(&app, &state, &alice, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let before = snapshot(&app, &alice, &id).await;
    let alice_id = alice.id.0.to_string();

    // 쓰기 권한이 있는 사람의 id를 머리에 넣어도 세션의 사용자로만 판단한다: 다르다고 거절할 뿐 쓰지 않는다.
    for who in [&reader, &outsider] {
        let (status, json) = raw(
            &app,
            Method::PUT,
            &uri,
            Some(who),
            &[&alice_id],
            &save_body(&doc, "남의 이름으로"),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
        assert_eq!(json["error"], "account_changed");
    }
    // 자기 id를 넣으면 평소의 권한 응답 그대로다(읽기 멤버 403, 멤버가 아니면 404).
    let reader_id = reader.id.0.to_string();
    let (status, _) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&reader),
        &[&reader_id],
        &save_body(&doc, "읽기 멤버"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let outsider_id = outsider.id.0.to_string();
    let (status, _) = raw(
        &app,
        Method::PUT,
        &uri,
        Some(&outsider),
        &[&outsider_id],
        &save_body(&doc, "바깥 사람"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(snapshot(&app, &alice, &id).await, before);
}

/// 초안을 내 노트로 보존하는 길(노트를 만든 뒤 저장)도 같다. 계정이 바뀌었으면 새 계정에 빈 노트(앞 계정 초안의
/// 제목을 단)를 만들지 않는다.
#[tokio::test]
async fn a_note_is_created_only_for_the_user_who_started_the_edit() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let notes = |user: TestUser| {
        let app = app.clone();
        async move {
            let r = send(&app, Method::GET, "/api/me/notes", Some(&user), None).await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json.as_array().map_or(0, Vec::len)
        }
    };
    let before = notes(alice.clone()).await;
    let create = || json!({ "request_id": new_request_id(), "title": "앞 계정 초안의 사본" });

    let bob_id = bob.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[&bob_id],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{json:?}");
    assert_eq!(json["error"], "account_changed");
    let (status, _) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(notes(alice.clone()).await, before);

    let alice_id = alice.id.0.to_string();
    let (status, json) = raw(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&alice),
        &[&alice_id],
        &create(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{json:?}");
    assert_eq!(notes(alice).await, before + 1);
}
