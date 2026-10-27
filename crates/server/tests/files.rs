#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 파일 첨부·버전·업로드 (D-06, D-35, D-52): 조각 업로드와 이어 올리기, 무결성, 권한, 버전, 한도, 휴지통·영구 삭제.

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, config_with, create_doc, create_workspace, fixture_sql, login,
    new_request_id, send, send_raw,
};
use fvoci_server::http::AppState;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const MIB: usize = 1024 * 1024;

fn state_with(overrides: &[(&str, &str)]) -> AppState {
    let mut all = vec![("FVOCI_UPLOAD_CHUNK_BYTES", "5242880")];
    all.extend_from_slice(overrides);
    fvoci_server::app::build_state(config_with(&all)).unwrap()
}

fn s3_overrides() -> Vec<(&'static str, String)> {
    let env = |k: &str| std::env::var(k).expect("run via `cargo xtask test`");
    vec![
        ("FVOCI_STORAGE", "s3".to_owned()),
        ("FVOCI_S3_ENDPOINT", env("FVOCI_TEST_S3_ENDPOINT")),
        ("FVOCI_S3_REGION", "us-east-1".to_owned()),
        ("FVOCI_S3_BUCKET", env("FVOCI_TEST_S3_BUCKET")),
        ("FVOCI_S3_ACCESS_KEY", env("FVOCI_TEST_S3_ACCESS_KEY")),
        ("FVOCI_S3_SECRET_KEY", env("FVOCI_TEST_S3_SECRET_KEY")),
    ]
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// 테이블 소유 역할로 연결한다(`database`가 있으면 그 DB). 지울 목록·버전 키를 직접 본다.
async fn owner(database: Option<&str>) -> sqlx::PgConnection {
    use sqlx::Connection as _;
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let url = database.map_or(url.clone(), |db| common::with_database(&url, db));
    sqlx::PgConnection::connect(&url).await.unwrap()
}

/// 파일의 버전들이 저장소에 쓰는 키.
async fn version_keys(owner: &mut sqlx::PgConnection, file_id: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT storage_key FROM fvoci.file_versions WHERE file_id = $1::uuid ORDER BY version",
    )
    .bind(file_id)
    .fetch_all(owner)
    .await
    .unwrap()
}

/// 지울 목록에 오른 이 객체들이 저장소에서 실제로 지워질 때까지 정리를 돌린다. 지울 목록은 DB 전체가 함께 써서 같은 DB의
/// 다른 시험의 정리가 먼저 가져갈 수 있으므로, 정리한 수가 아니라 이 키가 목록과 저장소에서 사라졌는지로 확인한다.
/// 다른 정리가 꺼낸 행은 그 트랜잭션이 끝나야(저장소에서 지운 뒤) 목록에서 사라진다.
async fn clean_until_gone(state: &AppState, owner: &mut sqlx::PgConnection, keys: &[String]) {
    assert!(!keys.is_empty());
    let mut queued = i64::MAX;
    for _ in 0..200 {
        fvoci_server::worker::run_all_now(&state.db, None, Some(&state.storage))
            .await
            .unwrap();
        queued = sqlx::query_scalar(
            "SELECT count(*) FROM fvoci.storage_deletions WHERE storage_key = ANY($1)",
        )
        .bind(keys)
        .fetch_one(&mut *owner)
        .await
        .unwrap();
        if queued == 0 {
            break;
        }
    }
    assert_eq!(queued, 0, "still queued: {keys:?}");
    for key in keys {
        assert!(
            state.storage.digest(key).await.is_err(),
            "{key} is still in storage"
        );
    }
}

/// 결정적인 내용(5 MiB 조각 둘: 앞은 꽉, 뒤는 작게).
fn content(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i % 251) as u8 ^ seed).collect()
}

async fn start(app: &Router, user: &TestUser, body: Value) -> (StatusCode, Value) {
    let r = send(app, Method::POST, "/api/uploads", Some(user), Some(body)).await;
    (r.status, r.json)
}

async fn part(
    app: &Router,
    user: &TestUser,
    upload: &str,
    n: usize,
    bytes: &[u8],
    hash: &str,
) -> StatusCode {
    send_raw(
        app,
        Method::PUT,
        &format!("/api/uploads/{upload}/parts/{n}"),
        Some(user),
        &[
            ("x-content-sha256", hash),
            ("content-type", "application/octet-stream"),
        ],
        bytes.to_vec(),
    )
    .await
    .0
}

/// 조각으로 나눠 모두 올리고 마친다.
async fn upload_all(app: &Router, user: &TestUser, body: Value, data: &[u8]) -> Value {
    let (status, up) = start(app, user, body).await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap();
    let chunk = usize::try_from(up["chunk_bytes"].as_i64().unwrap()).unwrap();
    for (i, piece) in data.chunks(chunk).enumerate() {
        assert_eq!(
            part(app, user, id, i + 1, piece, &sha(piece)).await,
            StatusCode::NO_CONTENT
        );
    }
    let done = send(
        app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(user),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(done.json["state"], "done");
    done.json
}

async fn download(
    app: &Router,
    user: &TestUser,
    uri: &str,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    send_raw(app, Method::GET, uri, Some(user), &[], Vec::new()).await
}

#[tokio::test]
async fn chunked_upload_resumes_checks_integrity_and_keeps_versions() {
    let state = state_with(&[]);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "파일").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &writer, &ws, "자료").await;
    let doc_id = doc["id"].as_str().unwrap();
    let data = content(5 * MIB + 10, 7);
    let request = new_request_id();
    let body = json!({ "request_id": request, "target": { "kind": "document", "id": doc_id },
                       "name": "data.csv", "content_type": "text/csv", "size": data.len() });

    // 읽기만 하는 사람은 올릴 수 없고, 볼 수 없는 사람에게는 없는 것과 같다.
    assert_eq!(
        start(&app, &reader, body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        start(&app, &outsider, body.clone()).await.0,
        StatusCode::NOT_FOUND
    );
    let (status, up) = start(&app, &writer, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    assert_eq!(up["parts_total"], 2);
    let id = up["id"].as_str().unwrap().to_owned();
    // 같은 요청 id로 다시 시작하면 같은 업로드.
    assert_eq!(
        start(&app, &writer, body.clone()).await.1["id"],
        id.as_str()
    );

    // 뒤 조각부터 보내고(끊겼다가 이어 올리기), 받은 조각을 확인한다.
    let (first, second) = data.split_at(5 * MIB);
    assert_eq!(
        part(&app, &writer, &id, 2, second, &sha(second)).await,
        StatusCode::NO_CONTENT
    );
    let seen = send(
        &app,
        Method::GET,
        &format!("/api/uploads/{id}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(seen.json["received"], json!([2]));
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/uploads/{id}"),
            Some(&reader),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    // 마치기 전에 조각이 모자라면 409. 해시·크기가 틀린 조각은 400.
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/uploads/{id}/complete"),
            Some(&writer),
            None
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        part(&app, &writer, &id, 1, first, &sha(second)).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        part(&app, &writer, &id, 1, second, &sha(second)).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        part(&app, &writer, &id, 3, second, &sha(second)).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        part(&app, &writer, &id, 1, first, &sha(first)).await,
        StatusCode::NO_CONTENT
    );
    let done = send(
        &app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(done.json["version"], 1);
    let file_id = done.json["file_id"].as_str().unwrap().to_owned();
    // 마치기를 다시 보내도 같은 결과(버전이 늘지 않음).
    let again = send(
        &app,
        Method::POST,
        &format!("/api/uploads/{id}/complete"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(again.json["file_id"], file_id.as_str());
    assert_eq!(again.json["version"], 1);

    // 문서를 볼 수 있는 사람은 목록과 내용을 본다. 쓰기는 쓸 수 있는 사람만.
    let listed = send(
        &app,
        Method::GET,
        &format!("/api/documents/{doc_id}/files"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(listed.json[0]["name"], "data.csv");
    assert_eq!(listed.json[0]["can_write"], false);
    assert_eq!(listed.json[0]["size"], data.len());
    // D-115: 수정 권한도 소유 권한이 아니면 allow_copy가 있어야 내려받는다.
    assert_eq!(listed.json[0]["can_download"], false);
    assert_eq!(
        download(&app, &reader, &format!("/api/files/{file_id}/content"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        download(
            &app,
            &reader,
            &format!("/api/files/{file_id}/content?inline=true")
        )
        .await
        .0,
        StatusCode::FORBIDDEN,
        "CSV cannot be viewed inline, so it is a download"
    );
    assert_eq!(
        download(&app, &writer, &format!("/api/files/{file_id}/content"))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let allowed = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc_id}/settings"),
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": true })),
    )
    .await;
    assert_eq!(allowed.status, StatusCode::OK, "{:?}", allowed.json);
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/documents/{doc_id}/files"),
            Some(&outsider),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let (status, headers, bytes) =
        download(&app, &reader, &format!("/api/files/{file_id}/content")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, data);
    assert_eq!(headers["content-type"], "text/csv");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    // 브라우저 안에서 열기는 안전한 형식만(CSV는 늘 내려받기).
    let (_, headers, _) = download(
        &app,
        &reader,
        &format!("/api/files/{file_id}/content?inline=true"),
    )
    .await;
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment;")
    );
    assert_eq!(
        download(&app, &outsider, &format!("/api/files/{file_id}/content"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );

    // 새 버전: 이전 버전은 남고 따로 받을 수 있다.
    let v2 = b"a,b\n1,2\n".to_vec();
    assert_eq!(
        start(
            &app,
            &reader,
            json!({ "request_id": new_request_id(), "file_id": file_id, "size": v2.len() })
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let second_version = upload_all(
        &app,
        &writer,
        json!({ "request_id": new_request_id(), "file_id": file_id, "note": "고친 값", "size": v2.len(),
                "content_type": "text/plain" }),
        &v2,
    )
    .await;
    assert_eq!(second_version["version"], 2);
    let detail = send(
        &app,
        Method::GET,
        &format!("/api/files/{file_id}"),
        Some(&reader),
        None,
    )
    .await;
    assert_eq!(detail.json["file"]["current_version"], 2);
    assert_eq!(detail.json["file"]["name"], "data.csv");
    assert_eq!(detail.json["versions"][0]["note"], "고친 값");
    assert_eq!(detail.json["versions"][1]["sha256"], sha(&data));
    let (_, headers, latest) = download(
        &app,
        &reader,
        &format!("/api/files/{file_id}/content?inline=true"),
    )
    .await;
    assert_eq!(latest, v2);
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline;")
    );
    assert_eq!(headers["content-type"], "text/plain; charset=utf-8");
    let (_, _, old) = download(
        &app,
        &reader,
        &format!("/api/files/{file_id}/content?version=1"),
    )
    .await;
    assert_eq!(old, data);

    // 이름 바꾸기·지우기는 쓸 수 있는 사람만. 지우면 목록·내용에서 빠지고 되살리면 돌아온다.
    let rename = |user: TestUser, name: &'static str| {
        let app = app.clone();
        let uri = format!("/api/files/{file_id}");
        async move {
            send(
                &app,
                Method::PUT,
                &uri,
                Some(&user),
                Some(json!({ "name": name })),
            )
            .await
            .status
        }
    };
    assert_eq!(rename(reader.clone(), "x.csv").await, StatusCode::FORBIDDEN);
    assert_eq!(
        rename(writer.clone(), "../x").await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(rename(writer.clone(), "자료.csv").await, StatusCode::OK);
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/files/{file_id}"),
            Some(&reader),
            None
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(trashed.status, StatusCode::OK, "{:?}", trashed.json);
    let batch = trashed.json["batch_id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/documents/{doc_id}/files"),
            Some(&reader),
            None
        )
        .await
        .json,
        json!([])
    );
    assert_eq!(
        download(&app, &reader, &format!("/api/files/{file_id}/content"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    let trash = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/trash"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(trash.json["entries"][0]["kind"], "file");
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("/api/trash/{batch}/restore"),
            Some(&writer),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        download(&app, &reader, &format!("/api/files/{file_id}/content"))
            .await
            .2,
        v2
    );

    // 영구 삭제하면 작업 프로세스가 저장소의 모든 버전을 지운다.
    let mut db = owner(None).await;
    let keys = version_keys(&mut db, &file_id).await;
    assert_eq!(keys.len(), 2);
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}"),
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
    clean_until_gone(&state, &mut db, &keys).await;
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/files/{file_id}"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn limits_abort_expiry_and_private_notes() {
    let state = state_with(&[
        ("FVOCI_MAX_FILE_BYTES", "6000000"),
        ("FVOCI_MAX_WORKSPACE_STORAGE_BYTES", "7000000"),
    ]);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "한도").await;
    let doc = create_doc(&app, &admin, &ws, "자료").await;
    let target = json!({ "kind": "document", "id": doc["id"] });
    let new = |size: usize| json!({ "request_id": new_request_id(), "target": target, "name": "a.bin", "size": size });

    // 파일 하나의 한도와 빈 파일.
    assert_eq!(
        start(&app, &admin, new(6_000_001)).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(start(&app, &admin, new(0)).await.0, StatusCode::BAD_REQUEST);
    // 진행 중인 업로드도 저장량에 든다.
    let (status, big) = start(&app, &admin, new(5_000_000)).await;
    assert_eq!(status, StatusCode::CREATED, "{big:?}");
    let (status, over) = start(&app, &admin, new(2_500_000)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{over:?}");
    // 그만두면 자리가 생긴다. 그만둔 업로드에는 더 보낼 수 없다.
    let big_id = big["id"].as_str().unwrap();
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/uploads/{big_id}"),
            Some(&admin),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let piece = content(5 * MIB, 1);
    assert_eq!(
        part(&app, &admin, big_id, 1, &piece, &sha(&piece)).await,
        StatusCode::CONFLICT
    );
    let (status, small) = start(&app, &admin, new(2_500_000)).await;
    assert_eq!(status, StatusCode::CREATED, "{small:?}");

    // 기한이 지난 업로드는 작업 프로세스가 닫는다.
    let small_id = small["id"].as_str().unwrap().to_owned();
    fixture_sql(
        "UPDATE uploads SET expires_at = now() - interval '1 minute' WHERE id = $1::uuid",
        &small_id,
    )
    .await;
    fvoci_server::worker::run_all_now(&state.db, None, Some(&state.storage))
        .await
        .unwrap();
    let seen = send(
        &app,
        Method::GET,
        &format!("/api/uploads/{small_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(seen.json["state"], "expired");

    // 개인 노트의 첨부는 소유자만(워크스페이스 관리자도 아님).
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "내 노트" })),
    )
    .await
    .json;
    let other = login(&state, "other").await;
    let note_target = json!({ "kind": "document", "id": note["id"] });
    let body = json!({ "request_id": new_request_id(), "target": note_target, "name": "비밀.txt", "size": 3,
                       "content_type": "text/plain" });
    assert_eq!(
        start(&app, &other, body.clone()).await.0,
        StatusCode::NOT_FOUND
    );
    let done = upload_all(&app, &admin, body, b"abc").await;
    let file_id = done["file_id"].as_str().unwrap();
    assert_eq!(
        download(&app, &other, &format!("/api/files/{file_id}/content"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        download(&app, &admin, &format!("/api/files/{file_id}/content"))
            .await
            .2,
        b"abc"
    );
}

#[tokio::test]
async fn concurrent_starts_and_old_version_cleanup() {
    let state = state_with(&[]);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "버전 정리").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let doc = create_doc(&app, &admin, &ws, "자료").await;
    let body = json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                       "name": "v.txt", "size": 3, "content_type": "text/plain" });
    // 같은 요청이 동시에 와도 업로드는 하나.
    let (a, b) = tokio::join!(
        start(&app, &admin, body.clone()),
        start(&app, &admin, body.clone())
    );
    assert_eq!(a.0, StatusCode::CREATED, "{:?}", a.1);
    assert_eq!(b.0, StatusCode::CREATED, "{:?}", b.1);
    assert_eq!(a.1["id"], b.1["id"]);
    let first = upload_all(&app, &admin, body, b"one").await;
    let file_id = first["file_id"].as_str().unwrap().to_owned();
    upload_all(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "file_id": file_id, "size": 3 }),
        b"two",
    )
    .await;
    let version = |n: i32| format!("/api/files/{file_id}/versions/{n}");
    assert_eq!(
        send(&app, Method::DELETE, &version(1), Some(&reader), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::DELETE, &version(2), Some(&admin), None)
            .await
            .status,
        StatusCode::CONFLICT
    );
    assert_eq!(
        send(&app, Method::DELETE, &version(9), Some(&admin), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    let mut db = owner(None).await;
    let old_version = version_keys(&mut db, &file_id).await[..1].to_vec();
    assert_eq!(
        send(&app, Method::DELETE, &version(1), Some(&admin), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        download(
            &app,
            &admin,
            &format!("/api/files/{file_id}/content?version=1")
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        download(&app, &admin, &format!("/api/files/{file_id}/content"))
            .await
            .2,
        b"two"
    );
    clean_until_gone(&state, &mut db, &old_version).await;
}

#[tokio::test]
async fn s3_files_round_trip_and_purge_with_the_document() {
    // 지울 목록은 DB 전체가 함께 쓴다. 이 시험의 정리가 같은 DB의 로컬 저장소 시험 키를 S3에서 "지우고"(없는 키도 성공)
    // 목록에서 빼지 않게 새 DB를 쓴다(운영은 저장소가 하나다).
    use sqlx::Connection as _;
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin_db = sqlx::PgConnection::connect(&admin_url).await.unwrap();
    let fresh = common::fresh_database(&mut admin_db, "s3_files").await;
    let mut overrides = s3_overrides();
    overrides.push(("DATABASE_URL", fresh.app_url.clone()));
    let pairs: Vec<(&str, &str)> = overrides.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let state = state_with(&pairs);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    // D-111·D-112: 별도 S3 시험 DB의 사용자에게 기본 시험 DB의 조직을 섞지 않는다.
    let mut fixture =
        sqlx::PgConnection::connect(fresh.db.migration_url.as_ref().unwrap().expose())
            .await
            .unwrap();
    sqlx::query("INSERT INTO fvoci.server_admins (user_id) VALUES ($1)")
        .bind(admin.id.0)
        .execute(&mut fixture)
        .await
        .unwrap();
    let mut tx = fixture.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1::text, true)")
        .bind(admin.id.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    let organization: uuid::Uuid =
        sqlx::query_scalar("SELECT fvoci.app_create_organization($1, $2, $3, $4)")
            .bind(uuid::Uuid::now_v7())
            .bind("S3 조직")
            .bind(admin.id.0)
            .bind((0..3).map(|_| uuid::Uuid::now_v7()).collect::<Vec<_>>())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    fixture.close().await.unwrap();
    let workspace = send(&app, Method::POST, "/api/workspaces", Some(&admin),
        Some(json!({ "request_id": new_request_id(), "organization_id": organization, "name": "S3 파일" }))).await;
    assert_eq!(
        workspace.status,
        StatusCode::CREATED,
        "{:?}",
        workspace.json
    );
    let ws = workspace.json["id"].as_str().unwrap().to_owned();
    let doc = create_doc(&app, &admin, &ws, "자료").await;
    let data = content(5 * MIB + 3, 9);
    let done = upload_all(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "그림.png", "content_type": "image/png", "size": data.len() }),
        &data,
    )
    .await;
    let file_id = done["file_id"].as_str().unwrap().to_owned();
    let (_, headers, bytes) = download(
        &app,
        &admin,
        &format!("/api/files/{file_id}/content?inline=true"),
    )
    .await;
    assert_eq!(bytes, data);
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("inline;")
    );

    // 문서를 휴지통에 넣고 영구 삭제하면 붙은 파일과 저장소의 내용도 지워진다.
    let mut db = owner(Some(&fresh.name)).await;
    let keys = version_keys(&mut db, &file_id).await;
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    let batch = r.json["batch_id"].as_str().unwrap().to_owned();
    assert_eq!(
        download(&app, &admin, &format!("/api/files/{file_id}/content"))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
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
    clean_until_gone(&state, &mut db, &keys).await;
}

/// 감사 반영: 문서를 다른 워크스페이스로 옮기면 첨부도 함께 옮겨진다. 옛 워크스페이스 관리자는 그 파일의 휴지통
/// 묶음을 보거나 되살리거나 영구 삭제할 수 없고, 휴지통·저장 용량은 새 워크스페이스 몫이다.
#[tokio::test]
async fn moving_a_document_takes_its_attachments_along() {
    let state = state_with(&[]);
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let src = create_workspace(&app, &alice, "첨부 출발").await;
    let dst = create_workspace(&app, &alice, "첨부 도착").await;
    let (src_admin, _) = add_member(&app, &state, &alice, &src, "src-admin", "admin").await;
    let (dst_writer, _) = add_member(&app, &state, &alice, &dst, "dst-writer", "member").await;
    let doc = create_doc(&app, &alice, &src, "첨부 문서").await;
    let doc_id = doc["id"].as_str().unwrap().to_owned();
    let body = json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc_id },
                       "name": "자료.txt", "size": 3, "content_type": "text/plain" });
    let done = upload_all(&app, &alice, body, b"abc").await;
    let file_id = done["file_id"].as_str().unwrap().to_owned();
    // 옮기기 전에 휴지통에 넣은 첨부의 묶음도 함께 옮겨진다.
    let body = json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc_id },
                       "name": "먼저 지움.txt", "size": 3, "content_type": "text/plain" });
    let early = upload_all(&app, &alice, body, b"xyz").await;
    let early_trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{}", early["file_id"].as_str().unwrap()),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(
        early_trashed.status,
        StatusCode::OK,
        "{:?}",
        early_trashed.json
    );
    let early_batch = early_trashed.json["batch_id"].as_str().unwrap().to_owned();

    let uri = format!("/api/documents/{doc_id}/move");
    let preview = send(
        &app,
        Method::POST,
        &uri,
        Some(&alice),
        Some(json!({ "target_workspace_id": dst })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::CONFLICT, "{:?}", preview.json);
    // D-114: 이름 배열 대신 사용자별 등급 변화를 확인한다.
    let ack = json!({ "changes": preview.json["changes"] });
    let moved = send(
        &app,
        Method::POST,
        &uri,
        Some(&alice),
        Some(json!({ "target_workspace_id": dst, "acknowledged": ack })),
    )
    .await;
    assert_eq!(moved.status, StatusCode::OK, "{:?}", moved.json);

    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let owner = sqlx::PgPool::connect(&owner_url).await.unwrap();
    let place: (Option<uuid::Uuid>, Option<uuid::Uuid>) =
        sqlx::query_as("SELECT workspace_id, owner_user_id FROM fvoci.files WHERE id = $1::uuid")
            .bind(&file_id)
            .fetch_one(&owner)
            .await
            .unwrap();
    assert_eq!(place.0.map(|w| w.to_string()), Some(dst.clone()));
    assert_eq!(place.1, None);

    // 새 워크스페이스 작성자가 휴지통에 넣으면 그 묶음은 새 워크스페이스 관리자에게만 보인다.
    let trashed = send(
        &app,
        Method::DELETE,
        &format!("/api/files/{file_id}"),
        Some(&dst_writer),
        None,
    )
    .await;
    assert_eq!(trashed.status, StatusCode::OK, "{:?}", trashed.json);
    let batch = trashed.json["batch_id"].as_str().unwrap().to_owned();
    let listed = |user: TestUser, ws: String, batch: String| {
        let app = app.clone();
        async move {
            send(
                &app,
                Method::GET,
                &format!("/api/workspaces/{ws}/trash"),
                Some(&user),
                None,
            )
            .await
            .json["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == batch.as_str())
        }
    };
    for batch in [batch, early_batch] {
        assert!(!listed(src_admin.clone(), src.clone(), batch.clone()).await);
        assert!(listed(alice.clone(), dst.clone(), batch.clone()).await);
        let restore = send(
            &app,
            Method::POST,
            &format!("/api/trash/{batch}/restore"),
            Some(&src_admin),
            None,
        )
        .await;
        assert_eq!(restore.status, StatusCode::NOT_FOUND, "{:?}", restore.json);
        let purge = send(
            &app,
            Method::DELETE,
            &format!("/api/trash/{batch}"),
            Some(&src_admin),
            None,
        )
        .await;
        assert_eq!(purge.status, StatusCode::NOT_FOUND, "{:?}", purge.json);
    }
    owner.close().await;
}

/// 감사 S11: 업로드 마무리 중 일시적 오류(여기서는 파일 버전을 기록할 때의 교착)가 나면 업로드가 'completing'에 남아,
/// 다시 마치기가 5분 동안 "마치는 중"(409)이었다. 웹은 약 60초만 다시 묻고 실패로 알렸다. 멈춘 마무리의 점유를 바로
/// 풀어 다음 요청이 이어서 마친다.
#[tokio::test]
async fn a_transient_failure_while_finishing_lets_the_next_complete_go_through() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "마무리 재시도").await;
    let doc = create_doc(&app, &admin, &ws, "첨부 문서").await;
    // 다른 시험의 업로드와 겹치지 않는 크기.
    let data = vec![7_u8; 3217];
    let (status, up) = start(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "자료.bin", "size": data.len(), "content_type": "application/octet-stream" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap().to_owned();
    assert_eq!(
        part(&app, &admin, &id, 1, &data, &sha(&data)).await,
        StatusCode::NO_CONTENT
    );

    // 이 크기의 파일 버전을 기록하는 순간 교착(40P01)으로 실패하게 한다.
    let mut o = owner(None).await;
    let name = format!("test_deadlock_{}", uuid::Uuid::now_v7().simple());
    sqlx::query(&format!(
        "CREATE FUNCTION fvoci.{name}() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN IF NEW.size = 3217 THEN RAISE EXCEPTION 'test deadlock' USING ERRCODE = '40P01'; END IF;
         RETURN NEW; END $$"
    ))
    .execute(&mut o)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE TRIGGER {name} BEFORE INSERT ON fvoci.file_versions FOR EACH ROW EXECUTE FUNCTION fvoci.{name}()"
    ))
    .execute(&mut o)
    .await
    .unwrap();
    let uri = format!("/api/uploads/{id}/complete");
    let first = send(&app, Method::POST, &uri, Some(&admin), None).await;
    for cleanup in [
        format!("DROP TRIGGER {name} ON fvoci.file_versions"),
        format!("DROP FUNCTION fvoci.{name}()"),
    ] {
        sqlx::query(&cleanup).execute(&mut o).await.unwrap();
    }
    assert_eq!(first.status, StatusCode::CONFLICT, "{:?}", first.json);

    // 바로 다시 마치면 이어서 끝난다.
    let again = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);
    assert_eq!(again.json["state"], "done");
}

/// 사용자로서 마치기를 시작하고(점유) 그 시도의 점유 시각을 돌려준다.
async fn begin_complete(
    state: &AppState,
    user: &TestUser,
    upload: uuid::Uuid,
) -> time::OffsetDateTime {
    let mut tx = state.db.begin_as(user.id).await.unwrap();
    let claim: time::OffsetDateTime =
        sqlx::query_scalar("SELECT state_changed_at FROM app_begin_complete($1)")
            .bind(upload)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    tx.commit().await.unwrap();
    claim
}

/// 리뷰(감사 S11 후속): 오래 걸린 앞 마치기가 5분 뒤 다른 요청에 넘겨진 다음 늦게 실패해 점유를 풀면, 넘겨받은 요청의
/// 점유까지 풀려 세 번째 마치기가 동시에 저장소 작업을 시작할 수 있었다(한쪽이 조각을 지우고 업로드를 되돌릴 수 있음).
/// 점유는 시도마다 구별하고, 늦게 돌아온 시도는 넘겨받은 점유를 건드리지 않는다.
#[tokio::test]
async fn a_late_attempt_leaves_a_claim_taken_over_by_another_alone() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "넘겨받은 마무리").await;
    let doc = create_doc(&app, &admin, &ws, "첨부 문서").await;
    let data = vec![9_u8; 3229];
    let (status, up) = start(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "자료.bin", "size": data.len(), "content_type": "application/octet-stream" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap().to_owned();
    assert_eq!(
        part(&app, &admin, &id, 1, &data, &sha(&data)).await,
        StatusCode::NO_CONTENT
    );
    let upload: uuid::Uuid = id.parse().unwrap();

    // 첫 시도가 점유한 뒤 오래 걸려, 5분이 지나 다른 요청이 넘겨받는다.
    let first = begin_complete(&state, &admin, upload).await;
    let mut o = owner(None).await;
    sqlx::query(
        "UPDATE fvoci.uploads SET state_changed_at = state_changed_at - interval '6 minutes' WHERE id = $1",
    )
    .bind(upload)
    .execute(&mut o)
    .await
    .unwrap();
    let second = begin_complete(&state, &admin, upload).await;
    assert_ne!(first, second);

    // 첫 시도가 늦게 실패해 점유를 풀려 해도 넘겨받은 점유는 그대로다.
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    sqlx::query("SELECT app_release_upload_claim($1, $2)")
        .bind(upload)
        .bind(first)
        .execute(tx.conn())
        .await
        .unwrap();
    let held = |claim: time::OffsetDateTime| {
        sqlx::query_scalar::<_, bool>("SELECT app_upload_claim_held($1, $2)")
            .bind(upload)
            .bind(claim)
    };
    let first_holds = held(first).fetch_one(tx.conn()).await.unwrap();
    let second_holds = held(second).fetch_one(tx.conn()).await.unwrap();
    tx.commit().await.unwrap();
    assert!(!first_holds, "the late attempt must not hold the claim");
    assert!(second_holds, "the take-over keeps its claim");
    let uri = format!("/api/uploads/{id}/complete");
    let third = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(third.status, StatusCode::CONFLICT, "{:?}", third.json);

    // 넘겨받은 시도가 멈춰 스스로 풀면 다음 마치기가 이어서 끝낸다.
    let mut tx = state.db.begin_as(admin.id).await.unwrap();
    sqlx::query("SELECT app_release_upload_claim($1, $2)")
        .bind(upload)
        .bind(second)
        .execute(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let again = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);
    assert_eq!(again.json["state"], "done");
}

/// 리뷰(감사 S11 후속): 마무리 작업은 연결이 끊겨도 끝까지 돌지만, 점유 풀기는 요청 핸들러에 있어 연결이 끊기면(프록시
/// 시간 제한 등) 실행되지 않았다. 그러면 업로드가 'completing'에 남아 다음 마치기가 5분 동안 409를 받았다. 풀기는 마무리
/// 작업 안에서 한다.
#[tokio::test]
async fn a_dropped_complete_request_still_releases_a_stalled_claim() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "끊긴 마무리").await;
    let doc = create_doc(&app, &admin, &ws, "첨부 문서").await;
    // 다른 시험의 업로드와 겹치지 않는 크기.
    let data = vec![5_u8; 3221];
    let (status, up) = start(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "자료.bin", "size": data.len(), "content_type": "application/octet-stream" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap().to_owned();
    assert_eq!(
        part(&app, &admin, &id, 1, &data, &sha(&data)).await,
        StatusCode::NO_CONTENT
    );

    // 이 크기의 파일 버전을 기록하는 순간 시험이 쥔 잠금을 기다렸다가 교착(40P01)으로 실패하게 한다.
    let mut o = owner(None).await;
    let name = format!("test_stall_{}", uuid::Uuid::now_v7().simple());
    let key: i64 = sqlx::query_scalar("SELECT hashtextextended($1, 0)")
        .bind(&name)
        .fetch_one(&mut o)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(key)
        .execute(&mut o)
        .await
        .unwrap();
    sqlx::query(&format!(
        "CREATE FUNCTION fvoci.{name}() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN IF NEW.size = 3221 THEN
           PERFORM pg_advisory_xact_lock({key});
           RAISE EXCEPTION 'test deadlock' USING ERRCODE = '40P01';
         END IF;
         RETURN NEW; END $$"
    ))
    .execute(&mut o)
    .await
    .unwrap();
    sqlx::query(&format!(
        "CREATE TRIGGER {name} BEFORE INSERT ON fvoci.file_versions FOR EACH ROW EXECUTE FUNCTION fvoci.{name}()"
    ))
    .execute(&mut o)
    .await
    .unwrap();

    // 마치기 요청이 기록 단계에서 멈춘 사이 연결이 끊긴다(요청 핸들러가 사라진다).
    let uri = format!("/api/uploads/{id}/complete");
    let request = {
        let (app, admin, uri) = (app.clone(), admin.clone(), uri.clone());
        tokio::spawn(async move { send(&app, Method::POST, &uri, Some(&admin), None).await })
    };
    let mut waiting = false;
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND NOT granted \
             AND ((classid::bigint << 32) | objid::bigint) = $1",
        )
        .bind(key)
        .fetch_one(&mut o)
        .await
        .unwrap();
        if n > 0 {
            waiting = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(waiting, "the completion never reached the stalled insert");
    request.abort();
    assert!(request.await.is_err_and(|e| e.is_cancelled()));

    // 멈췄던 마무리가 일시적 오류로 끝난다.
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(key)
        .execute(&mut o)
        .await
        .unwrap();
    for cleanup in [
        format!("DROP TRIGGER {name} ON fvoci.file_versions"),
        format!("DROP FUNCTION fvoci.{name}()"),
    ] {
        sqlx::query(&cleanup).execute(&mut o).await.unwrap();
    }

    // 다음 마치기는 5분을 기다리지 않고 이어서 끝낸다(점유는 마무리 작업이 풀었다).
    let mut last = None;
    for _ in 0..100 {
        let again = send(&app, Method::POST, &uri, Some(&admin), None).await;
        if again.status != StatusCode::CONFLICT {
            last = Some(again);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let again = last.expect("the claim stayed held after the dropped request");
    assert_eq!(again.status, StatusCode::OK, "{:?}", again.json);
    assert_eq!(again.json["state"], "done");
}

/// #65 리뷰(감사 S11 후속): 로컬 저장소는 마칠 때 조각을 임시 파일에 이은 뒤 최종 키로 이름을 바꿨다. 앞 시도가 잇는
/// 도중 오래 멈춰 5분 뒤 다른 요청이 넘겨받고, 그 요청이 조각 오류로 업로드를 되돌린 뒤 다른 바이트로 다시 받아 마치면,
/// 늦게 돌아온 앞 시도의 이름 바꾸기가 최종 키를 덮어 저장된 바이트와 기록된 SHA-256이 어긋났다. 최종 키로 게시하는 것은
/// 점유를 쥔 시도뿐이다. 앞 시도는 첫 조각 자리에 둔 FIFO에서 읽다가 멈춘다.
#[cfg(unix)]
#[tokio::test]
async fn a_late_attempt_cannot_overwrite_what_the_take_over_stored() {
    let dir = std::env::temp_dir().join(format!("fvoci-files-{}", uuid::Uuid::now_v7().simple()));
    let state = state_with(&[("FVOCI_STORAGE_DIR", dir.to_str().unwrap())]);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "늦은 게시").await;
    let doc = create_doc(&app, &admin, &ws, "첨부 문서").await;
    let old = vec![3_u8; 3233];
    let new = vec![4_u8; 3233];
    let (status, up) = start(
        &app,
        &admin,
        json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc["id"] },
                "name": "자료.bin", "size": old.len(), "content_type": "application/octet-stream" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{up:?}");
    let id = up["id"].as_str().unwrap().to_owned();
    assert_eq!(
        part(&app, &admin, &id, 1, &old, &sha(&old)).await,
        StatusCode::NO_CONTENT
    );
    let upload: uuid::Uuid = id.parse().unwrap();
    let mut o = owner(None).await;
    let (key, storage_upload): (String, String) =
        sqlx::query_as("SELECT storage_key, storage_upload_id FROM fvoci.uploads WHERE id = $1")
            .bind(upload)
            .fetch_one(&mut o)
            .await
            .unwrap();
    let first_part = dir.join("uploads").join(&storage_upload).join("00001");
    std::fs::remove_file(&first_part).unwrap();
    let made = std::process::Command::new("mkfifo")
        .arg(&first_part)
        .status()
        .unwrap();
    assert!(made.success());

    // 앞 시도가 점유하고 첫 조각을 읽다가 멈춘다(FIFO에 쓰는 쪽이 열리면 읽는 쪽도 열린 것이다).
    let uri = format!("/api/uploads/{id}/complete");
    let late = {
        let (app, admin, uri) = (app.clone(), admin.clone(), uri.clone());
        tokio::spawn(async move { send(&app, Method::POST, &uri, Some(&admin), None).await })
    };
    let writer = {
        let path = first_part.clone();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            tokio::task::spawn_blocking(move || std::fs::OpenOptions::new().write(true).open(path)),
        )
        .await
        .expect("the first attempt never started reading the parts")
        .unwrap()
        .unwrap()
    };

    // 5분이 지나 다음 시도가 넘겨받는다. 첫 조각이 없어 잇지 못하고 업로드를 되돌린다.
    sqlx::query(
        "UPDATE fvoci.uploads SET state_changed_at = state_changed_at - interval '6 minutes' WHERE id = $1",
    )
    .bind(upload)
    .execute(&mut o)
    .await
    .unwrap();
    std::fs::remove_file(&first_part).unwrap();
    let take_over = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(
        take_over.status,
        StatusCode::CONFLICT,
        "{:?}",
        take_over.json
    );

    // 다른 바이트를 다시 받아 마친다.
    assert_eq!(
        part(&app, &admin, &id, 1, &new, &sha(&new)).await,
        StatusCode::NO_CONTENT
    );
    let done = send(&app, Method::POST, &uri, Some(&admin), None).await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    assert_eq!(done.json["state"], "done");
    let recorded: String =
        sqlx::query_scalar("SELECT sha256 FROM fvoci.file_versions WHERE storage_key = $1")
            .bind(&key)
            .fetch_one(&mut o)
            .await
            .unwrap();
    assert_eq!(recorded, sha(&new));

    // 앞 시도가 남은 조각을 다 읽고 늦게 돌아온다. 점유를 잃었으니 최종 키를 덮지 못한다.
    {
        use std::io::Write as _;
        let mut writer = writer;
        writer.write_all(&old).unwrap();
    }
    let late = tokio::time::timeout(std::time::Duration::from_secs(30), late)
        .await
        .expect("the first attempt never came back")
        .unwrap();
    assert!(!late.status.is_server_error(), "{:?}", late.json);
    let (stored, size) = state.storage.digest(&key).await.unwrap();
    assert_eq!(size, new.len() as u64);
    assert_eq!(
        stored, recorded,
        "the late attempt overwrote the stored content"
    );
    assert!(
        !dir.join("uploads").join(&storage_upload).exists(),
        "the received parts were left behind"
    );
    let _ignored = std::fs::remove_dir_all(&dir);
}
