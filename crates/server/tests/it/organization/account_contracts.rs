//! #178·D-105: 표시 설정과 마지막 조직은 각각 한 명령이다. D-96 기대 사용자도 두 경로에 적용한다.

use crate::common::{self, Reply, TestUser};
use crate::sql_support::World;
use axum::Router;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};
use uuid::Uuid;

async fn request(
    app: &Router,
    user: Option<&TestUser>,
    path: &str,
    body: Value,
    expected: Option<&str>,
) -> Reply {
    let mut headers = vec![("content-type", "application/json")];
    if let Some(expected) = expected {
        headers.push(("x-fvoci-expected-user", expected));
    }
    let (status, _, bytes) = common::send_raw(
        app,
        Method::PUT,
        path,
        user,
        &headers,
        serde_json::to_vec(&body).unwrap(),
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

async fn me(app: &Router, user: &TestUser) -> Value {
    let reply = common::send(app, Method::GET, "/api/me", Some(user), None).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.json);
    reply.json
}

#[tokio::test]
async fn last_organization_and_display_settings_are_independent_commands() {
    let w = World::new().await;
    let other = World::new().await;
    let app = common::app(&w.state);
    assert_eq!(
        me(&app, &w.owner).await["last_organization_id"],
        Value::Null
    );
    assert_eq!(
        request(
            &app,
            Some(&w.owner),
            "/api/me/settings",
            json!({"theme":"dark"}),
            None
        )
        .await
        .status,
        204
    );
    let selected = request(
        &app,
        Some(&w.owner),
        "/api/me/last-organization",
        json!({"organization_id":w.org}),
        None,
    )
    .await;
    assert_eq!(selected.status, 204, "{:?}", selected.json);
    assert_eq!(
        request(
            &app,
            Some(&w.owner),
            "/api/me/settings",
            json!({"locale":"en"}),
            None
        )
        .await
        .status,
        204
    );
    let saved = me(&app, &w.owner).await;
    assert_eq!(saved["last_organization_id"], json!(w.org));
    assert_eq!(saved["theme"], "dark");
    assert_eq!(saved["locale"], "en");
    for organization in [other.org, Uuid::now_v7()] {
        let rejected = request(
            &app,
            Some(&w.owner),
            "/api/me/last-organization",
            json!({"organization_id":organization}),
            None,
        )
        .await;
        assert_eq!(rejected.status, 400, "{:?}", rejected.json);
        assert_eq!(
            me(&app, &w.owner).await,
            saved,
            "a rejected organization must preserve all personal settings"
        );
    }
    let invalid_display = request(
        &app,
        Some(&w.owner),
        "/api/me/settings",
        json!({"theme":"invalid"}),
        None,
    )
    .await;
    assert_eq!(invalid_display.status, 400, "{:?}", invalid_display.json);
    assert_eq!(
        me(&app, &w.owner).await,
        saved,
        "invalid display settings preserve the selected organization"
    );
    assert_eq!(
        request(
            &app,
            Some(&w.owner),
            "/api/me/last-organization",
            json!({"organization_id":null}),
            None
        )
        .await
        .status,
        204
    );
    let cleared = me(&app, &w.owner).await;
    assert_eq!(cleared["last_organization_id"], Value::Null);
    assert_eq!(cleared["theme"], "dark");
    assert_eq!(cleared["locale"], "en");
    for (body, reason) in [
        (json!({"organization_id":"invalid"}), "invalid_format"),
        (json!({}), "required"),
    ] {
        let rejected = request(
            &app,
            Some(&w.owner),
            "/api/me/last-organization",
            body,
            None,
        )
        .await;
        assert_eq!(rejected.status, 400, "{:?}", rejected.json);
        assert_eq!(rejected.json["field"], "organization_id");
        assert_eq!(rejected.json["reason"], reason);
        assert_eq!(me(&app, &w.owner).await, cleared);
    }
}

#[tokio::test]
async fn display_settings_reject_the_retired_organization_field_without_changing_anything() {
    let w = World::new().await;
    let other = World::new().await;
    let app = common::app(&w.state);
    let before = me(&app, &w.owner).await;
    for organization in [
        json!(w.org),
        json!(other.org),
        json!(Uuid::now_v7()),
        Value::Null,
    ] {
        let reply = request(
            &app,
            Some(&w.owner),
            "/api/me/settings",
            json!({"last_organization_id":organization,"theme":"dark"}),
            None,
        )
        .await;
        assert_eq!(reply.status, 400, "{:?}", reply.json);
        assert_eq!(reply.json["error"], "bad_request");
        assert_eq!(reply.json["field"], "last_organization_id");
        assert_eq!(reply.json["reason"], "invalid_format");
        assert_eq!(me(&app, &w.owner).await, before);
    }
}

#[tokio::test]
async fn both_personal_commands_compare_a_sent_user_and_allow_an_omitted_user() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let matching = w.owner.id.0.to_string();
    let other = Uuid::now_v7().to_string();
    for (path, body, changed) in [
        ("/api/me/settings", json!({"theme":"dark"}), "theme"),
        (
            "/api/me/last-organization",
            json!({"organization_id":w.org}),
            "last_organization_id",
        ),
    ] {
        let before = me(&app, &w.owner).await;
        for (expected, status, error) in [
            (other.as_str(), 409, "account_changed"),
            ("invalid", 400, "bad_request"),
        ] {
            let reply = request(&app, Some(&w.owner), path, body.clone(), Some(expected)).await;
            assert_eq!(reply.status, status, "{path}: {:?}", reply.json);
            assert_eq!(reply.json["error"], error);
            assert_eq!(me(&app, &w.owner).await, before);
        }
        let unauthenticated = request(&app, None, path, body.clone(), Some(&matching)).await;
        assert_eq!(unauthenticated.status, 401, "{path}");
        assert_eq!(
            request(&app, Some(&w.owner), path, body.clone(), None)
                .await
                .status,
            204,
            "{path}"
        );
        assert_eq!(
            request(&app, Some(&w.owner), path, body, Some(&matching))
                .await
                .status,
            204,
            "{path}"
        );
        let after = me(&app, &w.owner).await;
        assert_ne!(after[changed], before[changed], "{path}");
        for field in ["theme", "locale", "last_organization_id"] {
            if field != changed {
                assert_eq!(after[field], before[field], "{path}: {field}");
            }
        }
    }
}
