#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 문서 템플릿과 본문 안전 규칙 (D-27): 소유 권한자가 템플릿으로 표시하고, 읽을 수 있는 사람이 새 문서의 본문으로
//! 쓴다. 이미지 주소는 이 서비스의 첨부만, 링크는 위험한 스킴을 받지 않는다.

use crate::common;

use axum::http::{Method, StatusCode};
use common::{add_member, create_doc, create_workspace, doc_body, login, new_request_id, send};
use serde_json::{Value, json};

#[tokio::test]
async fn documents_start_from_readable_templates() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "템플릿").await;
    let (writer, _) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let outsider = login(&state, "outsider").await;

    let template = create_doc(&app, &admin, &ws, "회의록 양식").await;
    let template_id = template["id"].as_str().unwrap().to_owned();
    let saved = common::live::save(
        &app,
        &admin,
        &format!("/api/documents/{template_id}"),
        &json!({
        "request_id": new_request_id(), "base_revision": 1, "title": "회의록 양식",
        "body": doc_body("안건 / 결정 / 다음 할 일") }),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK, "{:?}", saved.json);
    let open = send(
        &app,
        Method::PATCH,
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
        Method::PATCH,
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

/// SR1-4: 공간 문서 템플릿은 같은 조직의 공간에서만 새 문서의 본문이 된다(D-114, 복제와 같다). 템플릿 예외는
/// `allow_copy`에 대한 것뿐이다(D-115). 쓰면 템플릿에 복제 감사가 남는다.
#[tokio::test]
async fn space_templates_stay_within_the_organization() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "조직 A").await;
    let template = create_doc(&app, &admin, &ws, "회의록 양식").await;
    let template_id = template["id"].as_str().unwrap().to_owned();
    let mark = send(
        &app,
        Method::PUT,
        &format!("/api/documents/{template_id}/template"),
        Some(&admin),
        Some(json!({ "template": true })),
    )
    .await;
    assert_eq!(mark.status, StatusCode::NO_CONTENT, "{:?}", mark.json);

    // 같은 사람이 다른 조직에도 속해 그 조직의 공간을 쓸 수 있다.
    let boss = login(&state, "boss").await;
    let other = common::organization_for(&boss).await;
    common::join_organization(other, &admin, "admin").await;
    let other_ws = send(
        &app,
        Method::POST,
        "/api/workspaces",
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "organization_id": other, "name": "조직 B" })),
    )
    .await;
    assert_eq!(other_ws.status, StatusCode::CREATED, "{:?}", other_ws.json);
    let other_ws = other_ws.json["id"].as_str().unwrap().to_owned();
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{other_ws}/modules"),
        Some(&admin),
        Some(
            json!({ "request_id": new_request_id(), "kind": "project", "name": "B", "key": "BTP" }),
        ),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", module.json["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "항목" })),
    )
    .await;
    assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
    let item = item.json["id"].as_str().unwrap().to_owned();

    let body = || json!({ "request_id": new_request_id(), "title": "새 회의", "template_id": template_id });
    for uri in [
        format!("/api/workspaces/{other_ws}/wiki/documents"),
        format!("/api/items/{item}/documents"),
    ] {
        let refused = send(&app, Method::POST, &uri, Some(&admin), Some(body())).await;
        assert_eq!(
            refused.status,
            StatusCode::CONFLICT,
            "{uri}: {:?}",
            refused.json
        );
    }

    // 같은 request_id로 다시 보내면 같은 문서를 돌려받고 감사는 하나만 남는다.
    let request = body();
    let mut made = Vec::new();
    for _ in 0..2 {
        let reply = send(
            &app,
            Method::POST,
            &format!("/api/workspaces/{ws}/wiki/documents"),
            Some(&admin),
            Some(request.clone()),
        )
        .await;
        assert_eq!(reply.status, StatusCode::CREATED, "{:?}", reply.json);
        made.push(reply.json["id"].clone());
    }
    assert_eq!(made[0], made[1]);
    let mut conn = common::fixture_conn().await;
    let audits: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT detail FROM fvoci.audit_log
         WHERE target_type = 'document' AND target_id = $1::uuid AND action = 'copy' AND actor = $2",
    )
    .bind(&template_id)
    .bind(admin.id.0)
    .fetch_all(&mut conn)
    .await
    .unwrap();
    assert_eq!(audits, [json!({ "target": "workspace", "template": true })]);
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
            common::live::save(&app, &admin, &uri, &json!({ "request_id": new_request_id(),
                "base_revision": revision, "title": "그림 문서", "body": { "type": "doc", "content": [node] } }))
                .await
                .status
        }
    };
    let outside = json!({ "type": "image", "attrs": { "src": "https://tracker.example/p.png" } });
    assert!(
        fvoci_server::features::documents::validate_body(
            &json!({"type":"doc","content":[outside]}),
            &state.config.limits
        )
        .is_err()
    );
    let link = json!({ "type": "paragraph", "content": [{ "type": "text", "text": "x",
        "marks": [{ "type": "link", "attrs": { "href": "javascript:alert(1)" } }] }] });
    assert!(
        fvoci_server::features::documents::validate_body(
            &json!({"type":"doc","content":[link]}),
            &state.config.limits
        )
        .is_err()
    );
    let inside = json!({ "type": "image", "attrs": { "src": format!("/api/files/{id}/content?inline=true") } });
    assert_eq!(save(1, inside).await, StatusCode::OK);
    let table = json!({ "type": "table", "content": [{ "type": "tableRow", "content": [
        { "type": "tableHeader", "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "항목" }] }] }] }] });
    assert_eq!(save(2, table).await, StatusCode::OK);
}

/// 문서 복원 규칙(D-58, D-29): 문서를 옛 리비전으로 되돌려도 본문 안의 체크리스트만 돌아가고, 항목의 업무 완료 상태나
/// 강의 진도는 그대로다.
#[tokio::test]
async fn restoring_a_document_leaves_tasks_and_progress_alone() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "복원 규칙").await;
    let m = send(&app, Method::POST, &format!("/api/workspaces/{ws}/modules"), Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "course", "name": "강의", "key": "RST" }))).await;
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "강좌" })),
    )
    .await;
    let item_id = item.json["id"].as_str().unwrap().to_owned();
    let task = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/tasks"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "과제" })),
    )
    .await;
    send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/episodes"),
        Some(&admin),
        Some(json!({ "episodes": [{ "request_id": new_request_id(), "title": "1강" }] })),
    )
    .await;
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "노트" })),
    )
    .await;
    let doc_id = doc.json["id"].as_str().unwrap().to_owned();
    let checklist = |checked: bool| {
        json!({ "type": "doc", "content": [{ "type": "taskList", "content": [
        { "type": "taskItem", "attrs": { "checked": checked },
          "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": "복습" }] }] }] }] })
    };
    let v2 = common::live::save(&app, &admin, &format!("/api/documents/{doc_id}"), &json!({
        "request_id": new_request_id(), "base_revision": 1, "title": "노트", "body": checklist(false) })).await;
    assert_eq!(v2.status, StatusCode::OK, "{:?}", v2.json);
    let v3 = common::live::save(&app, &admin, &format!("/api/documents/{doc_id}"), &json!({
        "request_id": new_request_id(), "base_revision": 2, "title": "노트", "body": checklist(true) })).await;
    assert_eq!(v3.status, StatusCode::OK, "{:?}", v3.json);

    // 업무 완료, 강의 완료 표시.
    let task_id = task.json["id"].as_str().unwrap().to_owned();
    let done = send(&app, Method::PATCH, &format!("/api/tasks/{task_id}"), Some(&admin), Some(json!({
        "base_version": task.json["version"], "title": "과제", "description": "", "status": "완료", "priority": "none",
        "start_date": null, "due_date": null, "estimate_minutes": null, "tags": [], "milestone": false,
        "assignee_ids": [] }))).await;
    assert_eq!(done.status, StatusCode::OK, "{:?}", done.json);
    let course = send(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/episodes"),
        Some(&admin),
        None,
    )
    .await;
    let episode = course.json[0]["id"].as_str().unwrap().to_owned();
    send(
        &app,
        Method::PUT,
        &format!("/api/episodes/{episode}/progress"),
        Some(&admin),
        Some(json!({ "status": "done", "note": "" })),
    )
    .await;

    // 체크 표시 전(리비전 2)으로 되돌린다: 본문 체크리스트만 돌아간다.
    let restored = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/revisions/2/restore"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "base_revision": 3 })),
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK, "{:?}", restored.json);
    assert_eq!(restored.json["body"], checklist(false));
    let task_now = send(
        &app,
        Method::GET,
        &format!("/api/tasks/{task_id}"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(task_now.json["status"], "완료");
    // D-116: 상태 이름과 별개로 집계에 쓰는 범주도 보존한다.
    assert_eq!(task_now.json["status_category"], "done");
    let course_now = send(
        &app,
        Method::GET,
        &format!("/api/items/{item_id}/episode-progress"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(course_now.json[0]["status"], "done");
}
