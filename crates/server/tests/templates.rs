#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 문서 템플릿과 본문 안전 규칙 (D-27): 소유 권한자가 템플릿으로 표시하고, 읽을 수 있는 사람이 새 문서의 본문으로
//! 쓴다. 이미지 주소는 이 서비스의 첨부만, 링크는 위험한 스킴을 받지 않는다.

mod common;

use axum::http::{Method, StatusCode};
use common::{add_member, create_doc, create_workspace, doc_body, login, new_request_id, send};
use serde_json::{Value, json};

#[tokio::test]
async fn documents_start_from_readable_templates() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "템플릿").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "writer", "writer").await;
    let outsider = login(&state, "outsider").await;

    let template = create_doc(&app, &admin, &ws, "회의록 양식").await;
    let template_id = template["id"].as_str().unwrap().to_owned();
    let saved = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{template_id}"),
        Some(&admin),
        Some(json!({
        "request_id": new_request_id(), "base_revision": 1, "title": "회의록 양식",
        "body": doc_body("안건 / 결정 / 다음 할 일") })),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let open = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{template_id}/settings"),
        Some(&admin),
        Some(json!({ "visibility": "workspace", "allow_copy": false })),
    )
    .await;
    assert_eq!(open.status, StatusCode::OK, "{:?}", open.json);

    // 표시는 소유 권한자(팀 문서는 관리자)만.
    let mark = format!("/api/documents/{template_id}/template");
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &mark,
            Some(&writer),
            Some(json!({ "template": true }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &mark,
            Some(&admin),
            Some(json!({ "template": true }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let list = send(
        &app,
        Method::GET,
        &format!("/api/document-templates?workspace_id={ws}"),
        Some(&writer),
        None,
    )
    .await;
    assert_eq!(list.json[0]["title"], "회의록 양식", "{:?}", list.json);
    assert_eq!(list.json[0]["personal"], false);
    assert_eq!(
        send(
            &app,
            Method::GET,
            &format!("/api/document-templates?workspace_id={ws}"),
            Some(&outsider),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    // 템플릿 본문으로 새 위키 문서. 템플릿이 아니거나 볼 수 없으면 404.
    let create = |user: &common::TestUser, template: &str| {
        let app = app.clone();
        let user = user.clone();
        let body = json!({ "request_id": new_request_id(), "title": "10월 회의", "template_id": template });
        let ws = ws.clone();
        async move {
            send(
                &app,
                Method::POST,
                &format!("/api/workspaces/{ws}/wiki/documents"),
                Some(&user),
                Some(body),
            )
            .await
        }
    };
    let made = create(&writer, &template_id).await;
    assert_eq!(made.status, StatusCode::CREATED, "{:?}", made.json);
    assert_eq!(made.json["body"], doc_body("안건 / 결정 / 다음 할 일"));
    assert_eq!(made.json["title"], "10월 회의");
    let plain = create_doc(&app, &admin, &ws, "보통 문서").await;
    assert_eq!(
        create(&writer, plain["id"].as_str().unwrap()).await.status,
        StatusCode::NOT_FOUND
    );
    send(
        &app,
        Method::PUT,
        &format!("/api/documents/{template_id}/settings"),
        Some(&admin),
        Some(json!({ "visibility": "private", "allow_copy": false })),
    )
    .await;
    assert_eq!(
        create(&writer, &template_id).await.status,
        StatusCode::NOT_FOUND
    );

    // 개인 노트 템플릿은 본인 목록에만.
    let note = send(
        &app,
        Method::POST,
        "/api/me/notes",
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "일기 양식" })),
    )
    .await;
    let note_id = note.json["id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &format!("/api/documents/{note_id}/template"),
            Some(&writer),
            Some(json!({ "template": true }))
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let mine = send(
        &app,
        Method::GET,
        "/api/document-templates",
        Some(&writer),
        None,
    )
    .await;
    let titles: Vec<&Value> = mine
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|t| &t["title"])
        .collect();
    assert_eq!(titles, [&json!("일기 양식")]);
    assert_eq!(mine.json[0]["personal"], true);
    assert!(
        send(
            &app,
            Method::GET,
            "/api/document-templates",
            Some(&admin),
            None
        )
        .await
        .json
        .as_array()
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn bodies_reject_outside_images_and_unsafe_links() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "본문 규칙").await;
    let doc = create_doc(&app, &admin, &ws, "그림 문서").await;
    let id = doc["id"].as_str().unwrap().to_owned();
    let save = |revision: i64, node: Value| {
        let app = app.clone();
        let admin = admin.clone();
        let uri = format!("/api/documents/{id}");
        async move {
            send(&app, Method::PUT, &uri, Some(&admin), Some(json!({ "request_id": new_request_id(),
                "base_revision": revision, "title": "그림 문서", "body": { "type": "doc", "content": [node] } })))
                .await
                .status
        }
    };
    let outside = json!({ "type": "image", "attrs": { "src": "https://tracker.example/p.png" } });
    assert_eq!(save(1, outside).await, StatusCode::BAD_REQUEST);
    let link = json!({ "type": "paragraph", "content": [{ "type": "text", "text": "x",
        "marks": [{ "type": "link", "attrs": { "href": "javascript:alert(1)" } }] }] });
    assert_eq!(save(1, link).await, StatusCode::BAD_REQUEST);
    let inside = json!({ "type": "image", "attrs": { "src": format!("/api/files/{id}/content?inline=true") } });
    assert_eq!(save(1, inside).await, StatusCode::OK);
    let table = json!({ "type": "table", "content": [{ "type": "tableRow", "content": [
        { "type": "tableHeader", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "항목" }] }] }] }] });
    assert_eq!(save(2, table).await, StatusCode::OK);
}
