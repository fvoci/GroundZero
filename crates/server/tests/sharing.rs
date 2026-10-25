#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 개인 노트·명시적 공유·문서별 비공개·복제·이동 (D-20, D-22, D-23, D-24, D-25, D-40, D-49, D-57, D-59).

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{
    TestUser, add_member, create_doc, create_workspace, doc_body, login, login_with_email,
    new_request_id, send, unique_email,
};
use serde_json::{Value, json};
use sqlx::Connection;

async fn create_note(app: &Router, user: &TestUser, title: &str) -> Value {
    let r = send(
        app,
        Method::POST,
        "/api/me/notes",
        Some(user),
        Some(json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

async fn save(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    base: i64,
    text: &str,
) -> (StatusCode, Value) {
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let r = send(
        app,
        Method::PUT,
        &uri,
        Some(user),
        Some(json!({ "request_id": new_request_id(), "base_revision": base, "title": doc["title"], "body": doc_body(text) })),
    )
    .await;
    (r.status, r.json)
}

async fn get(app: &Router, user: &TestUser, doc: &Value) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::GET,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(user),
        None,
    )
    .await;
    (r.status, r.json)
}

async fn share(
    app: &Router,
    owner: &TestUser,
    doc: &Value,
    email: &str,
    permission: &str,
) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/documents/{}/shares", doc["id"].as_str().unwrap()),
        Some(owner),
        Some(json!({ "email": email, "permission": permission })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

async fn settings(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    visibility: &str,
    allow_copy: bool,
) -> StatusCode {
    send(
        app,
        Method::PUT,
        &format!("/api/documents/{}/settings", doc["id"].as_str().unwrap()),
        Some(user),
        Some(json!({ "visibility": visibility, "allow_copy": allow_copy })),
    )
    .await
    .status
}

#[tokio::test]
async fn private_team_document_is_visible_to_creator_admin_and_shares_only() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "비공개").await;
    let (creator, _) = add_member(&app, &state, &admin, &ws, "creator", "member").await;
    let (other, other_email) = add_member(&app, &state, &admin, &ws, "other", "member").await;
    let doc = create_doc(&app, &creator, &ws, "작성자 비공개").await;

    // 작성자는 소유 권한이 아니므로 설정을 바꿀 수 없다. 관리자는 바꿀 수 있다.
    assert_eq!(
        settings(&app, &creator, &doc, "private", false).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        settings(&app, &admin, &doc, "private", false).await,
        StatusCode::OK
    );

    assert_eq!(
        get(&app, &admin, &doc).await.0,
        StatusCode::OK,
        "admin reads private docs (D-40)"
    );
    assert_eq!(get(&app, &creator, &doc).await.0, StatusCode::OK);
    assert_eq!(get(&app, &other, &doc).await.0, StatusCode::NOT_FOUND);
    let list = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&other),
        None,
    )
    .await;
    assert!(
        list.json
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["id"] != doc["id"])
    );

    // 명시적 공유로 다시 볼 수 있다.
    share(&app, &admin, &doc, &other_email, "read").await;
    let (status, body) = get(&app, &other, &doc).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access"], "read");
}

#[tokio::test]
async fn share_permissions_and_revocation_take_effect_immediately() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob_email = unique_email("bob");
    let bob = login_with_email(&state, "bob", &bob_email, true).await;
    let note = create_note(&app, &alice, "앨리스 노트").await;
    assert_eq!(note["placement"], "personal");
    assert_eq!(note["reference"], Value::Null);
    assert_eq!(get(&app, &bob, &note).await.0, StatusCode::NOT_FOUND);

    let s = share(&app, &alice, &note, &bob_email.to_uppercase(), "read").await;
    let (status, body) = get(&app, &bob, &note).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["can_edit"], false);
    assert_eq!(
        save(&app, &bob, &note, 1, "밥이 고침").await.0,
        StatusCode::FORBIDDEN
    );
    // 공유받은 사람은 공유 목록을 관리할 수 없다.
    let shares = send(
        &app,
        Method::GET,
        &format!("/api/documents/{}/shares", note["id"].as_str().unwrap()),
        Some(&bob),
        None,
    )
    .await;
    assert_eq!(shares.status, StatusCode::FORBIDDEN);

    // 쓰기로 올리면 원본에 반영된다 (D-23).
    share(&app, &alice, &note, &bob_email, "write").await;
    let (status, saved) = save(&app, &bob, &note, 1, "밥이 고침").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["revision"], 2);
    assert_eq!(
        get(&app, &alice, &note).await.1["body"],
        doc_body("밥이 고침")
    );

    let mine = send(&app, Method::GET, "/api/me/shared", Some(&bob), None).await;
    assert_eq!(mine.json[0]["shared_by"], "alice");
    assert_eq!(mine.json[0]["permission"], "write");

    // 철회 직후 접근이 막힌다.
    let r = send(
        &app,
        Method::DELETE,
        &format!("/api/document-shares/{}", s["id"].as_str().unwrap()),
        Some(&alice),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    assert_eq!(get(&app, &bob, &note).await.0, StatusCode::NOT_FOUND);
    assert!(
        send(&app, Method::GET, "/api/me/shared", Some(&bob), None)
            .await
            .json
            .as_array()
            .unwrap()
            .is_empty()
    );

    // 같은 이메일이라도 제공자가 검증하지 않은 계정은 공유 대상이 아니다.
    share(&app, &alice, &note, &bob_email, "read").await;
    let unverified = login_with_email(&state, "bob-unverified", &bob_email, false).await;
    assert_eq!(get(&app, &unverified, &note).await.0, StatusCode::NOT_FOUND);
}

/// B에게 공유한 개인 노트는 B가 속한 워크스페이스의 관리자에게 보이지 않는다 (D-49).
#[tokio::test]
async fn shared_personal_note_does_not_reach_the_grantees_workspace_admin() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let boss = login(&state, "boss").await;
    let ws = create_workspace(&app, &boss, "B의 회사").await;
    let (bob, bob_email) = add_member(&app, &state, &boss, &ws, "bob", "member").await;

    let note = create_note(&app, &alice, "앨리스 개인 노트").await;
    share(&app, &alice, &note, &bob_email, "write").await;
    assert_eq!(get(&app, &bob, &note).await.0, StatusCode::OK);
    assert_eq!(get(&app, &boss, &note).await.0, StatusCode::NOT_FOUND);

    // DB 수준 반례: 앱 역할로 관리자 문맥을 설정해도 행이 없다.
    let url = std::env::var("DATABASE_URL").unwrap();
    let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(boss.id.0.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id = $1::uuid")
        .bind(note["id"].as_str().unwrap())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 0);

    // 앨리스의 개인 노트 목록은 앨리스에게만 있다.
    let boss_notes = send(&app, Method::GET, "/api/me/notes", Some(&boss), None).await;
    assert!(boss_notes.json.as_array().unwrap().is_empty());
    let alice_notes = send(&app, Method::GET, "/api/me/notes", Some(&alice), None).await;
    assert_eq!(alice_notes.json[0]["id"], note["id"]);
}

#[tokio::test]
async fn copy_requires_permission_and_is_independent() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let bob_email = unique_email("bob");
    let bob = login_with_email(&state, "bob", &bob_email, true).await;
    let note = create_note(&app, &alice, "원본").await;
    let (_, saved) = save(&app, &alice, &note, 1, "원본 내용").await;
    share(&app, &alice, &note, &bob_email, "read").await;

    let copy_uri = format!("/api/documents/{}/copy", note["id"].as_str().unwrap());
    let body = json!({ "request_id": new_request_id(), "target_workspace_id": null, "title": "밥의 사본" });
    assert_eq!(
        send(
            &app,
            Method::POST,
            &copy_uri,
            Some(&bob),
            Some(body.clone())
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        settings(&app, &alice, &note, "private", true).await,
        StatusCode::OK
    );
    let copied = send(
        &app,
        Method::POST,
        &copy_uri,
        Some(&bob),
        Some(body.clone()),
    )
    .await;
    assert_eq!(copied.status, StatusCode::CREATED);
    assert_ne!(copied.json["id"], note["id"]);
    assert_eq!(copied.json["revision"], 1);
    assert_eq!(copied.json["body"], doc_body("원본 내용"));
    assert_eq!(copied.json["access"], "owner");
    // 재시도는 같은 사본을 돌려준다.
    let retry = send(&app, Method::POST, &copy_uri, Some(&bob), Some(body)).await;
    assert_eq!(retry.json["id"], copied.json["id"]);

    // 원본을 고쳐도 사본은 바뀌지 않는다.
    save(
        &app,
        &alice,
        &note,
        saved["revision"].as_i64().unwrap(),
        "원본만 바뀜",
    )
    .await;
    assert_eq!(
        get(&app, &bob, &copied.json).await.1["body"],
        doc_body("원본 내용")
    );
    // 사본에는 앨리스가 접근하지 못한다(독립된 밥의 노트).
    assert_eq!(
        get(&app, &alice, &copied.json).await.0,
        StatusCode::NOT_FOUND
    );
}

/// 미리보기 → 그 미리보기를 확인한 채로 이동. (상태, 응답) 반환.
async fn move_doc(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    target: Option<&str>,
) -> (StatusCode, Value) {
    let uri = format!("/api/documents/{}/move", doc["id"].as_str().unwrap());
    let preview = send(
        app,
        Method::POST,
        &uri,
        Some(user),
        Some(json!({ "target_workspace_id": target })),
    )
    .await;
    // 409라도 미리보기가 아니라 상태 오류(예: 팀 문서 → 개인)면 그대로 돌려준다.
    if preview.status != StatusCode::CONFLICT || preview.json.get("gains").is_none() {
        return (preview.status, preview.json);
    }
    let ack = json!({ "gains": preview.json["gains"], "loses": preview.json["loses"] });
    let r = send(
        app,
        Method::POST,
        &uri,
        Some(user),
        Some(json!({ "target_workspace_id": target, "acknowledged": ack })),
    )
    .await;
    (r.status, r.json)
}

#[tokio::test]
async fn move_requires_the_reviewed_preview_and_changes_who_can_see() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let src = create_workspace(&app, &alice, "출발").await;
    let dst = create_workspace(&app, &alice, "도착").await;
    let (src_reader, _) = add_member(&app, &state, &alice, &src, "src-reader", "readonly").await;
    let (dst_writer, _) = add_member(&app, &state, &alice, &dst, "dst-writer", "member").await;
    let doc = create_doc(&app, &alice, &src, "옮길 문서").await;
    assert_eq!(get(&app, &src_reader, &doc).await.0, StatusCode::OK);

    let uri = format!("/api/documents/{}/move", doc["id"].as_str().unwrap());
    let preview = send(
        &app,
        Method::POST,
        &uri,
        Some(&alice),
        Some(json!({ "target_workspace_id": dst })),
    )
    .await;
    assert_eq!(preview.status, StatusCode::CONFLICT);
    assert_eq!(preview.json["loses"], json!(["src-reader"]));
    assert_eq!(preview.json["gains"], json!(["dst-writer"]));
    assert_eq!(preview.json["history_moves_with_document"], true);

    // 확인한 미리보기가 지금과 다르면(그사이 멤버가 바뀐 경우 등) 옮기지 않고 새 미리보기를 준다.
    let stale = send(
        &app,
        Method::POST,
        &uri,
        Some(&alice),
        Some(json!({ "target_workspace_id": dst, "acknowledged": { "gains": [], "loses": [] } })),
    )
    .await;
    assert_eq!(stale.status, StatusCode::CONFLICT);
    assert_eq!(
        get(&app, &src_reader, &doc).await.0,
        StatusCode::OK,
        "not moved"
    );

    let (status, moved) = move_doc(&app, &alice, &doc, Some(&dst)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["workspace_id"], dst.as_str());
    assert_eq!(moved["reference"], "WIKI-1");
    assert_eq!(
        moved["document"]["revision"], doc["revision"],
        "moving does not create a revision"
    );
    assert_eq!(get(&app, &src_reader, &doc).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, &dst_writer, &doc).await.0, StatusCode::OK);

    // 팀 문서를 개인 노트로 빼는 이동은 막는다 (D-25).
    assert_eq!(
        move_doc(&app, &alice, &doc, None).await.0,
        StatusCode::CONFLICT
    );
    // 대상 워크스페이스에서 작성 권한이 없는 사람은 옮길 수 없다.
    let note = create_note(&app, &src_reader, "독자의 노트").await;
    assert_eq!(
        move_doc(&app, &src_reader, &note, Some(&src)).await.0,
        StatusCode::FORBIDDEN
    );
}

/// 비공개 문서를 작성 권한만 있는 워크스페이스로 옮기면 옮긴 사람이 볼 수 없게 될 수 있다. 이동은 완료되고
/// 응답은 새 위치와 참조만 준다(검토에서 찾은 404·롤백 결함의 회귀 테스트).
#[tokio::test]
async fn moving_a_private_document_where_the_mover_loses_access_still_completes() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let src = create_workspace(&app, &alice, "출발").await;
    let (creator, _) = add_member(&app, &state, &alice, &src, "creator", "member").await;
    let boss = login(&state, "boss").await;
    let dst = create_workspace(&app, &boss, "남의 팀").await;
    let alice_email = unique_email("alice-dst");
    let alice_dst = login_with_email(&state, "alice-dst", &alice_email, true).await;
    let _ = alice_dst;
    // 앨리스를 대상 워크스페이스의 작성자로 들인다(같은 사람이 다른 이메일로 두 번 로그인하는 대신 초대를 쓴다).
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{dst}/invitations"),
        Some(&boss),
        Some(json!({ "email": alice_email, "role": "member" })),
    )
    .await;
    let alice_with_email = login_with_email(&state, "alice-src", &alice_email, true).await;
    send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv.json["id"].as_str().unwrap()
        ),
        Some(&alice_with_email),
        None,
    )
    .await;
    // alice_with_email을 출발 워크스페이스의 관리자로 만든다.
    let inv2 = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{src}/invitations"),
        Some(&alice),
        Some(json!({ "email": alice_email, "role": "admin" })),
    )
    .await;
    send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv2.json["id"].as_str().unwrap()
        ),
        Some(&alice_with_email),
        None,
    )
    .await;

    let doc = create_doc(&app, &creator, &src, "작성자 비공개").await;
    assert_eq!(
        settings(&app, &alice_with_email, &doc, "private", false).await,
        StatusCode::OK
    );
    let (status, moved) = move_doc(&app, &alice_with_email, &doc, Some(&dst)).await;
    assert_eq!(status, StatusCode::OK, "{moved:?}");
    assert_eq!(moved["workspace_id"], dst.as_str());
    assert_eq!(moved["document"], Value::Null);
    assert_eq!(
        get(&app, &boss, &doc).await.0,
        StatusCode::OK,
        "target admin sees it"
    );
    assert_eq!(
        get(&app, &alice_with_email, &doc).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn personal_note_moves_into_a_workspace_as_private() {
    let state = common::state();
    let app = common::app(&state);
    let boss = login(&state, "boss").await;
    let ws = create_workspace(&app, &boss, "팀").await;
    let alice_email = unique_email("alice2");
    let alice2 = login_with_email(&state, "alice2", &alice_email, true).await;
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(&boss),
        Some(json!({ "email": alice_email, "role": "member" })),
    )
    .await;
    send(
        &app,
        Method::POST,
        &format!(
            "/api/invitations/{}/accept",
            inv.json["id"].as_str().unwrap()
        ),
        Some(&alice2),
        None,
    )
    .await;
    let (peer, _) = add_member(&app, &state, &boss, &ws, "peer", "member").await;

    let note = create_note(&app, &alice2, "개인 → 팀").await;
    let uri = format!("/api/documents/{}/move", note["id"].as_str().unwrap());
    let preview = send(
        &app,
        Method::POST,
        &uri,
        Some(&alice2),
        Some(json!({ "target_workspace_id": ws })),
    )
    .await;
    // 개인 노트는 비공개로 들어가므로 관리자만 새로 볼 수 있다.
    assert_eq!(preview.json["gains"], json!(["boss"]));
    let (status, moved) = move_doc(&app, &alice2, &note, Some(&ws)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(moved["document"]["placement"], "wiki");
    assert_eq!(moved["document"]["visibility"], "private");
    assert_eq!(get(&app, &boss, &note).await.0, StatusCode::OK);
    assert_eq!(get(&app, &peer, &note).await.0, StatusCode::NOT_FOUND);
}

/// 비공개 문서·개인 노트를 워크스페이스로 복제하면 사본도 비공개다.
#[tokio::test]
async fn copies_of_private_documents_stay_private() {
    let state = common::state();
    let app = common::app(&state);
    let boss = login(&state, "boss").await;
    let ws = create_workspace(&app, &boss, "팀").await;
    let (member, member_email) = add_member(&app, &state, &boss, &ws, "member", "member").await;
    let (peer, _) = add_member(&app, &state, &boss, &ws, "peer", "member").await;
    let outsider = login(&state, "outsider").await;
    let note = create_note(&app, &outsider, "외부 노트").await;
    share(&app, &outsider, &note, &member_email, "read").await;
    settings(&app, &outsider, &note, "private", true).await;

    let r = send(
        &app,
        Method::POST,
        &format!("/api/documents/{}/copy", note["id"].as_str().unwrap()),
        Some(&member),
        Some(json!({ "request_id": new_request_id(), "target_workspace_id": ws, "title": "사본" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED);
    assert_eq!(r.json["visibility"], "private");
    assert_eq!(get(&app, &peer, &r.json).await.0, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, &boss, &r.json).await.0, StatusCode::OK);
}

/// 내용 변경 없이 리비전만 올려 가짜 리비전 기록을 끼워 넣을 수 없다.
#[tokio::test]
async fn revision_cannot_advance_without_content_change() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "리비전").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let url = std::env::var("DATABASE_URL").unwrap();
    let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(alice.id.0.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let err = sqlx::query("UPDATE documents SET revision = revision + 1, updated_by = app_user_id() WHERE id = $1::uuid")
        .bind(doc["id"].as_str().unwrap())
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("23514")
    );
}

#[tokio::test]
async fn settings_need_definer_path_and_personal_audit_is_visible_to_owner() {
    let state = common::state();
    let app = common::app(&state);
    let alice = login(&state, "alice").await;
    let ws = create_workspace(&app, &alice, "설정").await;
    let doc = create_doc(&app, &alice, &ws, "문서").await;
    let note = create_note(&app, &alice, "노트").await;
    assert_eq!(
        settings(&app, &alice, &note, "workspace", false).await,
        StatusCode::BAD_REQUEST,
        "personal notes stay private"
    );

    // 앱 역할은 공개 범위·복제 허용을 직접 바꿀 수 없다(감사 기록 없는 변경 방지).
    let url = std::env::var("DATABASE_URL").unwrap();
    let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
    let mut tx = conn.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.user_id', $1, true)")
        .bind(alice.id.0.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let err = sqlx::query("UPDATE documents SET visibility = 'private' WHERE id = $1::uuid")
        .bind(doc["id"].as_str().unwrap())
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().and_then(|e| e.code()).as_deref(),
        Some("42501")
    );
    drop(tx);

    let bob_email = unique_email("bob");
    share(&app, &alice, &note, &bob_email, "read").await;
    let mut tx = state.db.begin_as(alice.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE target_id = $1::uuid AND owner_user_id = app_user_id() ORDER BY created_at, id",
    )
    .bind(note["id"].as_str().unwrap())
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(actions, vec!["create", "share"]);
}

async fn export(
    app: &Router,
    user: &TestUser,
    doc: &Value,
    revision: i64,
    format: &str,
) -> (StatusCode, Value) {
    let r = send(
        app,
        Method::POST,
        &format!(
            "/api/documents/{}/revisions/{revision}/export",
            doc["id"].as_str().unwrap()
        ),
        Some(user),
        Some(json!({ "format": format })),
    )
    .await;
    (r.status, r.json)
}

/// 문서 한 건 내보내기(D-75): 첨부 내려받기와 같은 권한. 작성자·관리자는 되고, 독자는 소유자가 복제·내려받기를
/// 허용했을 때만, 볼 수 없는 사람은 404. 고른 저장 리비전의 내용을 주고 내보내기 기록을 남긴다. 개인 노트도 같은
/// 규칙이며 기록은 노트 소유자의 개인 영역에 남는다.
#[tokio::test]
async fn export_follows_download_permission_and_is_audited() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "내보내기").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let (reader, _) = add_member(&app, &state, &admin, &ws, "readonly", "readonly").await;
    let outsider = login(&state, "outsider").await;
    let doc = create_doc(&app, &writer, &ws, "보고서").await;
    assert_eq!(
        save(&app, &writer, &doc, 1, "첫 저장").await.0,
        StatusCode::OK
    );
    assert_eq!(
        save(&app, &writer, &doc, 2, "둘째 저장").await.0,
        StatusCode::OK
    );
    assert_eq!(
        settings(&app, &admin, &doc, "workspace", false).await,
        StatusCode::OK
    );

    let (status, rev) = export(&app, &writer, &doc, 2, "markdown").await;
    assert_eq!(status, StatusCode::OK, "{rev:?}");
    assert_eq!(rev["revision"], 2);
    assert!(rev["body"].to_string().contains("첫 저장"), "{rev:?}");
    assert_eq!(export(&app, &admin, &doc, 3, "pdf").await.0, StatusCode::OK);
    assert_eq!(get(&app, &reader, &doc).await.1["can_export"], false);
    assert_eq!(
        export(&app, &reader, &doc, 3, "zip").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        export(&app, &outsider, &doc, 3, "zip").await.0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        export(&app, &writer, &doc, 99, "markdown").await.0,
        StatusCode::NOT_FOUND
    );
    assert!(
        export(&app, &writer, &doc, 3, "docx")
            .await
            .0
            .is_client_error()
    );

    assert_eq!(
        settings(&app, &admin, &doc, "workspace", true).await,
        StatusCode::OK
    );
    assert_eq!(get(&app, &reader, &doc).await.1["can_export"], true);
    assert_eq!(
        export(&app, &reader, &doc, 3, "zip").await.0,
        StatusCode::OK
    );

    // 개인 노트: 공유받은 독자는 소유자가 허용해야 내보낸다. 기록은 소유자 개인 영역.
    let bob_email = unique_email("bob");
    let bob = login_with_email(&state, "bob", &bob_email, true).await;
    let note = create_note(&app, &writer, "개인 노트").await;
    share(&app, &writer, &note, &bob_email, "read").await;
    assert_eq!(
        export(&app, &bob, &note, 1, "markdown").await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        settings(&app, &writer, &note, "private", true).await,
        StatusCode::OK
    );
    assert_eq!(
        export(&app, &bob, &note, 1, "markdown").await.0,
        StatusCode::OK
    );

    let owner_url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let mut owner = sqlx::PgConnection::connect(&owner_url).await.unwrap();
    let doc_audits: Vec<(Option<uuid::Uuid>, Value)> = sqlx::query_as(
        "SELECT workspace_id, detail FROM fvoci.audit_log
         WHERE action = 'export' AND target_type = 'document' AND target_id = $1::uuid ORDER BY created_at",
    )
    .bind(doc["id"].as_str().unwrap())
    .fetch_all(&mut owner)
    .await
    .unwrap();
    assert_eq!(doc_audits.len(), 3, "{doc_audits:?}");
    assert_eq!(
        doc_audits[0].1,
        json!({ "revision": 2, "format": "markdown" })
    );
    assert!(doc_audits.iter().all(|(w, _)| w.is_some()));
    let note_audit: (Option<uuid::Uuid>, uuid::Uuid, uuid::Uuid) = sqlx::query_as(
        "SELECT workspace_id, owner_user_id, actor FROM fvoci.audit_log
         WHERE action = 'export' AND target_id = $1::uuid",
    )
    .bind(note["id"].as_str().unwrap())
    .fetch_one(&mut owner)
    .await
    .unwrap();
    assert_eq!(note_audit, (None, writer.id.0, bob.id.0));
}

/// 감사 W6: 문서 설정은 두 칸을 늘 함께 받아, 한 칸만 바꾼 화면이 연 뒤 다시 읽지 않은 다른 칸을 함께 보내 다른 소유
/// 권한자가 켠 비공개를 되돌렸다. 바꾸는 칸만 보내고, 보내지 않은 칸은 잠근 뒤의 지금 값 그대로다.
#[tokio::test]
async fn document_settings_change_only_the_fields_sent() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "설정 한 칸").await;
    let (second, _) = add_member(&app, &state, &admin, &ws, "second", "admin").await;
    let doc = create_doc(&app, &admin, &ws, "회의록").await;
    let uri = format!("/api/documents/{}/settings", doc["id"].as_str().unwrap());

    // 관리자가 비공개로 바꾼다. 다른 관리자의 화면은 아직 공개로 열려 있고, 복제 허용만 켠다.
    assert_eq!(
        settings(&app, &admin, &doc, "private", false).await,
        StatusCode::OK
    );
    let copy = send(
        &app,
        Method::PUT,
        &uri,
        Some(&second),
        Some(json!({ "allow_copy": true })),
    )
    .await;
    assert_eq!(copy.status, StatusCode::OK, "{:?}", copy.json);
    assert_eq!(copy.json["visibility"], "private");
    assert_eq!(copy.json["allow_copy"], true);
    // 공개 범위만 보내면 복제 허용은 그대로다.
    let open = send(
        &app,
        Method::PUT,
        &uri,
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": null })),
    )
    .await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.json);
    assert_eq!(open.json["visibility"], "workspace");
    assert_eq!(open.json["allow_copy"], true);
    // 개인 노트는 여전히 비공개만 된다.
    let note = create_note(&app, &admin, "메모").await;
    let note_uri = format!("/api/documents/{}/settings", note["id"].as_str().unwrap());
    let r = send(
        &app,
        Method::PUT,
        &note_uri,
        Some(&admin),
        Some(json!({ "allow_copy": true })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["visibility"], "private");
    let r = send(
        &app,
        Method::PUT,
        &note_uri,
        Some(&admin),
        Some(json!({ "visibility": "workspace" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{:?}", r.json);
}
