#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 멤버·초대·역할·소유권·그룹 (D-20, D-25, D-51, D-57).

mod common;

use axum::http::{Method, StatusCode};
use common::{
    TestUser, create_doc, create_workspace, login, login_with_email, new_request_id, send,
    unique_email,
};
use serde_json::{Value, json};

async fn invite(app: &axum::Router, admin: &TestUser, ws: &str, email: &str, role: &str) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(admin),
        Some(json!({ "email": email, "role": role })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

async fn accept(app: &axum::Router, user: &TestUser, id: &str) -> StatusCode {
    send(
        app,
        Method::POST,
        &format!("/api/invitations/{id}/accept"),
        Some(user),
        None,
    )
    .await
    .status
}

/// 초대한 멤버를 워크스페이스에 넣는다.
async fn add_member(
    app: &axum::Router,
    state: &fvoci_server::http::AppState,
    admin: &TestUser,
    ws: &str,
    label: &str,
    role: &str,
) -> TestUser {
    let email = unique_email(label);
    let user = login_with_email(state, label, &email, true).await;
    let inv = invite(app, admin, ws, &email, role).await;
    assert_eq!(
        accept(app, &user, inv["id"].as_str().unwrap()).await,
        StatusCode::OK
    );
    user
}

#[tokio::test]
async fn invitation_requires_the_verified_invited_email() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "초대").await;
    let email = unique_email("bob");
    let inv = invite(&app, &owner, &ws, &email, "writer").await;
    let id = inv["id"].as_str().unwrap();

    // 다른 이메일, 같은 이메일이지만 미검증: 초대가 보이지 않고 수락할 수 없다(링크만으로 가입 불가).
    let stranger = login_with_email(&state, "stranger", &unique_email("x"), true).await;
    let unverified = login_with_email(&state, "unverified", &email, false).await;
    for user in [&stranger, &unverified] {
        let mine = send(&app, Method::GET, "/api/invitations", Some(user), None).await;
        assert!(mine.json.as_array().unwrap().is_empty());
        assert_eq!(accept(&app, user, id).await, StatusCode::NOT_FOUND);
    }

    // 대소문자가 달라도 같은 이메일이다. 검증된 대상은 초대를 보고 수락한다.
    let bob = login_with_email(&state, "bob", &email.to_uppercase(), true).await;
    let mine = send(&app, Method::GET, "/api/invitations", Some(&bob), None).await;
    assert_eq!(mine.json[0]["workspace_name"], "초대");
    assert_eq!(mine.json[0]["role"], "writer");
    assert_eq!(accept(&app, &bob, id).await, StatusCode::OK);
    assert_eq!(
        accept(&app, &bob, id).await,
        StatusCode::CONFLICT,
        "closed invitation"
    );

    let list = send(&app, Method::GET, "/api/workspaces", Some(&bob), None).await;
    assert!(
        list.json
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == ws.as_str() && w["role"] == "writer")
    );
    // 작성자는 문서를 만들 수 있다.
    create_doc(&app, &bob, &ws, "밥의 문서").await;
}

#[tokio::test]
async fn decline_and_revoke_close_invitations() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "거절").await;
    let email = unique_email("carol");
    let carol = login_with_email(&state, "carol", &email, true).await;

    let inv = invite(&app, &owner, &ws, &email, "reader").await;
    let id = inv["id"].as_str().unwrap();
    let declined = send(
        &app,
        Method::POST,
        &format!("/api/invitations/{id}/decline"),
        Some(&carol),
        None,
    )
    .await;
    assert_eq!(declined.status, StatusCode::NO_CONTENT);
    assert_eq!(accept(&app, &carol, id).await, StatusCode::CONFLICT);

    let again = invite(&app, &owner, &ws, &email, "reader").await;
    let again_id = again["id"].as_str().unwrap();
    let pending = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(pending.json.as_array().unwrap().len(), 1);
    let revoked = send(
        &app,
        Method::DELETE,
        &format!("/api/invitations/{again_id}"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(revoked.status, StatusCode::NO_CONTENT);
    assert_eq!(accept(&app, &carol, again_id).await, StatusCode::CONFLICT);
}

#[tokio::test]
async fn roles_are_enforced_and_only_admins_manage_members() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "역할").await;
    let writer = add_member(&app, &state, &owner, &ws, "writer", "writer").await;
    let doc = create_doc(&app, &writer, &ws, "작성자 문서").await;

    // 작성자는 멤버를 관리할 수 없다(403). 비멤버는 404.
    let outsider = login(&state, "outsider").await;
    let path = format!("/api/workspaces/{ws}/members/{}", writer.id.0);
    let body = json!({ "role": "reader" });
    assert_eq!(
        send(&app, Method::PUT, &path, Some(&writer), Some(body.clone()))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            Method::PUT,
            &path,
            Some(&outsider),
            Some(body.clone())
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );
    let inv = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(&writer),
        Some(json!({ "email": unique_email("z"), "role": "admin" })),
    )
    .await;
    assert_eq!(inv.status, StatusCode::FORBIDDEN);

    // 관리자가 독자로 내리면 쓰기는 403, 읽기는 된다.
    assert_eq!(
        send(&app, Method::PUT, &path, Some(&owner), Some(body))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());
    let read = send(&app, Method::GET, &uri, Some(&writer), None).await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.json["can_edit"], false);
    let save = send(
        &app,
        Method::PUT,
        &uri,
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "base_revision": 1, "title": "x", "body": common::doc_body("x") })),
    )
    .await;
    assert_eq!(save.status, StatusCode::FORBIDDEN);
    let create = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&writer),
        Some(json!({ "request_id": new_request_id(), "title": "독자" })),
    )
    .await;
    assert_eq!(create.status, StatusCode::FORBIDDEN);

    // 이메일은 관리자에게만 보인다.
    let as_admin = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/members"),
        Some(&owner),
        None,
    )
    .await;
    let as_reader = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/members"),
        Some(&writer),
        None,
    )
    .await;
    let email_of = |v: &Value| {
        v["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["user_id"] == writer.id.0.to_string())
            .unwrap()["email"]
            .clone()
    };
    assert!(email_of(&as_admin.json).is_string());
    assert!(email_of(&as_reader.json).is_null());
    assert_eq!(as_reader.json["my_role"], "reader");
}

#[tokio::test]
async fn owner_is_protected_and_ownership_transfer_needs_acceptance() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "소유").await;
    let admin2 = add_member(&app, &state, &owner, &ws, "admin2", "admin").await;
    let owner_path = format!("/api/workspaces/{ws}/members/{}", owner.id.0);

    // 소유자는 탈퇴·제거·강등될 수 없다.
    let leave = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/leave"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(leave.status, StatusCode::CONFLICT);
    assert_eq!(
        send(&app, Method::DELETE, &owner_path, Some(&admin2), None)
            .await
            .status,
        StatusCode::CONFLICT
    );
    let demote = send(
        &app,
        Method::PUT,
        &owner_path,
        Some(&admin2),
        Some(json!({ "role": "writer" })),
    )
    .await;
    assert_eq!(demote.status, StatusCode::CONFLICT);

    // 소유자 아닌 관리자는 인계를 제안할 수 없다. 소유자가 제안해도 수락 전에는 바뀌지 않는다.
    let offer = json!({ "to_user": admin2.id.0 });
    let path = format!("/api/workspaces/{ws}/ownership");
    assert_eq!(
        send(
            &app,
            Method::POST,
            &path,
            Some(&admin2),
            Some(json!({ "to_user": owner.id.0 }))
        )
        .await
        .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, Method::POST, &path, Some(&owner), Some(offer))
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    let view = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/members"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(view.json["i_am_owner"], true);
    assert_eq!(view.json["pending_owner"], admin2.id.0.to_string());
    // 제안받지 않은 사람은 수락할 수 없다.
    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{path}/accept"),
            Some(&owner),
            None
        )
        .await
        .status,
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        send(
            &app,
            Method::POST,
            &format!("{path}/accept"),
            Some(&admin2),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let list = send(&app, Method::GET, "/api/workspaces", Some(&admin2), None).await;
    assert!(
        list.json
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == ws.as_str() && w["is_owner"] == true)
    );
    // 이전 소유자는 이제 탈퇴할 수 있다.
    let leave = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/leave"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(leave.status, StatusCode::NO_CONTENT);
    let gone = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn removed_member_loses_access_and_team_documents_remain() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "제거").await;
    let writer = add_member(&app, &state, &owner, &ws, "leaver", "writer").await;
    let doc = create_doc(&app, &writer, &ws, "팀 문서").await;
    let uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    let removed = send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", writer.id.0),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(removed.status, StatusCode::NO_CONTENT);
    assert_eq!(
        send(&app, Method::GET, &uri, Some(&writer), None)
            .await
            .status,
        StatusCode::NOT_FOUND
    );
    // 팀 공간에서 만든 문서는 남는다 (D-25).
    let kept = send(&app, Method::GET, &uri, Some(&owner), None).await;
    assert_eq!(kept.status, StatusCode::OK);
    assert_eq!(kept.json["title"], "팀 문서");
}

#[tokio::test]
async fn groups_hold_only_workspace_members() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "그룹").await;
    let member = add_member(&app, &state, &owner, &ws, "member", "writer").await;
    let outsider = login(&state, "outsider").await;

    let created = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&owner),
        Some(json!({ "name": "연구팀" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let dup = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&owner),
        Some(json!({ "name": "연구팀" })),
    )
    .await;
    assert_eq!(dup.status, StatusCode::CONFLICT);
    let gid = created.json["id"].as_str().unwrap();

    let add = |user: &TestUser| format!("/api/groups/{gid}/members/{}", user.id.0);
    assert_eq!(
        send(&app, Method::PUT, &add(&member), Some(&owner), None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(&app, Method::PUT, &add(&outsider), Some(&owner), None)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        send(&app, Method::PUT, &add(&member), Some(&member), None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    let groups = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&member),
        None,
    )
    .await;
    assert_eq!(groups.json[0]["members"], json!([member.id.0.to_string()]));

    // 워크스페이스에서 제거되면 그룹에서도 빠진다.
    send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", member.id.0),
        Some(&owner),
        None,
    )
    .await;
    let groups = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(groups.json[0]["members"], json!([]));
    assert_eq!(
        send(
            &app,
            Method::DELETE,
            &format!("/api/groups/{gid}"),
            Some(&owner),
            None
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn membership_changes_are_audited() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "감사").await;
    let member = add_member(&app, &state, &owner, &ws, "m", "writer").await;
    send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", member.id.0),
        Some(&owner),
        Some(json!({ "role": "reader" })),
    )
    .await;
    send(
        &app,
        Method::DELETE,
        &format!("/api/workspaces/{ws}/members/{}", member.id.0),
        Some(&owner),
        None,
    )
    .await;

    let mut tx = state.db.begin_as(owner.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT target_type || ':' || action FROM audit_log WHERE workspace_id = $1::uuid ORDER BY created_at, id",
    )
    .bind(&ws)
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert_eq!(
        actions,
        vec![
            "workspace:create",
            "invitation:invite",
            "member:join",
            "member:change_role",
            "member:remove"
        ]
    );
}
