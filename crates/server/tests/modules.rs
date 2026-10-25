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
    let uri = format!("/api/items/{}", item["id"].as_str().unwrap());
    // 공개 범위만 바꾸는 도우미라 지금 판으로 보낸다(볼 수 없으면 판과 상관없이 404).
    let (_, now) = get(app, user, &uri).await;
    let version = now["version"].as_i64().unwrap_or(1);
    let r = send(
        app,
        Method::PUT,
        &uri,
        Some(user),
        Some(json!({ "base_version": version, "title": item["title"], "description": "", "visibility": visibility })),
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
    if preview.status != StatusCode::CONFLICT || preview.json.get("changes").is_none() {
        return (preview.status, preview.json);
    }
    let mut body = target;
    // D-114: 표시 이름 집합 대신 사용자별 등급 변화를 확인한다.
    body["acknowledged"] = json!({ "changes": preview.json["changes"] });
    let r = send(app, Method::POST, &uri, Some(user), Some(body)).await;
    (r.status, r.json)
}

#[tokio::test]
async fn module_keys_are_admin_managed_unique_and_case_insensitive() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "모듈").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
    let (creator, _) = add_member(&app, &state, &admin, &ws, "creator", "member").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
         VALUES ($1::uuid, $2::uuid, 999, 'item', $3)",
    )
    .bind(&ws)
    .bind(dev["id"].as_str().unwrap())
    .bind(uuid::Uuid::now_v7())
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
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
    // D-113: 보관 아래 쓰기는 읽기 전용 역할의 403과 달리 상태 충돌이다.
    assert_eq!(
        save(&app, &writer, &doc, "보관 뒤").await,
        StatusCode::CONFLICT
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
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
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
    assert_eq!(
        preview.json["changes"],
        json!([
            { "change": "lose", "user_id": peer.id.0, "display_name": "peer", "from_grade": "write", "to_grade": null }
        ])
    );
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

/// D-113: 보관 중에는 관리자도 설정·공유·링크를 바꾸지 못하며 해제 뒤 철회한다.
#[tokio::test]
async fn archived_documents_require_unarchiving_before_owner_changes() {
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
    let organization = common::organization_of_workspace(&ws).await;
    common::join_organization(organization, &guest, "member").await;
    let share = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(json!({ "email": email, "permission": "write", "organization_id": organization })),
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
    assert_eq!(archived["can_manage"], false);
    assert_eq!(
        get(&app, &guest, &format!("/api/documents/{doc_id}"))
            .await
            .1["access"],
        "read"
    );

    // D-113: 노출을 줄이는 변경에도 보관 중 관리자 예외는 없다.
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
    assert_eq!(narrow.status, StatusCode::CONFLICT, "{:?}", narrow.json);
    let share_id = share.json["id"].as_str().unwrap();
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/document-shares/{share_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    assert_eq!(
        get(&app, &guest, &format!("/api/documents/{doc_id}"))
            .await
            .0,
        StatusCode::OK
    );
    let link_id = link.json["id"].as_str().unwrap();
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/public-links/{link_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    // 새 공유·새 링크는 보관 중에 만들 수 없다.
    let share = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(json!({ "email": email, "permission": "read", "organization_id": organization })),
    )
    .await;
    assert_eq!(share.status, StatusCode::CONFLICT);
    let unarchived = send(
        &app,
        Method::PUT,
        &format!("/api/modules/{}/archived", dev["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "archived": false })),
    )
    .await;
    assert_eq!(unarchived.status, StatusCode::OK);
    for uri in [
        format!("/api/document-shares/{share_id}"),
        format!("/api/public-links/{link_id}"),
    ] {
        assert_eq!(
            send(&app, Method::DELETE, &uri, Some(&admin), None)
                .await
                .status,
            StatusCode::NO_CONTENT
        );
    }
    assert_eq!(
        get(&app, &guest, &format!("/api/documents/{doc_id}"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

/// 검토 반영: 사본·같은 모듈 안 이동·request_id 재사용·공개 링크 안내.
#[tokio::test]
async fn copies_moves_and_retries_respect_item_boundaries() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "경계").await;
    let (peer, _) = add_member(&app, &state, &admin, &ws, "peer", "member").await;
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

/// 감사 W13: 항목 정보 저장이 공개 범위를 늘 받아, 화면은 연 때의 값을 보냈다. 그사이 다른 소유 권한자가 비공개로
/// 바꿨으면 설명만 고친 저장이 다시 공개했고, 소유자가 아닌 작성자는 옛 값 때문에 제목도 못 고쳤다(403). 공개 범위는
/// 보낼 때만 바꾸고, 보내지 않으면 잠근 뒤의 지금 값 그대로다. D-104 뒤로는 연 때의 판으로 보낸 저장이 409이므로(아래
/// `stale_item_details_are_refused_with_the_latest`), 여기서는 지금 판으로 보내 "보내지 않은 공개 범위는 그대로"만 본다.
#[tokio::test]
async fn item_details_keep_the_visibility_unless_it_is_sent() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "공개 범위").await;
    let (creator, _) = add_member(&app, &state, &admin, &ws, "creator", "member").await;
    let (helper, _) = add_member(&app, &state, &admin, &ws, "helper", "member").await;
    let m = module(&app, &admin, &ws, "VIS").await;
    let it = item(&app, &creator, &m, "설계").await;
    let uri = format!("/api/items/{}", it["id"].as_str().unwrap());

    // 만든 사람의 화면은 공개로 열려 있는데 관리자가 비공개로 바꾼다.
    let (status, json) = set_item(&app, &creator, &it, "workspace").await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    let (status, json) = set_item(&app, &admin, &it, "private").await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    let typo = send(
        &app,
        Method::PUT,
        &uri,
        Some(&creator),
        Some(json!({ "base_version": 3, "title": "설계", "description": "오타 고침" })),
    )
    .await;
    assert_eq!(typo.status, StatusCode::OK, "{:?}", typo.json);
    assert_eq!(typo.json["visibility"], "private");
    assert_eq!(typo.json["description"], "오타 고침");

    // 공개 항목에서 소유자가 아닌 작성자는 공개 범위 없이 제목을 고친다.
    let (status, json) = set_item(&app, &admin, &it, "workspace").await;
    assert_eq!(status, StatusCode::OK, "{json:?}");
    let renamed = send(
        &app,
        Method::PUT,
        &uri,
        Some(&helper),
        Some(
            json!({ "base_version": 5, "title": "설계 2", "description": "", "visibility": null }),
        ),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{:?}", renamed.json);
    assert_eq!(renamed.json["visibility"], "workspace");

    // 보내면 바꾼다. 소유 권한자만.
    let denied = send(
        &app,
        Method::PUT,
        &uri,
        Some(&helper),
        Some(json!({ "base_version": 6, "title": "설계 2", "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{:?}", denied.json);
    let closed = send(
        &app,
        Method::PUT,
        &uri,
        Some(&creator),
        Some(json!({ "base_version": 6, "title": "설계 2", "description": "", "visibility": "private" })),
    )
    .await;
    assert_eq!(closed.status, StatusCode::OK, "{:?}", closed.json);
    assert_eq!(closed.json["visibility"], "private");
}

/// D-104(감사 P-30): 항목 정보 저장(`PUT /api/items/{id}`의 제목·설명·공개 범위)은 읽은 판(`base_version`)을 확인한다.
/// 두 탭이 판 1을 읽고 B가 먼저 저장하면 A의 저장은 409와 서버 최신본을 받고 아무것도 바꾸지 않는다. 같은 값을 다시
/// 보내는 재시도(응답을 잃은 B)는 바꿀 것이 없어 그대로 성공한다. 판은 이 세 칸이 바뀔 때만 오르고(누가 바꿨든),
/// 항목 정보 패널(프로필)·보관·템플릿 표시는 이 판을 올리지 않는다.
#[tokio::test]
async fn stale_item_details_are_refused_with_the_latest() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "항목 판").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let m = module(&app, &admin, &ws, "IVER").await;
    let it = item(&app, &admin, &m, "설계").await;
    let uri = format!("/api/items/{}", it["id"].as_str().unwrap());
    let put = |user: &TestUser, body: Value| {
        let (app, user, uri) = (app.clone(), user.clone(), uri.clone());
        async move { send(&app, Method::PUT, &uri, Some(&user), Some(body)).await }
    };

    let b = put(
        &admin,
        json!({ "base_version": 1, "title": "설계 B", "description": "B가 쓴 설명", "visibility": "workspace" }),
    )
    .await;
    assert_eq!(b.status, StatusCode::OK, "{:?}", b.json);
    let a = put(
        &admin,
        json!({ "base_version": 1, "title": "설계 A", "description": "A가 쓴 설명" }),
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT, "{:?}", a.json);
    assert_eq!(b.json["version"], 2);
    assert_eq!(a.json["error"], "conflict");
    assert_eq!(a.json["current"]["title"], "설계 B");
    assert_eq!(a.json["current"]["description"], "B가 쓴 설명");
    assert_eq!(a.json["current"]["version"], 2);
    let (_, now) = get(&app, &admin, &uri).await;
    assert_eq!(now["title"], "설계 B");
    assert_eq!(now["visibility"], "workspace");
    assert_eq!(now["version"], 2);

    // B의 응답을 잃고 같은 내용으로 다시 보낸다: 바뀔 것이 없어 성공, 판도 그대로.
    let retry = put(
        &admin,
        json!({ "base_version": 1, "title": "설계 B", "description": "B가 쓴 설명", "visibility": "workspace" }),
    )
    .await;
    assert_eq!(retry.status, StatusCode::OK, "{:?}", retry.json);
    assert_eq!(retry.json["version"], 2);

    // 다른 경로의 쓰기는 이 판을 올리지 않는다: 항목 정보 패널, 템플릿 표시, 보관과 해제.
    let profile = send(
        &app,
        Method::PUT,
        &format!("{uri}/profile"),
        Some(&admin),
        Some(
            json!({ "base_version": 1, "leader_id": null, "goal": "출시", "scope": "",
            "starts_on": null, "ends_on": null, "status": "on_hold", "details": {} }),
        ),
    )
    .await;
    assert_eq!(profile.status, StatusCode::NO_CONTENT, "{:?}", profile.json);
    for (path, body) in [
        ("template", json!({ "template": true })),
        ("archived", json!({ "archived": true })),
        ("archived", json!({ "archived": false })),
    ] {
        let r = send(
            &app,
            Method::PUT,
            &format!("{uri}/{path}"),
            Some(&admin),
            Some(body),
        )
        .await;
        assert!(r.status.is_success(), "{path}: {:?}", r.json);
    }
    let (_, now) = get(&app, &admin, &uri).await;
    assert_eq!(now["version"], 2, "{now:?}");

    // 쓰기 권한자(작성자)가 판 2로 고친다. 그사이 소유 권한자가 공개 범위만 바꿨으면(판 3) 작성자의 옛 판은 409다.
    let closed = put(
        &admin,
        json!({ "base_version": 2, "title": "설계 B", "description": "B가 쓴 설명", "visibility": "private" }),
    )
    .await;
    assert_eq!(closed.status, StatusCode::OK, "{:?}", closed.json);
    assert_eq!(closed.json["version"], 3);
    let reopened = put(
        &admin,
        json!({ "base_version": 3, "title": "설계 B", "description": "B가 쓴 설명", "visibility": "workspace" }),
    )
    .await;
    assert_eq!(reopened.json["version"], 4);
    let stale = put(
        &writer,
        json!({ "base_version": 2, "title": "작성자 제목", "description": "" }),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    let fresh = put(
        &writer,
        json!({ "base_version": 4, "title": "작성자 제목", "description": "" }),
    )
    .await;
    assert_eq!(fresh.status, StatusCode::OK, "{:?}", fresh.json);
    assert_eq!(fresh.json["version"], 5);
    assert_eq!(fresh.json["visibility"], "workspace");

    // 보관된 항목은 판보다 먼저 보관으로 거절한다(최신본 없는 409).
    send(
        &app,
        Method::PUT,
        &format!("{uri}/archived"),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    let archived = put(
        &admin,
        json!({ "base_version": 5, "title": "보관 뒤", "description": "" }),
    )
    .await;
    assert_eq!(archived.status, StatusCode::CONFLICT, "{:?}", archived.json);
    assert_eq!(archived.json["error"], "invalid_state");
    assert!(
        archived.json.get("current").is_none(),
        "{:?}",
        archived.json
    );
}
