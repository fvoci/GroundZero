#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
use crate::common;

use axum::http::{Method, StatusCode};
use serde_json::json;

use common::{add_member, create_workspace, doc_body, login, new_request_id, send};

/// 개정 2026-10-26(⑯, D-113·D-106): 보관된 항목의 문서는 등급이 read로 낮아지지만, 워크스페이스 관리자는 보관 해제 없이
/// 공유·공개 링크·설정을 바꿀 수 있다. 멤버는 보관 중 409 그대로이고, 관리자라도 내용 편집은 보관 중 409다.
#[tokio::test]
async fn admins_manage_sharing_and_settings_of_archived_documents_but_cannot_edit_them() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "보관 관리").await;
    let organization = common::organization_of_workspace(&ws).await;
    let (member, member_email) = add_member(&app, &state, &admin, &ws, "member", "member").await;
    let module = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": "보관", "key": "ARCH" })),
    )
    .await;
    assert_eq!(module.status, StatusCode::CREATED, "{:?}", module.json);
    let module_id = module.json["id"].as_str().unwrap().to_owned();
    let item = send(
        &app,
        Method::POST,
        &format!("/api/modules/{module_id}/items"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "보관될 항목" })),
    )
    .await;
    assert_eq!(item.status, StatusCode::CREATED, "{:?}", item.json);
    let item_id = item.json["id"].as_str().unwrap().to_owned();
    let opened = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}"),
        Some(&admin),
        Some(json!({ "base_version": item.json["version"], "title": "보관될 항목", "description": "", "visibility": "workspace" })),
    )
    .await;
    assert_eq!(opened.status, StatusCode::OK, "{:?}", opened.json);
    let doc = send(
        &app,
        Method::POST,
        &format!("/api/items/{item_id}/documents"),
        Some(&admin),
        Some(json!({ "request_id": new_request_id(), "title": "보관될 문서", "parent_id": null, "template_id": null })),
    )
    .await;
    assert_eq!(doc.status, StatusCode::CREATED, "{:?}", doc.json);
    let doc_id = doc.json["id"].as_str().unwrap().to_owned();
    let settings = |user: &common::TestUser, visibility: &str, previous_visibility: &str| {
        let app = app.clone();
        let doc_id = doc_id.clone();
        let user = user.clone();
        let visibility = visibility.to_owned();
        let previous_visibility = previous_visibility.to_owned();
        async move {
            send(
                &app,
                Method::PATCH,
                &format!("/api/documents/{doc_id}/settings"),
                Some(&user),
                Some(
                    json!({ "visibility": visibility, "previous_visibility": previous_visibility }),
                ),
            )
            .await
        }
    };
    let visible = settings(
        &admin,
        "workspace",
        doc.json["visibility"].as_str().unwrap(),
    )
    .await;
    assert_eq!(visible.status, StatusCode::OK, "{:?}", visible.json);
    // 보관 전: 문서를 볼 수 있는 멤버도 관리자가 아니라 설정을 바꾸지 못한다(403).
    assert_eq!(
        settings(&member, "workspace", "workspace").await.status,
        StatusCode::FORBIDDEN
    );

    let archived = send(
        &app,
        Method::PUT,
        &format!("/api/items/{item_id}/archived"),
        Some(&admin),
        Some(json!({ "archived": true })),
    )
    .await;
    assert_eq!(archived.status, StatusCode::OK, "{:?}", archived.json);

    // 보관 중: 관리자는 설정·공유를 바꾼다.
    let changed = settings(&admin, "private", "workspace").await;
    assert_eq!(changed.status, StatusCode::OK, "{:?}", changed.json);
    let shared = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/shares"),
        Some(&admin),
        Some(
            json!({ "email": member_email, "permission": "read", "organization_id": organization }),
        ),
    )
    .await;
    assert_eq!(shared.status, StatusCode::CREATED, "{:?}", shared.json);
    let link = send(
        &app,
        Method::POST,
        &format!("/api/documents/{doc_id}/public-links"),
        Some(&admin),
        Some(json!({ "expires_at": null })),
    )
    .await;
    assert_eq!(link.status, StatusCode::CREATED, "{:?}", link.json);
    // 관리 목록도 SQL의 보관 예외를 따르되 멤버·외부인에게 관리 정보를 열지 않는다.
    let stranger = login(&state, "stranger").await;
    for (suffix, created) in [("shares", &shared.json), ("public-links", &link.json)] {
        let path = format!("/api/documents/{doc_id}/{suffix}");
        let listed = send(&app, Method::GET, &path, Some(&admin), None).await;
        assert_eq!(listed.status, StatusCode::OK, "{:?}", listed.json);
        assert_eq!(listed.json.as_array().unwrap().len(), 1);
        assert_eq!(listed.json[0]["id"], created["id"]);
        for (user, expected) in [
            (&member, StatusCode::FORBIDDEN),
            (&stranger, StatusCode::NOT_FOUND),
        ] {
            let denied = send(&app, Method::GET, &path, Some(user), None).await;
            assert_eq!(denied.status, expected, "{:?}", denied.json);
        }
    }
    for (user, can_manage) in [(&admin, true), (&member, false)] {
        let viewed = send(
            &app,
            Method::GET,
            &format!("/api/documents/{doc_id}"),
            Some(user),
            None,
        )
        .await;
        assert_eq!(viewed.status, StatusCode::OK, "{:?}", viewed.json);
        assert_eq!(viewed.json["access"], "read");
        assert_eq!(viewed.json["can_manage"], can_manage);
        assert_eq!(viewed.json["can_edit"], false);
        assert_eq!(viewed.json["can_delete"], false);
    }
    // 멤버는 보관 중 409 그대로.
    assert_eq!(
        settings(&member, "workspace", "private").await.status,
        StatusCode::CONFLICT
    );
    // 관리자라도 내용 편집은 보관 중 409다.
    let base: i64 = doc.json["revision"].as_i64().unwrap_or(0);
    let edit = common::live::save(&app, &admin, &format!("/api/documents/{doc_id}"), &json!({ "request_id": new_request_id(), "base_revision": base, "title": "보관될 문서", "body": doc_body("보관 중 편집") }))
    .await;
    assert_eq!(edit.status, StatusCode::CONFLICT, "{:?}", edit.json);
}
