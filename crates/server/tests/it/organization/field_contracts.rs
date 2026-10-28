//! CR205-1·#76·#79: 이메일과 조직 한도는 칸·이유·상한을 담은 400으로 거절한다.

use crate::common::{self, Reply, TestUser};
use crate::sql_support::World;
use axum::Router;
use axum::http::Method;
use serde_json::{Value, json};

async fn request(app: &Router, user: &TestUser, method: Method, path: &str, body: Value) -> Reply {
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
        serde_json::to_vec(&body).unwrap(),
    )
    .await;
    Reply {
        status,
        json: serde_json::from_slice(&bytes).unwrap(),
    }
}

fn check(
    reply: &Reply,
    field: &str,
    reason: &str,
    limit: Option<u64>,
    context: &str,
    failures: &mut Vec<String>,
) {
    let actual = json!({"error":reply.json["error"], "field":reply.json["field"],
        "reason":reply.json["reason"], "limit":reply.json["limit"]});
    let wanted = json!({"error":"bad_request", "field":field, "reason":reason, "limit":limit});
    if reply.status != 400 || actual != wanted {
        failures.push(format!(
            "{context}: expected 400 {wanted}, got {} {actual}",
            reply.status
        ));
    }
}

#[tokio::test]
async fn invitation_and_document_share_email_errors_name_the_field_reason_and_limit() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let doc = w.doc().await;
    let mut failures = Vec::new();
    for (email, reason, limit) in [
        ("not-an-email".to_owned(), "invalid_format", None),
        (
            format!("{}@example.test", "a".repeat(242)),
            "too_long",
            Some(254),
        ),
    ] {
        for (path, body) in [
            (
                format!("/api/workspaces/{}/invitations", w.ws),
                json!({"email":email,"role":"member"}),
            ),
            (
                format!("/api/organizations/{}/invitations", w.org),
                json!({"email":email,"role":"member"}),
            ),
            (
                format!("/api/documents/{doc}/shares"),
                json!({"email":email,"permission":"read","organization_id":w.org}),
            ),
        ] {
            let reply = request(&app, &w.owner, Method::POST, &path, body).await;
            check(
                &reply,
                "email",
                reason,
                limit,
                &format!("{path} {reason}"),
                &mut failures,
            );
        }
        // ①이 둔 수신자 사전 조회도 같은 공통 검증을 쓴다. 처리기는 고치지 않는다.
        let path = format!(
            "/api/documents/{doc}/share-recipient?organization_id={}&email={email}",
            w.org
        );
        let reply = common::send(&app, Method::GET, &path, Some(&w.owner), None).await;
        check(
            &reply,
            "email",
            reason,
            limit,
            &format!("share-recipient {reason}"),
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn organization_and_workspace_limits_report_each_bad_field_without_applying_the_patch() {
    let w = World::new().await;
    let app = common::app(&w.state);
    let org = format!("/api/organizations/{}", w.org);
    let limits = format!("/api/workspaces/{}/limits", w.ws);
    let mut failures = Vec::new();
    for (read, write, root) in [
        (org.clone(), format!("{org}/settings"), "settings"),
        (limits.clone(), limits, "limits"),
    ] {
        let before = common::send(&app, Method::GET, &read, Some(&w.owner), None)
            .await
            .json;
        for (field, value, reason, limit) in [
            ("max_file_bytes", json!(0), "out_of_range", None),
            ("max_storage_bytes", json!(-1), "out_of_range", None),
            ("max_concurrent_uploads", json!(0), "out_of_range", None),
            ("notification_rate_per_hour", json!(0), "out_of_range", None),
            ("activity_retention_days", json!(0), "out_of_range", None),
            (
                "activity_retention_days",
                json!(3651),
                "out_of_range",
                Some(3650),
            ),
            ("audit_retention_days", json!(0), "out_of_range", None),
            (
                "audit_retention_days",
                json!(3651),
                "out_of_range",
                Some(3650),
            ),
            ("max_file_bytes", json!("invalid"), "invalid_format", None),
            ("max_file_bytes", json!(1.5), "invalid_format", None),
            (
                "max_concurrent_uploads",
                json!(2147483648_i64),
                "out_of_range",
                Some(2147483647),
            ),
        ] {
            let mut body = json!({});
            body[field] = value;
            let reply = request(&app, &w.owner, Method::PUT, &write, body).await;
            check(
                &reply,
                field,
                reason,
                limit,
                &format!("{write} {field}"),
                &mut failures,
            );
        }
        for body in [Value::Null, json!([])] {
            let reply = request(&app, &w.owner, Method::PUT, &write, body).await;
            check(&reply, root, "invalid_format", None, &write, &mut failures);
        }
        let reply = request(
            &app,
            &w.owner,
            Method::PUT,
            &write,
            json!({"unknown":1,"max_file_bytes":12}),
        )
        .await;
        check(
            &reply,
            "unknown",
            "invalid_format",
            None,
            &write,
            &mut failures,
        );
        assert_eq!(
            common::send(&app, Method::GET, &read, Some(&w.owner), None)
                .await
                .json,
            before,
            "{write}: a bad field must not apply a known field"
        );
    }
    let max_name = w.state.config.limits.max_title_chars;
    for (field, value, reason, limit) in [
        ("name", Value::Null, "required", None),
        ("name", json!(" "), "required", None),
        ("name", json!(17), "invalid_format", None),
        (
            "name",
            json!("x".repeat(max_name + 1)),
            "too_long",
            Some(u64::try_from(max_name).unwrap()),
        ),
        (
            "public_links_enabled",
            json!("invalid"),
            "invalid_format",
            None,
        ),
    ] {
        let mut body = json!({});
        body[field] = value;
        let reply = request(
            &app,
            &w.owner,
            Method::PUT,
            &format!("{org}/settings"),
            body,
        )
        .await;
        check(
            &reply,
            field,
            reason,
            limit,
            "organization settings",
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
