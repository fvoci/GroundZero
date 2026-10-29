//! 기록·학습의 칸 검증과 SQL 거절을 공통 오류 계약으로 바꾼다.

use fvoci_core::error::{AppError, InvalidReason};

/// 기록·회차·스터디 제목의 고정 상한. 서버의 일반 제목 설정과 독립적이다.
pub(crate) const TITLE_LIMIT: usize = 300;

pub(crate) fn invalid(field: &str, message: impl Into<String>) -> AppError {
    AppError::invalid(field, InvalidReason::InvalidFormat, None, message)
}

pub(crate) fn length(value: &str, field: &str, max: usize) -> Result<(), AppError> {
    if value.chars().count() > max {
        return Err(AppError::invalid(
            field,
            InvalidReason::TooLong,
            u64::try_from(max).ok(),
            format!("{field} must be at most {max} characters"),
        ));
    }
    Ok(())
}

pub(crate) fn title(value: &str) -> Result<(), AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::invalid(
            "title",
            InvalidReason::Required,
            None,
            "title must not be empty",
        ));
    }
    length(value, "title", TITLE_LIMIT)
}

pub(crate) fn count(count: usize, field: &str, min: usize, max: usize) -> Result<(), AppError> {
    if count < min {
        return Err(AppError::invalid(
            field,
            InvalidReason::Required,
            None,
            format!("{field} must contain at least {min} entries"),
        ));
    }
    if count > max {
        return Err(AppError::invalid(
            field,
            InvalidReason::TooMany,
            u64::try_from(max).ok(),
            format!("{field} must contain at most {max} entries"),
        ));
    }
    Ok(())
}

pub(crate) fn range(value: i32, field: &str, min: i32, max: i32) -> Result<(), AppError> {
    if !(min..=max).contains(&value) {
        return Err(AppError::invalid(
            field,
            InvalidReason::OutOfRange,
            u64::try_from(max).ok(),
            format!("{field} must be between {min} and {max}"),
        ));
    }
    Ok(())
}

pub(crate) fn url(value: &str) -> Result<(), AppError> {
    let value = value.trim();
    let lower = value.to_ascii_lowercase();
    let tail = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    if !value.is_empty()
        && (tail.is_none_or(str::is_empty) || value.chars().any(char::is_whitespace))
    {
        return Err(invalid(
            "url",
            "the link must start with http:// or https://",
        ));
    }
    Ok(())
}

pub(crate) fn progress(status: &str, note: &str) -> Result<(), AppError> {
    if !matches!(status, "not_started" | "in_progress" | "done") {
        return Err(invalid("status", "invalid progress status"));
    }
    length(note, "note", 20_000)
}

pub(crate) fn from_db(error: sqlx::Error, field: &str) -> AppError {
    match AppError::from_db_function(error) {
        AppError::BadRequest(message) if message == "today is too far from the server date" => {
            AppError::invalid("today", InvalidReason::OutOfRange, Some(1), message)
        }
        AppError::BadRequest(message) => invalid(field, message),
        other => other,
    }
}

pub(crate) fn is_stale(error: &sqlx::Error, resource: &str) -> bool {
    error.as_database_error().is_some_and(|error| {
        error.code().as_deref() == Some("55000")
            && error.message() == format!("the {resource} changed; reload it")
    })
}

pub(crate) fn sent(input: &impl serde::Serialize) -> Result<serde_json::Value, AppError> {
    let mut value = serde_json::to_value(input)
        .map_err(|error| AppError::Config(format!("write fields: {error}")))?;
    if let Some(fields) = value.as_object_mut() {
        fields.remove("version");
        for name in ["title", "chapter", "url"] {
            if let Some(text) = fields.get(name).and_then(serde_json::Value::as_str) {
                fields.insert(
                    name.into(),
                    serde_json::Value::String(text.trim().to_owned()),
                );
            }
        }
    }
    Ok(value)
}
