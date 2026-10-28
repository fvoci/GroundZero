#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 항목 기록 (D-33·D-34·D-35·D-36·D-38): 항목 권한을 그대로 따르고, 종류·칸은 서버가 확인하며, 분석에 연결한
//! 파일 버전은 새 버전이 올라와도 당시 버전으로 남고 따로 지울 수 없다. 파일을 영구 삭제해도 연결 정보는 남는다.

use crate::common;

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
        Some(json!({ "base_version": 1, "title": title, "description": "", "visibility": "workspace" })),
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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (other, _) = add_member(&app, &state, &admin, &ws, "other", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
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
    let stale = send(&app, Method::PUT, &edit_uri, Some(&other), Some(edit(1))).await;
    assert_eq!(stale.status, StatusCode::CONFLICT, "{:?}", stale.json);
    // 판 충돌은 다시 보내라는 재시도가 아니라 다시 읽으라는 뜻이다(감사 C32, #77): 409 `invalid_state`와 SQL의 설명 그대로.
    assert_eq!(stale.json["error"], "invalid_state");
    assert_eq!(stale.json["message"], "the record changed; reload it");
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
    // 휴지통에 있는(되살릴 수 있는) 파일을 연결한 기록도 고칠 수 있다(감사 반영: 예전에는 400).
    let edited = send(&app, Method::PUT, &edit_uri, Some(&other), Some(edit(2))).await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.json);
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
    // 그 뒤 기록을 고쳐도(화면은 사라진 파일을 다시 보내지 못한다) 당시 버전의 연결은 남는다(D-35).
    let edited = send(
        &app,
        Method::PUT,
        &edit_uri,
        Some(&other),
        Some(
            json!({ "version": 3, "title": "회귀 분석 3", "occurred_on": null,
                     "fields": { "status": "failed", "log": "수렴 실패" }, "files": [] }),
        ),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.json);
    let (_, list) = records(&app, &writer, &item).await;
    assert_eq!(list[0]["title"], "회귀 분석 3");
    assert_eq!(list[0]["files"][0]["file_name"], "data.csv");
    assert_eq!(list[0]["files"][0]["version"], 2);
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
    // 강의 모듈의 기록 종류는 질문·복습뿐이다(D-123).
    let course: Vec<&str> = list
        .iter()
        .filter(|k| k["module_kind"] == "course")
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(course, ["question", "review"]);
}

/// 이전 버전 보존 기간(D-56): 기간이 지난 이전 버전은 작업이 지우지만, 기록이 연결한 버전·문서 리비전이 가리키는 버전·
/// 지금 버전은 남는다. 리비전이 가리키는 버전은 하나만 따로 지우려 해도 409.
#[tokio::test]
async fn item_files_are_read_only_while_the_item_or_its_module_is_archived() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "파일 보관").await;
    let module = module(&app, &admin, &ws, "project", "FAR").await;
    let item = open_item(&app, &admin, &module, "항목").await;
    let file_id = new_file(&app, &admin, &item, "plan.csv", b"a,b\n1,2\n").await;

    let can_write = || {
        let (app, admin, item) = (app.clone(), admin.clone(), item.clone());
        async move {
            let r = send(
                &app,
                Method::GET,
                &format!("/api/items/{item}/files"),
                Some(&admin),
                None,
            )
            .await;
            assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
            r.json[0]["can_write"].as_bool().unwrap()
        }
    };
    let archive = |path: String, archived: bool| {
        let (app, admin) = (app.clone(), admin.clone());
        async move {
            send(
                &app,
                Method::PUT,
                &path,
                Some(&admin),
                Some(json!({ "archived": archived })),
            )
            .await
            .status
        }
    };
    let rename = || {
        let (app, admin, file_id) = (app.clone(), admin.clone(), file_id.clone());
        async move {
            send(
                &app,
                Method::PUT,
                &format!("/api/files/{file_id}"),
                Some(&admin),
                Some(json!({ "name": "plan-2.csv" })),
            )
            .await
            .status
        }
    };
    assert!(can_write().await);

    let module_path = format!("/api/modules/{module}/archived");
    assert_eq!(archive(module_path.clone(), true).await, StatusCode::OK);
    assert!(!can_write().await);
    assert_eq!(rename().await, StatusCode::CONFLICT);
    assert_eq!(archive(module_path, false).await, StatusCode::OK);

    let item_path = format!("/api/items/{item}/archived");
    assert_eq!(archive(item_path.clone(), true).await, StatusCode::OK);
    assert!(!can_write().await);
    assert_eq!(archive(item_path, false).await, StatusCode::OK);
    assert!(can_write().await);
    assert_eq!(rename().await, StatusCode::OK);
}

/// 감사 W15: 사람 칸(출석·참여자·공저자)에 나간 멤버가 남은 기록은, 다른 칸만 고쳐도 서버가 그대로인 id까지 다시
/// 확인해 400이었다(화면에는 그 사람을 뺄 칸도 없었다). 이미 저장된 id는 다시 확인하지 않는다. 새로 넣는 id는 그대로
/// 확인한다.
#[tokio::test]
async fn records_keep_a_member_who_left_while_other_fields_change() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "나간 출석자").await;
    let (gone, _) = add_member(&app, &state, &admin, &ws, "gone", "member").await;
    let outsider = login(&state, "outsider").await;
    let study = module(&app, &admin, &ws, "study", "LFT").await;
    let group = open_item(&app, &admin, &study, "읽기 모임").await;
    let created = send(
        &app,
        Method::POST,
        &format!("/api/items/{group}/records"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "meeting", "title": "1회",
                     "fields": { "attendees": [admin.id.0, gone.id.0] } }),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let uri = format!("/api/records/{}", created.json["id"].as_str().unwrap());
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", gone.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);

    // 제목만 고친다(출석은 그대로).
    let edited = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "version": 1, "title": "1회 모임",
        "fields": { "attendees": [admin.id.0, gone.id.0] } })),
    )
    .await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.json);
    // 새로 넣는 사람은 여전히 멤버여야 한다.
    let added = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "version": 2, "title": "1회 모임",
        "fields": { "attendees": [admin.id.0, gone.id.0, outsider.id.0] } })),
    )
    .await;
    assert_eq!(added.status, StatusCode::BAD_REQUEST, "{:?}", added.json);
    // 리뷰: 출석에 남은 나간 사람을 다른 칸(진행자)에 새로 넣는 것은 새로 넣는 것이라 확인한다.
    let promoted = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "version": 2, "title": "1회 모임",
        "fields": { "attendees": [admin.id.0, gone.id.0], "presenter": gone.id.0 } })),
    )
    .await;
    assert_eq!(
        promoted.status,
        StatusCode::BAD_REQUEST,
        "{:?}",
        promoted.json
    );
    // 나간 사람을 빼는 것도 된다.
    let dropped = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "version": 2, "title": "1회 모임",
        "fields": { "attendees": [admin.id.0] } })),
    )
    .await;
    assert_eq!(dropped.status, StatusCode::OK, "{:?}", dropped.json);
}

/// 리뷰(#58 요약의 보안 우려): 이미 저장된 id는 다시 확인하지 않는데, 그 "저장된 값"을 판 확인 없이 읽었다. 아직
/// 커밋되지 않은 다음 판을 짐작해 보낸 고치기는 옛 판(나간 사람이 남은 출석)을 읽어 그 사람을 확인 없이 넘기고, 다음 판이
/// 커밋된 뒤 판이 맞아 저장했다. 읽은 판이 보낸 판과 다르면 바로 409다.
#[tokio::test]
async fn kept_ids_come_from_the_version_being_replaced() {
    use sqlx::Connection;
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "판 묶음").await;
    let (gone, _) = add_member(&app, &state, &admin, &ws, "gone", "member").await;
    let study = module(&app, &admin, &ws, "study", "KVR").await;
    let group = open_item(&app, &admin, &study, "읽기 모임").await;
    let created = send(
        &app,
        Method::POST,
        &format!("/api/items/{group}/records"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "meeting", "title": "1회",
                     "fields": { "attendees": [admin.id.0, gone.id.0] } }),
        ),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{:?}", created.json);
    let id = created.json["id"].as_str().unwrap().to_owned();
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", gone.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert!(removed.status.is_success(), "{:?}", removed.json);

    // 다른 쪽이 판 2(나간 사람을 뺀 출석)를 쓰는 중이다(아직 커밋하지 않음, 행 잠금을 쥠).
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let mut owner = sqlx::PgConnection::connect(&owner_url).await.unwrap();
    let mut held = owner.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE fvoci.item_records SET fields = jsonb_build_object('attendees', jsonb_build_array($2::text)),
                version = version + 1 WHERE id = $1::uuid",
    )
    .bind(&id)
    .bind(admin.id.0.to_string())
    .execute(&mut *held)
    .await
    .unwrap();

    // 판 2를 짐작해 나간 사람을 다시 넣는다.
    let request = tokio::spawn({
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/records/{id}");
        let body = json!({ "version": 2, "title": "1회",
                           "fields": { "attendees": [admin.id.0, gone.id.0] } });
        async move { send(&app, Method::PUT, &uri, Some(&admin), Some(body)).await }
    });
    // 그 고치기가 잠금에 막히거나(고치기 전) 바로 끝날 때(고친 뒤)까지 기다린다.
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut watcher = sqlx::PgConnection::connect(&admin_url).await.unwrap();
    for _ in 0..200 {
        let waiting: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid))",
        )
        .bind(holder)
        .fetch_one(&mut watcher)
        .await
        .unwrap();
        if waiting > 0 || request.is_finished() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    held.commit().await.unwrap();
    let r = request.await.unwrap();
    assert_eq!(r.status, StatusCode::CONFLICT, "{:?}", r.json);
    assert_eq!(r.json["message"], "the record changed; reload it");
    let attendees: Value = sqlx::query_scalar(
        "SELECT fields->'attendees' FROM fvoci.item_records WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&mut owner)
    .await
    .unwrap();
    assert_eq!(attendees, json!([admin.id.0.to_string()]));
}

/// 감사 S10: 기록 만들기도 같은 요청 id로 다시 보낸 요청이 앞 요청을 기다린 뒤 고유 제약(23505)으로 500이었다. 앞
/// 요청이 만든 기록 하나를 돌려준다.
#[tokio::test]
async fn a_retried_record_create_waiting_on_the_first_returns_that_record() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "기록 재시도").await;
    let project = module(&app, &admin, &ws, "project", "RRT").await;
    let it = open_item(&app, &admin, &project, "회고 모임").await;
    let request: uuid::Uuid = new_request_id().parse().unwrap();
    let item_id: uuid::Uuid = it.parse().unwrap();

    let mut t1 = state.db.begin_as(admin.id).await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(t1.conn())
        .await
        .unwrap();
    let first: uuid::Uuid = sqlx::query_scalar(
        "SELECT app_create_record($1, $2, 'retrospective', '1차 회고', NULL, '', '{}'::jsonb, '[]'::jsonb, $3)",
    )
    .bind(request)
    .bind(item_id)
    .bind(fvoci_db::new_ids(2))
    .fetch_one(t1.conn())
    .await
    .unwrap();
    let retry = tokio::spawn({
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/items/{it}/records");
        let body = json!({ "request_id": request, "kind": "retrospective", "title": "1차 회고" });
        async move { send(&app, Method::POST, &uri, Some(&admin), Some(body)).await }
    });
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let probe = sqlx::PgPool::connect(&url).await.unwrap();
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&probe)
        .await
        .unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    probe.close().await;
    assert!(blocked, "the retried request never waited on the first one");
    t1.commit().await.unwrap();
    let retry = retry.await.unwrap();
    assert_eq!(retry.status, StatusCode::CREATED, "{:?}", retry.json);
    assert_eq!(retry.json["id"], first.to_string());
    let (_, list) = records(&app, &admin, &it).await;
    assert_eq!(list.as_array().unwrap().len(), 1, "{list:?}");
}
