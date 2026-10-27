#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 멤버·초대·역할·소유권·그룹 (D-20, D-25, D-51, D-57).

use crate::common;

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
    common::join_organization(common::organization_of_workspace(ws).await, &user, "member").await;
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
    let inv = invite(&app, &owner, &ws, &email, "member").await;
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
    // 워크스페이스 초대는 조직 멤버만 수락한다(D-114).
    common::join_organization(common::organization_of_workspace(&ws).await, &bob, "member").await;
    let mine = send(&app, Method::GET, "/api/invitations", Some(&bob), None).await;
    assert_eq!(mine.json[0]["workspace_name"], "초대");
    assert_eq!(mine.json[0]["role"], "member");
    assert_eq!(accept(&app, &bob, id).await, StatusCode::OK);
    assert_eq!(
        accept(&app, &bob, id).await,
        StatusCode::CONFLICT,
        "closed invitation"
    );

    let list = send(&app, Method::GET, "/api/workspaces", Some(&bob), None).await;
    // 역할은 표시용이고 쓰기 여부는 접근 등급으로 나간다(D-113): 멤버는 write.
    assert!(
        list.json
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["id"] == ws.as_str() && w["role"] == "member" && w["access"] == "write")
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

    let inv = invite(&app, &owner, &ws, &email, "readonly").await;
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

    let again = invite(&app, &owner, &ws, &email, "readonly").await;
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
    let writer = add_member(&app, &state, &owner, &ws, "member", "member").await;
    let doc = create_doc(&app, &writer, &ws, "작성자 문서").await;

    // 작성자는 멤버를 관리할 수 없다(403). 비멤버는 404.
    let outsider = login(&state, "outsider").await;
    let path = format!("/api/workspaces/{ws}/members/{}", writer.id.0);
    let body = json!({ "role": "readonly" });
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
    assert_eq!(as_reader.json["my_role"], "readonly");
    // 관리 가능 여부는 접근 등급(owner)에서 온다(D-113). 웹은 역할이 아니라 이 칸을 본다.
    assert_eq!(as_admin.json["can_manage_members"], true);
    assert_eq!(as_reader.json["can_manage_members"], false);
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
        Some(json!({ "role": "member" })),
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
    let writer = add_member(&app, &state, &owner, &ws, "leaver", "member").await;
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
    let member = add_member(&app, &state, &owner, &ws, "member", "member").await;
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
    let member = add_member(&app, &state, &owner, &ws, "m", "member").await;
    send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/members/{}", member.id.0),
        Some(&owner),
        Some(json!({ "role": "readonly" })),
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

/// 테이블 소유 역할 연결(시험 확인용).
async fn owner_pool() -> sqlx::PgPool {
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    sqlx::PgPool::connect(&url).await.unwrap()
}

/// 트랜잭션 하나를 열어 둔 채 다른 요청을 보내고, 그 요청이 이 트랜잭션의 잠금을 실제로 기다리게 된 뒤 트랜잭션을
/// 마친 다음 그 요청의 결과를 돌려준다. 고정 시간만 기다리면 느린 환경에서 요청이 잠금에 닿기 전에 커밋해, 경합을
/// 재현하지 못한 채 통과하거나 엉뚱한 이유로 실패한다. 끝내 기다리지 않으면 시험을 멈춘다.
async fn while_open<F>(mut tx: fvoci_server::db::UserTx, request: F) -> common::Reply
where
    F: std::future::Future<Output = common::Reply> + Send + 'static,
{
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(tx.conn())
        .await
        .unwrap();
    let pending = tokio::spawn(request);
    let db = owner_pool().await;
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&db)
        .await
        .unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    db.close().await;
    assert!(blocked, "no request waited on the held transaction");
    tx.commit().await.unwrap();
    pending.await.unwrap()
}

async fn private_item(app: &axum::Router, admin: &TestUser, ws: &str, key: &str) -> String {
    let m = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/modules"),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "kind": "project", "name": key, "key": key })),
    )
    .await;
    assert_eq!(m.status, StatusCode::CREATED, "{:?}", m.json);
    let i = send(
        app,
        Method::POST,
        &format!("/api/modules/{}/items", m.json["id"].as_str().unwrap()),
        Some(admin),
        Some(json!({ "request_id": new_request_id(), "title": "비공개 항목" })),
    )
    .await;
    assert_eq!(i.status, StatusCode::CREATED, "{:?}", i.json);
    let id = i.json["id"].as_str().unwrap().to_owned();
    let private = send(
        app,
        Method::PUT,
        &format!("/api/items/{id}"),
        Some(admin),
        Some(json!({ "base_version": 1, "title": "비공개 항목", "description": "", "visibility": "private" })),
    )
    .await;
    assert!(private.status.is_success(), "{:?}", private.json);
    id
}

/// 감사 S14: 소유권 인계를 수락하는 사이 그 사람을 내보내면, 내보내기가 옛 소유자를 보고 통과해 소유자가 멤버가 아닌
/// 워크스페이스가 남았다. 내보내기는 워크스페이스 행을 먼저 잠그고(수락과 같은 순서) 소유자를 다시 본다.
#[tokio::test]
async fn removing_a_member_who_is_accepting_ownership_meanwhile_conflicts() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "인계 경합").await;
    let heir = add_member(&app, &state, &owner, &ws, "heir", "admin").await;
    let other = add_member(&app, &state, &owner, &ws, "other-admin", "admin").await;
    let offered = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/ownership"),
        Some(&owner),
        Some(json!({ "to_user": heir.id.0 })),
    )
    .await;
    assert!(offered.status.is_success(), "{:?}", offered.json);
    let ws_id: uuid::Uuid = ws.parse().unwrap();

    let mut t1 = state.db.begin_as(heir.id).await.unwrap();
    sqlx::query("SELECT app_accept_ownership($1, $2)")
        .bind(ws_id)
        .bind(fvoci_server::db::new_ids(1))
        .execute(t1.conn())
        .await
        .unwrap();
    let uri = format!("/api/workspaces/{ws}/members/{}", heir.id.0);
    let removal = while_open(t1, {
        let app = app.clone();
        let other = other.clone();
        async move { send(&app, Method::DELETE, &uri, Some(&other), None).await }
    })
    .await;
    assert_eq!(removal.status, StatusCode::CONFLICT, "{:?}", removal.json);

    let db = owner_pool().await;
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM fvoci.workspaces w JOIN fvoci.workspace_members m
                        ON m.workspace_id = w.id AND m.user_id = w.owner_id WHERE w.id = $1)",
    )
    .bind(ws_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert!(member, "the workspace owner is not a member");
    db.close().await;
}

/// 감사 S15·S8: 멤버를 내보내는 사이 그 사람을 비공개 항목 멤버나 그룹에 넣으면, 넣기는 아직 커밋되지 않은 내보내기를
/// 보지 못해 통과했고 내보내기는 그 행을 보지 못해 지우지 않았다(다시 초대되면 비공개 항목이 저절로 열림). 넣는 쪽은
/// 대상의 멤버 행을 잠근 뒤 확인하고, 내보내기는 멤버 행을 먼저 잠근다.
#[tokio::test]
async fn adding_a_member_who_is_being_removed_does_not_survive_the_removal() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "내보내기 경합").await;
    let leaving = add_member(&app, &state, &admin, &ws, "leaving", "member").await;
    let item = private_item(&app, &admin, &ws, "PRV").await;
    let group = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&admin),
        Some(json!({ "name": "팀" })),
    )
    .await;
    assert_eq!(group.status, StatusCode::CREATED, "{:?}", group.json);
    let group_id = group.json["id"].as_str().unwrap().to_owned();
    let ws_id: uuid::Uuid = ws.parse().unwrap();

    for target in ["item", "group"] {
        // 매번 다시 넣고 시작한다.
        if target == "group" {
            let again = add_member(&app, &state, &admin, &ws, "leaving-again", "member").await;
            run_removal_race(
                &app, &state, &admin, ws_id, &again, target, &item, &group_id,
            )
            .await;
        } else {
            run_removal_race(
                &app, &state, &admin, ws_id, &leaving, target, &item, &group_id,
            )
            .await;
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "test helper with the race's inputs"
)]
async fn run_removal_race(
    app: &axum::Router,
    state: &fvoci_server::http::AppState,
    admin: &TestUser,
    ws_id: uuid::Uuid,
    member: &TestUser,
    target: &str,
    item: &str,
    group: &str,
) {
    // 리뷰: 경합 대상이 아닌 쪽의 멤버십을 미리 만들어, 내보내기가 이미 있던 행도 지우는지 함께 본다.
    let existing = if target == "item" {
        send(
            app,
            Method::PUT,
            &format!("/api/groups/{group}/members/{}", member.id.0),
            Some(admin),
            None,
        )
        .await
    } else {
        send(
            app,
            Method::PUT,
            &format!("/api/items/{item}/members"),
            Some(admin),
            Some(json!({ "user_ids": [member.id.0], "group_ids": [] })),
        )
        .await
    };
    assert!(
        existing.status.is_success(),
        "{target}: {:?}",
        existing.json
    );
    let n: i32 = {
        let mut tx = state.db.begin_as(admin.id).await.unwrap();
        let n = sqlx::query_scalar("SELECT app_remove_member_ids($1, $2)")
            .bind(ws_id)
            .bind(member.id)
            .fetch_one(tx.conn())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        n
    };
    let mut t1 = state.db.begin_as(admin.id).await.unwrap();
    sqlx::query("SELECT app_remove_member($1, $2, $3)")
        .bind(ws_id)
        .bind(member.id)
        .bind(fvoci_server::db::new_ids(usize::try_from(n).unwrap()))
        .execute(t1.conn())
        .await
        .unwrap();
    let (method, uri, body) = if target == "item" {
        (
            Method::PUT,
            format!("/api/items/{item}/members"),
            Some(json!({ "user_ids": [member.id.0], "group_ids": [] })),
        )
    } else {
        (
            Method::PUT,
            format!("/api/groups/{group}/members/{}", member.id.0),
            None,
        )
    };
    let added = while_open(t1, {
        let app = app.clone();
        let admin = admin.clone();
        async move { send(&app, method, &uri, Some(&admin), body).await }
    })
    .await;
    assert_eq!(
        added.status,
        StatusCode::BAD_REQUEST,
        "{target}: {:?}",
        added.json
    );
    let db = owner_pool().await;
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM fvoci.item_members WHERE user_id = $1)
              + (SELECT count(*) FROM fvoci.group_members WHERE user_id = $1)",
    )
    .bind(member.id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(left, 0, "{target}: a removed member kept a membership row");
    db.close().await;
}

/// 감사 S16: 초대를 수락하는 사이 철회하면, 철회가 수락된 초대를 '철회됨'으로 덮어썼다(이미 멤버인데). 철회는 수락과
/// 같은 순서로 잠근 뒤 아직 열려 있는지 다시 본다.
#[tokio::test]
async fn revoking_an_invitation_being_accepted_does_not_overwrite_the_acceptance() {
    let state = common::state();
    let app = common::app(&state);
    let admin = login(&state, "admin").await;
    let ws = create_workspace(&app, &admin, "철회 경합").await;
    let email = unique_email("invitee");
    let invitee = login_with_email(&state, "invitee", &email, true).await;
    common::join_organization(
        common::organization_of_workspace(&ws).await,
        &invitee,
        "member",
    )
    .await;
    let inv = invite(&app, &admin, &ws, &email, "member").await;
    let inv_id: uuid::Uuid = inv["id"].as_str().unwrap().parse().unwrap();

    let mut t1 = state.db.begin_as(invitee.id).await.unwrap();
    sqlx::query("SELECT app_respond_invitation($1, true, $2)")
        .bind(inv_id)
        .bind(fvoci_server::db::new_ids(1))
        .execute(t1.conn())
        .await
        .unwrap();
    let uri = format!("/api/invitations/{inv_id}");
    let revoked = while_open(t1, {
        let app = app.clone();
        let admin = admin.clone();
        async move { send(&app, Method::DELETE, &uri, Some(&admin), None).await }
    })
    .await;
    assert_eq!(revoked.status, StatusCode::CONFLICT, "{:?}", revoked.json);
    let db = owner_pool().await;
    let reason: Option<String> =
        sqlx::query_scalar("SELECT close_reason FROM fvoci.invitations WHERE id = $1")
            .bind(inv_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(reason.as_deref(), Some("accepted"));
    db.close().await;
}

/// 감사 S17: 소유자가 아닌 관리자 둘이 동시에 서로를 내보내면 둘 다 통과해 둘 다 빠졌다. 내보내기는 두 사람의 멤버 행을
/// id 순서로 잠근 뒤 관리자인지 본다(먼저 끝난 쪽만 통과).
#[tokio::test]
async fn two_admins_removing_each_other_at_once_leave_one_of_them() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "관리자 경합").await;
    let a = add_member(&app, &state, &owner, &ws, "admin-a", "admin").await;
    let b = add_member(&app, &state, &owner, &ws, "admin-b", "admin").await;
    let ws_id: uuid::Uuid = ws.parse().unwrap();
    let n: i32 = {
        let mut tx = state.db.begin_as(a.id).await.unwrap();
        let n = sqlx::query_scalar("SELECT app_remove_member_ids($1, $2)")
            .bind(ws_id)
            .bind(b.id)
            .fetch_one(tx.conn())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        n
    };
    let mut t1 = state.db.begin_as(a.id).await.unwrap();
    sqlx::query("SELECT app_remove_member($1, $2, $3)")
        .bind(ws_id)
        .bind(b.id)
        .bind(fvoci_server::db::new_ids(usize::try_from(n).unwrap()))
        .execute(t1.conn())
        .await
        .unwrap();
    let uri = format!("/api/workspaces/{ws}/members/{}", a.id.0);
    let second = while_open(t1, {
        let app = app.clone();
        let b = b.clone();
        async move { send(&app, Method::DELETE, &uri, Some(&b), None).await }
    })
    .await;
    assert_ne!(second.status, StatusCode::NO_CONTENT, "{:?}", second.json);
    let db = owner_pool().await;
    let admins: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM fvoci.workspace_members WHERE workspace_id = $1 AND user_id = ANY ($2)",
    )
    .bind(ws_id)
    .bind(vec![a.id.0, b.id.0])
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(admins, 1, "both admins removed each other");
    db.close().await;
}

/// 소유자가 관리자 `admin`을 작성자로 내리는 트랜잭션을 열어 둔다(커밋 전). 역할 바꾸기는 두 사람의 멤버 행을 잠근다.
async fn demoting(
    state: &fvoci_server::http::AppState,
    owner: &TestUser,
    ws_id: uuid::Uuid,
    admin: &TestUser,
) -> fvoci_server::db::UserTx {
    let n: i32 = {
        let mut tx = state.db.begin_as(owner.id).await.unwrap();
        let n = sqlx::query_scalar("SELECT app_set_member_role_ids($1, $2)")
            .bind(ws_id)
            .bind(admin.id)
            .fetch_one(tx.conn())
            .await
            .unwrap();
        tx.commit().await.unwrap();
        n
    };
    let mut t1 = state.db.begin_as(owner.id).await.unwrap();
    sqlx::query("SELECT app_set_member_role($1, $2, 'member', $3)")
        .bind(ws_id)
        .bind(admin.id)
        .bind(fvoci_server::db::new_ids(usize::try_from(n).unwrap()))
        .execute(t1.conn())
        .await
        .unwrap();
    t1
}

/// 리뷰(감사 S17 후속): 그룹에 사람을 넣는 관리자 확인이 호출한 사람의 멤버 행을 잠그지 않아, 그 관리자가 동시에 작성자로
/// 내려져도(먼저 커밋) 넣기가 통과했다. 호출한 사람과 대상의 멤버 행을 잠근 뒤 관리자인지 다시 본다.
#[tokio::test]
async fn an_admin_demoted_meanwhile_cannot_add_to_a_group() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "강등 중 그룹").await;
    let admin = add_member(&app, &state, &owner, &ws, "admin", "admin").await;
    let member = add_member(&app, &state, &owner, &ws, "member", "member").await;
    let group = send(
        &app,
        Method::POST,
        &format!("/api/workspaces/{ws}/groups"),
        Some(&owner),
        Some(json!({ "name": "팀" })),
    )
    .await;
    assert_eq!(group.status, StatusCode::CREATED, "{:?}", group.json);
    let gid = group.json["id"].as_str().unwrap().to_owned();
    let ws_id: uuid::Uuid = ws.parse().unwrap();

    let t1 = demoting(&state, &owner, ws_id, &admin).await;
    let uri = format!("/api/groups/{gid}/members/{}", member.id.0);
    let added = while_open(t1, {
        let app = app.clone();
        let admin = admin.clone();
        async move { send(&app, Method::PUT, &uri, Some(&admin), None).await }
    })
    .await;
    assert_eq!(added.status, StatusCode::FORBIDDEN, "{:?}", added.json);
    let db = owner_pool().await;
    let rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fvoci.group_members WHERE user_id = $1")
            .bind(member.id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(rows, 0, "a demoted admin added a group member");
    db.close().await;
}

/// 리뷰(감사 S16 후속): 초대 철회의 관리자 확인이 잠그기 전에 한 번뿐이라, 그 관리자가 동시에 작성자로 내려져도(먼저
/// 커밋) 철회가 통과했다. 워크스페이스와 호출한 사람의 멤버 행을 잠근 뒤 관리자인지 다시 본다.
#[tokio::test]
async fn an_admin_demoted_meanwhile_cannot_revoke_an_invitation() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "강등 중 철회").await;
    let admin = add_member(&app, &state, &owner, &ws, "admin", "admin").await;
    let inv = invite(&app, &owner, &ws, &unique_email("invitee"), "member").await;
    let inv_id = inv["id"].as_str().unwrap().to_owned();
    let ws_id: uuid::Uuid = ws.parse().unwrap();

    let t1 = demoting(&state, &owner, ws_id, &admin).await;
    let uri = format!("/api/invitations/{inv_id}");
    let revoked = while_open(t1, {
        let app = app.clone();
        let admin = admin.clone();
        async move { send(&app, Method::DELETE, &uri, Some(&admin), None).await }
    })
    .await;
    assert_eq!(revoked.status, StatusCode::FORBIDDEN, "{:?}", revoked.json);
    let db = owner_pool().await;
    let closed: Option<String> =
        sqlx::query_scalar("SELECT close_reason FROM fvoci.invitations WHERE id = $1::uuid")
            .bind(&inv_id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(closed, None, "a demoted admin revoked the invitation");
    db.close().await;
}

/// 리뷰(감사 S15 후속): 항목 멤버 정하기가 항목 행을 잡은 뒤에 담당자·리더 정리의 워크스페이스 잠금을 기다려, 그 잠금을
/// 먼저 쥐고 항목 행을 기다리는 역할 바꾸기(그 항목의 리더)와 교착했다. 이제 그 잠금을 항목 행보다 먼저 잡는다. 여기서는
/// 역할 바꾸기 쪽을 직접 흉내 낸다: 워크스페이스 잠금을 쥔 채 항목 멤버 정하기가 기다리게 한 뒤 항목 행을 잠근다.
#[tokio::test]
async fn setting_item_members_does_not_deadlock_with_the_workspace_assignee_lock() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "항목 멤버 잠금 순서").await;
    let member = add_member(&app, &state, &owner, &ws, "member", "member").await;
    let item = private_item(&app, &owner, &ws, "LKM").await;

    let db = owner_pool().await;
    let mut hold = db.begin().await.unwrap();
    let holder: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *hold)
        .await
        .unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended('fvoci-assignees:' || $1, 0))")
        .bind(&ws)
        .execute(&mut *hold)
        .await
        .unwrap();
    let request = tokio::spawn({
        let (app, owner, uri, user) = (
            app.clone(),
            owner.clone(),
            format!("/api/items/{item}/members"),
            member.id.0,
        );
        async move {
            send(
                &app,
                Method::PUT,
                &uri,
                Some(&owner),
                Some(json!({ "user_ids": [user], "group_ids": [] })),
            )
            .await
        }
    });
    let mut blocked = false;
    for _ in 0..500 {
        blocked = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1 = ANY (pg_blocking_pids(pid)))",
        )
        .bind(holder)
        .fetch_one(&db)
        .await
        .unwrap();
        if blocked {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(blocked, "setting item members never waited on the lock");
    // 역할 바꾸기처럼 그 잠금을 쥔 채 항목 행을 잠근다. 고치기 전에는 요청이 항목 행을 쥐고 있어 교착(40P01)이었다.
    let locked = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        sqlx::query("SELECT 1 FROM fvoci.items WHERE id = $1::uuid FOR UPDATE")
            .bind(&item)
            .execute(&mut *hold),
    )
    .await
    .expect("locking the item row waited too long");
    assert!(locked.is_ok(), "{locked:?}");
    hold.commit().await.unwrap();
    let set = request.await.unwrap();
    db.close().await;
    assert!(set.status.is_success(), "{:?}", set.json);
}

/// 감사 C23: 소유권 인계 제안 취소(`DELETE /api/workspaces/{ws}/ownership`)에는 시험이 없었다. 취소는 소유자만 하고,
/// 열린 제안이 없으면 404이며, 감사 기록을 남긴다.
#[tokio::test]
async fn only_the_owner_cancels_an_ownership_offer() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "인계 취소").await;
    let admin2 = add_member(&app, &state, &owner, &ws, "admin2", "admin").await;
    let path = format!("/api/workspaces/{ws}/ownership");
    let offered = send(
        &app,
        Method::POST,
        &path,
        Some(&owner),
        Some(json!({ "to_user": admin2.id.0 })),
    )
    .await;
    assert_eq!(offered.status, StatusCode::NO_CONTENT, "{:?}", offered.json);

    // 소유자가 아닌 관리자(제안받은 사람이어도)는 취소하지 못한다.
    let denied = send(&app, Method::DELETE, &path, Some(&admin2), None).await;
    assert_eq!(denied.status, StatusCode::FORBIDDEN, "{:?}", denied.json);
    let cancelled = send(&app, Method::DELETE, &path, Some(&owner), None).await;
    assert_eq!(
        cancelled.status,
        StatusCode::NO_CONTENT,
        "{:?}",
        cancelled.json
    );
    let view = send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/members"),
        Some(&owner),
        None,
    )
    .await;
    assert!(view.json["pending_owner"].is_null(), "{:?}", view.json);
    // 열린 제안이 없으면 404, 취소된 제안은 수락할 수 없다.
    let again = send(&app, Method::DELETE, &path, Some(&owner), None).await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{:?}", again.json);
    let accept = send(
        &app,
        Method::POST,
        &format!("{path}/accept"),
        Some(&admin2),
        None,
    )
    .await;
    assert_eq!(accept.status, StatusCode::NOT_FOUND, "{:?}", accept.json);

    let mut tx = state.db.begin_as(owner.id).await.unwrap();
    let actions: Vec<String> = sqlx::query_scalar(
        "SELECT action FROM audit_log WHERE workspace_id = $1::uuid AND target_type = 'workspace' ORDER BY created_at, id",
    )
    .bind(&ws)
    .fetch_all(tx.conn())
    .await
    .unwrap();
    assert!(
        actions.iter().any(|a| a == "cancel_ownership_offer"),
        "{actions:?}"
    );
}

/// 조직·워크스페이스 초대를 함께 표시하고, 공간 초대를 먼저 수락하면 조직에도 가입한다(⑬).
/// 이후 조직 초대를 수락해도 기존 멤버 역할을 유지한다. 조직 초대 생성은 관리자만 할 수 있다.
#[tokio::test]
async fn workspace_invitation_over_http_joins_the_organization_before_its_pending_invitation() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "owner").await;
    let ws = create_workspace(&app, &owner, "조직 초대").await;
    let org = common::organization_of_workspace(&ws).await;
    let email = unique_email("newcomer");
    let newcomer = login_with_email(&state, "newcomer", &email, true).await;
    let ws_inv = invite(&app, &owner, &ws, &email, "member").await;
    let ws_inv_id = ws_inv["id"].as_str().unwrap();

    let org_path = format!("/api/organizations/{org}/invitations");
    let input = json!({ "email": email, "role": "member" });
    // 조직을 볼 수 없는 사람에게는 404(존재를 드러내지 않음), 관리자가 아닌 조직 멤버에게는 403 — 2단계 상태 행렬과 같다.
    let stranger = login(&state, "stranger").await;
    let r = send(
        &app,
        Method::POST,
        &org_path,
        Some(&stranger),
        Some(input.clone()),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND, "{:?}", r.json);
    let plain = login(&state, "plain member").await;
    common::join_organization(org, &plain, "member").await;
    let r = send(
        &app,
        Method::POST,
        &org_path,
        Some(&plain),
        Some(input.clone()),
    )
    .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{:?}", r.json);
    let r = send(&app, Method::POST, &org_path, Some(&owner), Some(input)).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(r.json["role"], "member");
    let org_inv_id = r.json["id"].as_str().unwrap().to_owned();

    let mine = send(&app, Method::GET, "/api/invitations", Some(&newcomer), None).await;
    let listed = mine.json.as_array().unwrap();
    let org_row = listed
        .iter()
        .find(|v| v["id"] == org_inv_id.as_str())
        .expect("organization invitation listed");
    assert_eq!(org_row["kind"], "organization");
    assert_eq!(org_row["organization_id"], json!(org));
    assert!(org_row["workspace_id"].is_null() && org_row["workspace_name"].is_null());
    let ws_row = listed
        .iter()
        .find(|v| v["id"] == ws_inv_id)
        .expect("workspace invitation listed");
    assert_eq!(ws_row["kind"], "workspace");
    assert_eq!(ws_row["workspace_id"], ws.as_str());

    assert_eq!(accept(&app, &newcomer, ws_inv_id).await, StatusCode::OK);
    let mut tx = state.db.begin_as(newcomer.id).await.unwrap();
    let role: String = sqlx::query_scalar("SELECT role FROM app_my_organizations() WHERE id = $1")
        .bind(org)
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(role, "member", "공간 초대 수락이 조직 멤버 가입을 포함한다");

    let r = send(
        &app,
        Method::POST,
        &format!("/api/invitations/{org_inv_id}/accept"),
        Some(&newcomer),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["kind"], "organization");
    assert_eq!(r.json["organization_id"], json!(org));
    assert!(r.json["workspace_id"].is_null());
    assert_eq!(
        accept(&app, &newcomer, ws_inv_id).await,
        StatusCode::CONFLICT
    );
}
