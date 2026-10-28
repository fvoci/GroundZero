//! 파일 자원·칸 검증·선택적 기대 사용자 계약(PR3, D-96).
use super::fixtures::{part, sha, upload_all};
use crate::common::{self, TestUser, new_request_id, send, send_raw};
use crate::sql_support::World;
use axum::Router;
use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

const HEADER: &str = "x-fvoci-expected-user";

async fn setup() -> (World, Router, String, String) {
    let w = World::new().await;
    let app = common::app(&w.state);
    let doc = w.doc().await;
    let done = upload_all(&app, &w.owner, json!({"request_id":new_request_id(),
        "target":{"kind":"document","id":doc},"name":"resource.txt","size":3,"content_type":"text/plain"}), b"one").await;
    let file = done["file_id"].as_str().unwrap().to_owned();
    upload_all(
        &app,
        &w.owner,
        json!({"request_id":new_request_id(),"file_id":file,
        "name":"resource.txt","size":3,"content_type":"text/plain"}),
        b"two",
    )
    .await;
    (w, app, doc.to_string(), file)
}

async fn snapshot(w: &World) -> Value {
    let mut conn = common::fixture_conn().await;
    sqlx::query_scalar("SELECT jsonb_build_object(
        'files',(SELECT jsonb_agg(to_jsonb(f) ORDER BY id) FROM fvoci.files f WHERE workspace_id=$1),
        'versions',(SELECT jsonb_agg(to_jsonb(v) ORDER BY id) FROM fvoci.file_versions v WHERE file_id IN (SELECT id FROM fvoci.files WHERE workspace_id=$1)),
        'uploads',(SELECT jsonb_agg(to_jsonb(u) ORDER BY id) FROM fvoci.uploads u WHERE workspace_id=$1),
        'parts',(SELECT jsonb_agg(to_jsonb(p) ORDER BY upload_id,number) FROM fvoci.upload_parts p WHERE upload_id IN (SELECT id FROM fvoci.uploads WHERE workspace_id=$1)),
        'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM fvoci.audit_log a WHERE workspace_id=$1),
        'trash',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM fvoci.trash_batches t WHERE workspace_id=$1))")
        .bind(w.ws).fetch_one(&mut conn).await.unwrap()
}

async fn raw(
    app: &Router,
    user: Option<&TestUser>,
    method: Method,
    uri: &str,
    body: &Value,
    expected: &[&str],
) -> (StatusCode, Value) {
    let part_body = uri.contains("/parts/");
    let digest = sha(b"xyz");
    let mut headers = if part_body {
        vec![
            ("content-type", "application/octet-stream"),
            ("x-content-sha256", digest.as_str()),
        ]
    } else {
        vec![("content-type", "application/json")]
    };
    headers.extend(expected.iter().map(|value| (HEADER, *value)));
    let (status, _, bytes) = send_raw(
        app,
        method,
        uri,
        user,
        &headers,
        if part_body {
            b"xyz".to_vec()
        } else {
            serde_json::to_vec(body).unwrap()
        },
    )
    .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn file_metadata_and_versions_are_separate_resources_and_put_is_removed() {
    let (w, app, _, file) = setup().await;
    let uri = format!("/api/files/{file}");
    let meta = send(&app, Method::GET, &uri, Some(&w.owner), None).await;
    assert_eq!(meta.status, StatusCode::OK, "{}", meta.json);
    assert_eq!(meta.json["id"], file);
    assert!(meta.json.get("file").is_none() && meta.json.get("versions").is_none());
    let versions = send(
        &app,
        Method::GET,
        &format!("{uri}/versions"),
        Some(&w.owner),
        None,
    )
    .await;
    assert_eq!(versions.status, StatusCode::OK, "{}", versions.json);
    assert_eq!(versions.json[0]["version"], 2);
    assert_eq!(versions.json[1]["version"], 1);
    let outsider = common::login(&w.state, "hidden file reader").await;
    for path in [&uri, &format!("{uri}/versions")] {
        assert_eq!(
            send(&app, Method::GET, path, Some(&outsider), None)
                .await
                .status,
            StatusCode::NOT_FOUND
        );
    }
    let old = send(
        &app,
        Method::PUT,
        &uri,
        Some(&w.owner),
        Some(json!({"name":"old.txt"})),
    )
    .await;
    assert_eq!(old.status, StatusCode::METHOD_NOT_ALLOWED);
    let renamed = send(
        &app,
        Method::PATCH,
        &uri,
        Some(&w.owner),
        Some(json!({"name":"new.txt"})),
    )
    .await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.json);
    assert_eq!(renamed.json["name"], "new.txt");
}

#[tokio::test]
async fn every_file_command_compares_a_sent_expected_user_before_mutating() {
    let (w, app, doc, file) = setup().await;
    let up = send(&app, Method::POST, "/api/uploads", Some(&w.owner), Some(json!({
        "request_id":new_request_id(),"target":{"kind":"document","id":doc},"name":"pending.txt","size":3}))).await;
    assert_eq!(up.status, StatusCode::CREATED, "{}", up.json);
    let pending = up.json["id"].as_str().unwrap();
    assert_eq!(
        part(&app, &w.owner, pending, 1, b"abc", &sha(b"abc")).await,
        StatusCode::NO_CONTENT
    );
    let create = json!({"request_id":new_request_id(),"target":{"kind":"document","id":doc},"name":"another.txt","size":3});
    let requests = [
        (Method::POST, "/api/uploads".to_owned(), create),
        (
            Method::PUT,
            format!("/api/uploads/{pending}/parts/1"),
            Value::Null,
        ),
        (
            Method::POST,
            format!("/api/uploads/{pending}/complete"),
            Value::Null,
        ),
        (
            Method::DELETE,
            format!("/api/uploads/{pending}"),
            Value::Null,
        ),
        (
            Method::PATCH,
            format!("/api/files/{file}"),
            json!({"name":"guarded.txt"}),
        ),
        (
            Method::DELETE,
            format!("/api/files/{file}/versions/1"),
            Value::Null,
        ),
        (Method::DELETE, format!("/api/files/{file}"), Value::Null),
    ];
    let owner = w.owner.id.0.to_string();
    let wrong = uuid::Uuid::now_v7().to_string();
    let before = snapshot(&w).await;
    for (method, uri, body) in requests {
        for (headers, session, expected, code) in [
            (
                vec![wrong.as_str()],
                Some(&w.owner),
                StatusCode::CONFLICT,
                "account_changed",
            ),
            (
                vec!["invalid"],
                Some(&w.owner),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                vec![owner.as_str(), owner.as_str()],
                Some(&w.owner),
                StatusCode::BAD_REQUEST,
                "bad_request",
            ),
            (
                vec![owner.as_str()],
                None,
                StatusCode::UNAUTHORIZED,
                "unauthorized",
            ),
        ] {
            let (actual, response) =
                raw(&app, session, method.clone(), &uri, &body, &headers).await;
            assert_eq!(actual, expected, "{method} {uri}: {response}");
            assert_eq!(response["error"], code);
            assert_eq!(
                snapshot(&w).await,
                before,
                "rejected {method} {uri} changed state"
            );
        }
    }
    let (missing, _) = raw(
        &app,
        Some(&w.owner),
        Method::POST,
        "/api/uploads",
        &json!({}),
        &[],
    )
    .await;
    assert_eq!(missing, StatusCode::BAD_REQUEST);
    let (matching, renamed) = raw(
        &app,
        Some(&w.owner),
        Method::PATCH,
        &format!("/api/files/{file}"),
        &json!({"name":"matching.txt"}),
        &[&owner],
    )
    .await;
    assert_eq!(matching, StatusCode::OK, "{renamed}");
    assert_eq!(renamed["name"], "matching.txt");
    let (read, _) = raw(
        &app,
        Some(&w.owner),
        Method::GET,
        &format!("/api/files/{file}"),
        &Value::Null,
        &[&wrong],
    )
    .await;
    assert_eq!(read, StatusCode::OK);
}

#[tokio::test]
async fn invalid_file_names_and_part_fields_report_reason_field_and_limit() {
    let (w, app, doc, file) = setup().await;
    for (name, reason, limit) in [
        ("".to_owned(), "required", Value::Null),
        ("x".repeat(256), "too_long", json!(255)),
        ("a/b".to_owned(), "invalid_format", Value::Null),
        ("a\nb".to_owned(), "control_characters", Value::Null),
    ] {
        let result = send(
            &app,
            Method::PATCH,
            &format!("/api/files/{file}"),
            Some(&w.owner),
            Some(json!({"name":name})),
        )
        .await;
        assert_eq!(result.status, StatusCode::BAD_REQUEST, "{}", result.json);
        assert_eq!(result.json["field"], "name");
        assert_eq!(result.json["reason"], reason);
        assert_eq!(result.json["limit"], limit);
    }
    let up = send(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&w.owner),
        Some(json!({"request_id":new_request_id(),
        "target":{"kind":"document","id":doc},"name":"parts.txt","size":3})),
    )
    .await;
    let id = up.json["id"].as_str().unwrap();
    for (number, data, headers, field, reason, limit) in [
        (
            0,
            b"abc".to_vec(),
            vec![],
            "number",
            "out_of_range",
            json!(1),
        ),
        (
            1,
            b"ab".to_vec(),
            vec![],
            "size",
            "invalid_format",
            json!(3),
        ),
        (
            1,
            b"abc".to_vec(),
            vec![],
            "x-content-sha256",
            "required",
            Value::Null,
        ),
        (
            1,
            b"abc".to_vec(),
            vec![("x-content-sha256", "wrong")],
            "x-content-sha256",
            "invalid_format",
            json!(64),
        ),
    ] {
        let (status, _, bytes) = send_raw(
            &app,
            Method::PUT,
            &format!("/api/uploads/{id}/parts/{number}"),
            Some(&w.owner),
            &headers,
            data,
        )
        .await;
        let error: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["field"], field);
        assert_eq!(error["reason"], reason);
        assert_eq!(error["limit"], limit);
    }
}
