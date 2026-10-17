#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 항목 기록 (D-33·D-34·D-35·D-36·D-38): 항목 권한을 그대로 따르고, 종류·칸은 서버가 확인하며, 분석에 연결한
//! 파일 버전은 새 버전이 올라와도 당시 버전으로 남고 따로 지울 수 없다. 파일을 영구 삭제해도 연결 정보는 남는다.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{TestUser, add_member, create_workspace, login, new_request_id, send, send_raw};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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

/// 워크스페이스에 공개한 항목.
async fn open_item(app: &Router, admin: &TestUser, module: &str, title: &str) -> String {
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    let id = i.json["id"].as_str().unwrap().to_owned();
    let r = send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "title": title, "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    id
}

/// 항목에 파일(또는 새 버전)을 올린다. 파일 id.
async fn upload(app: &Router, user: &TestUser, body: Value, bytes: &[u8]) -> String {
    let up = send(app, Method::POST, "/api/uploads", Some(user), Some(body)).await;
    assert_eq!(up.status, StatusCode::CREATED, "{:?}", up.json);
    let id = up.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(bytes));
    send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{id}/parts/1"),
        Some(user),
        &[("x-content-sha256", hash.as_str())],
        bytes.to_vec(),
    )
    .await;
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    done.json["file_id"].as_str().unwrap().to_owned()
}

async fn new_file(app: &Router, user: &TestUser, item: &str, name: &str, bytes: &[u8]) -> String {
    upload(
        app,
        user,
        json!({ "request_id": new_request_id(), "target": { "kind": "item", "id": item }, "name": name,
                "size": bytes.len(), "content_type": "text/csv" }),
        bytes,
    )
    .await
}

async fn records(app: &Router, user: &TestUser, item: &str) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::GET,
        &format!("/api/items/{item}/records"),
        Some(user),
        None,
    )
    .await;
    (r.status, r.json)
}

#[tokio::test]
async fn analysis_records_pin_file_versions_and_follow_item_access() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "연구 기록").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let outsider = login(&state, "outsider").await;
    let research = module(&app, &admin, &ws, "research", "RES").await;
    let item = open_item(&app, &admin, &research, "학습 동기").await;
    let other_item = open_item(&app, &admin, &research, "다른 연구").await;
    let data = new_file(&app, &writer, &item, "data.csv", b"a,b\n1,2\n").await;
    let code = new_file(&app, &writer, &item, "model.R", b"lm(b ~ a)").await;
    let elsewhere = new_file(&app, &writer, &other_item, "x.csv", b"x").await;
    let uri = format!("/api/items/{item}/records");

    // 분석: 입력 데이터 v1·코드를 연결한다.
    let analysis = json!({ "request_id": new_request_id(), "kind": "analysis", "title": "회귀 분석",
        "occurred_on": "2026-10-17",
        "fields": { "purpose": "동기와 성적", "status": "done", "owner": writer.id.0, "compute_minutes": 12 },
        "files": [{ "file_id": data, "version": 1, "role": "input" }, { "file_id": code, "role": "code" }] });
    let created = send(
        &app,
        Method::POST,
        &uri,
        Some(&writer),
        Some(analysis.clone()),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let record_id = created.json["id"].as_str().unwrap().to_owned();
    // 같은 요청 id면 같은 기록.
    let again = send(&app, Method::POST, &uri, Some(&writer), Some(analysis)).await;
    assert_eq!(again.json["id"], created.json["id"]);

    // 새 버전이 올라와도 분석은 v1을 가리킨다. 연결된 v1은 따로 지울 수 없다.
    upload(
        &app,
        &writer,
        json!({ "request_id": new_request_id(), "file_id": data, "size": 4 }),
        b"a,b\n",
    )
    .await;
    let (_, list) = records(&app, &reader, &item).await;
    let rec = &list[0];
    assert_eq!(rec["files"][0]["version"], 1);
    assert_eq!(rec["files"][0]["file_name"], "data.csv");
    assert_eq!(rec["files"][0]["available"], true);
    assert_eq!(rec["files"][1]["role"], "code");
    assert_eq!(rec["fields"]["compute_minutes"], 12.0);
    assert_eq!(rec["can_edit"], false);
    let pinned = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{data}/versions/1"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(pinned.status, StatusCode::CONFLICT, "{:?}", pinned.json);

    // 항목 권한: 독자는 읽기만, 밖의 사람은 404.
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            Some(&reader),
            Some(json!({ "request_id": new_request_id(), "kind": "dataset", "title": "x" }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        records(&app, &outsider, &item).await.0,
        StatusCode::NOT_FOUND
    );

    // 종류·칸·사람·파일은 서버가 확인한다.
    for (body, why) in [
        (
            json!({ "kind": "risk", "title": "x" }),
            "kind of another module",
        ),
        (
            json!({ "kind": "reference", "title": "x", "fields": { "read_status": "maybe" } }),
            "bad choice",
        ),
        (
            json!({ "kind": "reference", "title": "x", "fields": { "url": "javascript:alert(1)" } }),
            "bad url",
        ),
        (
            json!({ "kind": "analysis", "title": "x", "fields": { "owner": outsider.id.0 } }),
            "person outside",
        ),
        (
            json!({ "kind": "analysis", "title": "x", "files": [{ "file_id": elsewhere, "role": "input" }] }),
            "file of another item",
        ),
        (
            json!({ "kind": "variable", "title": "x", "files": [{ "file_id": data, "role": "input" }] }),
            "kind without files",
        ),
        (json!({ "kind": "dataset", "title": " " }), "blank title"),
    ] {
        let mut body = body;
        body["request_id"] = json!(new_request_id());
        let r = send(&app, Method::POST, &uri, Some(&writer), Some(body)).await;
        assert_eq!(r.status, StatusCode::BAD_REQUEST, "{why}: {:?}", r.json);
    }

    // 고치기: 읽은 판이 아니면 409. 지우기: 만든 사람이나 소유자만.
    let edit_uri = format!("/api/records/{record_id}");
    let edit = |version: i64| {
        json!({ "version": version, "title": "회귀 분석 2", "occurred_on": null,
        "fields": { "status": "failed", "log": "수렴 실패" },
        "files": [{ "file_id": data, "version": 2, "role": "input" }] })
    };
    let edited = send(&app, Method::PUT, &edit_uri, Some(&other), Some(edit(1))).await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.json);
    assert_eq!(edited.json["version"], 2);
    assert_eq!(
        send(&app, Method::PUT, &edit_uri, Some(&other), Some(edit(1)))
            .await
            .status,
        StatusCode::CONFLICT
    );
    // 연결을 v2로 바꿨으니 v1은 이제 지울 수 있다.
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/files/{data}/versions/1"),
            Some(&writer),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::DELETE, &edit_uri, Some(&other), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // 파일을 영구 삭제해도 연결은 이름·버전·SHA-256으로 남는다.
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{data}"),
        Some(&writer),
        None,
    )
    .await;
    let batch = trashed.json["batch_id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/trash/{batch}"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let (_, list) = records(&app, &writer, &item).await;
    assert_eq!(list[0]["files"][0]["file_name"], "data.csv");
    assert_eq!(list[0]["files"][0]["version"], 2);
    assert_eq!(list[0]["files"][0]["available"], false);
    assert!(list[0]["files"][0]["file_id"].is_null());

    assert_eq!(
        send(&app, Method::DELETE, &edit_uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(records(&app, &writer, &item).await.1, json!([]));
}

#[tokio::test]
async fn study_materials_link_courses_and_kinds_are_published() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "스터디 기록").await;
    let study = module(&app, &admin, &ws, "study", "STU").await;
    let lecture = module(&app, &admin, &ws, "course", "LEC").await;
    let group = open_item(&app, &admin, &study, "통계 스터디").await;
    let course = open_item(&app, &admin, &lecture, "회귀분석 강의").await;
    let uri = format!("/api/items/{group}/records");
    let material = |linked: &str| {
        json!({ "request_id": new_request_id(), "kind": "material", "title": "강의",
        "fields": { "material_type": "course", "course": linked } })
    };
    let r = send(
        &app,
        Method::POST,
        &uri,
        Some(&admin),
        Some(material(&course)),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(
        send(
            &app,
            Method::POST,
            &uri,
            Some(&admin),
            Some(material(&group))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    let exam = json!({ "request_id": new_request_id(), "kind": "exam", "title": "중간 점검",
        "occurred_on": "2026-10-20", "fields": { "taker": admin.id.0, "score": 85, "max_score": 100 } });
    assert_eq!(
        send(&app, Method::POST, &uri, Some(&admin), Some(exam))
            .await
            .status,
        StatusCode::CREATED
    );

    let kinds = send(&app, Method::GET, "/api/record-kinds", Some(&admin), None).await;
    let list = kinds.json.as_array().unwrap();
    assert!(
        list.iter()
            .any(|k| k["kind"] == "analysis" && k["module_kind"] == "research")
    );
    assert!(
        list.iter()
            .any(|k| k["kind"] == "supply" && k["module_kind"] == "hobby")
    );
    assert!(list.iter().all(|k| k["module_kind"] != "course"));
}

/// 이전 버전 보존 기간(D-56): 기간이 지난 이전 버전은 작업이 지우지만, 기록이 연결한 버전·문서 리비전이 가리키는 버전·
/// 지금 버전은 남는다. 리비전이 가리키는 버전은 하나만 따로 지우려 해도 409.
#[tokio::test]
async fn old_versions_expire_unless_something_refers_to_them() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "버전 보존").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let research = module(&app, &admin, &ws, "research", "KEEP").await;
    let item = open_item(&app, &admin, &research, "보존 연구").await;
    // 파일 하나에 버전 4개: v1(기록이 연결), v2(아무도 안 씀), v3(문서 리비전의 이미지), v4(지금).
    let file = new_file(&app, &writer, &item, "data.csv", b"v1").await;
    for body in [&b"v2"[..], b"v3", b"v4"] {
        upload(
            &app,
            &writer,
            json!({ "request_id": new_request_id(), "file_id": file, "size": body.len() }),
            body,
        )
        .await;
    }
    let pin = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/records"),
        Some(&writer),
        Some(json!({
        "request_id": new_request_id(), "kind": "analysis", "title": "고정",
        "files": [{ "file_id": file, "version": 1, "role": "input" }] })),
    )
    .await;
    assert_eq!(pin.status, StatusCode::CREATED, "{:?}", pin.json);
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/documents"),
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "그림" })),
    )
    .await;
    let doc_id = doc.json["id"].as_str().unwrap().to_owned();
    let image = json!({ "type": "doc", "content": [{ "type": "image",
        "attrs": { "src": format!("/api/files/{file}/content?version=3&inline=true") } }] });
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc_id}"),
        Some(&writer),
        Some(json!({
        "request_id": new_request_id(), "base_revision": 1, "title": "그림", "body": image })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    // 리비전이 가리키는 버전은 따로 지울 수 없다.
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/files/{file}/versions/3"),
            Some(&writer),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );

    // 보존 기간: 관리자만, 1~3650일.
    let retention = format!("/api/workspaces/{ws}/version-retention");
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &retention,
            Some(&writer),
            Some(json!({ "days": 1 }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &retention,
            Some(&admin),
            Some(json!({ "days": 0 }))
        )
        .await
        .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &retention,
            Some(&admin),
            Some(json!({ "days": 1 }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let trash = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(trash.json["version_retention_days"], 1);

    // 버전들을 열흘 전 것으로 만든 뒤 작업을 돌린다(테이블 소유 역할로 시각만 바꾼다).
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    sqlx::query(
        "UPDATE fvoci.file_versions SET created_at = now() - interval '10 days' WHERE file_id = $1",
    )
    .bind(uuid::Uuid::parse_str(&file).unwrap())
    .execute(&owner)
    .await
    .unwrap();
    owner.close().await;
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();

    let detail = send(
        &app,
        Method::GET,
        &format!("/api/files/{file}"),
        Some(&writer),
        None,
    )
    .await;
    let mut kept: Vec<i64> = detail.json["versions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["version"].as_i64().unwrap())
        .collect();
    kept.sort_unstable();
    assert_eq!(kept, [1, 3, 4], "{:?}", detail.json);
}
