//! 라우팅과 HTTP 공통 경계. 기능별 핸들러는 `features/`에 두고 여기서 조립한다.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use sqlx::PgPool;
use utoipa::{OpenApi, ToSchema};

use crate::db;

/// 핸들러가 공유하는 상태.
#[derive(Debug, Clone)]
pub struct AppState {
    /// PostgreSQL 연결 풀.
    pub pool: PgPool,
}

/// 서비스 상태 응답.
#[derive(Debug, Serialize, ToSchema)]
pub struct Health {
    /// 전체 상태. `ok` 또는 `degraded`.
    pub status: &'static str,
    /// 데이터베이스 왕복 결과. `ok` 또는 `error`.
    pub database: &'static str,
}

/// 서비스와 데이터베이스 연결 상태를 돌려준다.
#[utoipa::path(
    get,
    path = "/healthz",
    responses(
        (status = 200, description = "모두 정상", body = Health),
        (status = 503, description = "데이터베이스 연결 실패", body = Health),
    ),
    tag = "system"
)]
pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    match db::ping(&state.pool).await {
        Ok(()) => (
            StatusCode::OK,
            Json(Health {
                status: "ok",
                database: "ok",
            }),
        ),
        Err(err) => {
            tracing::warn!(error = %err, "database ping failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(Health {
                    status: "degraded",
                    database: "error",
                }),
            )
        }
    }
}

/// 공개 API 계약. `contracts/openapi.json`은 여기서 생성한다.
#[derive(OpenApi)]
#[openapi(
    info(title = "fvoci API", version = "0.1.0"),
    paths(healthz),
    components(schemas(Health)),
    tags((name = "system", description = "서비스 상태"))
)]
#[derive(Debug)]
pub struct ApiDoc;

/// 라우터를 조립한다.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .with_state(state)
}
