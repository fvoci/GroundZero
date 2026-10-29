//! #233: SQL 직접 제거 뒤의 체크포인트 거절과 보류 편집 보존, HTTP prepare의 선행 체크포인트를 구분한다.

use super::*;
use crate::sql_support::*;
use uuid::Uuid;

#[tokio::test]
async fn live_personal_note_checkpoint_waits_for_organization_removal() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let app = common::app(&w.state);
    let addr = serve(app.clone()).await;
    let mut client = connect(addr, Some(&editor), &note.to_string(), true)
        .await
        .unwrap();
    client.paragraph("revoked checkpoint").await;
    assert!(client.until(acked).await);
    let mut removal = w.state.db.begin_as(w.owner.id).await.unwrap();
    on(
        removal.conn(),
        "SELECT fvoci.app_remove_organization_member_ids($1,$2)",
        &[w.org, editor.id.0],
    )
    .await
    .unwrap();
    let holder = pid(removal.conn()).await;
    // 마지막 연결 종료가 실제 write_snapshot → lock_for_save를 실행한다.
    client.close().await;
    until_waiting_on_lock("app_lock_document_workspace", holder).await;
    on(
        removal.conn(),
        "SELECT fvoci.app_remove_organization_member($1,$2,@ids)",
        &[w.org, editor.id.0],
    )
    .await
    .unwrap();
    removal.commit().await.unwrap();
    until_no_room(&w.state, &note.to_string()).await;
    let current = get_doc(&app, &w.owner, &note.to_string()).await;
    // SQL 직접 호출은 HTTP의 #243 prepare를 건너뛴다. 철회된 편집자의 새 리비전은 거절하지만,
    // acked로 확인한 CRDT는 아래 보류 상태에 남아 소유자가 다시 열 때 이어받는다(D-95·D-99).
    assert_eq!(current["revision"], 1);
    assert_eq!(current["body"], json!({"type":"doc","content":[]}));
    let reopened = connect(addr, Some(&editor), &note.to_string(), true).await;
    assert!(matches!(reopened, Err(StatusCode::NOT_FOUND)));

    let mut fixture = common::fixture_conn().await;
    let (pending, revision, state, editors): (bool, i32, Vec<u8>, Vec<Uuid>) = sqlx::query_as(
        "SELECT pending, revision, state, editors FROM fvoci.document_collab_states WHERE document_id=$1",
    )
    .bind(note)
    .fetch_one(&mut fixture)
    .await
    .unwrap();
    assert!(pending, "acknowledged edits must remain pending");
    assert_eq!(revision, 1);
    assert_eq!(editors, vec![editor.id.0]);
    let stored = Doc::new();
    stored
        .transact_mut()
        .apply_update(Update::decode_v1(&state).unwrap())
        .unwrap();
    let fragment = stored.get_or_insert_xml_fragment(FRAGMENT);
    assert_eq!(
        read_doc(&stored.transact(), &fragment),
        common::doc_body("revoked checkpoint")
    );

    // 실제 재접속도 보류 편집을 읽는다. 소유자가 연결된 동안 기존 체크포인트 경로로 리비전을 남긴다.
    let owner = connect(addr, Some(&w.owner), &note.to_string(), true)
        .await
        .unwrap();
    assert!(owner.text().contains("revoked checkpoint"));
    w.state
        .collab
        .snapshot_documents(&std::collections::HashSet::from([
            fvoci_core::ids::DocumentId(note),
        ]))
        .await;
    owner.close().await;
    until_no_room(&w.state, &note.to_string()).await;
    let recovered = get_doc(&app, &w.owner, &note.to_string()).await;
    assert_eq!(recovered["revision"], 2);
    assert_eq!(recovered["body"], common::doc_body("revoked checkpoint"));
    let pending: bool =
        sqlx::query_scalar("SELECT pending FROM fvoci.document_collab_states WHERE document_id=$1")
            .bind(note)
            .fetch_one(&mut fixture)
            .await
            .unwrap();
    assert!(!pending, "recovered edits must have a saved revision");
}

#[tokio::test]
async fn http_organization_removal_checkpoints_acknowledged_personal_note_edits() {
    let w = World::new().await;
    let editor = w.member("member").await;
    let note = w.note().await;
    w.share(note, &editor, "write").await;
    let app = common::app(&w.state);
    let addr = serve(app.clone()).await;
    let mut client = connect(addr, Some(&editor), &note.to_string(), true)
        .await
        .unwrap();
    client.paragraph("http removal checkpoint").await;
    assert!(client.until(acked).await);
    // 실제 HTTP 제거는 prepare의 체크포인트가 성공한 뒤에만 멤버십·공유를 회수한다(#243).
    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/organizations/{}/members/{}", w.org, editor.id.0),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT, "{:?}", removed.json);
    until_no_room(&w.state, &note.to_string()).await;
    let current = get_doc(&app, &w.owner, &note.to_string()).await;
    assert_eq!(current["revision"], 2);
    assert_eq!(current["body"], common::doc_body("http removal checkpoint"));
    let mut fixture = common::fixture_conn().await;
    let pending: bool =
        sqlx::query_scalar("SELECT pending FROM fvoci.document_collab_states WHERE document_id=$1")
            .bind(note)
            .fetch_one(&mut fixture)
            .await
            .unwrap();
    assert!(!pending, "HTTP removal must checkpoint before revocation");
    let reopened = connect(addr, Some(&editor), &note.to_string(), true).await;
    assert!(matches!(reopened, Err(StatusCode::NOT_FOUND)));
    client.close().await;
}
