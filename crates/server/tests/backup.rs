#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 백업·복구 (D-39): 새 DB와 새 저장소에 복구하면 같은 사람이 같은 문서·업무·파일을 그대로 본다.

mod common;

use axum::http::{Method, StatusCode};
use common::{
    TestUser, config_with, create_doc, create_workspace, doc_body, new_request_id, send, send_raw,
};
use fvoci_server::backup;
use fvoci_server::config::{DbCommandConfig, Secret};
use fvoci_server::features::accounts::{SESSION_COOKIE, VerifiedIdentity, start_session};
use fvoci_server::http::AppState;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

async fn session(state: &AppState, subject: &str) -> TestUser {
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: subject.to_owned(),
        email: None,
        email_verified: false,
        display_name: "백업 사용자".to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (id, token) = start_session(&state.db, state.config.session_ttl, &identity, None)
        .await
        .unwrap();
    TestUser {
        id,
        cookie: format!("{SESSION_COOKIE}={token}"),
    }
}

fn with_database(url: &str, database: &str) -> String {
    let mut parsed = url::Url::parse(url).unwrap();
    parsed.set_path(&format!("/{database}"));
    parsed.to_string()
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("fvoci-{label}-{}", uuid::Uuid::now_v7().simple()))
}

#[tokio::test]
async fn backup_restores_documents_tasks_and_files_into_a_fresh_install() {
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let migration_url =
        std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let app_url = std::env::var("DATABASE_URL").expect("run via `cargo xtask test`");
    let source_files = temp_dir("source-files");
    let source_dir = source_files.to_string_lossy().into_owned();
    let state =
        fvoci_server::app::build_state(config_with(&[("FVOCI_STORAGE_DIR", &source_dir)])).unwrap();
    let app = common::app(&state);
    let subject = format!("backup-{}", uuid::Uuid::now_v7());
    let owner = session(&state, &subject).await;

    // 원본: 워크스페이스·문서(본문)·업무·첨부 파일.
    let ws = create_workspace(&app, &owner, "백업할 공간").await;
    let doc = create_doc(&app, &owner, &ws, "회의록").await;
    let doc_id = doc["id"].as_str().unwrap().to_owned();
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{doc_id}"),
        Some(&owner),
        Some(
            json!({ "request_id": new_request_id(), "base_revision": 1, "title": "회의록",
                     "body": doc_body("복구될 내용") }),
        ),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let bytes = b"backup me".to_vec();
    let up = send(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&owner),
        Some(
            json!({ "request_id": new_request_id(), "target": { "kind": "document", "id": doc_id },
                     "name": "자료.txt", "size": bytes.len(), "content_type": "text/plain" }),
        ),
    )
    .await;
    let upload = up.json["id"].as_str().unwrap().to_owned();
    let hash = hex::encode(Sha256::digest(&bytes));
    let (status, _, _) = send_raw(
        &app,
        Method::PUT,
        &format!("/api/uploads/{upload}/parts/1"),
        Some(&owner),
        &[("x-content-sha256", hash.as_str())],
        bytes.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let done = send(
        &app,
        Method::POST,
        &format!("/api/uploads/{upload}/complete"),
        Some(&owner),
        None,
    )
    .await;
    let file_id = done.json["file_id"].as_str().unwrap().to_owned();

    // 백업.
    let out = temp_dir("backup");
    let db = DbCommandConfig {
        admin_url: Some(Secret::new(admin_url.clone())),
        migration_url: Some(Secret::new(migration_url.clone())),
        migrator_password: None,
        app_password: None,
    };
    let manifest = backup::create(&state.config, &db, &out).await.unwrap();
    assert!(manifest.objects.iter().any(|o| o.sha256 == hash));
    assert!(
        manifest.tables.iter().all(|t| t.name != "sessions"),
        "sessions are not backed up"
    );
    // 같은 곳에 다시 쓰지 않는다.
    assert!(backup::create(&state.config, &db, &out).await.is_err());

    // 새 DB(같은 클러스터의 다른 데이터베이스)를 같은 마이그레이션까지 만든다.
    let target = format!("fvoci_restore_{}", uuid::Uuid::now_v7().simple());
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    sqlx::raw_sql(&format!("CREATE DATABASE {target}"))
        .execute(&mut admin)
        .await
        .unwrap();
    let mut target_admin = PgConnection::connect(&with_database(&admin_url, &target))
        .await
        .unwrap();
    sqlx::raw_sql(&format!(
        "GRANT CONNECT ON DATABASE {target} TO fvoci_migrator, fvoci_app;
         CREATE SCHEMA fvoci AUTHORIZATION fvoci_migrator;"
    ))
    .execute(&mut target_admin)
    .await
    .unwrap();
    let restored_db = DbCommandConfig {
        admin_url: Some(Secret::new(with_database(&admin_url, &target))),
        migration_url: Some(Secret::new(with_database(&migration_url, &target))),
        migrator_password: None,
        app_password: None,
    };
    fvoci_server::db::admin::migrate(&restored_db)
        .await
        .unwrap();
    let target_files = temp_dir("target-files");
    let target_dir = target_files.to_string_lossy().into_owned();
    let target_app_url = with_database(&app_url, &target);
    let restored = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_STORAGE_DIR", &target_dir),
        ("DATABASE_URL", &target_app_url),
    ]))
    .unwrap();
    // 백업 파일은 소유자만 읽는다.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(out.join("manifest.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "backup files are private");
    }
    // 손상된 덤프는 아무것도 건드리기 전에 거부한다.
    let damaged = temp_dir("damaged");
    copy_dir(&out, &damaged);
    std::fs::write(damaged.join("db").join("documents.copy"), b"broken\n").unwrap();
    let err = backup::restore(&restored.config, &restored_db, &damaged)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("damaged"), "{err}");
    backup::restore(&restored.config, &restored_db, &out)
        .await
        .unwrap();
    // 빈 DB에만 복구한다.
    assert!(
        backup::restore(&restored.config, &restored_db, &out)
            .await
            .is_err()
    );

    // 같은 사람(발급자·subject)이 다시 로그인하면 그대로 본다.
    let restored_app = common::app(&restored);
    let again = session(&restored, &subject).await;
    assert_eq!(
        again.id, owner.id,
        "the same account is matched by issuer and subject"
    );
    let opened = send(
        &restored_app,
        Method::GET,
        &format!("/api/documents/{doc_id}"),
        Some(&again),
        None,
    )
    .await;
    assert_eq!(opened.status, StatusCode::OK, "{:?}", opened.json);
    assert_eq!(opened.json["body"], doc_body("복구될 내용"));
    assert_eq!(opened.json["reference"], doc["reference"]);
    let (status, _, content) = send_raw(
        &restored_app,
        Method::GET,
        &format!("/api/files/{file_id}/content"),
        Some(&again),
        &[],
        Vec::new(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(content, bytes);
    // 옛 세션은 복구되지 않는다.
    assert_eq!(
        send(&restored_app, Method::GET, "/api/me", Some(&owner), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    restored.db.close().await;
    drop(target_admin);
    sqlx::raw_sql(&format!("DROP DATABASE {target} WITH (FORCE)"))
        .execute(&mut admin)
        .await
        .unwrap();
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
