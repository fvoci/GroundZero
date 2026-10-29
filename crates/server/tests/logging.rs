#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 운영 로그(D-83): 모든 줄이 따로 JSON으로 읽히고, 요청 id로 오류를 찾을 수 있고, 거르기가 적용되고, TRACE에서도
//! 가짜 비밀 값이 나오지 않고, 로그를 낮춰도 감사 기록은 남는다.

mod common;

use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use fvoci_server::app::build_state;
use fvoci_server::logging::{self, Format};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

/// 로그 줄을 모으는 곳.
#[derive(Clone, Default)]
struct Buf(Arc<Mutex<Vec<u8>>>);

impl Write for Buf {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> MakeWriter<'a> for Buf {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}

impl Buf {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
    /// 줄마다 JSON 객체 하나여야 한다(아니면 실패).
    fn lines(&self) -> Vec<Value> {
        self.text()
            .lines()
            .map(|line| {
                let v: Value = serde_json::from_str(line)
                    .map_err(|e| format!("not one JSON object per line ({e}): {line}"))
                    .unwrap();
                assert!(v.is_object(), "{line}");
                for key in ["timestamp", "level", "target", "event", "message"] {
                    assert!(v[key].is_string(), "missing {key}: {line}");
                }
                v
            })
            .collect()
    }
}

/// 이 시험 바이너리 전체의 로그: TRACE까지, JSON으로.
fn global() -> &'static Buf {
    static GLOBAL: OnceLock<Buf> = OnceLock::new();
    GLOBAL.get_or_init(|| {
        let buf = Buf::default();
        let subscriber = tracing_subscriber::registry().with(logging::layer(
            EnvFilter::new("trace"),
            Format::Json,
            buf.clone(),
        ));
        tracing::subscriber::set_global_default(subscriber).unwrap();
        buf
    })
}

/// 이 스레드에서만 쓰는 로그(거르기 시험).
fn scoped(filter: &str) -> (Buf, tracing::subscriber::DefaultGuard) {
    let buf = Buf::default();
    let subscriber = tracing_subscriber::registry().with(logging::layer(
        EnvFilter::new(filter),
        Format::Json,
        buf.clone(),
    ));
    let guard = tracing::subscriber::set_default(subscriber);
    (buf, guard)
}

struct Raw {
    status: StatusCode,
    request_id: Option<String>,
    json: Value,
}

async fn raw(app: &axum::Router, request: Request<Body>) -> Raw {
    let res = app.clone().oneshot(request).await.unwrap();
    let status = res.status();
    let request_id = res
        .headers()
        .get("x-request-id")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Raw {
        status,
        request_id,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

fn get(uri: &str) -> axum::http::request::Builder {
    Request::builder().method(Method::GET).uri(uri)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn every_line_is_json_even_with_concurrent_requests() {
    let buf = global();
    let state = common::state();
    let app = common::app(&state);
    let user = common::login(&state, "logs").await;
    let mut tasks = Vec::new();
    for i in 0..48 {
        let app = app.clone();
        let user = user.clone();
        tasks.push(tokio::spawn(async move {
            let uri = match i % 4 {
                0 => "/healthz".to_owned(),
                1 => "/api/me".to_owned(),
                2 => "/api/no-such-thing".to_owned(),
                _ => format!("/api/workspaces/{}/modules", uuid::Uuid::now_v7()),
            };
            let req = get(&uri)
                .header(header::COOKIE, &user.cookie)
                .body(Body::empty())
                .unwrap();
            raw(&app, req).await.status
        }));
    }
    for t in tasks {
        let status = t.await.unwrap();
        assert!(status.is_success() || status.is_client_error(), "{status}");
    }
    let lines = buf.lines();
    let access: Vec<&Value> = lines
        .iter()
        .filter(|l| l["event"] == "http.request")
        .collect();
    assert!(access.len() >= 48, "{}", access.len());
    for line in &access {
        assert!(line["status_code"].is_u64(), "{line}");
        assert!(line["duration_ms"].is_number(), "{line}");
        assert!(line["request_id"].is_string(), "{line}");
        // 실제 경로가 아니라 경로 틀.
        assert!(!line["route"].as_str().unwrap().contains("0199"), "{line}");
    }
    // 상태 확인은 DEBUG, API는 INFO. 입력·권한 오류(4xx)는 ERROR가 아니다.
    assert!(
        access
            .iter()
            .any(|l| l["route"] == "/healthz" && l["level"] == "DEBUG")
    );
    assert!(
        access
            .iter()
            .any(|l| l["route"] == "/api/me" && l["level"] == "INFO")
    );
    assert!(
        access
            .iter()
            .any(|l| l["route"] == "/api/workspaces/{workspace_id}/modules"
                && l["status_code"] == 404
                && l["error_code"] == "not_found")
    );
    assert!(
        !lines.iter().any(|l| l["level"] == "ERROR"
            && l["event"] == "http.error"
            && l["error_code"] != "internal"),
        "4xx must not be logged as errors"
    );
}

#[tokio::test]
async fn errors_carry_the_request_id_into_the_log_and_the_response() {
    let buf = global();
    // 닿지 않는 DB: 요청이 내부 오류(500)로 끝난다.
    let cfg = common::config_with(&[(
        "DATABASE_URL",
        "postgres://nobody:FAKE-DBPW-41c2@127.0.0.1:1/none",
    )]);
    let app = common::app(&build_state(cfg).unwrap());
    let r = raw(
        &app,
        get("/api/me")
            .header(header::COOKIE, "fvoci_session=FAKE-COOKIE-41c2")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::INTERNAL_SERVER_ERROR, "{:?}", r.json);
    let id = r.request_id.clone().unwrap();
    assert_eq!(r.json["request_id"], id.as_str());
    assert_eq!(r.json["error"], "internal");
    // 화면에 내부 세부를 보내지 않는다.
    assert_eq!(r.json["message"], "Internal Server Error");

    let mine: Vec<Value> = buf
        .lines()
        .into_iter()
        .filter(|l| l["request_id"] == id.as_str())
        .collect();
    let errors: Vec<&Value> = mine.iter().filter(|l| l["level"] == "ERROR").collect();
    assert_eq!(errors.len(), 1, "logged once: {mine:?}");
    assert_eq!(errors[0]["event"], "http.error");
    assert_eq!(errors[0]["error_code"], "internal");
    assert_eq!(errors[0]["status_code"], 500);
    assert!(
        errors[0]["detail"]
            .as_str()
            .unwrap()
            .starts_with("database"),
        "{}",
        errors[0]
    );
    assert!(
        mine.iter()
            .any(|l| l["event"] == "http.request" && l["status_code"] == 500)
    );

    // 앞단이 준 안전한 id는 그대로 쓰고, 이상한 값은 새로 만든다.
    let app = common::app(&common::state());
    let r = raw(
        &app,
        get("/api/nothing-here")
            .header("x-request-id", "edge-0001.abcd")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    assert_eq!(r.request_id.as_deref(), Some("edge-0001.abcd"));
    assert_eq!(r.json["request_id"], "edge-0001.abcd");
    let r = raw(
        &app,
        get("/healthz")
            .header("x-request-id", "x")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_ne!(r.request_id.as_deref(), Some("x"));
}

#[tokio::test]
async fn fake_secrets_never_reach_the_log_even_at_trace() {
    let buf = global();
    let state = common::state();
    let app = common::app(&state);
    let user = common::login(&state, "secret").await;
    let ws = common::create_workspace(&app, &user, "로그").await;
    // 본문(문서 제목·내용), 쿠키, Authorization, OIDC 코드·state, 공개 링크 토큰, 쿼리, 망가진 JSON 본문.
    let doc = common::create_doc(&app, &user, &ws, "FAKE-BODY-7f3a").await;
    let r = common::live::save(
        &app,
        &user,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        &json!({
            "request_id": common::new_request_id(),
            "base_revision": doc["revision"],
            "title": doc["title"],
            "body": common::doc_body("FAKE-DOCTEXT-7f3a"),
        }),
    )
    .await;
    assert!(r.status.is_success(), "{:?}", r.json);
    for req in [
        get("/api/me")
            .header(
                header::COOKIE,
                format!("{}; extra=FAKE-COOKIE-7f3a", user.cookie),
            )
            .header(header::AUTHORIZATION, "Bearer FAKE-AUTH-7f3a")
            .body(Body::empty()),
        get("/auth/callback?code=FAKE-CODE-7f3a&state=FAKE-STATE-7f3a").body(Body::empty()),
        get("/api/public/FAKE-LINK-7f3a?x=FAKE-QUERY-7f3a").body(Body::empty()),
        Request::builder()
            .method(Method::POST)
            .uri(format!("/api/workspaces/{ws}/wiki/documents"))
            .header(header::ORIGIN, common::PUBLIC_URL)
            .header(header::COOKIE, &user.cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"title": "FAKE-JSON-7f3a", "#)),
    ] {
        let _reply = raw(&app, req.unwrap()).await;
    }
    let cfg = common::config_with(&[(
        "DATABASE_URL",
        "postgres://nobody:FAKE-DBPW-7f3a@127.0.0.1:1/none",
    )]);
    let broken = common::app(&build_state(cfg).unwrap());
    let _reply = raw(&broken, get("/api/me").body(Body::empty()).unwrap()).await;
    // 라이브러리의 trace도 상한에 걸린다.
    tracing::trace!(target: "hyper::proto::h1", header = "FAKE-LIB-7f3a", "library trace");
    tracing::trace!(target: "fvoci_server::probe", event = "test.probe", "trace is on");

    let text = buf.text();
    let _all_json = buf.lines();
    assert!(
        text.contains("test.probe"),
        "TRACE must be enabled for this test"
    );
    let token = user.cookie.split('=').nth(1).unwrap();
    for secret in [
        "FAKE-BODY-7f3a",
        "FAKE-DOCTEXT-7f3a",
        "FAKE-COOKIE-7f3a",
        "FAKE-AUTH-7f3a",
        "FAKE-CODE-7f3a",
        "FAKE-STATE-7f3a",
        "FAKE-LINK-7f3a",
        "FAKE-QUERY-7f3a",
        "FAKE-JSON-7f3a",
        "FAKE-DBPW-7f3a",
        "FAKE-LIB-7f3a",
        token,
    ] {
        assert!(!text.contains(secret), "{secret} leaked into the log");
    }
}

#[tokio::test]
async fn filters_apply_by_level_and_module() {
    let _global = global();
    let state = common::state();
    let app = common::app(&state);
    let user = common::login(&state, "filter").await;
    let call = |uri: &'static str| {
        get(uri)
            .header(header::COOKIE, &user.cookie)
            .body(Body::empty())
            .unwrap()
    };
    {
        let (buf, _guard) = scoped("info");
        let _r = raw(&app, call("/healthz")).await;
        let _r = raw(&app, call("/api/me")).await;
        let lines = buf.lines();
        assert!(lines.iter().any(|l| l["route"] == "/api/me"), "{lines:?}");
        assert!(!lines.iter().any(|l| l["route"] == "/healthz"), "{lines:?}");
    }
    {
        let (buf, _guard) = scoped("warn,fvoci_server::http=debug");
        let _r = raw(&app, call("/healthz")).await;
        tracing::info!(target: "fvoci_server::worker", event = "test.other_module", "hidden");
        let lines = buf.lines();
        assert!(lines.iter().any(|l| l["route"] == "/healthz"), "{lines:?}");
        assert!(
            !lines.iter().any(|l| l["event"] == "test.other_module"),
            "{lines:?}"
        );
    }
    {
        // 라이브러리는 거르기를 trace로 올려도 info까지만.
        let (buf, _guard) = scoped("trace");
        tracing::debug!(target: "reqwest::connect", event = "test.lib_debug", "hidden");
        tracing::info!(target: "reqwest::connect", event = "test.lib_info", "shown");
        // PDF 뽑기 라이브러리는 경고·오류에 파일 내용을 찍으므로 어느 레벨에서도 내보내지 않는다.
        tracing::error!(target: "pdf_extract", event = "test.pdf", "Stream: FAKE-PDF-7f3a");
        tracing::warn!(target: "lopdf::reader", event = "test.pdf", "Object load error");
        // 사건 필드가 공통 필드를 덮지 못한다.
        tracing::info!(target: "fvoci_server::probe", event = "test.collide", level = "FAKE", "x");
        let lines = buf.lines();
        assert!(!lines.iter().any(|l| l["event"] == "test.lib_debug"));
        assert!(lines.iter().any(|l| l["event"] == "test.lib_info"));
        assert!(!lines.iter().any(|l| l["event"] == "test.pdf"), "{lines:?}");
        let collide = lines.iter().find(|l| l["event"] == "test.collide").unwrap();
        assert_eq!(collide["level"], "INFO");
        assert_eq!(collide["field.level"], "FAKE");
    }
}

#[tokio::test]
async fn pretty_output_has_the_same_event_and_fields_on_one_line() {
    let _global = global();
    let buf = Buf::default();
    let subscriber = tracing_subscriber::registry().with(logging::layer(
        EnvFilter::new("info"),
        Format::Pretty,
        buf.clone(),
    ));
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::info!(
        target: "fvoci_server::probe",
        event = "test.pretty",
        status_code = 200_u16,
        "two\nlines"
    );
    let text = buf.text();
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.contains("INFO"), "{text}");
    assert!(text.contains("test.pretty"), "{text}");
    assert!(text.contains("status_code=200"), "{text}");
    assert!(text.contains("two\\nlines"), "{text}");
    assert!(!text.contains('\u{1b}'), "no ANSI: {text}");
}

#[tokio::test]
async fn audit_events_stay_when_the_log_level_is_lowered() {
    let _global = global();
    let (buf, _guard) = scoped("error");
    let state = common::state();
    let app = common::app(&state);
    let admin = common::login(&state, "audit-admin").await;
    let ws = common::create_workspace(&app, &admin, "감사 로그").await;
    let (_writer, _) = common::add_member(&app, &state, &admin, &ws, "member", "member").await;
    let doc = common::create_doc(&app, &admin, &ws, "지울 문서").await;
    let r = common::send(
        &app,
        Method::DELETE,
        &format!("/api/documents/{}", doc["id"].as_str().unwrap()),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let log = common::send(
        &app,
        Method::GET,
        &format!("/api/workspaces/{ws}/audit"),
        Some(&admin),
        None,
    )
    .await;
    assert_eq!(log.status, StatusCode::OK, "{:?}", log.json);
    let seen: Vec<String> = log
        .json
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            format!(
                "{}:{}",
                e["action"].as_str().unwrap(),
                e["target_type"].as_str().unwrap()
            )
        })
        .collect();
    for wanted in ["invite:invitation", "join:member", "delete:document"] {
        assert!(seen.contains(&wanted.to_owned()), "{seen:?}");
    }
    // 이 요청들은 오류가 없으니 ERROR 거르기에서는 운영 로그가 한 줄도 없다.
    assert!(buf.lines().is_empty(), "{}", buf.text());
}

#[tokio::test]
async fn malformed_bodies_get_the_json_error_shape_with_the_request_id() {
    let _global = global();
    let state = common::state();
    let app = common::app(&state);
    let user = common::login(&state, "rejection").await;
    let r = raw(
        &app,
        Request::builder()
            .method(Method::POST)
            .uri("/api/workspaces")
            .header(header::ORIGIN, common::PUBLIC_URL)
            .header(header::COOKIE, &user.cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"request_id": "#))
            .unwrap(),
    )
    .await;
    assert!(r.status.is_client_error(), "{}", r.status);
    assert_eq!(r.json["error"], "bad_request", "{:?}", r.json);
    assert!(r.json["message"].is_string());
    assert_eq!(
        r.json["request_id"].as_str(),
        r.request_id.as_deref(),
        "{:?}",
        r.json
    );
}
