//! 조직 명령의 409는 호출자가 볼 수 없는 문서의 종류와 식별자를 숨긴다(D-49·D-112·D-113).

use super::*;

#[tokio::test]
async fn inaccessible_personal_note_and_workspace_document_share_a_generic_conflict() {
    let mut f = Fixture::new().await;
    let org = common::organization_of_workspace(&f.workspace).await;
    let note = send(
        &f.app,
        Method::POST,
        "/api/me/notes",
        Some(&f.owner),
        Some(json!({"request_id": common::new_request_id(), "title": "비공개 공유 노트"})),
    )
    .await;
    assert_eq!(note.status, StatusCode::CREATED, "{:?}", note.json);
    f.document = note.json["id"].as_str().unwrap().to_owned();
    let mut conn = common::fixture_conn().await;
    let email: String = sqlx::query_scalar("SELECT email FROM fvoci.users WHERE id=$1")
        .bind(f.writer.id)
        .fetch_one(&mut conn)
        .await
        .unwrap();
    let shared = send(
        &f.app,
        Method::POST,
        &format!("/api/documents/{}/shares", f.document),
        Some(&f.owner),
        Some(json!({"email": email, "permission": "write", "organization_id": org})),
    )
    .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{:?}", shared.json);
    let mut writer = Client::connect(f.addr, &f.document, &f.writer).await;
    assert!(writer.edit_and_wait(PENDING).await);
    // 마지막 연결이 체크포인트를 남기지 못한 닫힌 방을 실제 보류 편집으로 만든다.
    sqlx::query("UPDATE fvoci.document_shares SET permission='read' WHERE document_id=$1::uuid")
        .bind(&f.document)
        .execute(&mut conn)
        .await
        .unwrap();
    writer.close().await;
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while f
            .state
            .collab
            .has_room(DocumentId(f.document.parse().unwrap()))
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("personal note room did not close");
    sqlx::query("UPDATE fvoci.document_shares SET permission='write' WHERE document_id=$1::uuid")
        .bind(&f.document)
        .execute(&mut conn)
        .await
        .unwrap();
    assert!(f.pending().await);
    let before = contracts::stored(&f).await;
    let current = f.current().await;
    assert_eq!(current["revision"], 1);
    let admin = common::login(&f.state, "personal note metadata admin").await;
    common::join_organization(org, &admin, "admin").await;
    let hidden = send(
        &f.app,
        Method::GET,
        &format!("/api/documents/{}", f.document),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{:?}", hidden.json);
    let blocked = send(
        &f.app,
        Method::DELETE,
        &format!("/api/organizations/{org}/members/{}", f.writer.id.0),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(blocked.status, StatusCode::CONFLICT, "{:?}", blocked.json);
    // 거절해도 멤버십·공유·저장 상태를 바꾸면 안 된다.
    let membership: (bool, bool, bool) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2),
                EXISTS(SELECT 1 FROM fvoci.workspace_members WHERE workspace_id=$3::uuid AND user_id=$2),
                EXISTS(SELECT 1 FROM fvoci.document_shares WHERE document_id=$4::uuid AND grantee_user_id=$2 AND permission='write')",
    )
    .bind(org)
    .bind(f.writer.id)
    .bind(&f.workspace)
    .bind(&f.document)
    .fetch_one(&mut conn)
    .await
    .unwrap();
    assert_eq!(membership, (true, true, true), "409 applied the removal");
    assert_eq!(contracts::stored(&f).await, before, "409 changed edits");
    assert_eq!(f.current().await, current, "409 changed the saved revision");
    let mut rejoined = Client::connect(f.addr, &f.document, &f.writer).await;
    assert_eq!(rejoined.body(), common::doc_body(PENDING));
    assert!(
        rejoined
            .edit_and_wait("개인 노트 거절 뒤 이어 쓴 편집")
            .await
    );
    rejoined.close().await;
    let body = blocked.json.to_string();
    assert!(
        !body.contains(&f.document)
            && !body.contains(&f.workspace)
            && !body.contains("personal note"),
        "hidden personal note metadata leaked: {body}"
    );
    let message = blocked.json["message"].as_str().unwrap();
    assert!(
        message.contains("save") && message.contains("retry"),
        "{message}"
    );
    f.stop().await;
    let mut personal = blocked.json;
    let mut workspace = contracts::inaccessible_workspace_conflict().await;
    // 요청 추적 id만 다르다. 문서 종류에 따라 나머지 응답이 달라지면 안 된다.
    personal.as_object_mut().unwrap().remove("request_id");
    workspace.as_object_mut().unwrap().remove("request_id");
    assert_eq!(
        personal, workspace,
        "hidden document kinds are distinguishable"
    );
}
