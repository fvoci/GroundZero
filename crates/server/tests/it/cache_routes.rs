//! #212 3·부조종사 00:25 (4), D-77: 계약의 새 경로도 자동으로 캐시 감시를 받는다.
//! 성공 응답 머리글은 층 시험이 보장(SR-221-3, http::cache::tests의 경로·메서드·상태 행렬).

use super::*;

use crate::common::{add_member, doc_body, new_request_id};
use axum::extract::{MatchedPath, Request};
use axum::middleware::{self, Next};
use axum::response::Response;
use fvoci_server::http::ApiDoc;
use sha2::{Digest, Sha256};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use utoipa::OpenApi;

const REVALIDATE: &str = "private, no-cache, no-transform";

fn save_body(request_id: &str, base: i64, text: &str) -> Value {
    json!({ "request_id": request_id, "base_revision": base, "title": "문서", "body": doc_body(text) })
}

fn no_store(status: StatusCode, headers: &HeaderMap, context: &str) {
    assert_eq!(
        header(headers, "cache-control").as_deref(),
        Some(NO_STORE),
        "{context}: {status}"
    );
    assert!(
        headers.get("etag").is_none(),
        "{context}: {status} has ETag"
    );
}

fn cache_headers(method: &Method, template: &str, status: StatusCode, headers: &HeaderMap) {
    let document_read = method == Method::GET
        && template == "/api/documents/{document_id}"
        && matches!(status, StatusCode::OK | StatusCode::NOT_MODIFIED);
    // #212 5행, PR4c가 본문 PUT을 지울 때 이 예외도 지운다
    let write_etag = status == StatusCode::OK
        && ((method == Method::PUT && template == "/api/documents/{document_id}")
            || (method == Method::POST
                && template == "/api/documents/{document_id}/revisions/{revision}/restore"));
    assert_eq!(
        header(headers, "cache-control").as_deref(),
        Some(if document_read { REVALIDATE } else { NO_STORE }),
        "{method} {template}: {status}"
    );
    if document_read {
        assert!(
            headers.contains_key("etag"),
            "{method} {template}: {status}"
        );
    } else if !write_etag {
        assert!(
            !headers.contains_key("etag"),
            "{method} {template}: {status} has ETag"
        );
    }
}

// 처리기의 404와 배선 누락의 404를 구별하려고 시험의 층에서 실제 MatchedPath도 기록한다.
async fn matched_route(request: Request, next: Next) -> Response {
    let path = request.extensions().get::<MatchedPath>().cloned();
    let mut response = next.run(request).await;
    if let Some(path) = path {
        response
            .headers_mut()
            .insert("x-test-matched-route", path.as_str().parse().unwrap());
    }
    response
}

fn concrete_path(template: &str) -> String {
    template
        .split('/')
        .map(|segment| {
            if segment.starts_with('{') {
                match segment {
                    "{revision}" | "{part_number}" => "1",
                    "{token}" => "missing-public-token",
                    _ => "01900000-0000-7000-8000-000000000001",
                }
            } else {
                segment
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[tokio::test]
async fn every_openapi_operation_and_websocket_route_reject_shared_caching() {
    let state = common::state();
    let app = common::app(&state).layer(middleware::from_fn(matched_route));
    let contract = serde_json::to_value(ApiDoc::openapi()).unwrap();
    let paths = contract["paths"].as_object().unwrap();
    let mut operations = 0;
    for (template, item) in paths {
        for (verb, _) in item.as_object().unwrap() {
            if ["parameters", "summary", "description", "$ref", "servers"].contains(&verb.as_str())
            {
                continue;
            }
            let method: Method = verb.to_uppercase().parse().unwrap();
            let path = concrete_path(template);
            let got = call(&app, method.clone(), &path, None, &[], None).await;
            let context = format!("{method} {template}");
            assert_eq!(
                header(&got.headers, "x-test-matched-route").as_deref(),
                Some(template.as_str()),
                "uncovered {context}: {}",
                got.status
            );
            assert_ne!(
                got.status,
                StatusCode::METHOD_NOT_ALLOWED,
                "unwired {context}"
            );
            cache_headers(&method, template, got.status, &got.headers);
            operations += 1;
        }
    }
    let template = "/api/documents/{document_id}/collab";
    let got = call(&app, Method::GET, &concrete_path(template), None, &[], None).await;
    assert_eq!(
        header(&got.headers, "x-test-matched-route").as_deref(),
        Some(template)
    );
    no_store(got.status, &got.headers, "websocket rejection");
    println!(
        "cache route coverage: {} paths, {operations} OpenAPI operations + websocket",
        paths.len()
    );
    state.db.close().await;
}

#[tokio::test]
async fn cache_headers_cover_errors_files_exports_public_success_and_upgrade() {
    let state = common::state();
    let app = common::app(&state);
    let owner = login(&state, "cache-owner").await;
    let ws = create_workspace(&app, &owner, "cache routes").await;
    let doc = create_doc(&app, &owner, &ws, "cache document").await;
    let id = doc["id"].as_str().unwrap();
    let uri = format!("/api/documents/{id}");
    let (reader, _) = add_member(&app, &state, &owner, &ws, "cache-reader", "readonly").await;
    let cases = [
        (
            Method::GET,
            "/api/me".to_owned(),
            None,
            None,
            StatusCode::UNAUTHORIZED,
        ),
        (
            Method::PUT,
            format!("/api/workspaces/{ws}/realtime"),
            Some(&reader),
            Some(json!({"enabled": true})),
            StatusCode::FORBIDDEN,
        ),
        (
            Method::GET,
            "/api/public/missing".to_owned(),
            None,
            None,
            StatusCode::NOT_FOUND,
        ),
        (
            Method::PUT,
            uri.clone(),
            Some(&owner),
            Some(save_body(&new_request_id(), 0, "stale")),
            StatusCode::CONFLICT,
        ),
        (
            Method::GET,
            format!("/api/workspaces/{ws}/export"),
            Some(&owner),
            None,
            StatusCode::OK,
        ),
        (
            Method::GET,
            "/api/me/export".to_owned(),
            Some(&owner),
            None,
            StatusCode::OK,
        ),
    ];
    for (method, path, user, body, expected) in cases {
        let got = call(&app, method.clone(), &path, user, &[], body).await;
        assert_eq!(got.status, expected, "{method} {path}: {:?}", got.json);
        no_store(got.status, &got.headers, &path);
        if path.ends_with("/export") {
            assert_eq!(
                header(&got.headers, "content-type").as_deref(),
                Some("application/zip")
            );
            assert!(got.raw.starts_with(b"PK"));
        }
    }
    let got = call(
        &app,
        Method::PUT,
        &uri,
        Some(&owner),
        &[("if-match", "\"stale\"")],
        Some(save_body(&new_request_id(), 1, "stale")),
    )
    .await;
    assert_eq!(got.status, StatusCode::PRECONDITION_FAILED);
    no_store(got.status, &got.headers, "412");

    // 성공 쓰기 예외도 실제 처리기를 거쳐 검사한다. 둘 다 공유 캐시 금지는 그대로다.
    let saved = call(
        &app,
        Method::PUT,
        &uri,
        Some(&owner),
        &[],
        Some(save_body(&new_request_id(), 1, "fresh")),
    )
    .await;
    assert_eq!(saved.status, StatusCode::OK);
    cache_headers(
        &Method::PUT,
        "/api/documents/{document_id}",
        saved.status,
        &saved.headers,
    );
    let expected_user = owner.id.0.to_string();
    let restored = call(
        &app,
        Method::POST,
        &format!("{uri}/revisions/1/restore"),
        Some(&owner),
        &[(EXPECTED_USER_HEADER, &expected_user)],
        Some(json!({"request_id": new_request_id(), "base_revision": 2})),
    )
    .await;
    assert_eq!(restored.status, StatusCode::OK, "{:?}", restored.json);
    cache_headers(
        &Method::POST,
        "/api/documents/{document_id}/revisions/{revision}/restore",
        restored.status,
        &restored.headers,
    );
    let read = call(&app, Method::GET, &uri, Some(&owner), &[], None).await;
    assert_eq!(read.status, StatusCode::OK);
    cache_headers(
        &Method::GET,
        "/api/documents/{document_id}",
        read.status,
        &read.headers,
    );
    let tag = header(&read.headers, "etag").unwrap();
    let unchanged = call(
        &app,
        Method::GET,
        &uri,
        Some(&owner),
        &[("if-none-match", &tag)],
        None,
    )
    .await;
    assert_eq!(unchanged.status, StatusCode::NOT_MODIFIED);
    cache_headers(
        &Method::GET,
        "/api/documents/{document_id}",
        unchanged.status,
        &unchanged.headers,
    );

    let link = common::send(
        &app,
        Method::POST,
        &format!("{uri}/public-links"),
        Some(&owner),
        Some(json!({})),
    )
    .await;
    assert_eq!(link.status, StatusCode::CREATED);
    let token = link.json["url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap();
    let got = call(
        &app,
        Method::GET,
        &format!("/api/public/{token}"),
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(got.status, StatusCode::OK);
    no_store(got.status, &got.headers, "public success");

    // 실제 조각 업로드를 끝낸 이미지로 download와 inline 성공을 모두 확인한다.
    let bytes = b"\x89PNG\r\n\x1a\ncache";
    let upload = common::send(
        &app,
        Method::POST,
        "/api/uploads",
        Some(&owner),
        Some(json!({
            "request_id": new_request_id(), "target": {"kind": "document", "id": id},
            "name": "cache.png", "content_type": "image/png", "size": bytes.len()
        })),
    )
    .await;
    assert_eq!(upload.status, StatusCode::CREATED, "{:?}", upload.json);
    let upload_id = upload.json["id"].as_str().unwrap();
    let hash = hex::encode(Sha256::digest(bytes));
    let part = common::send_raw(
        &app,
        Method::PUT,
        &format!("/api/uploads/{upload_id}/parts/1"),
        Some(&owner),
        &[
            ("x-content-sha256", &hash),
            ("content-type", "application/octet-stream"),
        ],
        bytes.to_vec(),
    )
    .await;
    assert_eq!(part.0, StatusCode::NO_CONTENT);
    no_store(part.0, &part.1, "upload part");
    let complete = common::send(
        &app,
        Method::POST,
        &format!("/api/uploads/{upload_id}/complete"),
        Some(&owner),
        None,
    )
    .await;
    assert_eq!(complete.status, StatusCode::OK, "{:?}", complete.json);
    let file_id = complete.json["file_id"].as_str().unwrap();
    for (query, disposition) in [("", "attachment;"), ("?inline=true", "inline;")] {
        let path = format!("/api/files/{file_id}/content{query}");
        let got = call(&app, Method::GET, &path, Some(&owner), &[], None).await;
        assert_eq!(got.status, StatusCode::OK);
        no_store(got.status, &got.headers, &path);
        assert!(
            header(&got.headers, "content-disposition")
                .unwrap()
                .starts_with(disposition)
        );
        assert_eq!(got.raw, bytes);
    }

    let enabled = common::send(
        &app,
        Method::PUT,
        &format!("/api/workspaces/{ws}/realtime"),
        Some(&owner),
        Some(json!({"enabled": true})),
    )
    .await;
    assert_eq!(enabled.status, StatusCode::NO_CONTENT);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut request = format!(
        "ws://{addr}/api/documents/{id}/collab?expected_user={}",
        owner.id.0
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("cookie", owner.cookie.parse().unwrap());
    request
        .headers_mut()
        .insert("origin", common::PUBLIC_URL.parse().unwrap());
    let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::SWITCHING_PROTOCOLS);
    no_store(response.status(), response.headers(), "websocket 101");
    socket.close(None).await.unwrap();
    server.abort();
    state.db.close().await;
}

#[tokio::test]
async fn cache_headers_cover_natural_service_unavailable() {
    let state = common::state();
    let app = common::app(&state);
    // 이 시험만 소유한 풀을 닫는다. DB·다른 시험의 연결은 건드리지 않는다.
    state.db.close().await;
    let got = call(&app, Method::GET, "/healthz", None, &[], None).await;
    assert_eq!(got.status, StatusCode::SERVICE_UNAVAILABLE);
    no_store(got.status, &got.headers, "healthz 503");
}

#[tokio::test]
async fn cache_headers_cover_auth_redirect_with_set_cookie() {
    use axum::routing::get;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    // Config는 URL을 정규화한다. discovery의 issuer도 끝 슬래시까지 같은 문자열이어야 한다.
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let issuer = format!("{origin}/");
    let metadata = json!({
        "issuer": issuer, "authorization_endpoint": format!("{origin}/authorize"),
        "token_endpoint": format!("{origin}/token"), "jwks_uri": format!("{origin}/jwks"),
        "response_types_supported": ["code"], "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"]
    });
    let provider = Router::new()
        .route(
            "/.well-known/openid-configuration",
            get(move || async move { axum::Json(metadata) }),
        )
        .route("/jwks", get(|| async { axum::Json(json!({"keys": []})) }));
    let server = tokio::spawn(async move { axum::serve(listener, provider).await.unwrap() });
    let realm: Value =
        serde_json::from_str(include_str!("../../../../deploy/keycloak/realm-test.json")).unwrap();
    let secret = realm["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["clientId"] == "fvoci-web")
        .unwrap()["secret"]
        .as_str()
        .unwrap();
    let state = build_state(common::config_with(&[
        ("FVOCI_OIDC_ISSUER", &issuer),
        ("FVOCI_OIDC_CLIENT_ID", "fvoci-web"),
        ("FVOCI_OIDC_CLIENT_SECRET", secret),
    ]))
    .unwrap();
    let got = call(
        &common::app(&state),
        Method::GET,
        "/auth/login",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(got.status, StatusCode::SEE_OTHER);
    assert!(got.headers.contains_key("set-cookie"));
    no_store(got.status, &got.headers, "auth 303 + Set-Cookie");
    server.abort();
    state.db.close().await;
}
