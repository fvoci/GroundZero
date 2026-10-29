//! 라우팅과 HTTP 공통 경계: 상태, Origin 검사, 본문 크기 제한, 정적 웹 서빙, OpenAPI 계약.

mod api;
pub mod cache;
mod routes;

pub use api::ApiDoc;

use std::sync::Arc;
use std::time::Instant;

use axum::extract::{DefaultBodyLimit, FromRef, MatchedPath, Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use tower_http::services::{ServeDir, ServeFile};
use utoipa::ToSchema;

use crate::build_info::{self, BuildInfo};
use crate::config::Config;
use crate::features::accounts;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_db::Db;
use fvoci_http::origin::{PublicOrigin, require_same_origin};

/// 핸들러가 공유하는 상태.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub oidc: accounts::SharedProvider,
    /// 통합 검색(없으면 제목만 DB에서).
    pub search: Option<crate::search::SearchClient>,
    /// 파일 저장소(D-06).
    pub storage: crate::storage::Storage,
    /// 업로드 조각 동시 처리 한도(D-52).
    pub upload_permits: Arc<tokio::sync::Semaphore>,
    /// 반출 ZIP 동시 처리 한도.
    pub export_permits: Arc<tokio::sync::Semaphore>,
    /// 실시간 공동 편집 방(D-70). 한 앱 프로세스 안에서만 공유한다.
    pub collab: crate::collab::Rooms,
}

/// 서비스 상태 응답.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// 전체 상태. `ok` 또는 `degraded`.
    pub status: &'static str,
    /// 데이터베이스 왕복 결과. `ok` 또는 `error`.
    pub database: &'static str,
    /// 실행 중인 빌드(제품 버전·소스 SHA만, D-87). 화면의 버전 표시도 이 값을 쓴다.
    pub build: BuildInfo,
}

/// 서비스와 데이터베이스 연결 상태, 실행 중인 빌드를 돌려준다.
#[utoipa::path(
    get,
    path = "/healthz",
    security(),
    responses(
        (status = 200, description = "모두 정상", body = Health),
        (status = 503, description = "데이터베이스 연결 실패", body = Health),
    ),
    tag = "system"
)]
pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    match state.db.ping().await {
        Ok(()) => (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                database: "ok",
                build: build_info::CURRENT,
            }),
        ),
        Err(err) => {
            tracing::warn!(
                event = "health.database_unavailable",
                error_code = err.code(),
                detail = fvoci_core::logging::safe_error(&err),
                "database ping failed"
            );
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    database: "error",
                    build: build_info::CURRENT,
                }),
            )
        }
    }
}

impl fvoci_files::PersistWorkspace for crate::collab::Rooms {
    fn persist_workspace(
        &self,
        workspace: uuid::Uuid,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + '_>> {
        Box::pin(crate::collab::Rooms::persist_workspace(self, workspace))
    }
}

impl FromRef<AppState> for fvoci_files::FilesState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
            storage: state.storage.clone(),
            limits: state.config.files,
            upload_permits: Arc::clone(&state.upload_permits),
            collab: Arc::new(state.collab.clone()),
        }
    }
}

impl FromRef<AppState> for fvoci_calendar::CalendarState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
        }
    }
}

impl FromRef<AppState> for fvoci_activity::ActivityState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
        }
    }
}

impl FromRef<AppState> for fvoci_time::TimeState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
        }
    }
}

impl FromRef<AppState> for fvoci_work::WorkState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
            max_title_chars: state.config.limits.max_title_chars,
        }
    }
}

impl FromRef<AppState> for fvoci_references::ReferencesState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
            max_title_chars: state.config.limits.max_title_chars,
        }
    }
}

impl FromRef<AppState> for fvoci_documents::DocumentsState {
    fn from_ref(state: &AppState) -> Self {
        Self {
            db: state.db.clone(),
            limits: state.config.limits,
            public_url: state.config.public_url.clone(),
            runtime: Arc::new(state.collab.clone()),
        }
    }
}

impl FromRef<AppState> for PublicOrigin {
    fn from_ref(state: &AppState) -> Self {
        Self(state.config.public_origin())
    }
}

/// 요청과 그 오류를 잇는 id(D-83). 앞단 프록시가 준 `x-request-id`가 안전한 모양이면 그대로, 아니면 새로 만든다.
const REQUEST_ID_HEADER: &str = "x-request-id";

/// 맞은 경로의 틀(`/api/items/{item_id}`). 접근 줄에 실제 경로·쿼리(공개 링크 토큰 등) 대신 쓴다.
#[derive(Debug, Clone)]
struct RouteTemplate(String);

async fn record_route(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned());
    let mut response = next.run(request).await;
    if let Some(route) = route {
        response.extensions_mut().insert(RouteTemplate(route));
    }
    response
}

/// 요청마다 id를 정하고(로그 줄·오류 본문·응답 머리글에 같은 값), 끝나면 접근 줄을 하나 남긴다. 접근 줄에는 방법·경로
/// 틀·상태·처리 시간만 쓴다(쿼리·머리글·본문 없음). 상태 확인(`/healthz`)과 정적 웹 파일은 반복 잡음이라 DEBUG.
async fn request_context(request: Request, next: Next) -> Response {
    let id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| crate::logging::acceptable_request_id(v))
        .map_or_else(|| uuid::Uuid::now_v7().to_string(), str::to_owned);
    let method = request.method().clone();
    let path = request.uri().path();
    let quiet = path == "/healthz" || !(path.starts_with("/api/") || path.starts_with("/auth/"));
    let started = Instant::now();
    let mut response = fvoci_core::logging::REQUEST_ID
        .scope(id.clone(), next.run(request))
        .await;
    let duration_ms = (started.elapsed().as_secs_f64() * 1_000_000.0).round() / 1000.0;
    let status_code = response.status().as_u16();
    let route = response
        .extensions()
        .get::<RouteTemplate>()
        .map_or("-", |r| r.0.as_str());
    // 4xx도 어떤 거절인지(bad_request·invalid_state·bad_origin 등) 접근 줄에서 보이게 응답 오류 코드를 붙인다.
    let error_code = response
        .extensions()
        .get::<fvoci_core::logging::ErrorCode>()
        .map(|c| c.0);
    macro_rules! access {
        ($level:ident) => {
            tracing::$level!(
                event = "http.request",
                request_id = %id,
                method = %method,
                route,
                status_code,
                duration_ms,
                error_code,
                "request"
            )
        };
    }
    // 조용한 경로(상태 확인·정적 파일)라도 서버 쪽 실패는 보이게 WARN.
    match (quiet, status_code >= 500) {
        (true, false) => access!(debug),
        (true, true) => access!(warn),
        (false, _) => access!(info),
    }
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID_HEADER, value);
    }
    response
}

/// axum이 요청을 꺼내다 거절한 응답(깨진 JSON·빠진 필드·큰 본문 등)은 글자 본문이다. API의 오류 형식(오류 코드·
/// 사람용 설명·요청 id, D-83)으로 바꿔 화면이 같은 방식으로 다루게 한다. 설명은 그 요청을 보낸 사람에게만 가고 로그에는
/// 쓰지 않는다.
async fn json_rejections(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    let plain = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/plain"));
    if !status.is_client_error() || !plain {
        return response;
    }
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, 4096).await.unwrap_or_default();
    let message: String = String::from_utf8_lossy(&bytes).chars().take(300).collect();
    let error = if parts.status == StatusCode::PAYLOAD_TOO_LARGE {
        "too_large"
    } else {
        "bad_request"
    };
    let mut out = (
        parts.status,
        Json(ErrorBody::new(
            error,
            message,
            fvoci_core::logging::current_request_id(),
        )),
    )
        .into_response();
    out.extensions_mut()
        .insert(fvoci_core::logging::ErrorCode(error));
    out
}
/// 알 수 없는 API 경로는 웹 fallback이 아니라 404 JSON.
async fn api_not_found() -> AppError {
    AppError::NotFound
}

pub fn router(state: AppState) -> Router {
    // JSON 직렬화 여유를 두고 문서 상한보다 조금 크게 받는다. 정확한 상한 검사는 핸들러가 한다.
    let body_limit = state
        .config
        .limits
        .max_document_bytes
        .saturating_mul(2)
        .saturating_add(64 * 1024);

    let api = routes::api(&state);

    let auth = Router::new()
        .route("/login", get(accounts::login))
        .route("/callback", get(accounts::callback))
        .route("/logout", post(accounts::logout));

    let mut app = Router::new()
        .route("/healthz", get(healthz))
        .nest("/api", api)
        .nest("/auth", auth)
        .route_layer(middleware::from_fn(record_route))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            require_same_origin,
        ))
        .layer(DefaultBodyLimit::max(body_limit));

    // 정적 웹: 파일이 없으면 index.html을 돌려 SPA 경로(새로고침)를 지원한다. 해시 파일명 자원(`/assets`)과
    // HWP·HWPX 미리보기 WASM(`/rhwp`, D-76)은 따로 내보내 없는 파일이 SPA 화면(200)이 아니라 404가 되게 한다
    // (오래 캐시되는 주소에 HTML이 남지 않게). 둘 다 이미지 빌드가 만든 br·gzip 사본이 있으면 그대로 보낸다(서체·번들).
    // 사본이 없으면(개발 빌드) 원본을 보낸다.
    // 캐시 머리글은 맨 바깥 층(`cache::policy`, D-77)이 경로별로 정한다.
    if let Some(dir) = state.config.web_dir.clone() {
        let index = dir.join("index.html");
        app = app
            .nest_service(
                "/assets",
                ServeDir::new(dir.join("assets"))
                    .precompressed_br()
                    .precompressed_gzip(),
            )
            .nest_service(
                "/rhwp",
                ServeDir::new(dir.join("rhwp"))
                    .precompressed_br()
                    .precompressed_gzip(),
            )
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)));
    }

    app.layer(middleware::from_fn(cache::policy))
        .layer(middleware::from_fn(request_context))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use utoipa::OpenApi;

    use super::ApiDoc;

    #[test]
    fn session_authentication_is_declared_for_protected_operations() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let required = serde_json::json!([{ "session_https": [] }, { "session_http": [] }]);
        assert_eq!(doc["security"], required);
        for (scheme, name) in [
            (
                "session_https",
                crate::features::accounts::SECURE_SESSION_COOKIE,
            ),
            ("session_http", crate::features::accounts::SESSION_COOKIE),
        ] {
            let definition = &doc["components"]["securitySchemes"][scheme];
            assert_eq!(definition["type"], "apiKey");
            assert_eq!(definition["in"], "cookie");
            assert_eq!(definition["name"], name);
        }
        // These handlers do not require an existing application session. The
        // callback still validates its login state, and public links their token.
        let public = HashSet::from([
            ("/healthz", "get"),
            ("/auth/login", "get"),
            ("/auth/callback", "get"),
            ("/auth/logout", "post"),
            ("/api/auth-info", "get"),
            ("/api/public/{token}", "get"),
        ]);
        let mut seen_public = HashSet::new();
        for (path, item) in doc["paths"].as_object().unwrap() {
            for method in [
                "get", "put", "post", "delete", "patch", "head", "options", "trace",
            ] {
                let Some(operation) = item.get(method) else {
                    continue;
                };
                let effective = operation.get("security").unwrap_or(&doc["security"]);
                if public.contains(&(path.as_str(), method)) {
                    assert_eq!(*effective, serde_json::json!([]), "{method} {path}");
                    seen_public.insert((path.as_str(), method));
                } else {
                    assert_eq!(*effective, required, "{method} {path}");
                }
            }
        }
        assert_eq!(seen_public, public);
    }

    /// 핸들러 이름이 같으면(예: 두 모듈의 `list`) operationId가 겹쳐 생성 타입이 깨진다. 계약에서 미리 막는다.
    #[test]
    fn operation_ids_are_unique() {
        let doc = ApiDoc::openapi();
        let mut seen = HashSet::new();
        for (path, item) in &doc.paths.paths {
            for op in [&item.get, &item.post, &item.put, &item.delete, &item.patch]
                .into_iter()
                .flatten()
            {
                let id = op.operation_id.clone().unwrap_or_default();
                assert!(
                    seen.insert(id.clone()),
                    "duplicate operationId {id} at {path}"
                );
            }
        }
    }

    #[test]
    fn organization_selection_and_timer_modes_are_typed_contracts() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let schemas = &doc["components"]["schemas"];
        let selection = &schemas["MySettings"]["properties"]["last_organization_id"];
        assert_eq!(selection["format"], "uuid");
        assert!(
            selection["type"]
                .as_array()
                .is_some_and(|types| types.contains(&serde_json::json!("null")))
        );
        assert!(
            !schemas["MySettings"]["required"]
                .as_array()
                .is_some_and(|fields| fields.contains(&serde_json::json!("last_organization_id")))
        );
        assert_eq!(
            schemas["TimerStartMode"]["enum"],
            serde_json::json!(["continue", "new"])
        );
        assert_eq!(
            doc["paths"]["/api/me/timer"]["post"]["responses"]["409"]["content"]["application/json"]
                ["schema"]["$ref"],
            "#/components/schemas/ErrorBody"
        );
        for action in ["restore", "ownership-offer"] {
            let path = format!(
                "/api/organizations/{{organization_id}}/workspaces/{{workspace_id}}/{action}"
            );
            assert!(doc["paths"][path]["post"]["responses"]["204"].is_object());
        }
    }

    /// 관계 설정의 본문은 필수다(D-92로 본문 없는 옛 동작을 없앰). 계약이 서버 동작과 같아야 한다.
    #[test]
    fn the_link_body_is_required() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let body = &doc["paths"]["/api/tasks/{task_id}/predecessors/{predecessor_id}"]["put"]["requestBody"];
        assert_eq!(body["required"], true);
        assert_eq!(
            body["content"]["application/json"]["schema"],
            serde_json::json!({ "$ref": "#/components/schemas/SetPredecessor" })
        );
    }

    /// 남은 필수 기대 사용자 명령은 계약에서도 필수 머리를 표시한다(D-96).
    #[test]
    fn required_expected_user_headers_are_in_the_contract() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        for operation in [
            &doc["paths"]["/api/me/notes"]["post"],
            &doc["paths"]["/api/uploads"]["post"],
        ] {
            let header = operation["parameters"]
                .as_array()
                .and_then(|ps| {
                    ps.iter()
                        .find(|p| p["name"] == fvoci_http::session::EXPECTED_USER_HEADER)
                })
                .cloned()
                .unwrap_or_default();
            assert_eq!(header["in"], "header", "{operation}");
            assert_eq!(header["required"], true, "{operation}");
        }
    }

    /// 리뷰: 이동의 409는 두 모양이다. 확인한 미리보기가 맞지 않으면 `MovePreview`, 실시간 방의 편집을 먼저 리비전으로
    /// 남기지 못하면 `ErrorBody`. 계약이 둘 다 적는다.
    #[test]
    fn the_move_conflict_is_a_preview_or_error_body() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let conflict = &doc["paths"]["/api/documents/{document_id}/placement"]["put"]["responses"]
            ["409"]["content"]["application/json"]["schema"];
        assert_eq!(
            conflict,
            &serde_json::json!({ "$ref": "#/components/schemas/MoveConflict" })
        );
        let one_of: Vec<&str> = doc["components"]["schemas"]["MoveConflict"]["oneOf"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            one_of,
            [
                "#/components/schemas/MovePreview",
                "#/components/schemas/ErrorBody"
            ]
        );
    }

    /// D-62·D-131: 복원의 409도 두 모양이다. 기준이 지났으면 `StaleWrite_Document`(최신본), 실시간 편집을 먼저 남기지
    /// 못했거나 잠금 한도를 넘었으면 `ErrorBody`. 계약이 둘 다 적는다.
    #[test]
    fn the_restore_conflict_is_a_stale_base_or_error_body() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        let conflict = &doc["paths"]["/api/documents/{document_id}/revisions/{revision}/restore"]["post"]
            ["responses"]["409"]["content"]["application/json"]["schema"];
        assert_eq!(
            conflict,
            &serde_json::json!({ "$ref": "#/components/schemas/RestoreConflict" })
        );
        let one_of: Vec<&str> = doc["components"]["schemas"]["RestoreConflict"]["oneOf"]
            .as_array()
            .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
            .unwrap_or_default();
        assert_eq!(
            one_of,
            [
                "#/components/schemas/StaleWrite_Document",
                "#/components/schemas/ErrorBody"
            ]
        );
    }

    /// 항목 정보·항목 정보 패널 저장의 409는 두 모양이다(D-104): 판이 지났으면 서버 최신본(`current`), 보관 등이면
    /// `ErrorBody`(`invalid_state`, 최신본 없음).
    #[test]
    fn the_item_save_conflicts_are_a_stale_version_or_error_body() {
        let doc = serde_json::to_value(ApiDoc::openapi()).unwrap_or_default();
        for (path, wrapper, stale) in [
            ("/api/items/{item_id}", "ItemSaveConflict", "ItemConflict"),
            (
                "/api/items/{item_id}/profile",
                "ItemProfileSaveConflict",
                "ItemProfileConflict",
            ),
        ] {
            assert_eq!(
                doc["paths"][path]["put"]["responses"]["409"]["content"]["application/json"]["schema"],
                serde_json::json!({ "$ref": format!("#/components/schemas/{wrapper}") }),
                "{path}"
            );
            let one_of: Vec<&str> = doc["components"]["schemas"][wrapper]["oneOf"]
                .as_array()
                .map(|v| v.iter().filter_map(|s| s["$ref"].as_str()).collect())
                .unwrap_or_default();
            assert_eq!(
                one_of,
                [
                    format!("#/components/schemas/{stale}"),
                    "#/components/schemas/ErrorBody".to_owned()
                ],
                "{path}"
            );
        }
    }
}
