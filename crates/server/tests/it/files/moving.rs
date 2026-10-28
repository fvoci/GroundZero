use super::fixtures::{state_with, upload_all};
use crate::common::{
    self, TestUser, add_member, create_doc, create_workspace, login, new_request_id, send,
};
use axum::http::{Method, StatusCode};
use serde_json::json;
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
