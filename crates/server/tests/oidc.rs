#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 실제 Keycloak을 거치는 OIDC 통합 테스트. `cargo xtask test --oidc`가 Keycloak을 띄우고
//! `FVOCI_TEST_OIDC_ISSUER`를 넣는다. 없으면 실패한다. `--oidc` 없이 돌릴 때 xtask는 이 테스트들을
//! `--skip oidc_`로 제외하고 NOT RUN을 출력한다.
//!
//! 브라우저 대신 쿠키 저장소가 있는 HTTP 클라이언트로 Authorization Code 흐름을 끝까지 따라간다:
//! 서버 `/auth/login` → Keycloak 로그인 폼 → 자격 증명 제출 → 서버 `/auth/callback` → 세션 쿠키.

mod common;

use std::net::SocketAddr;
use std::time::Duration;

use fvoci_server::app::build_state;
use fvoci_server::http::router;
use serde_json::{Value, json};
use tokio::net::TcpListener;

const CLIENT_ID: &str = "fvoci-web";
const CLIENT_SECRET: &str = "fvoci-test-client-secret";
const USER1: (&str, &str) = ("test-user", "fvoci-test-user-password");
const USER2: (&str, &str) = ("test-user-2", "fvoci-test-user-2-password");

fn issuer() -> String {
    std::env::var("FVOCI_TEST_OIDC_ISSUER").unwrap() // set by `cargo xtask test --oidc`
}

/// 임의 포트에 실제 서버를 띄운다. 공개 URL은 그 포트.
async fn spawn_server(extra: &[(&str, &str)]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let issuer = issuer();
    let mut overrides = vec![
        ("FVOCI_PUBLIC_URL", base.as_str()),
        ("FVOCI_OIDC_ISSUER", issuer.as_str()),
        ("FVOCI_OIDC_CLIENT_ID", CLIENT_ID),
        ("FVOCI_OIDC_CLIENT_SECRET", CLIENT_SECRET),
    ];
    overrides.extend_from_slice(extra);
    let cfg = fvoci_server::config::Config::from_lookup(|k| {
        overrides
            .iter()
            .find(|(key, _)| *key == k)
            .map(|(_, v)| (*v).to_owned())
            .or_else(|| std::env::var(k).ok())
    })
    .unwrap();
    let app = router(build_state(cfg).unwrap());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    base
}

fn browser() -> reqwest::Client {
    reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap()
}

fn form_action(html: &str) -> Option<String> {
    let start = html.find("id=\"kc-form-login\"")?;
    let tail = &html[start..];
    let a = tail.find("action=\"")? + "action=\"".len();
    let end = tail[a..].find('"')?;
    Some(tail[a..a + end].replace("&amp;", "&"))
}

/// 로그인 시작 → Keycloak 폼 → 제출. 끝나면 `return_to`(여기서는 `/api/me`)의 응답을 돌려준다.
async fn login(
    client: &reqwest::Client,
    base: &str,
    (user, password): (&str, &str),
) -> (u16, Value) {
    let page = client
        .get(format!("{base}/auth/login?return_to=/api/me"))
        .send()
        .await
        .unwrap();
    let html = page.text().await.unwrap();
    let action = form_action(&html).unwrap(); // Keycloak login form must be shown
    let res = client
        .post(action)
        .form(&[
            ("username", user),
            ("password", password),
            ("credentialId", ""),
        ])
        .send()
        .await
        .unwrap();
    let status = res.status().as_u16();
    (status, res.json().await.unwrap_or(Value::Null))
}

async fn api(
    client: &reqwest::Client,
    base: &str,
    method: reqwest::Method,
    path: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let mut req = client
        .request(method, format!("{base}{path}"))
        .header("Origin", base);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let res = req.send().await.unwrap();
    let status = res.status().as_u16();
    (status, res.json().await.unwrap_or(Value::Null))
}

#[tokio::test]
async fn oidc_login_identifies_by_issuer_and_subject_then_logout_ends_both_sessions() {
    let base = spawn_server(&[]).await;
    let client = browser();

    let (status, me) = login(&client, &base, USER1).await;
    assert_eq!(status, 200, "{me:?}");
    assert_eq!(me["display_name"], "Test User");
    let user_id = me["id"].as_str().unwrap().to_owned();

    // 사용자 행은 (Keycloak issuer, subject)로 만들어졌다.
    let state = common::state();
    let mut tx = state
        .db
        .begin_as(fvoci_server::ids::UserId(user_id.parse().unwrap()))
        .await
        .unwrap();
    let (iss, sub): (String, String) =
        sqlx::query_as("SELECT issuer, subject FROM users WHERE id = $1::uuid")
            .bind(&user_id)
            .fetch_one(tx.conn())
            .await
            .unwrap();
    assert_eq!(iss, issuer());
    assert!(
        !sub.is_empty() && sub != "test-user",
        "subject is the provider's stable id, not the username"
    );

    // 로그아웃: 서버 세션이 사라지고, 제공자 로그아웃 URL에 id_token_hint가 붙는다.
    let (status, out) = api(&client, &base, reqwest::Method::POST, "/auth/logout", None).await;
    assert_eq!(status, 200);
    let redirect = out["redirect_to"].as_str().unwrap().to_owned();
    assert!(
        redirect.starts_with(&issuer()) && redirect.contains("id_token_hint="),
        "{redirect}"
    );
    assert_eq!(
        api(&client, &base, reqwest::Method::GET, "/api/me", None)
            .await
            .0,
        401
    );

    // 제공자 세션도 끝낸다. 이후 로그인 시작은 조용한 SSO가 아니라 다시 로그인 폼을 보여야 한다.
    // Keycloak은 확인 화면 없이 post_logout_redirect_uri(앱 홈)로 돌려보내야 한다.
    // 이 테스트 서버는 웹 디렉터리가 없어 홈이 404이므로 최종 상태가 아니라 도착 URL을 본다.
    let after = client.get(&redirect).send().await.unwrap();
    assert!(
        after.url().as_str().starts_with(&base),
        "provider did not redirect back after logout: {}",
        after.url()
    );
    let page = client
        .get(format!("{base}/auth/login?return_to=/api/me"))
        .send()
        .await
        .unwrap();
    let html = page.text().await.unwrap();
    assert!(
        form_action(&html).is_some(),
        "re-login must show the provider login form"
    );
}

#[tokio::test]
async fn oidc_second_user_is_denied_first_users_documents() {
    let base = spawn_server(&[]).await;
    let alice = browser();
    let bob = browser();
    assert_eq!(login(&alice, &base, USER1).await.0, 200);
    assert_eq!(login(&bob, &base, USER2).await.0, 200);

    let (status, ws) = api(
        &alice,
        &base,
        reqwest::Method::POST,
        "/api/workspaces",
        Some(json!({ "request_id": common::new_request_id(), "name": "OIDC 팀" })),
    )
    .await;
    assert_eq!(status, 201);
    let ws_id = ws["id"].as_str().unwrap();
    let (status, doc) = api(
        &alice,
        &base,
        reqwest::Method::POST,
        &format!("/api/workspaces/{ws_id}/wiki/documents"),
        Some(json!({ "request_id": common::new_request_id(), "title": "첫 문서" })),
    )
    .await;
    assert_eq!(status, 201);
    let doc_uri = format!("/api/documents/{}", doc["id"].as_str().unwrap());

    assert_eq!(
        api(&bob, &base, reqwest::Method::GET, &doc_uri, None)
            .await
            .0,
        404
    );
    assert_eq!(
        api(&alice, &base, reqwest::Method::GET, &doc_uri, None)
            .await
            .0,
        200
    );
}

#[tokio::test]
async fn oidc_session_expires() {
    let base = spawn_server(&[("FVOCI_SESSION_TTL_SECS", "1")]).await;
    let client = browser();
    assert_eq!(login(&client, &base, USER1).await.0, 200);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        api(&client, &base, reqwest::Method::GET, "/api/me", None)
            .await
            .0,
        401
    );
}

#[tokio::test]
async fn oidc_callback_rejects_forged_state() {
    let base = spawn_server(&[]).await;
    let client = browser();
    let res = client
        .get(format!("{base}/auth/callback?code=x&state=forged"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 400);
}
