//! SQL 거절의 칸 정보를 HTTP 계약으로 변환한다. 권한·멱등성·잠금은 SQL에서 먼저 검사한다.
use fvoci_core::error::{AppError, InvalidReason};

/// SQL의 btrim(text)과 length(text)가 쓰는 댓글 한도(Unicode 코드 포인트).
const BODY_LIMIT: usize = 10_000;

pub(super) fn command_error(err: sqlx::Error, body: Option<&str>) -> AppError {
    let error = AppError::from_db_function(err);
    let AppError::BadRequest(message) = error else {
        return error;
    };
    match message.as_str() {
        "a comment needs 1 to 10000 characters" => {
            let length = body.unwrap_or_default().trim_matches(' ').chars().count();
            if length == 0 {
                AppError::invalid("body", InvalidReason::Required, None, message)
            } else {
                fvoci_core::validate::too_long("body", BODY_LIMIT)
            }
        }
        "comment on a document or a task" => {
            AppError::invalid("document_id", InvalidReason::InvalidFormat, None, message)
        }
        "reply to the first comment of the thread" => {
            AppError::invalid("parent_id", InvalidReason::InvalidFormat, None, message)
        }
        "resolve the first comment of the thread" => {
            AppError::invalid("comment_id", InvalidReason::InvalidFormat, None, message)
        }
        _ => AppError::BadRequest(message),
    }
}
