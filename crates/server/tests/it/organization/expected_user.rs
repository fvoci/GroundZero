//! D-96: 모든 조직 도메인 명령은 보낸 기대 사용자를 비교하고 거절 뒤 아무것도 바꾸지 않는다.

use crate::common::{self, Reply, TestUser};
use crate::sql_support::{World, fixture, made, rows};
use axum::Router;
use axum::http::Method;
use serde_json::{Value, json};
use uuid::Uuid;

const COMMANDS: &[&str] = &[
    "display_settings",
    "last_organization",
    "create_organization",
    "organization_member",
    "remove_organization_member",
    "organization_invitation",
    "organization_settings",
    "workspace_limits",
    "join_as_admin",
    "restore_organization_workspace",
    "offer_organization_ownership",
    "create_workspace",
    "delete_workspace",
    "restore_workspace",
    "purge_workspace",
    "member_role",
    "remove_member",
    "leave",
    "workspace_invitation",
    "revoke_invitation",
    "accept_invitation",
    "decline_invitation",
    "offer_ownership",
    "cancel_ownership",
    "accept_ownership",
    "create_group",
    "delete_group",
    "add_group_member",
    "remove_group_member",
];

struct Command {
    method: Method,
    path: String,
    body: Option<Value>,
    actor: TestUser,
    success: u16,
    required: bool,
}

async fn invitation(w: &World) -> (Uuid, TestUser) {
    let email = common::unique_email("command-recipient");
    let recipient = common::login_with_email(&w.state, "command recipient", &email, true).await;
    let mut tx = w.state.db.begin_as(w.owner.id).await.unwrap();
    let id = sqlx::query_scalar("SELECT fvoci.app_invite($1,$2,'member',3600,$3)")
        .bind(w.ws)
        .bind(email)
        .bind(fvoci_db::new_ids(3))
        .fetch_one(tx.conn())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    (id, recipient)
}

async fn scenario(name: &str) -> (World, Command) {
    let w = World::new().await;
    let member = w.member("member").await;
    let mut c = Command {
        method: Method::POST,
        path: String::new(),
        body: None,
        actor: w.owner.clone(),
        success: 204,
        required: false,
    };
    match name {
        "display_settings" => {
            c.method = Method::PUT;
            c.path = "/api/me/settings".into();
            c.body = Some(json!({"theme":"dark"}));
        }
        "last_organization" => {
            c.method = Method::PUT;
            c.path = "/api/me/last-organization".into();
            c.body = Some(json!({"organization_id":w.org}));
        }
        "create_organization" => {
            common::make_server_admin(&w.owner).await;
            c.path = "/api/server/organizations".into();
            c.success = 201;
            c.body = Some(
                json!({"request_id":Uuid::now_v7(),"name":"new organization","first_admin_user_id":w.owner.id}),
            );
        }
        "organization_member" | "remove_organization_member" => {
            c.path = format!("/api/organizations/{}/members/{}", w.org, member.id);
            if name == "organization_member" {
                c.method = Method::PUT;
                c.body = Some(json!({"role":"admin"}));
            } else {
                c.method = Method::DELETE;
            }
        }
        "organization_invitation" => {
            c.path = format!("/api/organizations/{}/invitations", w.org);
            c.success = 201;
            c.body =
                Some(json!({"email":common::unique_email("organization-invite"),"role":"member"}));
        }
        "organization_settings" => {
            c.method = Method::PUT;
            c.path = format!("/api/organizations/{}/settings", w.org);
            c.body = Some(json!({"name":"changed organization"}));
            c.success = 200;
            c.required = true;
        }
        "workspace_limits" => {
            c.method = Method::PUT;
            c.path = format!("/api/workspaces/{}/limits", w.ws);
            c.body = Some(json!({"max_file_bytes":100}));
            c.success = 200;
            c.required = true;
        }
        "join_as_admin" => {
            common::join_organization(w.org, &member, "admin").await;
            fixture(
                "DELETE FROM fvoci.workspace_members WHERE workspace_id=$1 AND user_id=$2",
                &[w.ws, member.id.0],
            )
            .await;
            c.actor = member.clone();
            c.path = format!(
                "/api/organizations/{}/workspaces/{}/join-as-admin",
                w.org, w.ws
            );
        }
        "restore_organization_workspace" | "restore_workspace" | "purge_workspace" => {
            rows(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_delete_workspace($1,'SQL workspace',@ids)",
                &[w.ws],
            )
            .await;
            c.path = match name {
                "restore_organization_workspace" => {
                    format!("/api/organizations/{}/workspaces/{}/restore", w.org, w.ws)
                }
                "restore_workspace" => format!("/api/me/deleted-workspaces/{}/restore", w.ws),
                _ => {
                    c.method = Method::DELETE;
                    format!("/api/me/deleted-workspaces/{}", w.ws)
                }
            };
        }
        "offer_organization_ownership" | "offer_ownership" => {
            c.path = if name == "offer_organization_ownership" {
                format!(
                    "/api/organizations/{}/workspaces/{}/ownership-offer",
                    w.org, w.ws
                )
            } else {
                format!("/api/workspaces/{}/ownership", w.ws)
            };
            c.body = Some(json!({"to_user":member.id}));
        }
        "create_workspace" => {
            c.path = "/api/workspaces".into();
            c.success = 201;
            c.body = Some(
                json!({"request_id":Uuid::now_v7(),"organization_id":w.org,"name":"new workspace"}),
            );
        }
        "delete_workspace" => {
            c.method = Method::DELETE;
            c.path = format!("/api/workspaces/{}", w.ws);
            c.body = Some(json!({"name":"SQL workspace"}));
            c.success = 200;
        }
        "member_role" | "remove_member" => {
            c.path = format!("/api/workspaces/{}/members/{}", w.ws, member.id);
            if name == "member_role" {
                c.method = Method::PUT;
                c.body = Some(json!({"role":"readonly"}));
            } else {
                c.method = Method::DELETE;
            }
        }
        "leave" => {
            c.path = format!("/api/workspaces/{}/leave", w.ws);
            c.actor = member.clone();
        }
        "workspace_invitation" => {
            c.path = format!("/api/workspaces/{}/invitations", w.ws);
            c.success = 201;
            c.body =
                Some(json!({"email":common::unique_email("workspace-invite"),"role":"member"}));
        }
        "revoke_invitation" | "accept_invitation" | "decline_invitation" => {
            let (id, recipient) = invitation(&w).await;
            if name == "revoke_invitation" {
                c.method = Method::DELETE;
                c.path = format!("/api/invitations/{id}");
            } else {
                c.actor = recipient;
                c.path = format!(
                    "/api/invitations/{id}/{}",
                    if name == "accept_invitation" {
                        "accept"
                    } else {
                        "decline"
                    }
                );
                if name == "accept_invitation" {
                    c.success = 200;
                }
            }
        }
        "cancel_ownership" | "accept_ownership" => {
            rows(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_offer_ownership($1,$2,@ids)",
                &[w.ws, member.id.0],
            )
            .await;
            if name == "cancel_ownership" {
                c.method = Method::DELETE;
                c.path = format!("/api/workspaces/{}/ownership", w.ws);
            } else {
                c.actor = member.clone();
                c.path = format!("/api/workspaces/{}/ownership/accept", w.ws);
            }
        }
        "create_group" => {
            c.path = format!("/api/workspaces/{}/groups", w.ws);
            c.body = Some(json!({"name":"new group"}));
            c.success = 201;
        }
        "delete_group" | "add_group_member" | "remove_group_member" => {
            let group = made(
                &w.state,
                &w.owner,
                "SELECT fvoci.app_create_group($1,'existing group',@ids) AS id",
                &[w.ws],
            )
            .await;
            c.method = Method::DELETE;
            if name == "delete_group" {
                c.path = format!("/api/groups/{group}");
            } else {
                c.path = format!("/api/groups/{group}/members/{}", member.id);
                if name == "add_group_member" {
                    c.method = Method::PUT;
                } else {
                    rows(
                        &w.state,
                        &w.owner,
                        "SELECT fvoci.app_set_group_member($1,$2,true,@ids)",
                        &[group, member.id.0],
                    )
                    .await;
                }
            }
        }
        _ => panic!("unknown command fixture {name}"),
    }
    (w, c)
}

async fn snapshot(w: &World, actor: &TestUser) -> Value {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar("SELECT jsonb_build_object(
        'organizations',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id) FROM fvoci.organizations o
            WHERE o.id=$1 OR o.id IN (SELECT organization_id FROM fvoci.organization_members WHERE user_id=$2)),
        'organization_members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.organization_id,m.user_id)
            FROM fvoci.organization_members m WHERE m.organization_id=$1 OR m.user_id=$2),
        'workspaces',(SELECT jsonb_agg(to_jsonb(w) ORDER BY w.id) FROM fvoci.workspaces w WHERE w.organization_id=$1),
        'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.workspace_id,m.user_id) FROM fvoci.workspace_members m
            WHERE m.workspace_id IN (SELECT id FROM fvoci.workspaces WHERE organization_id=$1)),
        'invitations',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM fvoci.invitations i WHERE i.organization_id=$1),
        'groups',(SELECT jsonb_agg(to_jsonb(g) ORDER BY g.id) FROM fvoci.groups g
            WHERE g.workspace_id IN (SELECT id FROM fvoci.workspaces WHERE organization_id=$1)),
        'group_members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.group_id,m.user_id) FROM fvoci.group_members m
            JOIN fvoci.groups g ON g.id=m.group_id JOIN fvoci.workspaces w ON w.id=g.workspace_id WHERE w.organization_id=$1),
        'offers',(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.workspace_id) FROM fvoci.ownership_offers o
            WHERE o.workspace_id IN (SELECT id FROM fvoci.workspaces WHERE organization_id=$1)),
        'settings',(SELECT to_jsonb(s) FROM fvoci.user_settings s WHERE s.user_id=$2),
        'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM fvoci.audit_log a
            WHERE a.actor=$2 OR a.organization_id=$1 OR a.workspace_id IN (SELECT id FROM fvoci.workspaces WHERE organization_id=$1)))")
        .bind(w.org).bind(actor.id.0).fetch_one(&mut conn).await.unwrap()
}

async fn request(app: &Router, c: &Command, expected: &[&str], authenticated: bool) -> Reply {
    let mut headers = vec![("content-type", "application/json")];
    headers.extend(
        expected
            .iter()
            .map(|value| ("x-fvoci-expected-user", *value)),
    );
    let body = c
        .body
        .as_ref()
        .map(|body| serde_json::to_vec(body).unwrap())
        .unwrap_or_default();
    let (status, _, bytes) = common::send_raw(
        app,
        c.method.clone(),
        &c.path,
        authenticated.then_some(&c.actor),
        &headers,
        body,
    )
    .await;
    Reply {
        status,
        json: if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        },
    }
}

#[tokio::test]
async fn every_organization_command_compares_a_sent_user_without_changing_any_resource() {
    let mut failures = Vec::new();
    for name in COMMANDS {
        for sent in [false, true] {
            let (w, c) = scenario(name).await;
            let app = common::app(&w.state);
            let matching = c.actor.id.0.to_string();
            if !sent {
                let before = snapshot(&w, &c.actor).await;
                let other = Uuid::now_v7().to_string();
                let reply = request(&app, &c, &[&other], true).await;
                let unchanged = snapshot(&w, &c.actor).await == before;
                if reply.status != 409 || reply.json["error"] != "account_changed" || !unchanged {
                    failures.push(format!("{name}: mismatch must be 409 account_changed with no changes; got {} {:?}, unchanged={unchanged}", reply.status, reply.json));
                    w.state.db.close().await;
                    continue;
                }
                for expected in [vec!["invalid"], vec![matching.as_str(), matching.as_str()]] {
                    let reply = request(&app, &c, &expected, true).await;
                    if reply.status != 400
                        || reply.json["error"] != "bad_request"
                        || snapshot(&w, &c.actor).await != before
                    {
                        failures.push(format!("{name}: malformed or duplicate user must be 400 without changes: {} {:?}", reply.status, reply.json));
                    }
                }
                let reply = request(&app, &c, &[&matching], false).await;
                if reply.status != 401 || snapshot(&w, &c.actor).await != before {
                    failures.push(format!(
                        "{name}: no session must be 401 without changes: {} {:?}",
                        reply.status, reply.json
                    ));
                }
            }
            let before = snapshot(&w, &c.actor).await;
            let expected = if sent {
                vec![matching.as_str()]
            } else {
                vec![]
            };
            let reply = request(&app, &c, &expected, true).await;
            let wanted = if !sent && c.required { 400 } else { c.success };
            if reply.status != wanted {
                failures.push(format!(
                    "{name}: sent={sent}, expected {wanted}, got {} {:?}",
                    reply.status, reply.json
                ));
            }
            if !sent && c.required && snapshot(&w, &c.actor).await != before {
                failures.push(format!("{name}: omitting the required user changed data"));
            }
            w.state.db.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn organization_reads_keep_the_session_user_when_an_expected_user_is_sent() {
    let w = World::new().await;
    let other = Uuid::now_v7().to_string();
    let (status, _, bytes) = common::send_raw(
        &common::app(&w.state),
        Method::GET,
        "/api/me",
        Some(&w.owner),
        &[("x-fvoci-expected-user", &other)],
        vec![],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap()["id"],
        json!(w.owner.id)
    );
}
