//! 통합 테스트 공통. `DATABASE_URL`(앱 역할)이 없으면 실패한다. `cargo xtask test`가 주입하고
//! 역할 준비(`db bootstrap`)와 마이그레이션(`db migrate`)도 먼저 실행한다.
#![allow(dead_code, reason = "각 테스트 바이너리가 헬퍼의 일부만 쓴다")]

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use fvoci_server::app::build_state;
use fvoci_server::config::Config;
use fvoci_server::features::accounts::{
    EXPECTED_USER_HEADER, SESSION_COOKIE, VerifiedIdentity, start_session,
};
use fvoci_server::http::{AppState, router};
use fvoci_server::ids::UserId;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

/// 테스트 서버의 공개 원점. Origin 헤더 검사에 쓴다.
pub const PUBLIC_URL: &str = "http://127.0.0.1:8080";

/// 환경 변수 구성에 테스트 값을 덮어쓴다.
pub fn config_with(overrides: &[(&str, &str)]) -> Config {
    // 서버 오류 로그를 테스트 출력에 남긴다(실패한 테스트에서만 보인다).
    tracing_subscriber::fmt().with_test_writer().try_init().ok();
    let overrides: Vec<(String, String)> = overrides
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    Config::from_lookup(move |k| {
        if let Some((_, v)) = overrides.iter().find(|(key, _)| key == k) {
            return Some(v.clone());
        }
        if k == "FVOCI_PUBLIC_URL" {
            return Some(PUBLIC_URL.to_owned());
        }
        if k.starts_with("FVOCI_OIDC_") {
            return None;
        }
        std::env::var(k).ok()
    })
    .unwrap() // DATABASE_URL must be set; run via `cargo xtask test`
}

pub fn state() -> AppState {
    build_state(config_with(&[])).unwrap()
}

pub fn app(state: &AppState) -> Router {
    router(state.clone())
}

/// 로그인한 사용자. OIDC 콜백과 같은 `start_session` 경로로 세션을 만든다.
#[derive(Debug, Clone)]
pub struct TestUser {
    pub id: UserId,
    pub cookie: String,
}

/// 제공자가 검증한(또는 검증하지 않은) 이메일을 가진 사용자로 로그인한다.
pub async fn login_with_email(
    state: &AppState,
    label: &str,
    email: &str,
    verified: bool,
) -> TestUser {
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject: format!("{label}-{}", uuid::Uuid::now_v7()),
        email: Some(email.to_owned()),
        email_verified: verified,
        display_name: label.to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (id, token) = start_session(&state.db, state.config.session_ttl, &identity, None)
        .await
        .unwrap();
    TestUser {
        id,
        cookie: format!("{SESSION_COOKIE}={token}"),
    }
}

/// 테스트마다 겹치지 않는 이메일.
pub fn unique_email(label: &str) -> String {
    format!("{label}-{}@example.test", uuid::Uuid::now_v7().simple())
}

pub async fn login(state: &AppState, label: &str) -> TestUser {
    let subject = format!("{label}-{}", uuid::Uuid::now_v7());
    let identity = VerifiedIdentity {
        issuer: "https://test-issuer.invalid".to_owned(),
        subject,
        email: None,
        email_verified: false,
        display_name: label.to_owned(),
        id_token: "test-id-token".to_owned(),
    };
    let (id, token) = start_session(&state.db, state.config.session_ttl, &identity, None)
        .await
        .unwrap();
    TestUser {
        id,
        cookie: format!("{SESSION_COOKIE}={token}"),
    }
}

pub struct Reply {
    pub status: StatusCode,
    pub json: Value,
}

pub async fn send(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    body: Option<Value>,
) -> Reply {
    send_with_origin(app, method, uri, user, body, Some(PUBLIC_URL)).await
}

pub async fn send_with_origin(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    body: Option<Value>,
    origin: Option<&str>,
) -> Reply {
    let carries_expected_user = expects_user(&method, uri);
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(u) = user {
        req = req.header(header::COOKIE, &u.cookie);
        // 웹이 하듯 기대 사용자를 세션의 사용자로 붙인다. 머리 자체(없음·다름·형식)는 `expected_user.rs`가
        // `send_raw`로 직접 시험한다.
        if carries_expected_user {
            req = req.header(EXPECTED_USER_HEADER, u.id.0.to_string());
        }
    }
    if let Some(o) = origin {
        req = req.header(header::ORIGIN, o);
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&b).unwrap()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Reply { status, json }
}

/// 기대 사용자 머리가 필요한 요청(문서 저장 `PUT /api/documents/{id}`, 내 노트 만들기 `POST /api/me/notes`, 업로드 시작
/// `POST /api/uploads`).
fn expects_user(method: &Method, uri: &str) -> bool {
    let path = uri.split('?').next().unwrap_or_default();
    match *method {
        Method::PUT => path
            .strip_prefix("/api/documents/")
            .is_some_and(|rest| !rest.is_empty() && !rest.contains('/')),
        Method::POST => path == "/api/me/notes" || path == "/api/uploads",
        _ => false,
    }
}

pub fn new_request_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// 시험 준비 전용 마이그레이션 역할 연결(`FVOCI_TEST_MIGRATION_URL`). 앱 권한 검사를 거치지 않으므로 조직·서버 관리자
/// 시드 같은 준비에만 쓰고, 권한 단언은 앱 역할(`state.db`, HTTP)로 한다.
pub async fn fixture_conn() -> sqlx::PgConnection {
    use sqlx::Connection;
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    sqlx::PgConnection::connect(&url).await.unwrap()
}

/// 운영자가 지정한 초기 관리자처럼 이 사용자를 서버 관리자로 시드한다(D-51·D-112).
pub async fn make_server_admin(user: &TestUser) {
    let mut conn = fixture_conn().await;
    sqlx::query(
        "INSERT INTO fvoci.server_admins (user_id, detail) VALUES ($1, '{\"seed\": \"test\"}'::jsonb)
         ON CONFLICT (user_id) DO NOTHING",
    )
    .bind(user.id.0)
    .execute(&mut conn)
    .await
    .unwrap();
}

/// 이 사용자가 속한 첫 조직. 없으면 fixture로 조직을 만들어 그 사용자를 첫 조직 관리자로 둔다(D-111). 같은 사용자가 만드는
/// 워크스페이스는 모두 이 조직에 들어간다. 앱의 조직 생성 함수는 서버 관리자만 부르므로 여기서는 표에 바로 넣는다: 시험
/// 사용자를 서버 관리자로 올리면 권한 모양이 달라진다(서버 수준 감사 행이 보이는 등). 서버 관리자 경로는
/// `make_server_admin` + `app_create_organization`으로 따로 시험한다.
pub async fn organization_for(user: &TestUser) -> uuid::Uuid {
    let mut conn = fixture_conn().await;
    let existing: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT o.id FROM fvoci.organizations o JOIN fvoci.organization_members m ON m.organization_id = o.id
         WHERE m.user_id = $1 ORDER BY o.created_at, o.id LIMIT 1",
    )
    .bind(user.id.0)
    .fetch_optional(&mut conn)
    .await
    .unwrap();
    if let Some(id) = existing {
        return id;
    }
    let id = uuid::Uuid::now_v7();
    sqlx::query(
        "WITH o AS (
           INSERT INTO fvoci.organizations (id, name, created_by, request_id) VALUES ($1, $2, $3, $4) RETURNING id)
         INSERT INTO fvoci.organization_members (organization_id, user_id, role) SELECT id, $3, 'admin' FROM o",
    )
    .bind(id)
    .bind(format!("조직 {}", user.id.0.simple()))
    .bind(user.id.0)
    .bind(uuid::Uuid::now_v7())
    .execute(&mut conn)
    .await
    .unwrap();
    id
}

/// 사용자를 조직 멤버(`admin` 또는 `member`)로 넣는다. 워크스페이스 초대 수락에는 조직 멤버십이 필요하다(D-114).
pub async fn join_organization(organization: uuid::Uuid, user: &TestUser, role: &str) {
    let mut conn = fixture_conn().await;
    sqlx::query(
        "INSERT INTO fvoci.organization_members (organization_id, user_id, role) VALUES ($1, $2, $3)
         ON CONFLICT (organization_id, user_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(organization)
    .bind(user.id.0)
    .bind(role)
    .execute(&mut conn)
    .await
    .unwrap();
}

/// 워크스페이스를 만들고 id를 돌려준다. 만드는 사람의 조직(없으면 새로 만든다) 안에 만든다(D-111).
pub async fn create_workspace(app: &Router, user: &TestUser, name: &str) -> String {
    let organization = organization_for(user).await;
    let r = send(
        app,
        Method::POST,
        "/api/workspaces",
        Some(user),
        Some(serde_json::json!({
            "request_id": new_request_id(),
            "organization_id": organization,
            "name": name
        })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json["id"].as_str().unwrap().to_owned()
}

/// 워크스페이스가 속한 조직.
pub async fn organization_of_workspace(ws: &str) -> uuid::Uuid {
    let mut conn = fixture_conn().await;
    sqlx::query_scalar("SELECT organization_id FROM fvoci.workspaces WHERE id = $1::uuid")
        .bind(ws)
        .fetch_one(&mut conn)
        .await
        .unwrap()
}

/// 위키 문서를 만들고 응답 JSON을 돌려준다.
pub async fn create_doc(app: &Router, user: &TestUser, ws: &str, title: &str) -> Value {
    let r = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/wiki/documents"),
        Some(user),
        Some(serde_json::json!({ "request_id": new_request_id(), "title": title })),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    r.json
}

pub fn doc_body(text: &str) -> Value {
    serde_json::json!({
        "type": "doc",
        "content": [{ "type": "paragraph", "content": [{ "type": "text", "text": text }] }]
    })
}

/// 초대와 수락으로 멤버를 넣는다. (사용자, 검증된 이메일)을 돌려준다. 수락 전에 그 워크스페이스의 조직 멤버로 넣는다
/// (워크스페이스 초대는 조직 멤버만 수락할 수 있다, D-114).
pub async fn add_member(
    app: &Router,
    state: &AppState,
    admin: &TestUser,
    ws: &str,
    label: &str,
    role: &str,
) -> (TestUser, String) {
    let email = unique_email(label);
    let user = login_with_email(state, label, &email, true).await;
    join_organization(organization_of_workspace(ws).await, &user, "member").await;
    let inv = send(
        app,
        Method::POST,
        &format!("/api/workspaces/{ws}/invitations"),
        Some(admin),
        Some(json!({ "email": email, "role": role })),
    )
    .await;
    let id = inv.json["id"].as_str().unwrap();
    let r = send(
        app,
        Method::POST,
        &format!("/api/invitations/{id}/accept"),
        Some(&user),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    (user, email)
}

/// 시험 데이터의 시각을 옮기는 SQL(예: 휴지통 만료)을 테이블 소유 역할로 실행한다. 앱 권한 검사를 거치지 않으므로
/// 시험 준비에만 쓴다. `cargo xtask test`가 `FVOCI_TEST_MIGRATION_URL`을 넣는다.
pub async fn fixture_sql(sql: &str, id: &str) {
    use sqlx::Connection;
    let url = std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    let mut conn = sqlx::PgConnection::connect(&url).await.unwrap();
    sqlx::query(sql).bind(id).execute(&mut conn).await.unwrap();
}

/// 날것 본문과 머리글로 보내고 상태·머리글·본문 바이트를 돌려준다(파일 조각·내려받기).
pub async fn send_raw(
    app: &Router,
    method: Method,
    uri: &str,
    user: Option<&TestUser>,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, axum::http::HeaderMap, Vec<u8>) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ORIGIN, PUBLIC_URL);
    if let Some(u) = user {
        req = req.header(header::COOKIE, &u.cookie);
    }
    for (name, value) in headers {
        req = req.header(*name, *value);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    (status, headers, bytes)
}

/// 같은 클러스터의 다른 데이터베이스 주소.
pub fn with_database(url: &str, database: &str) -> String {
    let mut parsed = url::Url::parse(url).unwrap();
    parsed.set_path(&format!("/{database}"));
    parsed.to_string()
}

/// 같은 클러스터의 빈 데이터베이스를 지금 마이그레이션까지 만든다. DB 전체를 보는 작업(백업, 검색 대기열)을 다른 시험과
/// 섞이지 않게 시험할 때 쓴다(공유 테스트 DB에는 다른 시험의 행이 있다).
pub struct FreshDb {
    pub name: String,
    pub db: fvoci_server::config::DbCommandConfig,
    pub app_url: String,
}

pub async fn fresh_database(admin: &mut sqlx::PgConnection, label: &str) -> FreshDb {
    use sqlx::Connection as _;
    // 같은 시험 바이너리에서 새 DB 준비가 겹치면 클러스터 공용 행(역할 권한 등)을 함께 고쳐 "tuple concurrently
    // updated"(XX000)로 실패한다. 준비와 마이그레이션은 한 번에 하나씩 한다.
    static PREPARE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _serial = PREPARE.lock().await;
    let admin_url = std::env::var("FVOCI_TEST_ADMIN_URL").expect("run via `cargo xtask test`");
    let migration_url =
        std::env::var("FVOCI_TEST_MIGRATION_URL").expect("run via `cargo xtask test`");
    // Supavisor session 모드로 돌릴 때(`--session-pooler`)는 테넌트가 한 데이터베이스만 받으므로 새 DB는 직접 연결로 쓴다.
    let app_url = std::env::var("FVOCI_TEST_DIRECT_APP_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("run via `cargo xtask test`");
    let name = format!("fvoci_{label}_{}", uuid::Uuid::now_v7().simple());
    sqlx::raw_sql(&format!("CREATE DATABASE {name}"))
        .execute(&mut *admin)
        .await
        .unwrap();
    let mut db_admin = sqlx::PgConnection::connect(&with_database(&admin_url, &name))
        .await
        .unwrap();
    // 운영자 경로(`db bootstrap`)와 같은 준비. superuser가 아닌 관리자(셀프호스트 Supabase)에서도 된다.
    fvoci_server::db::admin::prepare_database(&mut db_admin)
        .await
        .unwrap();
    db_admin.close().await.unwrap();
    let db = fvoci_server::config::DbCommandConfig {
        admin_url: Some(fvoci_server::config::Secret::new(with_database(
            &admin_url, &name,
        ))),
        migration_url: Some(fvoci_server::config::Secret::new(with_database(
            &migration_url,
            &name,
        ))),
        migrator_password: None,
        app_password: None,
    };
    fvoci_server::db::admin::migrate(&db).await.unwrap();
    FreshDb {
        app_url: with_database(&app_url, &name),
        name,
        db,
    }
}
