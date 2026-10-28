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

use crate::common;

use std::net::SocketAddr;
use std::time::Duration;

use fvoci_server::app::build_state;
use fvoci_server::http::router;
use serde_json::{Value, json};
use tokio::net::TcpListener;

const CLIENT_ID: &str = "fvoci-web";
const USER1: &str = "test-user";
const USER2: &str = "test-user-2";

/// 테스트 realm(`deploy/keycloak/realm-test.json`)의 값. 비밀번호·클라이언트 비밀은 deploy/ 밖에 두지 않는다.
fn realm() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../deploy/keycloak/realm-test.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn client_secret() -> String {
    let realm = realm();
    let client = realm["clients"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["clientId"] == CLIENT_ID)
        .unwrap();
    client["secret"].as_str().unwrap().to_owned()
}

fn password(user: &str) -> String {
    let realm = realm();
    let account = realm["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == user)
        .unwrap();
    let credential = account["credentials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "password")
        .unwrap();
    credential["value"].as_str().unwrap().to_owned()
}

fn issuer() -> String {
    std::env::var("FVOCI_TEST_OIDC_ISSUER").unwrap() // set by `cargo xtask test --oidc`
}

/// 임의 포트에 실제 서버를 띄운다. 공개 URL은 그 포트.
async fn spawn_server(extra: &[(&str, &str)]) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let issuer = issuer();
    let secret = client_secret();
    // 앞의 것이 이긴다: 테스트가 넘긴 값이 기본값보다 먼저.
    let mut overrides = extra.to_vec();
    overrides.extend_from_slice(&[
        ("FVOCI_PUBLIC_URL", base.as_str()),
        ("FVOCI_OIDC_ISSUER", issuer.as_str()),
        ("FVOCI_OIDC_CLIENT_ID", CLIENT_ID),
        ("FVOCI_OIDC_CLIENT_SECRET", &secret),
    ]);
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
async fn login(client: &reqwest::Client, base: &str, user: &str) -> (u16, Value) {
    let password = password(user);
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
            ("password", password.as_str()),
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
        .begin_as(fvoci_core::ids::UserId(user_id.parse().unwrap()))
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
    let (status, me) = login(&alice, &base, USER1).await;
    assert_eq!(status, 200, "{me:?}");
    assert_eq!(login(&bob, &base, USER2).await.0, 200);
    // 워크스페이스는 조직 안에 만든다(D-111). 띄운 서버는 시험 DB를 같이 쓰므로 fixture 헬퍼로 조직을 준비한다.
    let alice_user = common::TestUser {
        id: fvoci_core::ids::UserId(me["id"].as_str().unwrap().parse().unwrap()),
        cookie: String::new(),
    };
    let organization = common::organization_for(&alice_user).await;

    let (status, ws) = api(
        &alice,
        &base,
        reqwest::Method::POST,
        "/api/workspaces",
        Some(json!({
            "request_id": common::new_request_id(),
            "organization_id": organization,
            "name": "OIDC 팀"
        })),
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

/// 테스트 Keycloak 부트스트랩 관리자 비밀번호(`deploy/compose.test.yaml`).
fn keycloak_admin_password() -> String {
    let compose = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../deploy/compose.test.yaml"
    ))
    .unwrap();
    compose
        .lines()
        .find_map(|line| line.trim().strip_prefix("KC_BOOTSTRAP_ADMIN_PASSWORD:"))
        .unwrap()
        .trim()
        .to_owned()
}

/// Keycloak 관리 API 토큰(테스트 Keycloak의 부트스트랩 관리자, `deploy/compose.test.yaml`의 테스트 값).
async fn keycloak_admin(base: &str) -> String {
    let res: Value = reqwest::Client::new()
        .post(format!(
            "{base}/realms/master/protocol/openid-connect/token"
        ))
        .form(&[
            ("grant_type", "password"),
            ("client_id", "admin-cli"),
            ("username", "admin"),
            ("password", keycloak_admin_password().as_str()),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    res["access_token"].as_str().unwrap().to_owned()
}

/// 감사 12번: 제공자가 서명 키를 바꾸면(키 회전) 서버가 처음 받아 둔 키 목록으로는 새 id_token을 검증하지 못한다.
/// 검증에 실패하면 한 번 다시 받아 같은 토큰을 다시 검증해야 한다(전에는 서버를 다시 시작할 때까지 로그인 불가).
/// 다른 테스트에 영향이 없게 이 테스트만의 realm을 만들어 그 realm의 키를 바꾼다.
#[tokio::test]
async fn oidc_login_survives_a_signing_key_rotation() {
    let issuer = issuer();
    let base = issuer[..issuer.find("/realms/").unwrap()].to_owned();
    let admin = keycloak_admin(&base).await;
    let http = reqwest::Client::new();
    let realm = format!("fvoci-rotate-{}", uuid::Uuid::now_v7().simple());
    let mut definition: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../deploy/keycloak/realm-test.json"
        ))
        .unwrap(),
    )
    .unwrap();
    definition["realm"] = json!(realm);
    // 사용자 id는 Keycloak 전체에서 하나여야 하므로 새 realm에서는 Keycloak이 새로 정하게 한다.
    for user in definition["users"].as_array_mut().unwrap() {
        user.as_object_mut().unwrap().remove("id");
    }
    let created = http
        .post(format!("{base}/admin/realms"))
        .bearer_auth(&admin)
        .json(&definition)
        .send()
        .await
        .unwrap();
    assert_eq!(created.status().as_u16(), 201);
    let realm_issuer = format!("{base}/realms/{realm}");
    let server = spawn_server(&[("FVOCI_OIDC_ISSUER", realm_issuer.as_str())]).await;

    // 첫 로그인으로 서버가 디스커버리와 키 목록을 받아 둔다.
    let (status, me) = login(&browser(), &server, USER1).await;
    assert_eq!(status, 200, "{me}");
    assert!(me["id"].is_string(), "{me}");

    // 우선순위가 더 높은 새 서명 키를 넣는다(이후 id_token은 새 키로 서명된다).
    let realm_id: Value = http
        .get(format!("{base}/admin/realms/{realm}"))
        .bearer_auth(&admin)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let added = http
        .post(format!("{base}/admin/realms/{realm}/components"))
        .bearer_auth(&admin)
        .json(&json!({
            "name": "rotated",
            "providerId": "rsa-generated",
            "providerType": "org.keycloak.keys.KeyProvider",
            "parentId": realm_id["id"],
            "config": { "priority": ["1000"], "algorithm": ["RS256"] }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(added.status().as_u16(), 201);

    let (status, me) = login(&browser(), &server, USER2).await;
    let _deleted = http
        .delete(format!("{base}/admin/realms/{realm}"))
        .bearer_auth(&admin)
        .send()
        .await;
    assert_eq!(status, 200, "login after the key rotation failed: {me}");
    assert!(
        me["id"].is_string(),
        "login after the key rotation failed: {me}"
    );
}
