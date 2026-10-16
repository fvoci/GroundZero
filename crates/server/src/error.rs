//! 서버 전역 오류 타입. 예상 가능한 오류는 `Result`로 전달하고 HTTP 경계에서 응답으로 바꾼다.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// 서버 시작·요청 처리에서 발생하는 오류.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// 환경 변수 등 구성 값이 없거나 잘못됨.
    #[error("invalid configuration: {0}")]
    Config(String),
    /// 데이터베이스 연결·쿼리 실패.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    /// 소켓 바인드·서빙 실패.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // 내부 오류 세부는 로그에만 남기고 응답에는 일반 메시지를 준다.
        tracing::error!(error = %self, "request failed");
        let status = match self {
            Self::Config(_) | Self::Database(_) | Self::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, status.canonical_reason().unwrap_or("error")).into_response()
    }
}
