#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! Zotero 읽기 연동 (D-34, X-05): 대역 Zotero 서버로 서지·컬렉션 가져오기, 다시 가져오기는 서지만 바꾸고 내 칸은 그대로,
//! 키는 저장하지 않는다. 실제 Zotero 서비스 검증은 계정이 없어 NOT RUN.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use common::{add_member, config_with, create_workspace, login, new_request_id, send};
use serde_json::{Value, json};

const KEY: &str = "goodTestKey123";

/// 대역 Zotero(v3): 키가 맞아야 하고, 두 번째로 문헌을 읽으면 저자가 바뀐 판을 준다.
async fn fake_zotero() -> String {
    let reads = Arc::new(AtomicUsize::new(0));
    let check = |headers: &HeaderMap| {
        headers.get("Zotero-API-Key").and_then(|v| v.to_str().ok()) == Some(KEY)
    };
    let app = Router::new()
        .route(
            "/users/7/collections",
            get(move |headers: HeaderMap| async move {
                if !check(&headers) {
                    return StatusCode::FORBIDDEN.into_response();
                }
                Json(json!([{ "key": "COLL1", "data": { "name": "동기 연구" } }])).into_response()
            }),
        )
        .route(
            "/users/7/items/top",
            get(move |State(reads): State<Arc<AtomicUsize>>, headers: HeaderMap| async move {
                if !check(&headers) {
                    return StatusCode::FORBIDDEN.into_response();
                }
                let n = reads.fetch_add(1, Ordering::SeqCst);
                let author = if n == 0 { "Kim" } else { "Park" };
                Json(json!([
                    { "key": "ART1", "links": { "alternate": { "href": "https://www.zotero.org/users/7/items/ART1" } },
                      "data": { "itemType": "journalArticle", "title": "학습 동기", "date": "2021-03",
                                "creators": [{ "lastName": author, "firstName": "M" }],
                                "publicationTitle": "교육학 연구", "DOI": "10.1/x", "abstractNote": "원래 요약",
                                "tags": [{ "tag": "동기" }], "collections": ["COLL1"] } },
                    { "key": "NOTE1", "data": { "itemType": "note", "note": "메모" } }
                ]))
                .into_response()
            }),
        )
        .route(
            "/users/7/collections/{key}/items/top",
            get(|Path(key): Path<String>| async move {
                if key != "COLL1" {
                    return StatusCode::NOT_FOUND.into_response();
                }
                Json(json!([])).into_response()
            }),
        )
        .with_state(reads);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

#[tokio::test]
async fn zotero_references_import_and_refresh_without_storing_the_key() {
    let url = fake_zotero().await;
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_ZOTERO_API_URL", &url)])).unwrap();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "Zotero").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let module = |kind: &str, key: &str| {
        let app = app.clone();
        let admin = admin.clone();
        let ws = ws.clone();
        let body = json!({ "request_id": new_request_id(), "kind": kind, "name": key, "key": key });
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/workspaces/{ws}/modules"),
                Some(&admin),
                Some(body),
            )
            .await
            .json["id"]
                .as_str()
                .unwrap()
                .to_owned()
        }
    };
    let item = |module: String| {
        let app = app.clone();
        let admin = admin.clone();
        async move {
            let i = send(
                &app,
                Method::POST,
                &format!("/api/modules/{module}/items"),
                Some(&admin),
                Some(json!({ "request_id": new_request_id(), "title": "항목" })),
            )
            .await;
            let id = i.json["id"].as_str().unwrap().to_owned();
            send(
                &app,
                Method::PUT,
                &format!("/api/items/{id}"),
                Some(&admin),
                Some(json!({ "base_version": 1, "title": "항목", "description": "", "visibility": "workspace" })),
            )
            .await;
            id
        }
    };
    let research = item(module("research", "ZOT").await).await;
    let project = item(module("project", "PRJ").await).await;
    let body = |key: &str| json!({ "api_key": key, "library_type": "user", "library_id": "7" });

    // 컬렉션 목록. 틀린 키는 400(키는 응답에 없다).
    let cols = send(
        &app,
        Method::POST,
        "/api/zotero/collections",
        Some(&admin),
        Some(body(KEY)),
    )
    .await;
    assert_eq!(cols.json, json!([{ "key": "COLL1", "name": "동기 연구" }]));
    let wrong = send(
        &app,
        Method::POST,
        "/api/zotero/collections",
        Some(&admin),
        Some(body("wrongKey9")),
    )
    .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    assert!(!wrong.json.to_string().contains("wrongKey9"));

    let import = |user: &common::TestUser, item: &str| {
        let app = app.clone();
        let user = user.clone();
        let uri = format!("/api/items/{item}/zotero/import");
        async move { send(&app, Method::POST, &uri, Some(&user), Some(body(KEY))).await }
    };
    assert_eq!(
        import(&admin, &project).await.status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        import(&reader, &research).await.status,
        StatusCode::FORBIDDEN
    );

    let first = import(&admin, &research).await;
    assert_eq!(first.status, StatusCode::OK, "{:?}", first.json);
    assert_eq!(
        first.json,
        json!({ "created": 1, "updated": 0, "skipped": 1 })
    );
    let records = |user: &common::TestUser| {
        let app = app.clone();
        let user = user.clone();
        let uri = format!("/api/items/{research}/records");
        async move { send(&app, Method::GET, &uri, Some(&user), None).await.json }
    };
    let list = records(&admin).await;
    let rec: Value = list[0].clone();
    assert_eq!(rec["title"], "학습 동기");
    assert_eq!(rec["fields"]["authors"], "Kim, M");
    assert_eq!(rec["fields"]["year"], 2021.0);
    assert_eq!(rec["fields"]["zotero_collections"], json!(["동기 연구"]));
    assert_eq!(
        rec["fields"]["zotero_link"],
        "https://www.zotero.org/users/7/items/ART1"
    );

    // 내 칸을 고친 뒤 다시 가져오면 서지만 바뀐다.
    let id = rec["id"].as_str().unwrap();
    let mut fields = rec["fields"].clone();
    fields["read_status"] = json!("read");
    fields["summary"] = json!("내 요약");
    let edited = send(&app, Method::PUT, &format!("/api/records/{id}"), Some(&admin), Some(json!({
        "version": rec["version"], "title": "학습 동기", "occurred_on": null, "body": "메모", "fields": fields,
        "files": [] }))).await;
    assert_eq!(edited.status, StatusCode::OK, "{:?}", edited.json);
    let second = import(&admin, &research).await;
    assert_eq!(
        second.json,
        json!({ "created": 0, "updated": 1, "skipped": 1 })
    );
    let again = records(&admin).await;
    assert_eq!(again.as_array().unwrap().len(), 1);
    assert_eq!(again[0]["fields"]["authors"], "Park, M");
    assert_eq!(again[0]["fields"]["read_status"], "read");
    assert_eq!(again[0]["fields"]["summary"], "내 요약");
    assert_eq!(again[0]["body"], "메모");

    // 없는 컬렉션은 400. 키는 DB 어디에도 남지 않는다.
    let missing = send(&app, Method::POST, &format!("/api/items/{research}/zotero/import"), Some(&admin),
        Some(json!({ "api_key": KEY, "library_type": "user", "library_id": "7", "collection_key": "NOPE" }))).await;
    assert_eq!(missing.status, StatusCode::BAD_REQUEST);
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let stored: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM fvoci.item_records WHERE fields::text LIKE '%' || $1 || '%')
              + (SELECT count(*) FROM fvoci.audit_log WHERE detail::text LIKE '%' || $1 || '%')",
    )
    .bind(KEY)
    .fetch_one(&owner)
    .await
    .unwrap();
    owner.close().await;
    assert_eq!(stored, 0);
}

/// #83: 재가져오기가 읽은 판 뒤에 다른 사람이 저장하면 SQL 판 검사에서 전체 요청을 거절한다.
#[tokio::test]
async fn zotero_refresh_rejects_a_concurrent_edit_without_overwriting_it() {
    use sqlx::Connection;
    use std::time::Duration;

    let url = fake_zotero().await;
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_ZOTERO_API_URL", &url)])).unwrap();
    let app = common::app(&state);
    let admin = login(&state, "importer").await;
    let ws = create_workspace(&app, &admin, "Zotero 충돌").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "member").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "research", "name": "문헌", "key": "ZCF" })),
    ).await;
    assert_eq!(module.status, StatusCode::CREATED, "{}", module.json);
    let module = module.json["id"].as_str().unwrap();
    let created = send(
        &app,
        Method::POST,
        &format!("/api/modules/{module}/items"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "문헌 충돌" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.json);
    let item = created.json["id"].as_str().unwrap();
    let opened = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item}"),
        Some(&admin),
        Some(json!({ "base_version": 1, "title": "문헌 충돌", "description": "", "visibility": "workspace" })),
    ).await;
    assert_eq!(opened.status, StatusCode::OK, "{}", opened.json);
    let import_uri = format!("/api/items/{item}/zotero/import");
    let import_body = json!({ "api_key": KEY, "library_type": "user", "library_id": "7" });
    let imported = send(
        &app,
        Method::POST,
        &import_uri,
        Some(&admin),
        Some(import_body.clone()),
    )
    .await;
    assert_eq!(imported.status, StatusCode::OK, "{}", imported.json);
    assert_eq!(
        imported.json,
        json!({ "created": 1, "updated": 0, "skipped": 1 })
    );
    let records_uri = format!("/api/items/{item}/records");
    let before = send(&app, Method::GET, &records_uri, Some(&writer), None).await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.json);
    let record = &before.json[0];
    let id: uuid::Uuid = record["id"].as_str().unwrap().parse().unwrap();
    let version = i32::try_from(record["version"].as_i64().unwrap()).unwrap();
    assert_eq!(record["fields"]["authors"], "Kim, M");

    // 앱 역할에는 표 UPDATE 권한이 없어 FOR UPDATE는 표 소유 역할 연결로 잡는다. 셀프호스트 Supabase의 시험 관리자는
    // superuser가 아니라 item_records를 읽지 못하고 fvoci_app으로 SET ROLE도 못 하므로, 먼저 저장하는 "다른 사용자"의
    // 변경은 같은 트랜잭션에서 app.user_id만 바꿔 정의자 함수 app_update_record로 한다(정의자 함수는 호출 역할과
    // 무관하게 소유자로 실행되므로 앱 역할 호출과 같은 검사·결과다). 재가져오기 요청 자체는 실제 앱 역할로 서버를 거친다.
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let mut lock_conn = sqlx::PgConnection::connect(&owner_url).await.unwrap();
    let mut held = lock_conn.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *held)
        .await
        .unwrap();
    let locked: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM fvoci.item_records WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_one(&mut *held)
            .await
            .unwrap();
    assert_eq!(locked, id);

    let request = tokio::spawn({
        let app = app.clone();
        let admin = admin.clone();
        async move {
            send(
                &app,
                Method::POST,
                &import_uri,
                Some(&admin),
                Some(import_body),
            )
            .await
        }
    });
    let mut watcher = sqlx::PgConnection::connect(&admin_url).await.unwrap();
    // 고정 지연으로 경쟁을 추측하지 않는다. app_update_record 호출이 이 연결의 행 잠금에서
    // 대기한다는 증거가 있어야 진행한다(이 호출 전 서버가 기존 fields와 version을 읽는다).
    let waiting = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
                 WHERE $1 = ANY(pg_blocking_pids(pid))
                   AND query LIKE 'SELECT app_update_record(%')",
            )
            .bind(holder)
            .fetch_one(&mut watcher)
            .await
            .unwrap();
            if blocked {
                return true;
            }
            if request.is_finished() {
                return false;
            }
            tokio::task::yield_now().await;
        }
    })
    .await;
    if !matches!(waiting, Ok(true)) {
        request.abort();
    }
    assert!(
        matches!(waiting, Ok(true)),
        "재가져오기가 app_update_record에서 잠금을 기다려야 한다"
    );

    sqlx::query("SET LOCAL search_path TO fvoci, pg_temp")
        .execute(&mut *held)
        .await
        .unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(writer.id.0.to_string())
        .execute(&mut *held)
        .await
        .unwrap();
    let acting: uuid::Uuid = sqlx::query_scalar("SELECT app_user_id()")
        .fetch_one(&mut *held)
        .await
        .unwrap();
    assert_eq!(acting, writer.id.0, "the first save must act as the writer");
    let mut latest_fields = record["fields"].clone();
    latest_fields["summary"] = json!("다른 사용자의 최신 요약");
    let latest_version: i32 =
        sqlx::query_scalar("SELECT app_update_record($1, $2, $3, NULL, $4, $5, '[]'::jsonb)")
            .bind(id)
            .bind(version)
            .bind("다른 사용자의 제목")
            .bind("다른 사용자의 메모")
            .bind(&latest_fields)
            .fetch_one(&mut *held)
            .await
            .unwrap();
    assert_eq!(latest_version, version + 1);
    held.commit().await.unwrap();

    let refreshed = tokio::time::timeout(Duration::from_secs(10), request)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refreshed.status, StatusCode::CONFLICT, "{}", refreshed.json);
    assert_eq!(refreshed.json["error"], "invalid_state");
    assert_eq!(refreshed.json["message"], "the record changed; reload it");
    let after = send(&app, Method::GET, &records_uri, Some(&writer), None).await;
    assert_eq!(after.status, StatusCode::OK, "{}", after.json);
    assert_eq!(after.json.as_array().unwrap().len(), 1);
    let saved = &after.json[0];
    assert_eq!(saved["id"], record["id"]);
    assert_eq!(saved["version"], latest_version);
    assert_eq!(saved["title"], "다른 사용자의 제목");
    assert_eq!(saved["body"], "다른 사용자의 메모");
    // 재가져오기 대역 응답의 Park, M을 포함해 어느 칸도 부분 저장되지 않는다.
    assert_eq!(saved["fields"], latest_fields);
    assert_eq!(saved["fields"]["authors"], "Kim, M");
}
