#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 첫 사용자 흐름: 워크스페이스 생성 → 위키 문서 작성 → 저장 → 재열기와 실패 경로.
//! 로그인은 OIDC 콜백과 같은 `start_session` 경로를 쓴다. 실제 OIDC 로그인은 `oidc.rs`.

use crate::common;

use std::time::Duration;

use axum::http::{Method, StatusCode};
use common::{
    create_doc, create_workspace, doc_body, login, new_request_id, send, send_with_origin,
};
use fvoci_server::app::build_state;
use fvoci_server::features::accounts::start_session;
use serde_json::json;
use sqlx::Connection;

#[tokio::test]
async fn workspace_create_is_idempotent_and_listed() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let request_id = new_request_id();
    // 워크스페이스는 조직 안에 만든다(D-111).
    let organization = common::organization_for(&alice).await;
    let body =
        json!({ "request_id": request_id, "organization_id": organization, "name": "  연구실  " });

    let first = send(
        &app,
        Method::POST,
        "/api/workspaces",
        Some(&alice),
        Some(body.clone()),
    )
    .await;
    assert_eq!(first.status, StatusCode::CREATED);
    assert_eq!(first.json["name"], "연구실");
    assert_eq!(first.json["role"], "admin");
    // 만든 사람은 관리자이고 접근 등급은 owner다(D-113).
    assert_eq!(first.json["access"], "owner");
    let retry = send(
        &app,
        Method::POST,
        "/api/workspaces",
        Some(&alice),
        Some(body),
    )
    .await;
    assert_eq!(
        retry.json["id"], first.json["id"],
        "retry must not create a second workspace"
    );

    let list = send(&app, Method::GET, "/api/workspaces", Some(&alice), None).await;
    let ids: Vec<_> = list
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["id"].clone())
        .collect();
    assert_eq!(ids.iter().filter(|id| **id == first.json["id"]).count(), 1);
}

/// HTTP 계층과 무관하게 앱 역할의 RLS가 다른 사용자의 행을 숨기는지 확인한다.
#[tokio::test]
async fn rls_hides_rows_from_other_users_under_app_role() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob = login(&state, "bob").await;
    let ws = create_workspace(&app, &alice, "RLS").await;
    let doc = create_doc(&app, &alice, &ws, "행 수준 보안").await;
    let doc_id = uuid::Uuid::parse_str(doc["id"].as_str().unwrap()).unwrap();

    let url = std::env::var("DATABASE_URL").unwrap();
    let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
    let role: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    assert_eq!(
        role, "fvoci_app",
        "tests must run as the app role, not an admin role"
    );
    let bypass: bool =
        sqlx::query_scalar("SELECT rolbypassrls FROM pg_roles WHERE rolname = current_user")
            .fetch_one(&mut conn)
            .await
            .unwrap();
    assert!(!bypass);

    let count = |user: Option<String>| {
        let url = url.clone();
        async move {
            let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
            let mut tx = conn.begin().await.unwrap();
            if let Some(u) = user {
                sqlx::query("SELECT set_config('app.user_id', $1, true)")
                    .bind(u)
                    .execute(&mut *tx)
                    .await
                    .unwrap();
            }
            let docs: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id = $1")
                .bind(doc_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            let revisions: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM document_revisions WHERE document_id = $1",
            )
            .bind(doc_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            let updated = sqlx::query(
                "UPDATE documents SET title = 'hacked', revision = revision + 1 WHERE id = $1",
            )
            .bind(doc_id)
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected();
            tx.rollback().await.unwrap();
            (docs, revisions, updated)
        }
    };
    assert_eq!(count(Some(bob.id.0.to_string())).await, (0, 0, 0));
    assert_eq!(count(None).await, (0, 0, 0));
    assert_eq!(count(Some(alice.id.0.to_string())).await, (1, 1, 1));

    // 앱 역할은 문서·리비전·감사 기록을 지울 수 없다.
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(alice.id.0.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let err = sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(doc_id)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
}

#[tokio::test]
async fn audit_records_creation_and_rename_without_body() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "감사").await;
    let doc = create_doc(&app, &alice, &ws, "이름1").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1,
                     "title": "이름2", "body": doc_body("비밀 본문") })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);

    let mut tx = state.db.begin_as(alice.id).await.unwrap();
    let rows: Vec<(String, String, serde_json::Value)> = sqlx::query_as(
        "SELECT action, target_type, detail FROM audit_log WHERE workspace_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(&ws)
    .fetch_all(tx.conn())
    .await
    .unwrap();
    let actions: Vec<_> = rows.iter().map(|(a, t, _)| format!("{t}:{a}")).collect();
    assert_eq!(
        actions,
        vec!["workspace:create", "document:create", "document:rename"]
    );
    assert!(
        rows.iter()
            .all(|(_, _, d)| !d.to_string().contains("비밀 본문"))
    );
}

#[tokio::test]
async fn input_validation_and_origin_and_session_failures() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "검증").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    let bad_body = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1, "title": "t", "body": { "type": "paragraph" } })),
    )
    .await;
    assert_eq!(bad_body.status, StatusCode::BAD_REQUEST);
    let organization = common::organization_for(&alice).await;
    let empty_title = send(
        &app,
        Method::POST,
        "/api/workspaces",
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "organization_id": organization, "name": "   " })),
    )
    .await;
    assert_eq!(empty_title.status, StatusCode::BAD_REQUEST);

    // 교차 출처·Origin 없는 변경 요청은 거부된다.
    let body =
        json!({ "request_id": new_request_id(), "organization_id": organization, "name": "x" });
    for origin in [None, Some("http://evil.example")] {
        let r = send_with_origin(
            &app,
            Method::POST,
            "/api/workspaces",
            Some(&alice),
            Some(body.clone()),
            origin,
        )
        .await;
        assert_eq!(r.status, StatusCode::FORBIDDEN);
        assert_eq!(r.json["error"], "bad_origin");
    }

    assert_eq!(
        send(&app, Method::GET, "/api/me", None, None).await.status,
        StatusCode::UNAUTHORIZED
    );
    let me = send(&app, Method::GET, "/api/me", Some(&alice), None).await;
    assert_eq!(me.json["display_name"], "alice");

    // 만료된 세션은 거부된다.
    let identity = fvoci_server::features::accounts::VerifiedIdentity {
        issuer: "https://test-issuer.invalid".into(),
        subject: format!("expiring-{}", uuid::Uuid::now_v7()),
        email: None,
        email_verified: false,
        display_name: "expiring".into(),
        id_token: "t".into(),
    };
    // 원격 DB 왕복 중 만료되기 전에 유효 세션 응답을 먼저 검사할 수 있도록 여유를 둔다.
    let session_ttl = Duration::from_secs(10);
    let (_, token) = start_session(&state.db, session_ttl, &identity, None)
        .await
        .unwrap();
    let expiring = common::TestUser {
        id: alice.id,
        cookie: format!("fvoci_session={token}"),
    };
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&expiring), None)
            .await
            .status,
        StatusCode::OK
    );
    tokio::time::sleep(session_ttl + Duration::from_millis(500)).await;
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&expiring), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    assert_eq!(
        send(&app, Method::GET, "/api/nope", Some(&alice), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
}

/// 새 풀과 새 상태(서버 재시작과 같은 조건)에서 같은 세션으로 문서를 다시 연다.
#[tokio::test]
async fn data_and_session_survive_fresh_state() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "재시작").await;
    let doc = create_doc(&app, &alice, &ws, "영속").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let r = send(
        &app,
        Method::PUT,
        &uri,
        Some(&alice),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1, "title": "영속", "body": doc_body("남아 있어야 함") })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    state.db.close().await;

    let fresh = build_state(common::config_with(&[])).unwrap();
    let reopened = send(&common::app(&fresh), Method::GET, &uri, Some(&alice), None).await;
    assert_eq!(reopened.status, StatusCode::OK);
    assert_eq!(reopened.json["body"], doc_body("남아 있어야 함"));
    assert_eq!(reopened.json["revision"], 2);
}

/// 앱 역할은 세션·로그인 상태·번호열·감사 기록을 직접 다룰 수 없고, 리비전을 되감을 수 없다.
#[tokio::test]
async fn app_role_cannot_touch_internal_tables_or_rewind_revisions() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "불변식").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let doc_id = uuid::Uuid::parse_str(doc["id"].as_str().unwrap()).unwrap();
    let ws_id = uuid::Uuid::parse_str(&ws).unwrap();

    let url = std::env::var("DATABASE_URL").unwrap();
    let denied = |sql: &'static str, bind: Option<uuid::Uuid>| {
        let url = url.clone();
        let user = alice.id.0.to_string();
        async move {
            let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
            let mut tx = conn.begin().await.unwrap();
            sqlx::query("SELECT set_config('app.user_id', $1, true)")
                .bind(user)
                .execute(&mut *tx)
                .await
                .unwrap();
            let mut q = sqlx::query(sql);
            if let Some(b) = bind {
                q = q.bind(b);
            }
            let err = q.execute(&mut *tx).await.unwrap_err();
            err.as_database_error()
                .and_then(|e| e.code())
                .map(|c| c.into_owned())
        }
    };
    let perm = Some("42501".to_owned());
    assert_eq!(denied("SELECT * FROM sessions", None).await, perm);
    assert_eq!(denied("SELECT * FROM oidc_login_states", None).await, perm);
    assert_eq!(
        denied("UPDATE reference_counters SET next_number = 1", None).await,
        perm
    );
    assert_eq!(
        denied(
            "INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id) VALUES ($1, app_user_id(), 'x', 'y', app_user_id())",
            Some(ws_id),
        )
        .await,
        perm
    );
    assert_eq!(
        denied("INSERT INTO documents (workspace_id, placement, scope, number, title, schema_version, body, revision, created_by, updated_by, request_id) VALUES ($1, 'wiki', 'WIKI', 99, 't', 1, '{}', 1, app_user_id(), app_user_id(), $1)", Some(ws_id)).await,
        perm
    );
    // 리비전은 1씩만 오른다(되감기·건너뛰기 거부).
    assert_eq!(
        denied(
            "UPDATE documents SET revision = revision - 1 WHERE id = $1",
            Some(doc_id)
        )
        .await,
        Some("23514".to_owned())
    );
}

#[tokio::test]
async fn duplicate_session_cookies_are_rejected() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let mallory = login(&state, "mallory").await;
    let tossed = common::TestUser {
        id: alice.id,
        cookie: format!("{}; {}", alice.cookie, mallory.cookie),
    };
    assert_eq!(
        send(&app, Method::GET, "/api/me", Some(&tossed), None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
}
