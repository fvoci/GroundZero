#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! D-112: 실제 HTTP·앱 역할로 조직 관리와 내용 접근 경계를 검증한다.
use crate::common;
use crate::sql_support;

use axum::Router;
use axum::http::{Method, StatusCode};
use common::{Reply, TestUser, login, send};
use serde_json::{Value, json};
use sql_support::{World, fixture, made, one, rows};
use uuid::Uuid;

/// 공통 헬퍼를 수정하지 않고 설정 폼의 기대 사용자 머리를 붙인다.
async fn request(
    app: &Router,
    method: Method,
    path: &str,
    user: &TestUser,
    body: Option<Value>,
) -> Reply {
    if method != Method::PUT || !(path.ends_with("/settings") || path.ends_with("/limits")) {
        return send(app, method, path, Some(user), body).await;
    }
    let expected = user.id.0.to_string();
    let (status, _, bytes) = common::send_raw(
        app,
        method,
        path,
        Some(user),
        &[
            ("content-type", "application/json"),
            ("x-fvoci-expected-user", &expected),
        ],
        serde_json::to_vec(&body.unwrap()).unwrap(),
    )
    .await;
    Reply {
        status,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

async fn expect(
    app: &Router,
    method: Method,
    path: &str,
    user: &TestUser,
    body: Option<Value>,
    status: u16,
) -> Value {
    let reply = request(app, method.clone(), path, user, body).await;
    assert_eq!(
        reply.status.as_u16(),
        status,
        "{method} {path}: {:?}",
        reply.json
    );
    reply.json
}

#[tokio::test]
async fn organization_api_status_matrix_and_email_visibility() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let admin = login(&w.state, "organization admin only").await;
    common::join_organization(w.org, &admin, "admin").await;
    let member = login(&w.state, "organization member only").await;
    common::join_organization(w.org, &member, "member").await;
    let outsider = login(&w.state, "outsider").await;
    let server = login(&w.state, "server admin only").await;
    common::make_server_admin(&server).await;
    let ws_member = w.member("member").await;
    // login 기본 fixture는 이메일이 없다. 마스킹 여부를 비교할 조직 멤버 모두에게 실제 값을 준다.
    fixture(
        "UPDATE fvoci.users SET email = id::text || '@example.test', email_verified = true WHERE id IN ($1,$2,$3)",
        &[w.owner.id.0, admin.id.0, member.id.0],
    ).await;
    let removable = login(&w.state, "removable").await;
    common::join_organization(w.org, &removable, "member").await;
    let org = format!("/api/organizations/{}", w.org);
    let limits = format!("/api/workspaces/{}/limits", w.ws);
    // 열: 조직 관리자(공간 밖), 조직 멤버(공간 밖), 조직 밖, 서버 관리자(조직 밖), 공간 멤버.
    let cases = [
        (Method::GET, org.clone(), None, [200, 200, 404, 404, 200]),
        (
            Method::GET,
            format!("{org}/members"),
            None,
            [200, 200, 404, 404, 200],
        ),
        (
            Method::PUT,
            format!("{org}/members/{}", removable.id.0),
            Some(json!({"role":"member"})),
            [204, 403, 404, 404, 403],
        ),
        (
            Method::GET,
            format!("{org}/members/{}/removal-preview", w.owner.id.0),
            None,
            [200, 403, 404, 404, 403],
        ),
        (
            Method::DELETE,
            format!("{org}/members/{}", removable.id.0),
            None,
            [204, 403, 404, 404, 403],
        ),
        (
            Method::GET,
            format!("{org}/invitations"),
            None,
            [200, 403, 404, 404, 403],
        ),
        (
            Method::POST,
            format!("{org}/invitations"),
            Some(json!({"email": common::unique_email("matrix"), "role":"member"})),
            [201, 403, 404, 404, 403],
        ),
        (
            Method::PUT,
            format!("{org}/settings"),
            Some(json!({"max_file_bytes": 123})),
            [200, 403, 404, 404, 403],
        ),
        (Method::GET, limits.clone(), None, [404, 404, 404, 404, 403]),
        (
            Method::PUT,
            limits,
            Some(json!({"max_file_bytes": 100})),
            [404, 404, 404, 404, 403],
        ),
        (
            Method::GET,
            format!("{org}/workspaces"),
            None,
            [200, 403, 404, 404, 403],
        ),
        (
            Method::POST,
            format!("{org}/workspaces/{}/join-as-admin", w.ws),
            None,
            [204, 403, 404, 404, 403],
        ),
        (
            Method::GET,
            "/api/server/organizations".into(),
            None,
            [403, 403, 403, 200, 403],
        ),
        (
            Method::GET,
            "/api/server/admins".into(),
            None,
            [403, 403, 403, 200, 403],
        ),
        (
            Method::POST,
            "/api/server/organizations".into(),
            Some(
                json!({"request_id":Uuid::now_v7(),"name":"created","first_admin_user_id":admin.id}),
            ),
            [403, 403, 403, 201, 403],
        ),
    ];
    for (method, path, body, statuses) in cases {
        for (user, status) in [&admin, &member, &outsider, &server, &ws_member]
            .into_iter()
            .zip(statuses)
        {
            expect(&app, method.clone(), &path, user, body.clone(), status).await;
        }
    }
    for (user, sees_email) in [(&admin, true), (&member, false), (&ws_member, false)] {
        let members = expect(
            &app,
            Method::GET,
            &format!("{org}/members"),
            user,
            None,
            200,
        )
        .await;
        assert!(
            members
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["email"].is_string() == sees_email)
        );
    }
    let absent = format!("/api/organizations/{}", Uuid::now_v7());
    for suffix in ["", "/members", "/workspaces", "/invitations"] {
        expect(
            &app,
            Method::GET,
            &format!("{absent}{suffix}"),
            &admin,
            None,
            404,
        )
        .await;
    }
}

#[tokio::test]
async fn last_admin_owner_guard_and_removal_preview_match_cleanup() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let org = format!("/api/organizations/{}", w.org);
    let owner = format!("{org}/members/{}", w.owner.id.0);
    expect(
        &app,
        Method::PUT,
        &owner,
        &w.owner,
        Some(json!({"role":"member"})),
        409,
    )
    .await;
    let second_admin = login(&w.state, "second admin").await;
    expect(
        &app,
        Method::PUT,
        &format!("{org}/members/{}", second_admin.id.0),
        &w.owner,
        Some(json!({"role":"admin"})),
        204,
    )
    .await;
    let owner_preview = expect(
        &app,
        Method::GET,
        &format!("{owner}/removal-preview"),
        &second_admin,
        None,
        200,
    )
    .await;
    assert_eq!(
        owner_preview,
        json!({"workspaces":1,"shares":0,"assignments":0})
    );
    expect(&app, Method::DELETE, &owner, &second_admin, None, 409).await;
    assert_eq!(
        expect(
            &app,
            Method::GET,
            &format!("{owner}/removal-preview"),
            &second_admin,
            None,
            200
        )
        .await,
        owner_preview
    );

    let member = w.member("member").await;
    let ws2 = sql_support::workspace(&w.state, &w.owner, w.org).await;
    fixture(
        "INSERT INTO fvoci.workspace_members(workspace_id,user_id,role) VALUES($1,$2,'member')",
        &[ws2, member.id.0],
    )
    .await;
    let doc = w.doc().await;
    let note = w.note().await;
    let team_share = w.share(doc, &member, "read").await;
    let note_share = w.share(note, &member, "write").await;
    let task = made(&w.state,&w.owner,"SELECT fvoci.app_create_task($1,$2,'assigned',NULL,'','할 일','none',NULL,NULL,NULL,'{}',ARRAY[$3],false,@ids) AS id",&[w.item,Uuid::now_v7(),member.id.0]).await;
    let path = format!("{org}/members/{}", member.id.0);
    let preview = format!("{path}/removal-preview");
    let before = json!({"workspaces":2,"shares":2,"assignments":1});
    assert_eq!(
        expect(&app, Method::GET, &preview, &w.owner, None, 200).await,
        before
    );
    assert_eq!(
        expect(&app, Method::GET, &preview, &member, None, 200).await,
        before
    );
    expect(&app, Method::DELETE, &path, &w.owner, None, 204).await;
    assert_eq!(
        expect(&app, Method::GET, &preview, &w.owner, None, 200).await,
        json!({"workspaces":0,"shares":0,"assignments":0})
    );
    assert!(rows(&w.state,&w.owner,"SELECT user_id FROM fvoci.workspace_members WHERE workspace_id IN ($1,$2) AND user_id=$3",&[w.ws,ws2,member.id.0]).await.is_empty());
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT id FROM fvoci.document_shares WHERE id IN ($1,$2)",
            &[team_share, note_share]
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &w.state,
            &w.owner,
            "SELECT user_id FROM fvoci.task_assignees WHERE task_id=$1 AND user_id=$2",
            &[task, member.id.0]
        )
        .await
        .is_empty()
    );
    for target in [doc, note] {
        expect(
            &app,
            Method::GET,
            &format!("/api/documents/{target}"),
            &member,
            None,
            404,
        )
        .await;
    }
    expect(&app, Method::GET, &org, &member, None, 404).await;
    expect(&app, Method::DELETE, &path, &w.owner, None, 404).await;
    // 본인의 조직 탈퇴도 SQL이 허용한다.
    let leaving = login(&w.state, "leaving").await;
    common::join_organization(w.org, &leaving, "member").await;
    expect(
        &app,
        Method::DELETE,
        &format!("{org}/members/{}", leaving.id.0),
        &leaving,
        None,
        204,
    )
    .await;
}

#[tokio::test]
async fn organization_and_workspace_invitations_have_typed_context_and_responses() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let email = common::unique_email("invited");
    let invitee = common::login_with_email(&w.state, "invitee", &email, true).await;
    let unverified = common::login_with_email(&w.state, "unverified", &email, false).await;
    let stranger = login(&w.state, "stranger").await;
    let org_path = format!("/api/organizations/{}/invitations", w.org);
    let input = json!({"email":email,"role":"member"});
    let org_inv = expect(
        &app,
        Method::POST,
        &org_path,
        &w.owner,
        Some(input.clone()),
        201,
    )
    .await;
    let org_id = org_inv["id"].as_str().unwrap();
    assert!(
        time::OffsetDateTime::parse(
            org_inv["expires_at"].as_str().unwrap(),
            &time::format_description::well_known::Rfc3339
        )
        .is_ok()
    );
    let ws_inv = expect(
        &app,
        Method::POST,
        &format!("/api/workspaces/{}/invitations", w.ws),
        &w.owner,
        Some(input.clone()),
        201,
    )
    .await;
    let ws_id = ws_inv["id"].as_str().unwrap();
    let listed = expect(&app, Method::GET, &org_path, &w.owner, None, 200).await;
    assert_eq!(listed.as_array().unwrap().len(), 1);
    assert_eq!(listed[0]["id"], org_id);
    let mine = expect(&app, Method::GET, "/api/invitations", &invitee, None, 200).await;
    assert_eq!(mine.as_array().unwrap().len(), 2);
    for (id, kind, ws) in [
        (org_id, "organization", Value::Null),
        (ws_id, "workspace", json!(w.ws)),
    ] {
        let inv = mine
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap();
        assert_eq!(inv["kind"], kind);
        assert_eq!(inv["organization_id"], json!(w.org));
        assert!(inv["organization_name"].is_string());
        assert_eq!(inv["workspace_id"], ws);
        assert_eq!(inv["workspace_name"].is_null(), kind == "organization");
    }
    for user in [&unverified, &stranger] {
        assert_eq!(
            expect(&app, Method::GET, "/api/invitations", user, None, 200).await,
            json!([])
        );
        expect(
            &app,
            Method::POST,
            &format!("/api/invitations/{org_id}/accept"),
            user,
            None,
            404,
        )
        .await;
    }
    for (id, kind, ws) in [
        (ws_id, "workspace", json!(w.ws)),
        (org_id, "organization", Value::Null),
    ] {
        let accepted = expect(
            &app,
            Method::POST,
            &format!("/api/invitations/{id}/accept"),
            &invitee,
            None,
            200,
        )
        .await;
        assert_eq!(
            accepted,
            json!({"kind":kind,"organization_id":w.org,"workspace_id":ws})
        );
        assert_eq!(one(&w.state, &invitee, "SELECT role FROM fvoci.organization_members WHERE organization_id=$1 AND user_id=$2", &[w.org, invitee.id.0]).await["role"], "member");
        expect(
            &app,
            Method::POST,
            &format!("/api/invitations/{id}/accept"),
            &invitee,
            None,
            409,
        )
        .await;
    }
    assert_eq!(one(&w.state, &w.owner, "SELECT count(*)::integer AS n FROM fvoci.audit_log WHERE organization_id=$1 AND target_id=$2 AND detail->>'via'='workspace_invitation'", &[w.org, invitee.id.0]).await["n"], 1);
    assert_eq!(
        expect(&app, Method::GET, "/api/invitations", &invitee, None, 200).await,
        json!([])
    );
    expect(&app, Method::POST, &org_path, &w.owner, Some(input), 409).await;
    let email = common::unique_email("decline");
    let declining = common::login_with_email(&w.state, "declining", &email, true).await;
    for action in ["decline", "revoke"] {
        let inv = expect(
            &app,
            Method::POST,
            &org_path,
            &w.owner,
            Some(json!({"email":email,"role":"admin"})),
            201,
        )
        .await;
        let id = inv["id"].as_str().unwrap();
        if action == "decline" {
            expect(
                &app,
                Method::POST,
                &format!("/api/invitations/{id}/decline"),
                &declining,
                None,
                204,
            )
            .await;
        } else {
            for (user, status) in [(&invitee, 403), (&stranger, 404)] {
                expect(
                    &app,
                    Method::DELETE,
                    &format!("/api/invitations/{id}"),
                    user,
                    None,
                    status,
                )
                .await;
            }
            expect(
                &app,
                Method::DELETE,
                &format!("/api/invitations/{id}"),
                &w.owner,
                None,
                204,
            )
            .await;
        }
        expect(
            &app,
            Method::POST,
            &format!("/api/invitations/{id}/accept"),
            &declining,
            None,
            409,
        )
        .await;
        assert_eq!(
            expect(&app, Method::GET, &org_path, &w.owner, None, 200).await,
            json!([])
        );
    }
}

#[tokio::test]
async fn organization_names_on_create_use_configured_unicode_validation() {
    let state =
        fvoci_server::app::build_state(common::config_with(&[("FVOCI_MAX_TITLE_CHARS", "4")]))
            .unwrap();
    let app = common::app(&state);
    let server = login(&state, "name-validation-server").await;
    common::make_server_admin(&server).await;
    let first = login(&state, "name-validation-admin").await;
    let mut failures = Vec::new();
    for name in ["", " \t\u{2003}\u{00a0}", "a\nb", "a\u{7f}b", "가나다라마"] {
        let request_id = Uuid::now_v7();
        let response = request(
            &app,
            Method::POST,
            "/api/server/organizations",
            &server,
            Some(json!({"request_id":request_id,"name":name,"first_admin_user_id":first.id})),
        )
        .await;
        if response.status != StatusCode::BAD_REQUEST {
            failures.push(format!(
                "create {name:?}: expected400, got {} {}",
                response.status, response.json
            ));
        }
        if !rows(
            &state,
            &first,
            "SELECT id FROM fvoci.organizations WHERE request_id=$1",
            &[request_id],
        )
        .await
        .is_empty()
        {
            failures.push(format!("create {name:?}: rejected name was stored"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for (raw, normalized) in [
        ("\t\u{2003}한글😀é\u{00a0}\n", "한글😀é"),
        (" e\u{301}😀a ", "e\u{301}😀a"),
        (" 가 나😀 ", "가 나😀"),
    ] {
        let created = expect(
            &app,
            Method::POST,
            "/api/server/organizations",
            &server,
            Some(json!({"request_id":Uuid::now_v7(),"name":raw,"first_admin_user_id":first.id})),
            201,
        )
        .await;
        let org = format!("/api/organizations/{}", created["id"].as_str().unwrap());
        assert_eq!(
            expect(&app, Method::GET, &org, &first, None, 200).await["name"],
            normalized
        );
    }
    state.db.close().await;
}

#[tokio::test]
async fn organization_names_on_settings_preserve_patch_semantics() {
    let state =
        fvoci_server::app::build_state(common::config_with(&[("FVOCI_MAX_TITLE_CHARS", "4")]))
            .unwrap();
    let app = common::app(&state);
    let server = login(&state, "settings-name-server").await;
    common::make_server_admin(&server).await;
    let admin = login(&state, "settings-name-admin").await;
    let created = expect(
        &app,
        Method::POST,
        "/api/server/organizations",
        &server,
        Some(json!({"request_id":Uuid::now_v7(),"name":"기존","first_admin_user_id":admin.id})),
        201,
    )
    .await;
    let org = format!("/api/organizations/{}", created["id"].as_str().unwrap());
    let settings = format!("{org}/settings");
    expect(&app, Method::PUT, &settings, &admin,
        Some(json!({"max_file_bytes":10,"max_storage_bytes":2,"max_concurrent_uploads":3,"public_links_enabled":false})), 200).await;
    let mut failures = Vec::new();
    for name in ["", " \t\u{2003}\u{00a0}", "a\nb", "a\u{7f}b", "가나다라마"] {
        let before = expect(&app, Method::GET, &org, &admin, None, 200).await;
        let response = request(
            &app,
            Method::PUT,
            &settings,
            &admin,
            Some(json!({"name":name,"max_file_bytes":null,"public_links_enabled":true})),
        )
        .await;
        if response.status != StatusCode::BAD_REQUEST {
            failures.push(format!(
                "settings {name:?}: expected400, got {} {}",
                response.status, response.json
            ));
        }
        if expect(&app, Method::GET, &org, &admin, None, 200).await != before {
            failures.push(format!("settings {name:?}: rejection changed settings"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));

    let renamed = expect(&app, Method::PUT, &settings, &admin,
        Some(json!({"name":"\t\u{2003}한글😀é\u{00a0}\n","max_file_bytes":null,"max_storage_bytes":7,"public_links_enabled":true})), 200).await;
    assert_eq!(renamed["name"], "한글😀é");
    assert!(renamed["limits"]["max_file_bytes"].is_null());
    assert_eq!(renamed["limits"]["max_storage_bytes"], 7);
    assert_eq!(renamed["limits"]["max_concurrent_uploads"], 3);
    assert_eq!(renamed["public_links_enabled"], true);
    let unchanged_name = expect(
        &app,
        Method::PUT,
        &settings,
        &admin,
        Some(json!({"max_storage_bytes":8})),
        200,
    )
    .await;
    assert_eq!(unchanged_name["name"], "한글😀é");
    for rejected in [
        json!({"name":null,"max_storage_bytes":9}),
        json!({"name":"\u{2003}이름\u{00a0}","max_file_bytes":null,"unknown":{"nested":[null,1]}}),
    ] {
        expect(&app, Method::PUT, &settings, &admin, Some(rejected), 400).await;
        assert_eq!(
            expect(&app, Method::GET, &org, &admin, None, 200).await,
            unchanged_name
        );
    }
    state.db.close().await;
}

#[tokio::test]
async fn settings_validate_patch_preserve_null_and_require_expected_user() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let org = format!("/api/organizations/{}", w.org);
    let settings = format!("{org}/settings");
    let limits = format!("/api/workspaces/{}/limits", w.ws);
    let patch = json!({"max_file_bytes":100,"max_storage_bytes":0,"max_concurrent_uploads":2,"notification_rate_per_hour":3,"activity_retention_days":4,"audit_retention_days":3650});
    let updated = expect(
        &app,
        Method::PUT,
        &settings,
        &w.owner,
        Some(patch.clone()),
        200,
    )
    .await;
    assert_eq!(updated["limits"], patch);
    assert_eq!(
        updated,
        expect(&app, Method::GET, &org, &w.owner, None, 200).await
    );
    assert_eq!(
        expect(
            &app,
            Method::PUT,
            &limits,
            &w.owner,
            Some(patch.clone()),
            200
        )
        .await,
        patch
    );
    assert_eq!(
        expect(&app, Method::GET, &limits, &w.owner, None, 200).await,
        patch
    );
    for path in [&settings, &limits] {
        for bad in [
            json!({"max_file_bytes":0}),
            json!({"max_storage_bytes":-1}),
            json!({"max_concurrent_uploads":0}),
            json!({"notification_rate_per_hour":0}),
            json!({"activity_retention_days":0}),
            json!({"audit_retention_days":3651}),
            json!({"max_concurrent_uploads":2147483648_i64}),
            json!({"max_file_bytes":"x"}),
            json!({"max_file_bytes":1.5}),
            json!({"unknown":1}),
            json!([]),
            Value::Null,
        ] {
            expect(&app, Method::PUT, path, &w.owner, Some(bad), 400).await;
        }
        for (expected, status, code) in [
            (None, 400, "bad_request"),
            (Some(Uuid::now_v7().to_string()), 409, "account_changed"),
        ] {
            let mut headers = vec![("content-type", "application/json")];
            if let Some(value) = expected.as_deref() {
                headers.push(("x-fvoci-expected-user", value));
            }
            let (actual, _, bytes) = common::send_raw(
                &app,
                Method::PUT,
                path,
                Some(&w.owner),
                &headers,
                serde_json::to_vec(&json!({"max_file_bytes":999})).unwrap(),
            )
            .await;
            assert_eq!(actual.as_u16(), status);
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap()["error"],
                code
            );
        }
    }
    for bad in [
        json!({"name":" "}),
        json!({"name":null}),
        json!({"public_links_enabled":"x"}),
    ] {
        expect(&app, Method::PUT, &settings, &w.owner, Some(bad), 400).await;
    }
    expect(
        &app,
        Method::PUT,
        &limits,
        &w.owner,
        Some(json!({"name":"invalid"})),
        400,
    )
    .await;
    assert_eq!(
        expect(&app, Method::GET, &limits, &w.owner, None, 200).await,
        patch
    );
    let renamed = expect(
        &app,
        Method::PUT,
        &settings,
        &w.owner,
        Some(json!({"name":" renamed "})),
        200,
    )
    .await;
    assert_eq!(renamed["name"], "renamed");
    assert_eq!(renamed["limits"], patch);
    for path in [&settings, &limits] {
        expect(
            &app,
            Method::PUT,
            path,
            &w.owner,
            Some(json!({"max_file_bytes":null})),
            200,
        )
        .await;
    }
    let configured = expect(&app, Method::GET, &limits, &w.owner, None, 200).await;
    assert!(configured["max_file_bytes"].is_null());
    assert_eq!(configured["max_concurrent_uploads"], 2);
    // 조직 한도가 더 작아도 GET은 실효값 대신 공간 설정값을 돌려준다.
    expect(
        &app,
        Method::PUT,
        &settings,
        &w.owner,
        Some(json!({"max_file_bytes":10})),
        200,
    )
    .await;
    expect(
        &app,
        Method::PUT,
        &limits,
        &w.owner,
        Some(json!({"max_file_bytes":20})),
        200,
    )
    .await;
    assert_eq!(
        expect(&app, Method::GET, &limits, &w.owner, None, 200).await["max_file_bytes"],
        20
    );
}

#[tokio::test]
async fn settings_reject_unknown_nested_fields_without_applying_known_changes() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let org = format!("/api/organizations/{}", w.org);
    let settings = format!("{org}/settings");
    let limits = format!("/api/workspaces/{}/limits", w.ws);

    for (read_path, write_path) in [(&org, &settings), (&limits, &limits)] {
        expect(
            &app,
            Method::PUT,
            write_path,
            &w.owner,
            Some(json!({"max_file_bytes":100,"max_concurrent_uploads":2})),
            200,
        )
        .await;
        let before = expect(&app, Method::GET, read_path, &w.owner, None, 200).await;

        let rejected = expect(
            &app,
            Method::PUT,
            write_path,
            &w.owner,
            Some(json!({
                "max_file_bytes":null,
                "max_concurrent_uploads":3,
                "unknown":{"nested":[null, {"max_file_bytes":1}]}
            })),
            400,
        )
        .await;
        assert_eq!(rejected["error"], "bad_request", "{write_path}");
        assert_eq!(
            expect(&app, Method::GET, read_path, &w.owner, None, 200).await,
            before,
            "{write_path}: rejecting an unknown field must preserve all settings"
        );
    }
}

#[tokio::test]
async fn public_link_switch_hides_existing_links_and_blocks_creation() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let doc = w.doc().await;
    let links = format!("/api/documents/{doc}/public-links");
    let created = expect(&app, Method::POST, &links, &w.owner, Some(json!({})), 201).await;
    let token = created["url"].as_str().unwrap().rsplit('/').next().unwrap();
    let public = format!("/api/public/{token}");
    assert_eq!(
        send(&app, Method::GET, &public, None, None).await.status,
        StatusCode::OK
    );
    let settings = format!("/api/organizations/{}/settings", w.org);
    expect(
        &app,
        Method::PUT,
        &settings,
        &w.owner,
        Some(json!({"public_links_enabled":false})),
        200,
    )
    .await;
    assert_eq!(
        send(&app, Method::GET, &public, None, None).await.status,
        StatusCode::NOT_FOUND
    );
    expect(&app, Method::POST, &links, &w.owner, Some(json!({})), 409).await;
    let reset = expect(
        &app,
        Method::PUT,
        &settings,
        &w.owner,
        Some(json!({"public_links_enabled":null})),
        200,
    )
    .await;
    assert_eq!(reset["public_links_enabled"], true);
    assert_eq!(
        send(&app, Method::GET, &public, None, None).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn admin_join_is_explicit_scoped_and_audited_twice_server_has_no_content() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let admin = login(&w.state, "organization admin").await;
    common::join_organization(w.org, &admin, "admin").await;
    let server = login(&w.state, "server admin").await;
    common::make_server_admin(&server).await;
    let doc = w.doc().await;
    let task = w.task().await;
    let file = sql_support::attachment(&w, doc).await;
    for user in [&admin, &server] {
        for path in [
            format!("/api/documents/{doc}"),
            format!("/api/items/{}", w.item),
            format!("/api/tasks/{task}"),
            format!("/api/files/{file}"),
            format!(
                "/api/search?q=SQL&workspace_id={}&organization_id={}",
                w.ws, w.org
            ),
        ] {
            expect(&app, Method::GET, &path, user, None, 404).await;
        }
    }
    let org_ws = format!("/api/organizations/{}/workspaces", w.org);
    let before = expect(&app, Method::GET, &org_ws, &admin, None, 200).await;
    assert_eq!(before[0]["owner_id"], json!(w.owner.id));
    assert_eq!(before[0]["member_count"], 1);
    assert!(before[0]["my_role"].is_null());
    assert!(before[0]["deleted_at"].is_null());
    let other_owner = login(&w.state, "other organization owner").await;
    let other_org = common::organization_for(&other_owner).await;
    common::join_organization(other_org, &admin, "admin").await;
    expect(
        &app,
        Method::POST,
        &format!(
            "/api/organizations/{other_org}/workspaces/{}/join-as-admin",
            w.ws
        ),
        &admin,
        None,
        404,
    )
    .await;
    let join = format!("{org_ws}/{}/join-as-admin", w.ws);
    expect(&app, Method::POST, &join, &admin, None, 204).await;
    expect(&app, Method::POST, &join, &admin, None, 204).await;
    expect(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}"),
        &admin,
        None,
        200,
    )
    .await;
    expect(
        &app,
        Method::GET,
        &format!("/api/documents/{doc}"),
        &server,
        None,
        404,
    )
    .await;
    let after = expect(&app, Method::GET, &org_ws, &admin, None, 200).await;
    assert_eq!(after[0]["my_role"], "admin");
    assert_eq!(after[0]["member_count"], 2);
    let audit = rows(&w.state,&admin,"SELECT workspace_id,organization_id FROM fvoci.audit_log WHERE actor=$1 AND action='admin_join' ORDER BY workspace_id NULLS FIRST",&[admin.id.0]).await;
    assert_eq!(
        audit,
        vec![
            json!({"workspace_id":null,"organization_id":w.org}),
            json!({"workspace_id":w.ws,"organization_id":null})
        ]
    );
    let note = w.note().await;
    for user in [&admin, &server] {
        expect(
            &app,
            Method::GET,
            &format!("/api/documents/{note}"),
            user,
            None,
            404,
        )
        .await;
    }
    // 삭제된 공간도 메타데이터로 보이되 참여는 불가능하다.
    let deleted = sql_support::workspace(&w.state, &w.owner, w.org).await;
    fixture(
        "UPDATE fvoci.workspaces SET deleted_at=now() WHERE id=$1",
        &[deleted],
    )
    .await;
    let list = expect(&app, Method::GET, &org_ws, &admin, None, 200).await;
    assert!(
        list.as_array()
            .unwrap()
            .iter()
            .any(|ws| ws["id"] == json!(deleted) && ws["deleted_at"].is_string())
    );
    expect(
        &app,
        Method::POST,
        &format!("{org_ws}/{deleted}/join-as-admin"),
        &admin,
        None,
        404,
    )
    .await;
}

#[tokio::test]
async fn server_creates_idempotent_organization_without_granting_itself_membership() {
    let state = common::state();
    let app = common::app(&state);
    let server = login(&state, "server").await;
    common::make_server_admin(&server).await;
    let first = login(&state, "first admin").await;
    let input =
        json!({"request_id":Uuid::now_v7(),"name":" new org ","first_admin_user_id":first.id});
    let created = expect(
        &app,
        Method::POST,
        "/api/server/organizations",
        &server,
        Some(input.clone()),
        201,
    )
    .await;
    assert_eq!(
        expect(
            &app,
            Method::POST,
            "/api/server/organizations",
            &server,
            Some(input),
            201
        )
        .await,
        created
    );
    let id = created["id"].as_str().unwrap();
    let org = format!("/api/organizations/{id}");
    let detail = expect(&app, Method::GET, &org, &first, None, 200).await;
    assert_eq!(detail["name"], "new org");
    assert_eq!(detail["role"], "admin");
    expect(&app, Method::GET, &org, &server, None, 404).await;
    assert_eq!(
        expect(
            &app,
            Method::GET,
            "/api/me/organizations",
            &server,
            None,
            200
        )
        .await,
        json!([])
    );
    let all = expect(
        &app,
        Method::GET,
        "/api/server/organizations",
        &server,
        None,
        200,
    )
    .await;
    let row = all
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == id)
        .unwrap();
    assert_eq!(row["member_count"], 1);
    assert_eq!(row["workspace_count"], 0);
    let admins = expect(&app, Method::GET, "/api/server/admins", &server, None, 200).await;
    assert!(
        admins
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["user_id"] == json!(server.id))
    );
    for (name, first_id) in [(" ", first.id.0), ("invalid", Uuid::now_v7())] {
        expect(
            &app,
            Method::POST,
            "/api/server/organizations",
            &server,
            Some(json!({"request_id":Uuid::now_v7(),"name":name,"first_admin_user_id":first_id})),
            400,
        )
        .await;
    }
    assert_eq!(one(&state,&first,"SELECT count(*)::integer AS n FROM fvoci.organization_members WHERE organization_id=$1", &[Uuid::parse_str(id).unwrap()]).await["n"], 1);
}

/// 리뷰 후속(04be7645): `time`에 serde-human-readable이 없어 `#[serde(with = rfc3339)]`가 빠진 시각은 배열로 직렬화된다.
/// 조직 목록·상세의 `created_at`이 계약대로 RFC3339 문자열인지, 빈 `tz=`가 저장된 시간대로 처리되는지 고정한다.
#[tokio::test]
async fn organization_timestamps_are_rfc3339_and_empty_tz_means_default() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let rfc3339 = |v: &Value, what: &str| {
        let s = v
            .as_str()
            .unwrap_or_else(|| panic!("{what} must be a string: {v}"));
        time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|e| panic!("{what} must be RFC3339: {s} ({e})"));
    };
    let mine = expect(
        &app,
        Method::GET,
        "/api/me/organizations",
        &w.owner,
        None,
        200,
    )
    .await;
    let row = mine
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["id"] == json!(w.org))
        .expect("the owner's organization is listed");
    rfc3339(&row["created_at"], "organizations[].created_at");
    let detail = expect(
        &app,
        Method::GET,
        &format!("/api/organizations/{}", w.org),
        &w.owner,
        None,
        200,
    )
    .await;
    rfc3339(&detail["created_at"], "organization.created_at");
    expect(
        &app,
        Method::GET,
        &format!("/api/me/home?tz=&organization_id={}", w.org),
        &w.owner,
        None,
        200,
    )
    .await;
    expect(
        &app,
        Method::GET,
        &format!("/api/me/home?tz=invalid&organization_id={}", w.org),
        &w.owner,
        None,
        400,
    )
    .await;
}

#[tokio::test]
async fn organization_admin_restores_and_offers_ownership_without_joining() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let admin = login(&w.state, "restore organization admin").await;
    common::join_organization(w.org, &admin, "admin").await;
    let member = login(&w.state, "ordinary organization member").await;
    common::join_organization(w.org, &member, "member").await;
    let outsider = login(&w.state, "outside organization").await;
    let recipient = w.member("member").await;
    let foreign = World::new().await;
    common::join_organization(foreign.org, &admin, "admin").await;
    let base = format!("/api/organizations/{}/workspaces/{}", w.org, w.ws);
    let restore = format!("{base}/restore");
    let offer = format!("{base}/ownership-offer");
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_workspace($1,'SQL workspace',@ids)",
        &[w.ws],
    )
    .await;
    for (user, status) in [(&member, 403), (&outsider, 404)] {
        expect(&app, Method::POST, &restore, user, None, status).await;
    }
    expect(
        &app,
        Method::POST,
        &format!(
            "/api/organizations/{}/workspaces/{}/restore",
            foreign.org, w.ws
        ),
        &admin,
        None,
        404,
    )
    .await;
    expect(&app, Method::POST, &restore, &admin, None, 204).await;
    expect(&app, Method::POST, &restore, &admin, None, 404).await;
    assert!(
        one(
            &w.state,
            &w.owner,
            "SELECT deleted_at FROM fvoci.workspaces WHERE id=$1",
            &[w.ws]
        )
        .await["deleted_at"]
            .is_null()
    );
    let body = json!({"to_user":recipient.id});
    for (user, status) in [(&member, 403), (&outsider, 404)] {
        expect(&app, Method::POST, &offer, user, Some(body.clone()), status).await;
    }
    expect(
        &app,
        Method::POST,
        &format!(
            "/api/organizations/{}/workspaces/{}/ownership-offer",
            foreign.org, w.ws
        ),
        &admin,
        Some(body.clone()),
        404,
    )
    .await;
    expect(
        &app,
        Method::POST,
        &offer,
        &admin,
        Some(json!({"to_user":outsider.id})),
        400,
    )
    .await;
    expect(&app, Method::POST, &offer, &admin, Some(body), 204).await;
    assert_eq!(
        one(
            &w.state,
            &w.owner,
            "SELECT to_user FROM fvoci.ownership_offers WHERE workspace_id=$1",
            &[w.ws]
        )
        .await["to_user"],
        json!(recipient.id)
    );
    assert_eq!(one(&w.state, &w.owner, "SELECT count(*)::integer AS n FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2", &[w.ws, admin.id.0]).await["n"], 0, "metadata operations must not join the administrator");
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_cancel_ownership_offer($1,@ids)",
        &[w.ws],
    )
    .await;
    rows(
        &w.state,
        &w.owner,
        "SELECT fvoci.app_delete_workspace($1,'SQL workspace',@ids)",
        &[w.ws],
    )
    .await;
    fixture(
        "UPDATE fvoci.workspaces SET deleted_at = now() - interval '3660 days' WHERE id=$1",
        &[w.ws],
    )
    .await;
    let expired = expect(&app, Method::POST, &restore, &admin, None, 409).await;
    assert_eq!(
        expired["error"], "invalid_state",
        "other SQLSTATE 55000 errors retain their code"
    );
}
