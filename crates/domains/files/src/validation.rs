//! SQL 함수의 칸 검증을 공통 400 계약으로 바꾼다. 실제 한도는 SQL이 계산한다.
use fvoci_core::error::{AppError, InvalidReason};
use serde::Deserialize;
use sqlx::postgres::PgDatabaseError;

#[derive(Deserialize)]
struct FieldError {
    field: String,
    reason: String,
    limit: Option<u64>,
}

pub(crate) fn from_file_function(err: sqlx::Error) -> AppError {
    if let Some(db) = err
        .as_database_error()
        .filter(|db| db.code().as_deref() == Some("22023"))
    {
        let detail = db
            .try_downcast_ref::<PgDatabaseError>()
            .and_then(PgDatabaseError::detail)
            .and_then(|raw| serde_json::from_str::<FieldError>(raw).ok());
        if let Some(detail) = detail {
            let reason = match detail.reason.as_str() {
                "required" => Some(InvalidReason::Required),
                "too_long" => Some(InvalidReason::TooLong),
                "control_characters" => Some(InvalidReason::ControlCharacters),
                "invalid_format" => Some(InvalidReason::InvalidFormat),
                "out_of_range" => Some(InvalidReason::OutOfRange),
                _ => None,
            };
            if let Some(reason) = reason {
                return AppError::invalid(&detail.field, reason, detail.limit, db.message());
            }
        }
    }
    AppError::from_db_function(err)
}
