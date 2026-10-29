//! SR-243-6: 두 번째 보류 조회에 새로 들어온 개인 노트의 document hold 충돌을 가린다.

use super::*;
use sqlx::{Connection, PgConnection, Postgres, Transaction};
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
enum Visibility {
    Hidden,
    Readable,
}

async fn shared_note(f: &Fixture, visibility: Visibility) -> String {
    let org = common::organization_of_workspace(&f.workspace).await;
    let email = common::unique_email("include-recipient");
    let recipient = common::login_with_email(&f.state, "note recipient", &email, true).await;
    common::join_organization(org, &recipient, "member").await;
    let note = send(
        &f.app,
        Method::POST,
        "/api/me/notes",
        Some(&f.writer),
        Some(json!({"request_id": common::new_request_id(), "title": "두 번째 조회 노트"})),
    )
    .await;
    assert_eq!(note.status, StatusCode::CREATED, "{:?}", note.json);
    let id = note.json["id"].as_str().unwrap().to_owned();
    let mut emails = vec![email];
    if matches!(visibility, Visibility::Readable) {
        let mut conn = common::fixture_conn().await;
        emails.push(
            sqlx::query_scalar("SELECT email FROM fvoci.users WHERE id=$1")
                .bind(f.owner.id)
                .fetch_one(&mut conn)
                .await
                .unwrap(),
        );
    }
    for email in emails {
        let shared = send(
            &f.app,
            Method::POST,
            &format!("/api/documents/{id}/shares"),
            Some(&f.writer),
            Some(json!({"email": email, "permission": "read", "organization_id": org})),
        )
        .await;
        assert_eq!(shared.status, StatusCode::CREATED, "{:?}", shared.json);
    }
    let access = send(
        &f.app,
        Method::GET,
        &format!("/api/documents/{id}"),
        Some(&f.owner),
        None,
    )
    .await;
    assert_eq!(
        access.status,
        match visibility {
            Visibility::Hidden => StatusCode::NOT_FOUND,
            Visibility::Readable => StatusCode::OK,
        },
        "{:?}",
        access.json
    );
    id
}

async fn lock_document<'conn>(
    conn: &'conn mut PgConnection,
    document: &str,
) -> (Transaction<'conn, Postgres>, i32) {
    let mut lock = conn.begin().await.unwrap();
    let holder = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    sqlx::query("SELECT id FROM fvoci.documents WHERE id=$1::uuid FOR UPDATE")
        .bind(document)
        .execute(&mut *lock)
        .await
        .unwrap();
    (lock, holder)
}

async fn include_conflict(visibility: Visibility) {
    let mut f = Fixture::new().await;
    let org = common::organization_of_workspace(&f.workspace).await;
    let (caller, _) = common::add_member(
        &f.app,
        &f.state,
        &f.owner,
        &f.workspace,
        "include-admin",
        "admin",
    )
    .await;
    common::join_organization(org, &caller, "admin").await;
    f.owner = caller;
    let note = shared_note(&f, visibility).await;
    // 개인 노트의 방은 공간/조직 hold 목록에 없고 아직 보류 편집도 없다.
    let mut personal = Client::connect(f.addr, &note, &f.writer).await;
    let mut workspace = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(workspace.edit_and_wait(PENDING).await);
    let mut workspace_conn = common::fixture_conn().await;
    let (workspace_lock, workspace_holder) = lock_document(&mut workspace_conn, &f.document).await;
    let removal = tokio::spawn({
        let (app, owner, member) = (f.app.clone(), f.owner.clone(), f.writer.id);
        async move {
            send(
                &app,
                Method::DELETE,
                &format!("/api/organizations/{org}/members/{}", member.0),
                Some(&owner),
                None,
            )
            .await
        }
    });
    let mut observer = common::fixture_conn().await;
    timeouts::blocked_backend(&mut observer, workspace_holder).await;
    // 첫 조회와 hold 목록이 확정된 뒤에만 개인 노트가 보류 상태가 된다.
    assert!(personal.edit_and_wait(PENDING).await);
    let before: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM fvoci.document_collab_states s WHERE document_id=$1::uuid",
    )
    .bind(&note)
    .fetch_one(&mut observer)
    .await
    .unwrap();
    assert_eq!(before["pending"], true);
    let mut note_conn = common::fixture_conn().await;
    let (note_lock, note_holder) = lock_document(&mut note_conn, &note).await;
    let restoring = tokio::spawn({
        let (app, owner, document) = (f.app.clone(), f.writer.clone(), note.clone());
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/documents/{document}/revisions/1/restore"),
                Some(&owner),
                Some(json!({"request_id": common::new_request_id(), "base_revision": 1})),
            )
            .await
        }
    });
    timeouts::blocked_backend(&mut observer, note_holder).await;
    // 소유자 복원이 document hold를 잡은 채 기다릴 때 두 번째 조회를 진행시킨다.
    workspace_lock.rollback().await.unwrap();
    let blocked = tokio::time::timeout(Duration::from_secs(3), removal)
        .await
        .expect("second pending lookup did not finish behind the document hold")
        .unwrap();
    assert_eq!(blocked.status, StatusCode::CONFLICT, "{:?}", blocked.json);
    assert!(
        !restoring.is_finished(),
        "owner restore no longer holds the note"
    );
    let after: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM fvoci.document_collab_states s WHERE document_id=$1::uuid",
    )
    .bind(&note)
    .fetch_one(&mut observer)
    .await
    .unwrap();
    assert_eq!(after, before, "include failure changed the pending note");
    let membership: (bool, bool) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2),
                EXISTS(SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$3::uuid AND user_id=$2)",
    )
    .bind(org)
    .bind(f.writer.id)
    .bind(&f.workspace)
    .fetch_one(&mut observer)
    .await
    .unwrap();
    assert_eq!(membership, (true, true), "409 removed the member");
    assert_eq!(f.current().await["body"], common::doc_body(PENDING));
    let current = send(
        &f.app,
        Method::GET,
        &format!("/api/documents/{note}"),
        Some(&f.writer),
        None,
    )
    .await;
    assert_eq!(current.status, StatusCode::OK, "{:?}", current.json);
    assert_eq!(current.json["revision"], 1, "409 revised the personal note");
    note_lock.rollback().await.unwrap();
    let restored = restoring.await.unwrap();
    // 소유자의 별도 명령은 편집을 먼저 저장하여 오래된 기준 판과 충돌한다.
    assert_eq!(restored.status, StatusCode::CONFLICT, "{:?}", restored.json);
    assert_eq!(personal.body(), common::doc_body(PENDING));
    assert!(
        personal
            .edit_and_wait("include 거절 뒤 보존한 개인 편집")
            .await
    );
    let mut rejoined = Client::connect(f.addr, &f.document, &f.writer).await;
    assert_eq!(rejoined.body(), common::doc_body(PENDING));
    rejoined.close().await;
    personal.close().await;
    workspace.close().await;
    let workspace_id = f.workspace.clone();
    f.stop().await;
    let body = blocked.json.to_string();
    assert!(!body.contains(&note), "note id leaked: {body}");
    assert!(!body.contains(&workspace_id), "workspace id leaked: {body}");
    assert!(!body.contains("personal note"), "note kind leaked: {body}");
    assert_eq!(
        blocked.json["message"],
        match visibility {
            Visibility::Hidden =>
                "an item you cannot open has pending edits; nothing was changed. Ask its owner or an editor to save the edits and retry",
            Visibility::Readable => "the document is being changed; retry after reconnecting",
        },
        "{visibility:?}: include failure returned a distinguishable conflict"
    );
}

#[tokio::test]
async fn hidden_personal_note_hold_in_second_lookup_returns_generic_conflict() {
    include_conflict(Visibility::Hidden).await;
}

#[tokio::test]
async fn readable_personal_note_hold_in_second_lookup_keeps_original_conflict() {
    include_conflict(Visibility::Readable).await;
}
