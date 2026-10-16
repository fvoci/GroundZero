#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 모듈 인스턴스·키·공통 번호·항목·항목 아래 문서·이동 일반화
//! (D-14, D-15, D-16, D-17, D-18, D-20, D-41, D-48, D-57, D-59).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, doc_body, login, login_with_email,
    new_request_id, send, unique_email,
};
use serde_json::{Value, json};

async fn create_module(
    app: &Router,
    user: &TestUser,
    ws: &str,
    kind: &str,
    name: &str,
    key: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "kind": kind, "name": name, "key": key })),
    )
    .await;
    (r.status, r.json)
}

async fn module(app: &Router, user: &TestUser, ws: &str, key: &str) -> Value {
    let (status, json) = create_module(app, user, ws, "project", key, key).await;
    assert_eq!(status, StatusCode::CREATED, "{json:?}");
    json
}

async fn create_item(
    app: &Router,
    user: &TestUser,
    module: &Value,
    title: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", module["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    (r.status, r.json)
}

async fn item(app: &Router, user: &TestUser, module: &Value, title: &str) -> Value {
    let (status, json) = create_item(app, user, module, title).await;
    assert_eq!(status, StatusCode::CREATED, "{json:?}");
    json
}

async fn item_doc(app: &Router, user: &TestUser, item: &Value, title: &str) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!("/api/items/{}/documents", item["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    (r.status, r.json)
}

async fn set_item(
    app: &Router,
    user: &TestUser,
    item: &Value,
    visibility: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{}", item["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "title": item["title"], "description": "", "visibility": visibility })),
    )
    .await;
    (r.status, r.json)
}

async fn get(app: &Router, user: &TestUser, uri: &str) -> (StatusCode, Value) {
    let r = send(app, Method::GET, uri, Some(user), None).await;
    (r.status, r.json)
}

async fn save(app: &Router, user: &TestUser, doc: &Value, text: &str) -> StatusCode {
    send(
        app,
        Method::PUT,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(user),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": doc["revision"],
                     "title": doc["title"], "body": doc_body(text) }),
        ),
    )
    .await
    .status
}

async fn update_module(
    app: &Router,
    user: &TestUser,
    module: &Value,
    key: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::PUT,
        &format!("/api/modules/{}", module["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "name": module["name"], "key": key })),
    )
    .await;
    (r.status, r.json)
}

/// 미리보기 → 확인한 채로 이동.
async fn move_to(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    ws: &str,
    item: Option<&Value>,
) -> (StatusCode, Value) {
    let uri = format!("/api/documents/{}/move", doc["id"].as_str().unwrap());
    let target =
        json!({ "target_workspace_id": ws, "target_item_id": item.map(|i| i["id"].clone()) });
    let preview = send(app, Method::POST, &uri, Some(user), Some(target.clone())).await;
    if preview.status != StatusCode::CONFLICT || preview.json.get("gains").is_none() {
        return (preview.status, preview.json);
    }
    let mut body = target;
    body["acknowledged"] =
        json!({ "gains": preview.json["gains"], "loses": preview.json["loses"] });
    let r = send(app, Method::POST, &uri, Some(user), Some(body)).await;
    (r.status, r.json)
}

#[tokio::test]
async fn module_keys_are_admin_managed_unique_and_case_insensitive() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "모듈").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let outsider = login(&state, "outsider").await;

    // 소문자는 대문자로 받는다. 같은 유형을 여러 개 둘 수 있다 (D-15).
    let (status, dev) = create_module(&app, &admin, &ws, "project", "개발", " dev ").await;
    assert_eq!(status, StatusCode::CREATED, "{dev:?}");
    assert_eq!(dev["key"], "DEV");
    assert_eq!(dev["kind"], "project");
    let (status, _) = create_module(&app, &admin, &ws, "project", "운영", "OPS").await;
    assert_eq!(status, StatusCode::CREATED);

    // 대소문자만 다른 키, 예약어, 잘못된 형식, 알 수 없는 유형.
    assert_eq!(
        create_module(&app, &admin, &ws, "study", "x", "Dev")
            .await
            .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        create_module(&app, &admin, &ws, "study", "x", "wiki")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        create_module(&app, &admin, &ws, "study", "x", "1AB")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        create_module(&app, &admin, &ws, "chores", "x", "CHO")
            .await
            .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    // 관리자만 만든다. 비멤버에게는 워크스페이스가 없는 것과 같다.
    assert_eq!(
        create_module(&app, &writer, &ws, "study", "x", "STU")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        create_module(&app, &outsider, &ws, "study", "x", "STU")
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(
            &app,
            &outsider,
            &format!("/api/modules/{}", dev["id"].as_str().unwrap())
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    // 같은 request_id 재시도는 같은 모듈.
    let rid = new_request_id();
    let body = json!({ "request_id": rid, "kind": "hobby", "name": "취미", "key": "FUN" });
    let uri = format!("/api/workspaces/{ws}/modules");
    let a = send(&app, Method::POST, &uri, Some(&admin), Some(body.clone())).await;
    let b = send(&app, Method::POST, &uri, Some(&admin), Some(body)).await;
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(a.json["id"], b.json["id"]);

    let (_, list) = get(&app, &writer, &format!("/api/workspaces/{ws}/modules")).await;
    let keys: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["DEV", "OPS", "FUN"]);
}

#[tokio::test]
async fn items_documents_share_one_sequence_per_module() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "번호").await;
    let dev = module(&app, &admin, &ws, "DEV").await;
    let ops = module(&app, &admin, &ws, "OPS").await;

    let first = item(&app, &admin, &dev, "첫 항목").await;
    assert_eq!(first["reference"], "DEV-1");
    assert_eq!(
        first["visibility"], "private",
        "new items are private by default (D-20)"
    );
    let (status, doc) = item_doc(&app, &admin, &first, "설계 문서").await;
    assert_eq!(status, StatusCode::CREATED, "{doc:?}");
    assert_eq!(doc["reference"], "DEV-2");
    assert_eq!(doc["placement"], "module");
    assert_eq!(doc["item_id"], first["id"]);
    let second = item(&app, &admin, &dev, "둘째 항목").await;
    assert_eq!(second["reference"], "DEV-3");

    // 다른 모듈과 위키는 번호열이 따로다.
    assert_eq!(
        item(&app, &admin, &ops, "운영 항목").await["reference"],
        "OPS-1"
    );
    assert_eq!(
        create_doc(&app, &admin, &ws, "위키").await["reference"],
        "WIKI-1"
    );

    let (_, docs) = get(
        &app,
        &admin,
        &format!("/api/items/{}/documents", first["id"].as_str().unwrap()),
    )
    .await;
    assert_eq!(docs.as_array().unwrap().len(), 1);
    assert_eq!(docs[0]["reference"], "DEV-2");

    // 동시에 만들어도 번호가 겹치거나 비지 않는다.
    let creates = (0..8).map(|i| {
        let app = app.clone();
        let admin = admin.clone();
        let dev = dev.clone();
        tokio::spawn(async move { item(&app, &admin, &dev, &format!("동시 {i}")).await })
    });
    let mut numbers: Vec<i64> = Vec::new();
    for handle in creates {
        let created = handle.await.unwrap();
        let reference = created["reference"].as_str().unwrap().to_owned();
        numbers.push(reference.trim_start_matches("DEV-").parse().unwrap());
    }
    numbers.sort_unstable();
    assert_eq!(numbers, (4..12).collect::<Vec<_>>());
}

#[tokio::test]
async fn key_change_keeps_numbers_and_retired_keys_stay_reserved() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "키 변경").await;
    let dev = module(&app, &admin, &ws, "DEV").await;
    let other = module(&app, &admin, &ws, "OPS").await;
    let it = item(&app, &admin, &dev, "항목").await;
    let (_, doc) = item_doc(&app, &admin, &it, "문서").await;

    let (status, renamed) = update_module(&app, &admin, &dev, "eng").await;
    assert_eq!(status, StatusCode::OK, "{renamed:?}");
    assert_eq!(renamed["key"], "ENG");
    assert_eq!(renamed["previous_keys"], json!(["DEV"]));
    // 번호와 id는 그대로, 참조는 새 키로 (D-18).
    let item_uri = format!("/api/items/{}", it["id"].as_str().unwrap());
    assert_eq!(get(&app, &admin, &item_uri).await.1["reference"], "ENG-1");
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    assert_eq!(get(&app, &admin, &doc_uri).await.1["reference"], "ENG-2");

    // 옛 키는 다른 모듈이 가져갈 수 없고, 새 모듈도 만들 수 없다. 원래 모듈은 되찾을 수 있다.
    assert_eq!(
        update_module(&app, &admin, &other, "DEV").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        create_module(&app, &admin, &ws, "study", "x", "dev")
            .await
            .0,
        StatusCode::CONFLICT
    );
    let (status, back) = update_module(&app, &admin, &dev, "DEV").await;
    assert_eq!(status, StatusCode::OK, "{back:?}");
    assert_eq!(back["key"], "DEV");
    assert_eq!(back["previous_keys"], json!(["ENG"]));
    assert_eq!(get(&app, &admin, &item_uri).await.1["reference"], "DEV-1");

    // 키 변경은 감사 기록에 남는다 (D-57).
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(dev["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(actions, vec!["create", "change_key", "change_key"]);
}

#[tokio::test]
async fn private_items_hide_their_documents_and_numbers() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "비공개 항목").await;
    let (creator, _) = add_member(&app, &state, &admin, &ws, "creator", "writer").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let dev = module(&app, &admin, &ws, "DEV").await;

    let it = item(&app, &creator, &dev, "내 실험").await;
    assert_eq!(it["access"], "owner");
    let (_, doc) = item_doc(&app, &creator, &it, "기록").await;
    let item_uri = format!("/api/items/{}", it["id"].as_str().unwrap());
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let items_uri = format!("/api/modules/{}/items", dev["id"].as_str().unwrap());

    // 비공개 항목: 만든 사람과 관리자만 본다. 항목 아래 문서도 같다.
    for user in [&peer, &reader] {
        assert_eq!(get(&app, user, &item_uri).await.0, StatusCode::NOT_FOUND);
        assert_eq!(get(&app, user, &doc_uri).await.0, StatusCode::NOT_FOUND);
        assert_eq!(get(&app, user, &items_uri).await.1, json!([]));
        assert_eq!(
            item_doc(&app, user, &it, "끼워 넣기").await.0,
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(get(&app, &admin, &doc_uri).await.1["access"], "owner");

    // 앱 역할로 번호 대장을 직접 읽어도 볼 수 없는 대상은 나오지 않는다.
    let mut tx = state.db.begin_as(peer.id).await.unwrap();
    let visible: i64 =
        sqlx::query_scalar("SELECT count(*) FROM numbered_targets WHERE scope_id = $1::uuid")
            .bind(dev["id"].as_str().unwrap())
            .fetch_one(tx.conn())
            .await
            .unwrap();
    assert_eq!(visible, 0);
    // 번호 대장과 카운터에 직접 쓸 수 없다(할당은 정의자 함수만).
    let err = sqlx::query(
        "INSERT INTO numbered_targets (workspace_id, scope_id, number, target_kind, target_id)
         VALUES ($1::uuid, $2::uuid, 999, 'item', gen_random_uuid())",
    )
    .bind(&ws)
    .bind(dev["id"].as_str().unwrap())
    .execute(tx.conn())
    .await
    .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    drop(tx);

    // 공개 범위는 항목 소유 권한자만 바꾼다. 공개하면 역할대로 접근한다.
    assert_eq!(
        set_item(&app, &peer, &it, "workspace").await.0,
        StatusCode::NOT_FOUND
    );
    let (status, opened) = set_item(&app, &creator, &it, "workspace").await;
    assert_eq!(status, StatusCode::OK, "{opened:?}");
    assert_eq!(get(&app, &peer, &doc_uri).await.1["access"], "write");
    assert_eq!(get(&app, &reader, &doc_uri).await.1["access"], "read");
    assert_eq!(get(&app, &peer, &item_uri).await.1["access"], "write");
    assert_eq!(
        set_item(&app, &peer, &it, "private").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        save(&app, &reader, &doc, "독자").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(save(&app, &peer, &doc, "동료").await, StatusCode::OK);

    // 독자는 항목도, 항목 아래 문서도 만들 수 없다.
    assert_eq!(
        create_item(&app, &reader, &dev, "독자 항목").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        item_doc(&app, &reader, &it, "독자 문서").await.0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn archived_items_and_modules_are_read_only_but_visible() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let dev = module(&app, &admin, &ws, "DEV").await;
    let it = item(&app, &writer, &dev, "항목").await;
    set_item(&app, &writer, &it, "workspace").await;
    let (_, doc) = item_doc(&app, &writer, &it, "문서").await;
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let archive_item = |archived: bool| {
        let app = app.clone();
        let writer = writer.clone();
        let uri = format!("/api/items/{}/archived", it["id"].as_str().unwrap());
        async move {
            send(
                &app,
                Method::PUT,
                &uri,
                Some(&writer),
                Some(json!({ "archived": archived })),
            )
            .await
        }
    };

    let r = archive_item(true).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["archived"], true);
    assert_eq!(get(&app, &writer, &doc_uri).await.1["access"], "read");
    assert_eq!(
        save(&app, &writer, &doc, "보관 뒤").await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        item_doc(&app, &writer, &it, "새 문서").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        set_item(&app, &writer, &it, "workspace").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(archive_item(false).await.status, StatusCode::OK);
    assert_eq!(save(&app, &writer, &doc, "해제 뒤").await, StatusCode::OK);

    // 모듈 보관은 관리자만, 그 아래 항목·문서는 읽기 전용이 되고 새 항목을 만들 수 없다.
    let module_uri = format!("/api/modules/{}/archived", dev["id"].as_str().unwrap());
    let r = send(
        &app,
        Method::PUT,
        &module_uri,
        Some(&writer),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = send(
        &app,
        Method::PUT,
        &module_uri,
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(get(&app, &writer, &doc_uri).await.1["access"], "read");
    assert_eq!(
        create_item(&app, &writer, &dev, "보관 중").await.0,
        StatusCode::CONFLICT
    );
    let (_, list) = get(&app, &writer, &format!("/api/workspaces/{ws}/modules")).await;
    assert_eq!(list[0]["archived"], true, "archived modules stay listed");
}

#[tokio::test]
async fn documents_move_between_wiki_and_items() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "이동").await;
    let other_ws = create_workspace(&app, &admin, "다른 곳").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "writer").await;
    let dev = module(&app, &admin, &ws, "DEV").await;
    let it = item(&app, &admin, &dev, "비공개 항목").await;
    let doc = create_doc(&app, &admin, &ws, "위키 문서").await;
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    assert_eq!(get(&app, &peer, &doc_uri).await.0, StatusCode::OK);

    // 위키 → 비공개 항목: 동료가 볼 수 없게 되는 것을 미리보기에서 보여 준다.
    let uri = format!(
        "/api/documents/{}/move/preview",
        doc["id"].as_str().unwrap()
    );
    let preview = send(
        &app,
        Method::POST,
        &uri,
        Some(&admin),
        Some(json!({ "target_workspace_id": ws, "target_item_id": it["id"] })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    assert_eq!(preview.json["loses"], json!(["peer"]));
    let (status, moved) = move_to(&app, &admin, &doc, &ws, Some(&it)).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["reference"], "DEV-2");
    assert_eq!(moved["document"]["placement"], "module");
    assert_eq!(get(&app, &peer, &doc_uri).await.0, StatusCode::NOT_FOUND);

    // 같은 곳으로는 옮기지 않는다. 다시 위키로 오면 새 위키 번호를 받는다.
    assert_eq!(
        move_to(&app, &admin, &doc, &ws, Some(&it)).await.0,
        StatusCode::CONFLICT
    );
    let (status, back) = move_to(&app, &admin, &doc, &ws, None).await;
    assert_eq!(status, StatusCode::OK, "{back:?}");
    assert_eq!(back["reference"], "WIKI-2");
    assert_eq!(get(&app, &peer, &doc_uri).await.0, StatusCode::OK);

    // 다른 워크스페이스의 항목을 대상으로 지정할 수 없다(항목과 워크스페이스가 맞아야 한다).
    assert_eq!(
        move_to(&app, &admin, &doc, &other_ws, Some(&it)).await.0,
        StatusCode::NOT_FOUND
    );
    // 작성자도 팀 문서의 소유 권한자가 아니면 옮길 수 없다.
    let peer_doc = create_doc(&app, &peer, &ws, "동료 문서").await;
    assert_eq!(
        move_to(&app, &peer, &peer_doc, &ws, None).await.0,
        StatusCode::FORBIDDEN
    );
    // 볼 수 없는(비공개) 항목으로는 자기 노트도 옮길 수 없다.
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&peer),
        Some(json!({ "request_id": new_request_id(), "title": "노트" })),
    )
    .await;
    assert_eq!(
        move_to(&app, &peer, &note.json, &ws, Some(&it)).await.0,
        StatusCode::NOT_FOUND
    );
}

/// 검토 반영: 보관 아래에서도 소유 권한자는 노출을 줄일 수 있고, 넓히는 변경은 막힌다.
#[tokio::test]
async fn archived_documents_can_still_be_restricted_by_their_owner() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관 관리").await;
    let dev = module(&app, &admin, &ws, "DEV").await;
    let it = item(&app, &admin, &dev, "항목").await;
    let (_, doc) = item_doc(&app, &admin, &it, "문서").await;
    let doc_id = doc["id"].as_str().unwrap();
    let email = unique_email("guest");
    let guest = login_with_email(&state, "guest", &email, true).await;
    let share = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(json!({ "email": email, "permission": "write" })),
    )
    .await;
    assert_eq!(share.status, StatusCode::CREATED, "{:?}", share.json);
    let link = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/public-links"),
        Some(&admin),
        Some(json!({})),
    )
    .await;
    assert_eq!(link.status, StatusCode::CREATED, "{:?}", link.json);
    // 공유받은 사람은 비공개 항목을 볼 수 없으므로 항목 id를 받지 않는다.
    let seen = get(&app, &guest, &format!("/api/documents/{doc_id}"))
        .await
        .1;
    assert_eq!(seen["item_id"], Value::Null);
    assert_eq!(seen["access"], "write");

    let r = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{}/archived", dev["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    let archived = get(&app, &admin, &format!("/api/documents/{doc_id}"))
        .await
        .1;
    assert_eq!(archived["access"], "read");
    assert_eq!(archived["can_manage"], true);
    assert_eq!(
        get(&app, &guest, &format!("/api/documents/{doc_id}"))
            .await
            .1["access"],
        "read"
    );

    // 넓히는 변경은 409, 좁히는 변경은 된다.
    let settings = format!("/api/documents/{doc_id}/settings");
    let widen = send(
        &app,
        Method::PUT,
        &settings,
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": true })),
    )
    .await;
    assert_eq!(widen.status, StatusCode::CONFLICT);
    let narrow = send(
        &app,
        Method::PUT,
        &settings,
        Some(&admin),
        Some(json!({ "visibility": "private", "allow_copy": false })),
    )
    .await;
    assert_eq!(narrow.status, StatusCode::OK, "{:?}", narrow.json);
    let shares = get(&app, &admin, &format!("/api/documents/{doc_id}/shares")).await;
    assert_eq!(shares.0, StatusCode::OK);
    let share_id = shares.1[0]["id"].as_str().unwrap().to_owned();
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/document-shares/{share_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(
        get(&app, &guest, &format!("/api/documents/{doc_id}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let links = get(
        &app,
        &admin,
        &format!("/api/documents/{doc_id}/public-links"),
    )
    .await;
    assert_eq!(links.0, StatusCode::OK);
    let link_id = links.1[0]["id"].as_str().unwrap().to_owned();
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/public-links/{link_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    // 새 공유·새 링크는 보관 중에 만들 수 없다.
    let share = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(json!({ "email": unique_email("x"), "permission": "read" })),
    )
    .await;
    assert_eq!(share.status, StatusCode::FORBIDDEN);
}

/// 검토 반영: 사본·같은 모듈 안 이동·request_id 재사용·공개 링크 안내.
#[tokio::test]
async fn copies_moves_and_retries_respect_item_boundaries() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "경계").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "writer").await;
    let dev = module(&app, &admin, &ws, "DEV").await;
    let first = item(&app, &admin, &dev, "첫 항목").await;
    let second = item(&app, &admin, &dev, "둘째 항목").await;
    let (_, doc) = item_doc(&app, &admin, &first, "문서").await;
    let doc_id = doc["id"].as_str().unwrap();
    assert_eq!(doc["reference"], "DEV-3");

    // 비공개 항목 아래 문서의 사본은 위키에서도 비공개다.
    let copy = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/copy"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "target_workspace_id": ws, "title": "사본" })),
    )
    .await;
    assert_eq!(copy.status, StatusCode::CREATED, "{:?}", copy.json);
    assert_eq!(copy.json["visibility"], "private");
    let copy_uri = format!("/api/documents/{}", copy.json["id"].as_str().unwrap());
    assert_eq!(get(&app, &peer, &copy_uri).await.0, StatusCode::NOT_FOUND);

    // 같은 모듈의 다른 항목으로 옮기면 번호를 유지한다. 활성 공개 링크 수를 미리보기에 보여 준다.
    send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/public-links"),
        Some(&admin),
        Some(json!({})),
    )
    .await;
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/move/preview"),
        Some(&admin),
        Some(json!({ "target_workspace_id": ws, "target_item_id": second["id"] })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::OK, "{:?}", preview.json);
    assert_eq!(preview.json["reference_changes"], false);
    assert_eq!(preview.json["active_public_links"], 1);
    let (status, moved) = move_to(&app, &admin, &doc, &ws, Some(&second)).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["reference"], "DEV-3");
    // 다른 범위(위키)로 옮기면 새 번호를 받는다.
    let preview = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/move/preview"),
        Some(&admin),
        Some(json!({ "target_workspace_id": ws })),
    )
    .await;
    assert_eq!(preview.json["reference_changes"], true);

    // 위키 생성에 쓴 request_id를 항목 문서 생성에 다시 쓰면 그 위키 문서를 돌려주지 않는다.
    let rid = new_request_id();
    let wiki = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&admin),
        Some(json!({ "request_id": rid, "title": "위키" })),
    )
    .await;
    assert_eq!(wiki.status, StatusCode::CREATED);
    let reused = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", first["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": rid, "title": "항목 문서" })),
    )
    .await;
    assert_eq!(reused.status, StatusCode::CONFLICT, "{:?}", reused.json);
    let (_, item_doc_json) = item_doc(&app, &admin, &first, "항목 문서").await;
    let rid2 = new_request_id();
    let a = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", first["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": rid2, "title": "재시도" })),
    )
    .await;
    let b = send(
        &app,
        Method::POST,
        &format!("/api/items/{}/documents", second["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": rid2, "title": "재시도" })),
    )
    .await;
    assert_eq!(a.status, StatusCode::CREATED);
    assert_eq!(b.status, StatusCode::CONFLICT, "{:?}", b.json);
    assert_ne!(item_doc_json["id"], a.json["id"]);
}
