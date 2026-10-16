//! 서버 전역 오류 타입. 예상 가능한 오류는 `Result`로 전달하고 HTTP 경계에서 응답으로 바꾼다.
//!
//! 권한 응답 규칙: 멤버가 아닌 워크스페이스·문서는 존재 여부를 숨기기 위해 404,
//! 멤버이지만 역할이 부족하면 403, 세션이 없거나 만료되면 401.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// 오류 응답 본문.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// 기계가 읽는 오류 코드.
    pub error: &'static str,
    /// 사람이 읽는 설명. 내부 세부는 담지 않는다.
    pub message: String,
}

/// 서버 시작·요청 처리에서 발생하는 오류.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 환경 변수 등 구성 값이 없거나 잘못됨.
    #[error("invalid configuration: {0}")]
    Config(String),
    /// 데이터베이스 연결·쿼리 실패.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    /// 마이그레이션 실패.
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// 소켓 바인드·서빙 실패.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// OIDC 제공자와의 통신·검증 실패.
    #[error("oidc error: {0}")]
    Oidc(String),
    /// 로그인 필요.
    #[error("unauthorized")]
    Unauthorized,
    /// 역할 부족.
    #[error("forbidden")]
    Forbidden,
    /// 대상이 없거나 볼 권한이 없음.
    #[error("not found")]
    NotFound,
    /// 입력 검증 실패.
    #[error("bad request: {0}")]
    BadRequest(String),
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Oidc(_) => (StatusCode::BAD_GATEWAY, "oidc_error"),
            Self::Config(_) | Self::Database(_) | Self::Migrate(_) | Self::Io(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error) = self.status_and_code();
        let message = match &self {
            Self::BadRequest(msg) => msg.clone(),
            other => {
                if status.is_server_error() {
                    // 내부 오류 세부는 로그에만 남긴다.
                    tracing::error!(error = %other, "request failed");
                }
                status.canonical_reason().unwrap_or("error").to_owned()
            }
        };
        (status, Json(ErrorBody { error, message })).into_response()
    }
}
