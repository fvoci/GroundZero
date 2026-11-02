#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! D-96(#307): 항목·모듈 쓰기(서버 crate의 `item_profile`·`modules` 경로)도 보낸 기대 사용자를 비교하고, 거절 뒤
//! 아무것도 바꾸지 않는다. 머리가 없으면 지금처럼 저장한다(다른 도메인 라우터의 `ExpectedUserIfSent`와 같은 뜻).

use crate::common::{self, Reply, TestUser};
use crate::sql_support::World;
use axum::Router;
use axum::http::Method;
use serde_json::{Value, json};
use uuid::Uuid;

const COMMANDS: &[&str] = &[
    "set_item_profile",
    "set_item_members",
    "set_module_icon",
    "copy_item",
    "set_item_template",
    "create_module",
    "update_module",
    "archive_module",
    "set_module_description",
    "create_item",
    "update_item",
    "archive_item",
];

struct Command {
    method: Method,
    path: String,
    body: Value,
    success: u16,
}

async fn read(app: &Router, w: &World, path: &str) -> Value {
    let reply = common::send(app, Method::GET, path, Some(&w.owner), None).await;
    assert_eq!(reply.status, 200, "{path}: {:?}", reply.json);
    reply.json
}

async fn scenario(name: &str, w: &World, app: &Router, mate: &TestUser) -> Result<Command, String> {
    let (item, module) = (w.item, w.module);
    let (method, path, body, success) = match name {
        "set_item_profile" => {
            let path = format!("/api/items/{item}/profile");
            let version = read(app, w, &path).await["version"].clone();
            let body = json!({"base_version":version,"leader_id":null,"goal":"changed goal",
                "scope":"","starts_on":null,"ends_on":null,"status":"active","details":{}});
            (Method::PUT, path, body, 204)
        }
        "set_item_members" => (
            Method::PUT,
            format!("/api/items/{item}/members"),
            json!({"user_ids":[mate.id.0],"group_ids":[]}),
            204,
        ),
        "set_module_icon" => (
            Method::PUT,
            format!("/api/modules/{module}/icon"),
            json!({"icon":"🏺"}),
            204,
        ),
        "copy_item" => (
            Method::POST,
            format!("/api/items/{item}/copy"),
            json!({"request_id":Uuid::now_v7(),"module_id":module,"title":"copied","reset":false}),
            201,
        ),
        "set_item_template" => (
            Method::PUT,
            format!("/api/items/{item}/template"),
            json!({"template":true}),
            204,
        ),
        "create_module" => (
            Method::POST,
            format!("/api/workspaces/{}/modules", w.ws),
            json!({"request_id":Uuid::now_v7(),"kind":"project","name":"new module","key":"NEW"}),
            201,
        ),
        "update_module" => (
            Method::PUT,
            format!("/api/modules/{module}"),
            json!({"name":"renamed module","key":"SQL"}),
            200,
        ),
        "archive_module" => (
            Method::PUT,
            format!("/api/modules/{module}/archived"),
            json!({"archived":true}),
            200,
        ),
        "set_module_description" => (
            Method::PUT,
            format!("/api/modules/{module}/description"),
            json!({"description":"changed description"}),
            200,
        ),
        "create_item" => (
            Method::POST,
            format!("/api/modules/{module}/items"),
            json!({"request_id":Uuid::now_v7(),"title":"new item"}),
            201,
        ),
        "update_item" => {
            let path = format!("/api/items/{item}");
            let version = read(app, w, &path).await["version"].clone();
            let body =
                json!({"base_version":version,"title":"changed item","description":"changed"});
            (Method::PUT, path, body, 200)
        }
        "archive_item" => (
            Method::PUT,
            format!("/api/items/{item}/archived"),
            json!({"archived":true}),
            200,
        ),
        other => return Err(format!("unknown command fixture {other}")),
    };
    Ok(Command {
        method,
        path,
        body,
        success,
    })
}

/// 이 공간의 항목·모듈·업무·문서·구성원과, 공간 또는 요청한 사용자의 감사·활동.
async fn snapshot(w: &World) -> Value {
    let mut conn = common::fixture_conn().await;
    let mut result = serde_json::Map::new();
    for (table, predicate) in [
        ("modules", "workspace_id=$1"),
        ("module_keys", "workspace_id=$1"),
        ("items", "workspace_id=$1"),
        (
            "item_members",
            "item_id IN (SELECT id FROM fvoci.items WHERE workspace_id=$1)",
        ),
        ("tasks", "workspace_id=$1"),
        ("documents", "workspace_id=$1"),
        ("reference_counters", "workspace_id=$1"),
        ("numbered_targets", "workspace_id=$1"),
        ("audit_log", "workspace_id=$1 OR actor=$2"),
        ("activity", "workspace_id=$1 OR actor=$2"),
    ] {
        let rows: Value = sqlx::query_scalar(&format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM fvoci.{table} t WHERE {predicate}"
        ))
        .bind(w.ws)
        .bind(w.owner.id.0)
        .fetch_one(&mut conn)
        .await
        .unwrap();
        result.insert(table.to_owned(), rows);
    }
    Value::Object(result)
}

async fn request(
    app: &Router,
    c: &Command,
    session: Option<&TestUser>,
    expected: &[&str],
) -> Reply {
    let mut headers = vec![("content-type", "application/json")];
    headers.extend(
        expected
            .iter()
            .map(|value| ("x-fvoci-expected-user", *value)),
    );
    let (status, _, bytes) = common::send_raw(
        app,
        c.method.clone(),
        &c.path,
        session,
        &headers,
        serde_json::to_vec(&c.body).unwrap(),
    )
    .await;
    Reply {
        status,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// 거절돼야 하는 요청들: 다른 사용자(A의 화면이 B 세션으로 보냄) 409, 형식 오류·중복 400, 세션 없음 401.
async fn refusals(
    name: &str,
    w: &World,
    app: &Router,
    c: &Command,
    started_by: &TestUser,
) -> Vec<String> {
    let mut failures = Vec::new();
    let owner = w.owner.id.0.to_string();
    let other = started_by.id.0.to_string();
    let cases: [(Option<&TestUser>, Vec<&str>, u16, &str); 4] = [
        (Some(&w.owner), vec![&other], 409, "account_changed"),
        (Some(&w.owner), vec!["invalid"], 400, "bad_request"),
        (Some(&w.owner), vec![&owner, &owner], 400, "bad_request"),
        (None, vec![&owner], 401, "unauthorized"),
    ];
    for (session, expected, status, error) in cases {
        let before = snapshot(w).await;
        let reply = request(app, c, session, &expected).await;
        let unchanged = snapshot(w).await == before;
        if reply.status != status || reply.json["error"] != error || !unchanged {
            failures.push(format!(
                "{name}: sent {expected:?} with session={} must be {status} {error} with no changes; got {} {:?}, unchanged={unchanged}",
                session.is_some(),
                reply.status,
                reply.json
            ));
        }
    }
    failures
}

#[tokio::test]
async fn every_item_and_module_write_compares_a_sent_user_before_changing_anything() {
    let mut failures = Vec::new();
    for name in COMMANDS {
        // 머리 없음(지금처럼 저장)과 같은 사용자(저장)를 각각 새 공간에서 본다.
        for sent in [false, true] {
            let w = World::new().await;
            let app = common::app(&w.state);
            let mate = w.member("member").await;
            let c = scenario(name, &w, &app, &mate).await.unwrap();
            if sent {
                failures.extend(refusals(name, &w, &app, &c, &mate).await);
            }
            let owner = w.owner.id.0.to_string();
            let expected: Vec<&str> = if sent { vec![&owner] } else { vec![] };
            let before = snapshot(&w).await;
            let reply = request(&app, &c, Some(&w.owner), &expected).await;
            if reply.status != c.success || snapshot(&w).await == before {
                failures.push(format!(
                    "{name}: sent={sent} must save like before ({}); got {} {:?}",
                    c.success, reply.status, reply.json
                ));
            }
            w.state.db.close().await;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn item_reads_ignore_a_sent_expected_user() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let other = Uuid::now_v7().to_string();
    for path in [
        format!("/api/items/{}", w.item),
        format!("/api/items/{}/profile", w.item),
        format!("/api/modules/{}", w.module),
        format!("/api/modules/{}/items", w.module),
    ] {
        let (status, _, _) = common::send_raw(
            &app,
            Method::GET,
            &path,
            Some(&w.owner),
            &[("x-fvoci-expected-user", &other)],
            vec![],
        )
        .await;
        assert_eq!(status, 200, "{path}");
    }
    w.state.db.close().await;
}
