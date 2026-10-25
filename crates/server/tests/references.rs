#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! @ 링크·사용자 언급·역방향 링크·알림 (D-18, D-27, D-39).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_doc, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

fn reference(kind: &str, id: &Value) -> Value {
    json!({ "type": "reference", "attrs": { "kind": kind, "id": id } })
}

fn user_ref(user: &TestUser) -> Value {
    reference("user", &json!(user.id.0.to_string()))
}

fn body_with(nodes: Vec<Value>) -> Value {
    let mut content = vec![json!({ "type": "text", "text": "참고 " })];
    content.extend(nodes);
    json!({ "type": "doc", "content": [{ "type": "paragraph", "content": content }] })
}

/// 본문을 저장하고 새 문서 JSON을 돌려준다.
async fn save_body(app: &Router, user: &TestUser, doc: &Value, body: Value) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(user),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": doc["revision"],
                     "title": doc["title"], "body": body }),
        ),
    )
    .await;
    (r.status, r.json)
}

async fn get(app: &Router, user: &TestUser, uri: &str) -> (StatusCode, Value) {
    let scoped = if uri == "/api/me/notifications" {
        format!(
            "{uri}?organization_id={}",
            common::organization_for(user).await
        )
    } else {
        uri.to_owned()
    };
    let mut r = send(app, Method::GET, &scoped, Some(user), None).await;
    if uri == "/api/me/notifications" {
        // D-118: 수락한 초대 알림은 남으므로 이 시험의 언급 알림과 분리한다.
        r.json = r.json["notifications"].take();
        r.json
            .as_array_mut()
            .unwrap()
            .retain(|row| row["kind"] != "invite");
    }
    (r.status, r.json)
}

async fn module_item(app: &Router, admin: &TestUser, ws: &str, key: &str, title: &str) -> Value {
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
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    i.json
}

async fn resolve(app: &Router, user: &TestUser, refs: Value) -> Value {
    let r = send(
        app,
        Method::POST,
        "/api/references/resolve",
        Some(user),
        Some(json!({ "refs": refs })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    r.json
}

#[tokio::test]
async fn hidden_targets_look_exactly_like_missing_ones() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "해석").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
    let secret = module_item(&app, &admin, &ws, "SEC", "비밀 실험").await;
    let missing = json!(uuid::Uuid::now_v7().to_string());

    let seen = resolve(
        &app,
        &admin,
        json!([{ "kind": "item", "id": secret["id"] }]),
    )
    .await;
    assert_eq!(seen[0]["visible"], true);
    assert_eq!(seen[0]["reference"], "SEC-1");
    assert_eq!(seen[0]["title"], "비밀 실험");

    let hidden = resolve(
        &app,
        &peer,
        json!([{ "kind": "item", "id": secret["id"] }, { "kind": "item", "id": missing }]),
    )
    .await;
    let strip = |v: &Value| {
        let mut v = v.clone();
        v.as_object_mut().unwrap().remove("id");
        v
    };
    assert_eq!(strip(&hidden[0]), strip(&hidden[1]), "hidden == missing");
    assert_eq!(hidden[0]["visible"], false);
    assert_eq!(hidden[0]["title"], Value::Null);

    // 본문에 제목·라벨을 넣을 수 없다(대상이 나중에 비공개가 되어도 드러나지 않게).
    let doc = create_doc(&app, &admin, &ws, "문서").await;
    let labelled = json!({ "type": "reference", "attrs": { "kind": "item", "id": secret["id"], "label": "비밀 실험" } });
    assert_eq!(
        save_body(&app, &admin, &doc, body_with(vec![labelled]))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let bad_kind =
        json!({ "type": "reference", "attrs": { "kind": "workspace", "id": secret["id"] } });
    assert_eq!(
        save_body(&app, &admin, &doc, body_with(vec![bad_kind]))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn backlinks_follow_access_to_the_source_document() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "역링크").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
    let target = create_doc(&app, &admin, &ws, "대상").await;
    let source = create_doc(&app, &admin, &ws, "출발").await;
    let it = module_item(&app, &admin, &ws, "DEV", "항목").await;

    let (status, saved) = save_body(
        &app,
        &admin,
        &source,
        body_with(vec![
            reference("document", &target["id"]),
            reference("item", &it["id"]),
        ]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved:?}");
    let target_links = format!(
        "/api/documents/{}/backlinks",
        target["id"].as_str().unwrap()
    );
    let (_, links) = get(&app, &peer, &target_links).await;
    assert_eq!(links.as_array().unwrap().len(), 1);
    assert_eq!(links[0]["id"], source["id"]);
    let (_, item_links) = get(
        &app,
        &admin,
        &format!("/api/items/{}/backlinks", it["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(item_links[0]["id"], source["id"]);
    // 비공개 항목의 역링크는 항목을 볼 수 없는 사람에게 404.
    assert_eq!(
        get(
            &app,
            &peer,
            &format!("/api/items/{}/backlinks", it["id"].as_str().unwrap())
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // 출발 문서가 비공개가 되면 역링크에서도 사라진다.
    let r = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{}/settings", source["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "visibility": "private", "allow_copy": false })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(get(&app, &peer, &target_links).await.1, json!([]));
    assert_eq!(
        get(&app, &admin, &target_links)
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // 링크를 지우고 저장하면 역링크도 사라진다.
    let (_, saved) = save_body(&app, &admin, &saved, body_with(vec![])).await;
    assert_eq!(saved["revision"], 3);
    assert_eq!(get(&app, &admin, &target_links).await.1, json!([]));
}

#[tokio::test]
async fn mentions_notify_once_only_people_who_can_read_and_never_grant_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "언급").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &admin, &ws, "회의록").await;
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    let (status, saved) = save_body(
        &app,
        &admin,
        &doc,
        body_with(vec![user_ref(&member), user_ref(&outsider)]),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved:?}");
    // 같은 언급으로 다시 저장해도 알림은 하나다.
    let (_, saved) = save_body(
        &app,
        &admin,
        &saved,
        body_with(vec![user_ref(&member), user_ref(&outsider)]),
    )
    .await;
    let (_, inbox) = get(&app, &member, "/api/me/notifications").await;
    assert_eq!(inbox.as_array().unwrap().len(), 1, "{inbox:?}");
    assert_eq!(inbox[0]["kind"], "mention");
    assert_eq!(
        inbox[0]["organization_id"],
        json!(common::organization_of_workspace(&ws).await)
    );
    assert_eq!(inbox[0]["invitation_id"], Value::Null);
    assert_eq!(inbox[0]["actor_name"], "admin");
    assert_eq!(inbox[0]["document_title"], "회의록");
    assert_eq!(inbox[0]["read"], false);

    // 볼 수 없는 사람에게는 알리지 않고, 언급이 권한을 주지도 않는다.
    assert_eq!(
        get(&app, &outsider, "/api/me/notifications").await.1,
        json!([])
    );
    assert_eq!(
        get(&app, &outsider, &doc_uri).await.0,
        StatusCode::NOT_FOUND
    );

    // 읽음 표시는 내 알림만.
    let id = inbox[0]["id"].clone();
    let r = send(
        &app,
        Method::POST,
        &format!(
            "/api/me/notifications/read?organization_id={}",
            common::organization_for(&outsider).await
        ),
        Some(&outsider),
        Some(json!({ "id": id })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.json["marked"], 0);
    assert_eq!(
        get(&app, &member, "/api/me/notifications").await.1[0]["read"],
        false
    );
    send(
        &app,
        Method::POST,
        &format!(
            "/api/me/notifications/read?organization_id={}",
            common::organization_for(&member).await
        ),
        Some(&member),
        Some(json!({ "id": id })),
    )
    .await;
    assert_eq!(
        get(&app, &member, "/api/me/notifications").await.1[0]["read"],
        true
    );

    // 문서가 비공개가 되면(접근 철회) 알림 목록에서 사라진다.
    send(
        &app,
        Method::PUT,
        &format!("{doc_uri}/settings"),
        Some(&admin),
        Some(json!({ "visibility": "private", "allow_copy": false })),
    )
    .await;
    assert_eq!(
        get(&app, &member, "/api/me/notifications").await.1,
        json!([])
    );
    // 비공개 문서에서 언급해도 볼 수 없는 사람에게는 알림이 생기지 않는다.
    let (member2, _) = add_member(&app, &state, &admin, &ws, "member2", "member").await;
    save_body(&app, &admin, &saved, body_with(vec![user_ref(&member2)])).await;
    assert_eq!(
        get(&app, &member2, "/api/me/notifications").await.1,
        json!([])
    );
}

#[tokio::test]
async fn candidates_find_visible_people_items_and_documents_by_title_or_reference() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "후보").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "동료", "member").await;
    let outsider = login(&state, "동료아님").await;
    let other_ws = create_workspace(&app, &outsider, "남의 공간").await;
    let secret = module_item(&app, &admin, &ws, "DEV", "비밀 설계").await;
    create_doc(&app, &admin, &ws, "설계 회의록").await;
    create_doc(&app, &outsider, &other_ws, "설계 남의 문서").await;

    let search = |user: &TestUser, q: &str| {
        let app = app.clone();
        let user = user.clone();
        let uri = format!(
            "/api/reference-candidates?workspace_id={ws}&q={}",
            urlencode(q)
        );
        async move { get(&app, &user, &uri).await }
    };
    let (status, found) = search(&admin, "설계").await;
    assert_eq!(status, StatusCode::OK, "{found:?}");
    let titles: Vec<&str> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert!(
        titles.contains(&"비밀 설계") && titles.contains(&"설계 회의록"),
        "{titles:?}"
    );
    assert!(!titles.contains(&"설계 남의 문서"));
    // 동료는 비공개 항목을 제목으로도 번호로도 찾지 못한다.
    let (_, found) = search(&peer, "설계").await;
    assert!(
        found
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["title"] != "비밀 설계")
    );
    assert_eq!(search(&peer, "dev-1").await.1, json!([]));
    // 관리자는 번호로 찾는다. 키를 바꾼 뒤에도 예전 키로 찾는다 (D-18).
    let (_, by_ref) = search(&admin, "dev-1").await;
    assert_eq!(by_ref[0]["id"], secret["id"]);
    assert_eq!(by_ref[0]["reference"], "DEV-1");
    let module_id = get(
        &app,
        &admin,
        &format!("/api/items/{}", secret["id"].as_str().unwrap()),
    )
    .await
    .1["module_id"]
        .clone();
    send(
        &app,
        Method::PUT,
        &format!("/api/modules/{}", module_id.as_str().unwrap()),
        Some(&admin),
        Some(json!({ "name": "DEV", "key": "ENG" })),
    )
    .await;
    let (_, by_old) = search(&admin, "DEV-1").await;
    assert_eq!(by_old[0]["reference"], "ENG-1");

    // 사람: 같은 워크스페이스 멤버만, 이메일은 없다. 멤버가 아니면 404.
    let (_, people) = search(&admin, "동료").await;
    let names: Vec<&str> = people
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == "user")
        .map(|c| c["title"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["동료"]);
    assert!(
        people
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c.get("email").is_none())
    );
    assert_eq!(search(&outsider, "설계").await.0, StatusCode::NOT_FOUND);
}

fn urlencode(q: &str) -> String {
    q.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// 검토 반영: 사본을 고쳐 저장해도 원본에 있던 언급으로 알림을 보내지 않는다(새로 넣은 언급만 알린다).
#[tokio::test]
async fn copying_and_editing_does_not_resend_existing_mentions() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "사본 언급").await;
    let (member, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (editor, _) = add_member(&app, &state, &admin, &ws, "editor", "member").await;
    let doc = create_doc(&app, &admin, &ws, "원본").await;
    save_body(&app, &admin, &doc, body_with(vec![user_ref(&member)])).await;
    assert_eq!(
        get(&app, &member, "/api/me/notifications")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let copy = send(
        &app,
        Method::POST,
        &format!("/api/documents/{}/copy", doc["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "target_workspace_id": ws, "title": "사본" })),
    )
    .await;
    assert_eq!(copy.status, StatusCode::CREATED, "{:?}", copy.json);
    // 다른 사람이 사본을 고쳐도(언급은 그대로) 알림이 늘지 않는다.
    let mut edited = body_with(vec![user_ref(&member)]);
    edited["content"][0]["content"][0]["text"] = json!("고친 ");
    let (status, _) = save_body(&app, &editor, &copy.json, edited).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        get(&app, &member, "/api/me/notifications")
            .await
            .1
            .as_array()
            .unwrap()
            .len(),
        1
    );
    // 새로 넣은 언급은 그것을 넣은 사람 이름으로 알린다.
    let (_, current) = get(
        &app,
        &editor,
        &format!("/api/documents/{}", copy.json["id"].as_str().unwrap()),
    )
    .await;
    let (status, _) = save_body(
        &app,
        &editor,
        &current,
        body_with(vec![user_ref(&member), user_ref(&admin)]),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let inbox = get(&app, &admin, "/api/me/notifications").await.1;
    assert_eq!(inbox.as_array().unwrap().len(), 1);
    assert_eq!(inbox[0]["actor_name"], "editor");
}

/// O-33: 이름이 같은 워크스페이스를 `이름/질의`로 함께 찾고 워크스페이스 id로 구분한다. 선택한 대상은
/// 내부 id로 저장되므로 키·제목이 바뀌어도 링크가 이어진다. 새 전역 고유 이름은 요구하지 않는다.
#[tokio::test]
async fn same_named_workspaces_are_searched_together_and_links_survive_renames() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let first = create_workspace(&app, &admin, "연구실").await;
    let second = create_workspace(&app, &admin, "연구실").await;
    let a = module_item(&app, &admin, &first, "DEV", "첫째 실험").await;
    let b = module_item(&app, &admin, &second, "DEV", "둘째 실험").await;

    // 문서의 워크스페이스(first)와 상관없이 이름 접두어로 고른 곳들에서 찾는다.
    let uri = format!("/api/reference-candidates?workspace_id={first}&q=%EC%97%B0%EA%B5%AC/DEV-1");
    let (status, found) = get(&app, &admin, &uri).await;
    assert_eq!(status, StatusCode::OK, "{found:?}");
    let mut hits: Vec<(String, String)> = found
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap().to_owned(),
                c["workspace_id"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    hits.sort();
    let mut expected = vec![
        (a["id"].as_str().unwrap().to_owned(), first.clone()),
        (b["id"].as_str().unwrap().to_owned(), second.clone()),
    ];
    expected.sort();
    assert_eq!(
        hits, expected,
        "both DEV-1 items, told apart by workspace id"
    );
    assert!(
        found
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["workspace_name"] == "연구실")
    );
    // 내가 속하지 않은 같은 이름의 워크스페이스는 범위에 들지 않는다.
    let stranger = login(&state, "stranger").await;
    let theirs = create_workspace(&app, &stranger, "연구실").await;
    module_item(&app, &stranger, &theirs, "DEV", "남의 실험").await;
    let (_, again) = get(&app, &admin, &uri).await;
    assert_eq!(again.as_array().unwrap().len(), 2);

    // 둘째 워크스페이스의 항목을 고르면 본문에는 id만 남는다. 키·제목을 바꿔도 같은 대상으로 이어진다.
    let doc = create_doc(&app, &admin, &first, "회의").await;
    let (_, saved) = save_body(
        &app,
        &admin,
        &doc,
        body_with(vec![reference("item", &b["id"])]),
    )
    .await;
    let module_id = get(
        &app,
        &admin,
        &format!("/api/items/{}", b["id"].as_str().unwrap()),
    )
    .await
    .1["module_id"]
        .clone();
    send(
        &app,
        Method::PUT,
        &format!("/api/modules/{}", module_id.as_str().unwrap()),
        Some(&admin),
        Some(json!({ "name": "개발", "key": "LAB" })),
    )
    .await;
    send(
        &app,
        Method::PUT,
        &format!("/api/items/{}", b["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "base_version": 1, "title": "둘째 실험 (수정)", "description": "", "visibility": "private" })),
    )
    .await;
    let resolved = resolve(&app, &admin, json!([{ "kind": "item", "id": b["id"] }])).await;
    assert_eq!(resolved[0]["reference"], "LAB-1");
    assert_eq!(resolved[0]["title"], "둘째 실험 (수정)");
    assert_eq!(resolved[0]["workspace_id"], second.as_str());
    let (_, reopened) = get(
        &app,
        &admin,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(reopened["body"], saved["body"], "the body did not change");
    let (_, links) = get(
        &app,
        &admin,
        &format!("/api/items/{}/backlinks", b["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(links[0]["id"], doc["id"]);
}
