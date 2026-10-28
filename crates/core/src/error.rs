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
    /// 이 요청의 id(응답 머리글 `x-request-id`와 같다). 운영 로그에서 같은 요청의 줄을 찾을 때 쓴다(D-83).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub request_id: Option<String>,
    /// 칸 검증 실패(400 `bad_request`)의 안정된 이유. 화면은 `message` 대신 이 값으로 문구를 고른다(D-83, #76·#79).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub reason: Option<InvalidReason>,
    /// 거절된 입력 칸 이름(요청 본문의 칸 이름, 예: `title`·`until`).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub field: Option<String>,
    /// 넘은 한도(글자 수·개수·최댓값). 실행 설정으로 정해지는 한도(제목 `FVOCI_MAX_TITLE_CHARS`)도 여기서 알린다.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(nullable = false)]
    pub limit: Option<u64>,
}

impl ErrorBody {
    /// 칸 검증 정보 없는 오류 본문.
    pub fn new(error: &'static str, message: String, request_id: Option<String>) -> Self {
        Self {
            error,
            message,
            request_id,
            reason: None,
            field: None,
            limit: None,
        }
    }
}

/// 칸 검증 실패의 이유(언어와 상관없는 코드).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InvalidReason {
    /// 비어 있으면 안 된다.
    Required,
    /// 글자 수가 `limit`을 넘는다.
    TooLong,
    /// 개수가 `limit`을 넘는다.
    TooMany,
    /// 줄바꿈·탭 밖의 제어 문자가 있다.
    ControlCharacters,
    /// 형식이 틀렸다(예: 날짜는 `YYYY-MM-DD`).
    InvalidFormat,
    /// 허용 범위 밖이다(`limit`이 있으면 그 최댓값).
    OutOfRange,
    /// 반복 끝(`until`)이 첫 회차보다 앞선다(#76).
    UntilBeforeFirstOccurrence,
}

/// 칸 검증 실패의 세부.
#[derive(Debug)]
pub struct Invalid {
    pub field: String,
    pub reason: InvalidReason,
    pub limit: Option<u64>,
    /// 사람이 읽는 영어 진단 문구.
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
    /// 칸 검증 실패: 400 `bad_request`에 칸·이유·한도를 함께 싣는다.
    #[error("bad request: {}", .0.message)]
    Invalid(Box<Invalid>),
    /// 현재 상태에서 할 수 없는 요청(예: 소유자 탈퇴, 이미 닫힌 초대).
    #[error("invalid state: {0}")]
    Conflict(String),
    /// 요청의 기대 사용자(`x-fvoci-expected-user`)가 세션의 사용자와 다름. 다른 탭에서 계정을 바꾼 뒤 앞 계정의 화면이
    /// 보낸 요청이다. 아무것도 바꾸지 않고 409 `account_changed`.
    #[error("account changed")]
    AccountChanged,
    /// 같은 요청 id로 만든 업무가 휴지통에 있다(`app_create_task`의 `FV004`, D-105). 그 요청 id로는 다시 만들 수 없으니
    /// 새 요청 id로 보내라는 뜻이고, 항목을 볼 수 없는 404와 구분된다. 아무것도 바꾸지 않고 409 `task_deleted`.
    #[error("task deleted")]
    TaskDeleted,
    /// 같은 대상의 타이머가 이미 실행 중이며 이어가기/새 기록 선택이 필요하다.
    #[error("a timer for this target is already running")]
    TimerRunning,
    /// 파일 저장소(로컬·S3) 실패.
    #[error("storage error: {0}")]
    Storage(String),
    /// 바깥 서비스(예: Zotero) 응답 실패. 메시지는 사용자에게 보인다(자격 증명을 넣지 않는다).
    #[error("upstream error: {0}")]
    Upstream(String),
}

/// 반복 끝이 첫 회차보다 앞설 때 SQL이 올리는 문구(`crates/db/migrations/202610280005_work.sql`).
const UNTIL_BEFORE_FIRST_OCCURRENCE: &str = "until must not be before the first occurrence";

impl AppError {
    /// 칸 검증 실패(400 `bad_request`, 칸·이유·한도 포함).
    pub fn invalid(
        field: &str,
        reason: InvalidReason,
        limit: Option<u64>,
        message: impl Into<String>,
    ) -> Self {
        Self::Invalid(Box::new(Invalid {
            field: field.to_owned(),
            reason,
            limit,
            message: message.into(),
        }))
    }

    /// 정의자 함수가 올린 오류 코드를 응답으로 바꾼다.
    /// P0002 → 404, 42501 → 403, 22023 → 400, 55000 → 409, FV004 → 409 `task_deleted`. 그 밖은 내부 오류.
    pub fn from_db_function(err: sqlx::Error) -> Self {
        let Some(db) = err.as_database_error() else {
            return Self::Database(err);
        };
        let message = db.message().to_owned();
        match db.code().as_deref() {
            Some("P0002") => Self::NotFound,
            Some("42501") => Self::Forbidden,
            // 반복 끝 거절은 일정·업무 반복이 함께 쓰는 문구다. 화면이 칸 옆에 이유를 보이도록 이유·칸을 싣는다(#76).
            Some("22023") if message == UNTIL_BEFORE_FIRST_OCCURRENCE => Self::invalid(
                "until",
                InvalidReason::UntilBeforeFirstOccurrence,
                None,
                message,
            ),
            Some("22023") => Self::BadRequest(message),
            Some("55000") if message == "a timer for this target is already running" => {
                Self::TimerRunning
            }
            Some("55000") => Self::Conflict(message),
            Some("FV004") => Self::TaskDeleted,
            // 동시에 같은 업무를 바꾸다 부딪힌 경우(교착 감지·다시 시도 요청): 아무것도 바뀌지 않았으니 다시 보내면 된다.
            Some("40001" | "40P01") => {
                Self::Conflict("the change collided with another one; try again".into())
            }
            // 앱이 넘긴 새 id가 모자랐다(D-88): 상한(`<함수>_ids`)을 본 뒤 다른 트랜잭션이 대상을 늘렸다. 트랜잭션은
            // 통째로 되돌아가므로(요청의 UserTx가 커밋되지 않음) 다시 보내면 상한부터 새로 본다. 상한 계산이 틀린
            // 버그도 같은 코드라 경고로 남겨 보이게 한다. 잘못된 id(FV002: 빈 값·UUIDv7 아님)는 서버 버그라 500이다.
            Some("FV001") => {
                tracing::warn!(
                    event = "db.new_ids_short",
                    "a function needed more new ids than the server passed; the change was rolled back"
                );
                Self::Conflict("the data changed while saving; try again".into())
            }
            _ => Self::Database(err),
        }
    }

    /// 응답·로그에 쓰는 안정된 오류 코드(UI 언어와 상관없다).
    #[must_use]
    pub fn code(&self) -> &'static str {
        self.status_and_code().1
    }

    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::BadRequest(_) | Self::Invalid(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Conflict(_) => (StatusCode::CONFLICT, "invalid_state"),
            Self::AccountChanged => (StatusCode::CONFLICT, "account_changed"),
            Self::TaskDeleted => (StatusCode::CONFLICT, "task_deleted"),
            Self::TimerRunning => (StatusCode::CONFLICT, "timer_running"),
            Self::Oidc(_) => (StatusCode::BAD_GATEWAY, "oidc_error"),
            Self::Storage(_) => (StatusCode::BAD_GATEWAY, "storage_error"),
            Self::Upstream(_) => (StatusCode::BAD_GATEWAY, "upstream_error"),
            Self::Config(_) | Self::Database(_) | Self::Migrate(_) | Self::Io(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error) = self.status_and_code();
        // 서버 쪽 실패는 여기서 한 번만 남긴다(D-83). 입력 오류·권한 거부·충돌(4xx)은 정상 흐름이라 로그로 남기지
        // 않고 접근 줄의 status_code로 본다. 바깥 서비스 실패(502)는 복구 가능한 이상이라 WARN.
        if status.is_server_error() {
            let detail = crate::logging::safe_error(&self);
            if status == StatusCode::INTERNAL_SERVER_ERROR {
                tracing::error!(
                    event = "http.error",
                    error_code = error,
                    status_code = status.as_u16(),
                    detail,
                    "request failed"
                );
            } else {
                tracing::warn!(
                    event = "http.upstream_error",
                    error_code = error,
                    status_code = status.as_u16(),
                    detail,
                    "an outside service failed"
                );
            }
        }
        let message = match &self {
            Self::BadRequest(msg) | Self::Conflict(msg) | Self::Upstream(msg) => msg.clone(),
            Self::AccountChanged => {
                "the signed-in account is not the one this edit was started with".to_owned()
            }
            Self::TaskDeleted => {
                "the task made by this request was deleted; send a new request id".to_owned()
            }
            Self::Invalid(invalid) => invalid.message.clone(),
            Self::TimerRunning => self.to_string(),
            _ => status.canonical_reason().unwrap_or("error").to_owned(),
        };
        let mut body = ErrorBody::new(error, message, crate::logging::current_request_id());
        if let Self::Invalid(invalid) = self {
            let Invalid {
                field,
                reason,
                limit,
                ..
            } = *invalid;
            body.reason = Some(reason);
            body.field = Some(field);
            body.limit = limit;
        }
        let mut response = (status, Json(body)).into_response();
        response
            .extensions_mut()
            .insert(crate::logging::ErrorCode(error));
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_of(err: AppError) -> (StatusCode, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn field_errors_keep_bad_request_and_add_reason_field_and_limit() {
        let (status, body) = body_of(crate::validate::too_long("title", 200)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            body,
            serde_json::json!({ "error": "bad_request", "message": "title must be at most 200 characters",
                "reason": "too_long", "field": "title", "limit": 200 })
        );
        // 칸 정보가 없는 오류는 예전 본문 그대로다(추가 칸 없음).
        let (_, plain) = body_of(AppError::BadRequest("nope".into())).await;
        assert_eq!(
            plain,
            serde_json::json!({ "error": "bad_request", "message": "nope" })
        );
    }
}
