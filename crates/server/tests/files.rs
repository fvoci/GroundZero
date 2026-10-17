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
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
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
    // D-20: 내려받기는 고칠 수 있는 사람이나, 소유자가 복제·다운로드를 허용한 문서의 독자만.
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
        StatusCode::OK
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
    let report = fvoci_server::worker::run_all_now(&state.db, None, Some(&state.storage))
        .await
        .unwrap();
    assert!(report.cleaned >= 2, "{report:?}");
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
    let (reader, _) = add_member(&app, &state, &admin, &ws, "reader", "reader").await;
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
    let report = fvoci_server::worker::run_all_now(&state.db, None, Some(&state.storage))
        .await
        .unwrap();
    assert!(report.cleaned >= 1, "{report:?}");
}

#[tokio::test]
async fn s3_files_round_trip_and_purge_with_the_document() {
    let overrides = s3_overrides();
    let pairs: Vec<(&str, &str)> = overrides.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let state = state_with(&pairs);
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "S3 파일").await;
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
    let report = fvoci_server::worker::run_all_now(&state.db, None, Some(&state.storage))
        .await
        .unwrap();
    assert!(report.cleaned >= 1, "{report:?}");
}
