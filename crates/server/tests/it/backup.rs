#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 백업·복구 (D-39): 새 DB와 새 저장소에 복구하면 같은 사람이 같은 문서·업무·파일을 그대로 본다.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{
    TestUser, config_with, create_doc, doc_body, fresh_database, new_request_id, send, send_raw,
};
use fvoci_server::backup;
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

/// 백업·복구의 저장소 잠금이 남지 않았는지: 잡힌 advisory lock이 없고, 다른 세션이 바로 잡았다 놓을 수 있다.
async fn assert_storage_lock_free(url: &str) {
    let mut conn = PgConnection::connect(url).await.unwrap();
    let held: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_locks WHERE locktype = 'advisory' AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
           AND ((classid::bigint << 32) | objid::bigint) = $1",
    )
    .bind(backup::STORAGE_LOCK)
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert_eq!(held, 0, "the storage lock is still held");
    let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(backup::STORAGE_LOCK)
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert!(got);
    let released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(backup::STORAGE_LOCK)
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert!(released);
    conn.close().await.unwrap();
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("fvoci-{label}-{}", uuid::Uuid::now_v7().simple()))
}

#[tokio::test]
async fn backup_restores_documents_tasks_and_files_into_a_fresh_install() {
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    // 원본 설치: 자기 DB와 자기 저장소(다른 시험의 행·파일이 섞이지 않게).
    let source = fresh_database(&mut admin, "backup_src").await;
    let source_files = temp_dir("source-files");
    let source_dir = source_files.to_string_lossy().into_owned();
    let state = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_STORAGE_DIR", &source_dir),
        ("DATABASE_URL", &source.app_url),
    ]))
    .unwrap();
    let app = common::app(&state);
    let subject = format!("backup-{}", uuid::Uuid::now_v7());
    let owner = session(&state, &subject).await;

    // 원본: 워크스페이스·문서(본문)·업무·첨부 파일.
    // D-111·D-112: 별도 설치의 조직·관리자는 그 설치 DB에만 시드한다.
    let mut fixture = PgConnection::connect(source.db.migration_url.as_ref().unwrap().expose())
        .await
        .unwrap();
    sqlx::query("INSERT INTO fvoci.server_admins (user_id) VALUES ($1)")
        .bind(owner.id.0)
        .execute(&mut fixture)
        .await
        .unwrap();
    let mut tx = fixture.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1::text, true)")
        .bind(owner.id.0)
        .execute(&mut *tx)
        .await
        .unwrap();
    let organization: uuid::Uuid =
        sqlx::query_scalar("SELECT fvoci.app_create_organization($1, $2, $3, $4)")
            .bind(uuid::Uuid::now_v7())
            .bind("백업 조직")
            .bind(owner.id.0)
            .bind((0..3).map(|_| uuid::Uuid::now_v7()).collect::<Vec<_>>())
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    fixture.close().await.unwrap();
    let workspace = send(&app, Method::POST, "/api/workspaces", Some(&owner),
        Some(json!({ "request_id": new_request_id(), "organization_id": organization, "name": "백업할 공간" }))).await;
    assert_eq!(
        workspace.status,
        StatusCode::CREATED,
        "{:?}",
        workspace.json
    );
    let ws = workspace.json["id"].as_str().unwrap().to_owned();
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
    let db = &source.db;
    let manifest = backup::create(&state.config, db, &out).await.unwrap();
    assert!(manifest.objects.iter().any(|o| o.sha256 == hash));
    assert!(
        manifest.tables.iter().all(|t| t.name != "sessions"),
        "sessions are not backed up"
    );
    // 같은 곳에 다시 쓰지 않는다.
    assert!(backup::create(&state.config, db, &out).await.is_err());
    // 저장소 정리를 멈추는 잠금은 백업이 잡은 세션에서 풀려 남지 않는다(다른 세션이 바로 잡을 수 있다).
    assert_storage_lock_free(&source.app_url).await;

    // 새 DB(같은 클러스터의 다른 데이터베이스)를 같은 마이그레이션까지 만든다.
    let target = fresh_database(&mut admin, "restore").await;
    let restored_db = &target.db;
    let target_files = temp_dir("target-files");
    let target_dir = target_files.to_string_lossy().into_owned();
    let restored = fvoci_server::app::build_state(config_with(&[
        ("FVOCI_STORAGE_DIR", &target_dir),
        ("DATABASE_URL", &target.app_url),
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
    let err = backup::restore(&restored.config, restored_db, &damaged)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("damaged"), "{err}");
    // 감사 10번: 목록에서 표가 빠진 백업도 거부한다(빠진 표의 내용 없이 복구를 끝내면 안 된다).
    let partial = temp_dir("partial");
    copy_dir(&out, &partial);
    let mut listed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(partial.join("manifest.json")).unwrap()).unwrap();
    listed["tables"]
        .as_array_mut()
        .unwrap()
        .retain(|t| t["name"] != "comments");
    std::fs::write(
        partial.join("manifest.json"),
        serde_json::to_vec(&listed).unwrap(),
    )
    .unwrap();
    let err = backup::restore(&restored.config, restored_db, &partial)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("comments"), "{err}");
    // 목록의 파일 크기가 손상되면(터무니없이 큼) 버퍼를 잡기 전에 거부한다.
    let oversized = temp_dir("oversized");
    copy_dir(&out, &oversized);
    let mut listed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(oversized.join("manifest.json")).unwrap()).unwrap();
    listed["objects"][0]["size"] = serde_json::json!(i64::MAX);
    std::fs::write(
        oversized.join("manifest.json"),
        serde_json::to_vec(&listed).unwrap(),
    )
    .unwrap();
    let err = backup::restore(&restored.config, restored_db, &oversized)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("size differs"), "{err}");
    backup::restore(&restored.config, restored_db, &out)
        .await
        .unwrap();
    // 빈 DB에만 복구한다.
    assert!(
        backup::restore(&restored.config, restored_db, &out)
            .await
            .is_err()
    );
    assert_storage_lock_free(&target.app_url).await;

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
    // 복구한 DB에서도 RLS가 그대로다: 멤버가 아닌 사람은 문서를 못 보고, 사용자 문맥 없는 앱 연결은 행을 못 본다.
    let stranger = session(
        &restored,
        &format!("backup-stranger-{}", uuid::Uuid::now_v7()),
    )
    .await;
    assert_eq!(
        send(
            &restored_app,
            Method::GET,
            &format!("/api/documents/{doc_id}"),
            Some(&stranger),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let mut app_conn = PgConnection::connect(&target.app_url).await.unwrap();
    let visible: i64 = sqlx::query_scalar("SELECT count(*) FROM fvoci.documents")
        .fetch_one(&mut app_conn)
        .await
        .unwrap();
    assert_eq!(visible, 0, "rows are hidden without a user context");
    app_conn.close().await.unwrap();

    restored.db.close().await;
    state.db.close().await;
    for name in [&target.name, &source.name] {
        sqlx::raw_sql(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
    }
}

/// 기준 SQL이 영역 파일 여럿이 된 뒤에도 백업은 마지막으로 적용한 번호를 적고, 복구할 DB의 번호와 같을 때만 복구한다.
/// 끝까지 적용한 설치의 백업은 같은 릴리스의 새 설치에 복구된다. 한 파일 기준(202610250001)의 백업과 일부 파일만
/// 적용된 DB의 백업은 복구가 거부되고, 일부만 적용된 DB에는 어떤 백업도 복구되지 않는다.
#[tokio::test]
async fn backup_restores_only_into_the_same_fully_applied_baseline() {
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    let migrations = sqlx::migrate!("../db/migrations").migrations;
    let last = migrations.last().unwrap().version;
    let partial_at = migrations[4].version;
    let files = temp_dir("generation-files");
    let config = config_with(&[("FVOCI_STORAGE_DIR", &files.to_string_lossy())]);

    let source = fresh_database(&mut admin, "generation_src").await;
    let full = temp_dir("generation-full");
    let manifest = backup::create(&config, &source.db, &full).await.unwrap();
    assert_eq!(manifest.migration_version, last);
    let target = fresh_database(&mut admin, "generation_target").await;
    backup::restore(&config, &target.db, &full).await.unwrap();

    let rejecting = fresh_database(&mut admin, "generation_rejecting").await;
    let single_file = temp_dir("generation-single-file");
    copy_dir(&full, &single_file);
    let mut listed: serde_json::Value =
        serde_json::from_slice(&std::fs::read(single_file.join("manifest.json")).unwrap()).unwrap();
    listed["migration_version"] = json!(202_610_250_001_i64);
    std::fs::write(
        single_file.join("manifest.json"),
        serde_json::to_vec(&listed).unwrap(),
    )
    .unwrap();
    let err = backup::restore(&config, &rejecting.db, &single_file)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("at migration {last} but the backup is at 202610250001")),
        "{err}"
    );

    let partial = common::partial_database(&mut admin, "generation_partial", 5).await;
    let from_partial = temp_dir("generation-partial");
    let manifest = backup::create(&config, &partial.db, &from_partial)
        .await
        .unwrap();
    assert_eq!(manifest.migration_version, partial_at);
    let err = backup::restore(&config, &rejecting.db, &from_partial)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("at migration {last} but the backup is at {partial_at}")),
        "{err}"
    );
    let err = backup::restore(&config, &partial.db, &full)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains(&format!("at migration {partial_at} but the backup is at {last}")),
        "{err}"
    );

    for name in [&source.name, &target.name, &rejecting.name, &partial.name] {
        sqlx::raw_sql(&format!("DROP DATABASE {name} WITH (FORCE)"))
            .execute(&mut admin)
            .await
            .unwrap();
    }
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

/// 관리자의 `fvoci_migrator` 직접 멤버십 레코드(grantor, ADMIN, INHERIT, SET).
async fn migrator_memberships(
    conn: &mut PgConnection,
    member: &str,
) -> Vec<(String, bool, bool, bool)> {
    sqlx::query_as(
        "SELECT g.rolname::text, m.admin_option, m.inherit_option, m.set_option
           FROM pg_auth_members m JOIN pg_roles g ON g.oid = m.grantor
          WHERE m.roleid = 'fvoci_migrator'::regrole
            AND m.member = (SELECT oid FROM pg_roles WHERE rolname = $1)
          ORDER BY 1",
    )
    .bind(member)
    .fetch_all(conn)
    .await
    .unwrap()
}

/// 표 소유자 역할을 잠시 빌렸다 돌려주면(bootstrap·복구, D-88) 관리자의 멤버십은 빌리기 전과 같다: 새로 만든
/// 레코드만 지우고, 관리자에게 이미 있던 레코드는 지우지 않고 옵션을 되돌린다. superuser가 아닌 관리자를 시험
/// 역할로 만들어 본다(시험 관리자는 superuser일 수 있다).
#[tokio::test]
async fn borrowing_the_migrator_leaves_the_admins_memberships_as_they_were() {
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let mut admin = PgConnection::connect(&admin_url).await.unwrap();
    let role = format!("fvoci_borrow_{}", uuid::Uuid::now_v7().simple());
    let password = uuid::Uuid::now_v7().to_string();
    sqlx::raw_sql(&format!(
        "CREATE ROLE {role} LOGIN PASSWORD '{password}';
         GRANT fvoci_migrator TO {role} WITH ADMIN TRUE, INHERIT FALSE, SET FALSE"
    ))
    .execute(&mut admin)
    .await
    .unwrap();
    let options = admin_url
        .parse::<sqlx::postgres::PgConnectOptions>()
        .unwrap()
        .username(&role)
        .password(&password);
    let mut conn = PgConnection::connect_with(&options).await.unwrap();

    // 관리자 자신이 준 레코드가 없다: 새 레코드를 만들고 그것만 지운다.
    let before = migrator_memberships(&mut admin, &role).await;
    let borrow = fvoci_db::admin::borrow_migrator(&mut conn).await.unwrap();
    assert!(
        matches!(borrow, fvoci_db::admin::MigratorBorrow::Added { .. }),
        "{borrow:?}"
    );
    sqlx::query("SET ROLE fvoci_migrator")
        .execute(&mut conn)
        .await
        .unwrap();
    fvoci_db::admin::release_migrator(&mut conn, borrow)
        .await
        .unwrap();
    assert_eq!(migrator_memberships(&mut admin, &role).await, before);

    // 관리자가 스스로 준 SET 없는 레코드(INHERIT 켬)가 있다: 그 레코드를 쓰고 원래 옵션으로 되돌린다.
    sqlx::raw_sql(&format!(
        "GRANT fvoci_migrator TO {role} WITH INHERIT TRUE, SET FALSE"
    ))
    .execute(&mut conn)
    .await
    .unwrap();
    let before = migrator_memberships(&mut admin, &role).await;
    assert_eq!(before.len(), 2, "{before:?}");
    let borrow = fvoci_db::admin::borrow_migrator(&mut conn).await.unwrap();
    assert!(
        matches!(
            &borrow,
            fvoci_db::admin::MigratorBorrow::Changed { grantor, inherit: true } if *grantor == role
        ),
        "{borrow:?}"
    );
    sqlx::query("SET ROLE fvoci_migrator")
        .execute(&mut conn)
        .await
        .unwrap();
    fvoci_db::admin::release_migrator(&mut conn, borrow)
        .await
        .unwrap();
    assert_eq!(migrator_memberships(&mut admin, &role).await, before);

    // 빌린 뒤 연결이 끊겼다(복구 중 장애): 같은 관리자 주소의 새 연결로 되돌린다.
    let borrow = fvoci_db::admin::borrow_migrator(&mut conn).await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    let mut other = PgConnection::connect_with(&options).await.unwrap();
    let ended: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(&mut other)
        .await
        .unwrap();
    assert!(ended);
    other.close().await.unwrap();
    let role_url = fvoci_core::secret::Secret::new(format!(
        "postgres://{role}:{password}@{}:{}/{}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap_or("postgres"),
    ));
    fvoci_db::admin::release_migrator_or_reconnect(&mut conn, &role_url, borrow)
        .await
        .unwrap();
    assert_eq!(migrator_memberships(&mut admin, &role).await, before);

    drop(conn);
    sqlx::raw_sql(&format!("DROP ROLE {role}"))
        .execute(&mut admin)
        .await
        .unwrap();
}
