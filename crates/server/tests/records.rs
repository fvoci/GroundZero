#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
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

/// 리뷰 반영(D-56): 보존 기간은 다음 버전이 올라온 때부터 센다. 붙잡힌 오래된 버전이 500개를 넘어도 다른 버전
/// 정리가 멈추지 않는다. 다른 곳(남의 개인 노트)의 문서가 주소를 적어도 버전을 붙잡지 못한다. 첨부 주소는
/// 표준형만 받는다.
#[tokio::test]
async fn version_retention_counts_from_replacement_and_ignores_foreign_pins() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let outsider = login(&state, "outsider").await;
    let ws = create_workspace(&app, &admin, "버전 보존 2").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let research = module(&app, &admin, &ws, "research", "KEEP2").await;
    let item = open_item(&app, &admin, &research, "보존 연구").await;
    let two_versions = |name: &'static str| {
        let (app, writer, item) = (app.clone(), writer.clone(), item.clone());
        async move {
            let id = new_file(&app, &writer, &item, name, b"one").await;
            upload(
                &app,
                &writer,
                json!({ "request_id": new_request_id(), "file_id": id, "size": 3 }),
                b"two",
            )
            .await;
            uuid::Uuid::parse_str(&id).unwrap()
        }
    };
    let fresh = two_versions("fresh.csv").await;
    let pinned = two_versions("pinned.csv").await;
    let foreign = two_versions("foreign.csv").await;
    let retention = send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/version-retention"),
        Some(&admin),
        Some(json!({ "days": 1 })),
    )
    .await;
    assert_eq!(retention.status, StatusCode::NO_CONTENT);
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let age = |file: uuid::Uuid, version: i32, days: i32| {
        let owner = owner.clone();
        async move {
            sqlx::query(
                "UPDATE fvoci.file_versions SET created_at = now() - make_interval(days => $3)
                 WHERE file_id = $1 AND version = $2",
            )
            .bind(file)
            .bind(version)
            .bind(days)
            .execute(&owner)
            .await
            .unwrap();
        }
    };
    let versions = |file: uuid::Uuid| {
        let owner = owner.clone();
        async move {
            sqlx::query_scalar::<_, i32>(
                "SELECT version FROM fvoci.file_versions WHERE file_id = $1 ORDER BY version",
            )
            .bind(file)
            .fetch_all(&owner)
            .await
            .unwrap()
        }
    };
    let run = || async {
        fvoci_server::worker::run_all_now(&state.db, None, None)
            .await
            .unwrap();
    };

    // 열흘 전에 올린 v1이라도 v2가 방금 올라왔으면 남는다.
    age(fresh, 1, 10).await;
    run().await;
    assert_eq!(versions(fresh).await, [1, 2]);

    // 같은 워크스페이스 문서가 붙잡은 오래된 버전 501개(정리 후보 맨 앞)가 있어도 다른 버전은 정리된다.
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/documents"),
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "붙잡기" })),
    )
    .await;
    let doc_id = uuid::Uuid::parse_str(doc.json["id"].as_str().unwrap()).unwrap();
    // 버전과 참조를 한 문장으로 넣는다(동시에 도는 다른 테스트의 정리 작업이 참조 없는 순간을 보지 않게).
    sqlx::query(
        "WITH v AS (
           INSERT INTO fvoci.file_versions (file_id, version, size, sha256, content_type, storage_key, uploaded_by, created_at)
           SELECT $1, n, 0, repeat('0', 64), 'text/csv', 'test-pinned/' || $1 || '/' || n, $2, now() - interval '20 days'
           FROM generate_series(1000, 1501) n
           RETURNING version)
         INSERT INTO fvoci.document_file_refs (document_id, revision, file_id, version)
         SELECT $3, 1, $1, version FROM v",
    )
    .bind(pinned)
    .bind(writer.id.0)
    .bind(doc_id)
    .execute(&owner)
    .await
    .unwrap();

    // 워크스페이스 밖 사람이 개인 노트에 이 파일 v1 주소를 적는다. 표준형이 아니면 400.
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&outsider),
        Some(json!({ "request_id": new_request_id(), "title": "남의 버전" })),
    )
    .await;
    let note_id = note.json["id"].as_str().unwrap().to_owned();
    let save = |src: String| {
        let (app, outsider, note_id) = (app.clone(), outsider.clone(), note_id.clone());
        async move {
            send(&app, Method::PUT, &format!("/api/documents/{note_id}"), Some(&outsider), Some(json!({
                "request_id": new_request_id(), "base_revision": 1, "title": "남의 버전",
                "body": { "type": "doc", "content": [{ "type": "image", "attrs": { "src": src } }] } })))
            .await
            .status
        }
    };
    let upper = foreign.to_string().to_uppercase();
    assert_eq!(
        save(format!("/api/files/{upper}/content?version=1")).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        save(format!(
            "/api/files/{foreign}/content?inline=true&version=1"
        ))
        .await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        save(format!("/api/files/{foreign}/content?version=1")).await,
        StatusCode::OK
    );
    let recorded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.document_file_refs WHERE file_id = $1 AND version = 1",
    )
    .bind(foreign)
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(recorded, 1);

    age(fresh, 2, 2).await;
    age(foreign, 1, 10).await;
    age(foreign, 2, 10).await;
    run().await;
    assert_eq!(versions(fresh).await, [2]);
    assert_eq!(versions(foreign).await, [2]);
    // 붙잡힌 502개는 그대로(v1은 가짜 새 버전들 때문에 오래전에 바뀐 것으로 보여 정리된다).
    assert_eq!(
        versions(pinned)
            .await
            .iter()
            .filter(|v| **v >= 1000)
            .count(),
        502
    );
    owner.close().await;
}

async fn owner_pool() -> sqlx::PgPool {
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    sqlx::PgPool::connect(&url).await.unwrap()
}

/// 휴지통 묶음을 보존 기간보다 오래전에 지운 것으로 만든다(테이블 소유 역할로 시각만 바꾼다).
async fn expire_trash(owner: &sqlx::PgPool, file: &str) {
    sqlx::query(
        "UPDATE fvoci.trash_batches SET deleted_at = now() - interval '400 days'
         WHERE id = (SELECT trash_batch_id FROM fvoci.files WHERE id = $1::uuid)",
    )
    .bind(file)
    .execute(owner)
    .await
    .unwrap();
}

async fn file_exists(owner: &sqlx::PgPool, file: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM fvoci.files WHERE id = $1::uuid)")
        .bind(file)
        .fetch_one(owner)
        .await
        .unwrap()
}

async fn save_image(app: &Router, user: &TestUser, doc: &str, file: &str) -> StatusCode {
    let body = json!({ "type": "doc", "content": [{ "type": "image",
        "attrs": { "src": format!("/api/files/{file}/content?version=1") } }] });
    send(
        app,
        Method::PUT,
        &format!("/api/documents/{doc}"),
        Some(user),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1, "title": "참조", "body": body })),
    )
    .await
    .status
}

async fn trash_file(app: &Router, user: &TestUser, file: &str) {
    let r = send(
        app,
        Method::DELETE,
        &format!("/api/files/{file}"),
        Some(user),
        None,
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
}

/// O-34(D-56): 휴지통 만료로 자동 영구 삭제할 때, 함께 사라지지 않는 기록·문서가 참조하는 버전이 있는 파일은 남기고
/// 화면에 알린다. 참조가 묶음 안에서 함께 사라지면(항목째 지움) 그대로 지운다. 관리자 직접 영구 삭제는 바꾸지 않는다.
#[tokio::test]
async fn trash_expiry_keeps_files_with_referenced_versions() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "만료 보호").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let research = module(&app, &admin, &ws, "research", "EXP").await;
    let item = open_item(&app, &admin, &research, "만료 연구").await;
    let with_v2 = |name: &'static str, item: String| {
        let (app, writer) = (app.clone(), writer.clone());
        async move {
            let id = new_file(&app, &writer, &item, name, b"one").await;
            upload(
                &app,
                &writer,
                json!({ "request_id": new_request_id(), "file_id": id, "size": 3 }),
                b"two",
            )
            .await;
            id
        }
    };
    let pinned = with_v2("pinned.csv", item.clone()).await;
    let pictured = with_v2("pictured.png", item.clone()).await;
    let loose = with_v2("loose.csv", item.clone()).await;
    let record = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/records"),
        Some(&writer),
        Some(
            json!({ "request_id": new_request_id(), "kind": "analysis", "title": "고정",
            "files": [{ "file_id": pinned, "version": 1, "role": "input" }] }),
        ),
    )
    .await;
    assert_eq!(record.status, StatusCode::CREATED, "{:?}", record.json);
    let wiki = common::create_doc(&app, &writer, &ws, "그림 문서").await;
    let wiki_id = wiki["id"].as_str().unwrap();
    assert_eq!(
        save_image(&app, &writer, wiki_id, &pictured).await,
        StatusCode::OK
    );

    let owner = owner_pool().await;
    for f in [&pinned, &pictured, &loose] {
        trash_file(&app, &writer, f).await;
        expire_trash(&owner, f).await;
    }
    let run = || async {
        fvoci_server::worker::run_all_now(&state.db, None, None)
            .await
            .unwrap();
    };
    run().await;
    assert!(
        file_exists(&owner, &pinned).await,
        "record-linked version kept"
    );
    assert!(
        file_exists(&owner, &pictured).await,
        "document-linked version kept"
    );
    assert!(
        !file_exists(&owner, &loose).await,
        "unreferenced file purged"
    );

    // 화면은 만료된 묶음 중 참조 때문에 남긴 것을 알린다.
    let trash = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(&admin),
        None,
    )
    .await;
    let entries = trash.json["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2, "{:?}", trash.json);
    assert!(
        entries
            .iter()
            .all(|e| e["expired"] == true && e["kept_for_references"] == true)
    );

    // 기록이 연결을 놓으면 다음 실행에서 지운다.
    let (_, list) = records(&app, &writer, &item).await;
    let r = &list[0];
    let unlinked = send(
        &app,
        Method::PUT,
        &format!("/api/records/{}", r["id"].as_str().unwrap()),
        Some(&writer),
        Some(
            json!({ "version": r["version"], "title": "고정", "occurred_on": null, "body": "",
            "fields": r["fields"], "files": [] }),
        ),
    )
    .await;
    assert_eq!(unlinked.status, StatusCode::OK, "{:?}", unlinked.json);
    run().await;
    assert!(!file_exists(&owner, &pinned).await);
    assert!(file_exists(&owner, &pictured).await);

    // 기록과 파일이 같은 항목에 있고 항목째 휴지통에서 만료되면, 참조도 함께 사라지므로 지운다.
    let gone_item = open_item(&app, &admin, &research, "함께 지울 연구").await;
    let inner = with_v2("inner.csv", gone_item.clone()).await;
    let inner_record = send(
        &app,
        Method::POST,
        &format!("/api/items/{gone_item}/records"),
        Some(&writer),
        Some(
            json!({ "request_id": new_request_id(), "kind": "analysis", "title": "안쪽",
            "files": [{ "file_id": inner, "version": 1, "role": "input" }] }),
        ),
    )
    .await;
    assert_eq!(inner_record.status, StatusCode::CREATED);
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/items/{gone_item}"),
        Some(&admin),
        None,
    )
    .await;
    assert!(trashed.status.is_success(), "{:?}", trashed.json);
    sqlx::query(
        "UPDATE fvoci.trash_batches SET deleted_at = now() - interval '400 days'
         WHERE root_kind = 'item' AND root_id = $1::uuid",
    )
    .bind(&gone_item)
    .execute(&owner)
    .await
    .unwrap();
    run().await;
    assert!(!file_exists(&owner, &inner).await);
    owner.close().await;
}

/// O-34 동시성: 자동 정리와 새 문서 참조가 겹쳐도 참조된 버전을 지우지 않는다. 저장이 버전 행을 먼저 잡으면
/// 정리는 기다리지 않고 그 묶음을 이번 실행에서 건너뛰며, 저장이 끝난 뒤에는 새 참조를 보고 남긴다. 정리가 먼저
/// 잡았으면 저장은 기다렸다가 오류 없이 끝난다(지워진 뒤의 주소일 뿐).
#[tokio::test]
async fn trash_expiry_and_a_new_reference_race_safely() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "만료 경합").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let research = module(&app, &admin, &ws, "research", "RACE").await;
    let item = open_item(&app, &admin, &research, "경합 연구").await;
    let owner = owner_pool().await;
    let wiki = common::create_doc(&app, &writer, &ws, "경합 문서").await;
    let wiki_id = uuid::Uuid::parse_str(wiki["id"].as_str().unwrap()).unwrap();

    // 1) 저장이 먼저: 리비전을 넣는 트랜잭션을 열어 둔 채(참조 트리거가 버전 행을 공유 잠금) 정리를 돌린다.
    let first = new_file(&app, &writer, &item, "first.png", b"one").await;
    trash_file(&app, &writer, &first).await;
    expire_trash(&owner, &first).await;
    // 서버의 저장과 같은 두 문장(문서 갱신, 리비전 추가)을 한 트랜잭션에서 하고 끝내지 않는다.
    let mut save = owner.begin().await.unwrap();
    let body = json!({ "type": "doc", "content": [{ "type": "image",
        "attrs": { "src": format!("/api/files/{first}/content?version=1") } }] });
    sqlx::query("UPDATE fvoci.documents SET body = $2, revision = revision + 1 WHERE id = $1")
        .bind(wiki_id)
        .bind(&body)
        .execute(&mut *save)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO fvoci.document_revisions (document_id, revision, title, schema_version, body, author, request_id)
         SELECT d.id, d.revision, d.title, d.schema_version, d.body, d.created_by, gen_random_uuid()
         FROM fvoci.documents d WHERE d.id = $1",
    )
    .bind(wiki_id)
    .execute(&mut *save)
    .await
    .unwrap();
    // 정리는 저장이 잡은 행을 기다리지 않고(SKIP LOCKED) 이번 실행에서 이 묶음을 건너뛴다.
    let run = || async {
        fvoci_server::worker::run_all_now(&state.db, None, None)
            .await
            .unwrap();
    };
    run().await;
    assert!(
        file_exists(&owner, &first).await,
        "skipped while a save holds the version"
    );
    save.commit().await.unwrap();
    run().await;
    assert!(
        file_exists(&owner, &first).await,
        "the version referenced by the committed save is kept"
    );

    // 2) 정리가 먼저: 정리 트랜잭션을 열어 둔 채 저장하면 저장은 기다렸다가 오류 없이 끝나고, 파일은 지워져 있다.
    let second = new_file(&app, &writer, &item, "second.png", b"one").await;
    trash_file(&app, &writer, &second).await;
    expire_trash(&owner, &second).await;
    let batch: uuid::Uuid =
        sqlx::query_scalar("SELECT trash_batch_id FROM fvoci.files WHERE id = $1::uuid")
            .bind(&second)
            .fetch_one(&owner)
            .await
            .unwrap();
    let mut purging = owner.begin().await.unwrap();
    // 작업 프로세스의 묶음 정리와 같은 경로를 이 묶음에만 쓴다(열어 둔 트랜잭션이 다른 테스트의 묶음까지 잡지 않게).
    sqlx::query("SELECT fvoci.app_purge_batch_as($1, true)")
        .bind(batch)
        .execute(&mut *purging)
        .await
        .unwrap();
    let purged: bool =
        sqlx::query_scalar("SELECT purged_at IS NOT NULL FROM fvoci.trash_batches WHERE id = $1")
            .bind(batch)
            .fetch_one(&mut *purging)
            .await
            .unwrap();
    assert!(purged, "the unreferenced bundle is purged");
    let doc2 = common::create_doc(&app, &writer, &ws, "늦은 참조").await;
    let doc2_id = doc2["id"].as_str().unwrap().to_owned();
    let (app2, writer2, second2) = (app.clone(), writer.clone(), second.clone());
    let late = tokio::spawn(async move { save_image(&app2, &writer2, &doc2_id, &second2).await });
    // 저장이 정리의 행 잠금을 실제로 기다리는지 서버 상태로 확인한다(시간 추측 대신).
    // 다른 역할의 쿼리 글은 관리 역할만 볼 수 있다.
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let watcher = sqlx::PgPool::connect(&admin_url).await.unwrap();
    let mut waited = false;
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity
             WHERE wait_event_type = 'Lock' AND query LIKE '%INSERT INTO document_revisions%'",
        )
        .fetch_one(&watcher)
        .await
        .unwrap();
        if n > 0 {
            waited = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let waiting: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT wait_event_type, left(query, 120) FROM pg_stat_activity
         WHERE datname = current_database() AND state <> 'idle' AND pid <> pg_backend_pid()",
    )
    .fetch_all(&watcher)
    .await
    .unwrap();
    watcher.close().await;
    if !waited {
        let finished = late.is_finished();
        panic!("no lock wait seen (save finished: {finished}); active: {waiting:?}");
    }
    assert!(!late.is_finished());
    purging.commit().await.unwrap();
    assert_eq!(late.await.unwrap(), StatusCode::OK);
    assert!(!file_exists(&owner, &second).await);
    owner.close().await;
}

/// O-34 리뷰 반영: 문서 묶음의 첨부를 바깥의 살아 있는 문서가 가리키면 남기고, 만료된 두 문서가 서로의 첨부를
/// 가리키면(둘 다 되살릴 수 없음) 둘 다 지운다.
#[tokio::test]
async fn trash_expiry_of_documents_honours_live_references_and_breaks_cycles() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "문서 만료").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let doc = |title: &'static str| {
        let (app, writer, ws) = (app.clone(), writer.clone(), ws.clone());
        async move {
            common::create_doc(&app, &writer, &ws, title).await["id"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    };
    let attach = |doc: String, name: &'static str| {
        let (app, writer) = (app.clone(), writer.clone());
        async move {
            upload(
                &app,
                &writer,
                json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc },
                        "name": name, "size": 3, "content_type": "image/png" }),
                b"png",
            )
            .await
        }
    };
    let (a, b, live, gone) = (
        doc("A").await,
        doc("B").await,
        doc("살아 있음").await,
        doc("지움").await,
    );
    let file_a = attach(a.clone(), "a.png").await;
    let file_b = attach(b.clone(), "b.png").await;
    let file_gone = attach(gone.clone(), "d.png").await;
    assert_eq!(save_image(&app, &writer, &a, &file_b).await, StatusCode::OK);
    assert_eq!(save_image(&app, &writer, &b, &file_a).await, StatusCode::OK);
    assert_eq!(
        save_image(&app, &writer, &live, &file_gone).await,
        StatusCode::OK
    );
    for d in [&a, &b, &gone] {
        let r = send(
            &app,
            Method::DELETE,
            &format!("/api/documents/{d}"),
            Some(&writer),
            None,
        )
        .await;
        assert!(r.status.is_success(), "{:?}", r.json);
    }
    let owner = owner_pool().await;
    sqlx::query(
        "UPDATE fvoci.trash_batches SET deleted_at = now() - interval '400 days'
         WHERE root_kind = 'document' AND root_id = ANY ($1::uuid[])",
    )
    .bind(vec![a.clone(), b.clone(), gone.clone()])
    .execute(&owner)
    .await
    .unwrap();
    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    assert!(
        file_exists(&owner, &file_gone).await,
        "a live document still shows it"
    );
    assert!(
        !file_exists(&owner, &file_a).await,
        "cycle between expired documents"
    );
    assert!(
        !file_exists(&owner, &file_b).await,
        "cycle between expired documents"
    );
    owner.close().await;
}

/// D-69(O-35): 참조 때문에 자동 삭제를 미룬 만료 묶음은 기한이 지나도 되살릴 수 있다. 되살리기 권한은 그대로이고
/// (지운 사람·관리자), 되살리면 문서 그림·기록이 가리키던 원래 버전이 다시 열린다. 참조 없는 만료 묶음은 여전히 409.
#[tokio::test]
async fn kept_expired_bundles_restore_with_existing_permissions() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보존 되살리기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
    let research = module(&app, &admin, &ws, "research", "KEPT").await;
    let item = open_item(&app, &admin, &research, "보존 연구").await;
    let file = new_file(&app, &writer, &item, "plot.png", b"one").await;
    upload(
        &app,
        &writer,
        json!({ "request_id": new_request_id(), "file_id": file, "size": 3 }),
        b"two",
    )
    .await;
    let record = send(
        &app,
        Method::POST,
        &format!("/api/items/{item}/records"),
        Some(&writer),
        Some(
            json!({ "request_id": new_request_id(), "kind": "analysis", "title": "고정",
            "files": [{ "file_id": file, "version": 1, "role": "result" }] }),
        ),
    )
    .await;
    assert_eq!(record.status, StatusCode::CREATED, "{:?}", record.json);
    let wiki = common::create_doc(&app, &writer, &ws, "그림").await;
    let wiki_id = wiki["id"].as_str().unwrap().to_owned();
    assert_eq!(
        save_image(&app, &writer, &wiki_id, &file).await,
        StatusCode::OK
    );
    let loose = new_file(&app, &writer, &item, "loose.csv", b"x").await;

    let owner = owner_pool().await;
    let batch_of = |f: String| {
        let owner = owner.clone();
        async move {
            sqlx::query_scalar::<_, uuid::Uuid>(
                "SELECT trash_batch_id FROM fvoci.files WHERE id = $1::uuid",
            )
            .bind(f)
            .fetch_one(&owner)
            .await
            .unwrap()
        }
    };
    // 묶음 id는 만료시키기 전에 읽는다. 만료된 뒤에는 병렬로 도는 다른 테스트의 정리가 참조 없는 묶음(loose)을
    // 지울 수 있다(CI에서 RowNotFound).
    for f in [&file, &loose] {
        trash_file(&app, &writer, f).await;
    }
    let kept_batch = batch_of(file.clone()).await;
    let loose_batch = batch_of(loose.clone()).await;
    for f in [&file, &loose] {
        expire_trash(&owner, f).await;
    }
    let restore = |user: &TestUser, batch: uuid::Uuid| {
        let (app, user) = (app.clone(), user.clone());
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/trash/{batch}/restore"),
                Some(&user),
                None,
            )
            .await
            .status
        }
    };
    // 참조 없는 만료 묶음은 되살릴 수 없다(기존 규칙). 같은 DB를 쓰는 다른 테스트의 정리 작업이 먼저 영구
    // 삭제했으면 404인데, 그때도 작업 프로세스가 지운 것인지 확인한다(둘 다 "되살릴 수 없음").
    match restore(&writer, loose_batch).await {
        StatusCode::CONFLICT => {}
        StatusCode::NOT_FOUND => {
            let purged: bool = sqlx::query_scalar(
                "SELECT purged_at IS NOT NULL FROM fvoci.trash_batches WHERE id = $1",
            )
            .bind(loose_batch)
            .fetch_one(&owner)
            .await
            .unwrap();
            assert!(purged, "404 only because the expired bundle was purged");
        }
        other => panic!("unexpected restore status {other}"),
    }

    fvoci_server::worker::run_all_now(&state.db, None, None)
        .await
        .unwrap();
    assert!(file_exists(&owner, &file).await, "kept for references");
    let image = format!("/api/files/{file}/content?version=1");
    assert_eq!(
        send_raw(&app, Method::GET, &image, Some(&reader), &[], Vec::new())
            .await
            .0,
        StatusCode::NOT_FOUND,
        "a trashed file is not readable"
    );

    // 권한은 넓어지지 않는다: 지우지 않은 작성자·독자는 404, 지운 사람은 되살린다.
    assert_eq!(restore(&other, kept_batch).await, StatusCode::NOT_FOUND);
    assert_eq!(restore(&reader, kept_batch).await, StatusCode::NOT_FOUND);
    let restored = restore(&writer, kept_batch).await;
    assert!(restored.is_success(), "{restored}");

    // 원래 버전을 가리키던 그림과 기록이 다시 열린다(독자에게도, 기존 읽기 권한대로).
    let (status, _, body) =
        send_raw(&app, Method::GET, &image, Some(&reader), &[], Vec::new()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"one");
    let doc = send(
        &app,
        Method::GET,
        &format!("/api/documents/{wiki_id}"),
        Some(&reader),
        None,
    )
    .await;
    assert!(doc.json["body"].to_string().contains(&image));
    let (_, list) = records(&app, &reader, &item).await;
    assert_eq!(list[0]["files"][0]["version"], 1, "{list:?}");
    assert_eq!(list[0]["files"][0]["available"], true, "{list:?}");
    let trash = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(&admin),
        None,
    )
    .await;
    assert!(
        trash.json["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"] != kept_batch.to_string()),
        "{:?}",
        trash.json
    );
    owner.close().await;
}
