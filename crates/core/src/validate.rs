//! 입력 칸 검증: 이름·제목, 여러 줄 글, 시각 없는 날짜(D-50). 여러 도메인이 같은 규칙을 쓴다.

use std::ops::RangeInclusive;

use time::Date;
use time::format_description::well_known::Iso8601;

use crate::error::{AppError, InvalidReason};

/// 저장할 수 있는 날짜의 연도(터무니없이 먼 날짜는 받지 않는다. 반복 펼치기·달력 계산이 그 거리만큼 커지지 않게).
pub const DATE_YEARS: RangeInclusive<i32> = 1900..=2200;

/// 이메일 길이 상한(RFC 5321).
const MAX_EMAIL: usize = 254;

pub fn validate_email(raw: &str) -> Result<String, AppError> {
    let email = raw.trim().to_lowercase();
    let valid = email.len() <= MAX_EMAIL
        && email
            .split_once('@')
            .is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.'))
        && !email.chars().any(|c| c.is_whitespace() || c.is_control());
    if valid {
        Ok(email)
    } else {
        Err(AppError::BadRequest("email is not valid".into()))
    }
}

/// 이름·제목 검증. 앞뒤 공백을 지우고 비어 있거나 상한을 넘으면 거부한다.
pub fn validate_title(raw: &str, max_chars: usize, field: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::invalid(
            field,
            InvalidReason::Required,
            None,
            format!("{field} must not be empty"),
        ));
    }
    if trimmed.chars().count() > max_chars {
        return Err(too_long(field, max_chars));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(control_characters(field));
    }
    Ok(trimmed.to_owned())
}

/// 여러 줄 글 검증. 앞뒤 공백을 지우고 상한을 넘거나 줄바꿈·탭 밖의 제어 문자가 있으면 거부한다(비어도 된다).
pub fn validate_text(raw: &str, max: usize, field: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > max {
        return Err(too_long(field, max));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(control_characters(field));
    }
    Ok(trimmed.to_owned())
}

/// `YYYY-MM-DD` 날짜. 비었거나 공백뿐이면 없음, 시각이 붙었거나 [`DATE_YEARS`] 밖이면 400.
pub fn parse_date(raw: Option<&str>, field: &str) -> Result<Option<Date>, AppError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        // 시각이 붙은 값은 받지 않는다(날짜만 있는 일정은 날짜 그대로, D-50).
        Some(s) if s.len() != 10 => Err(date_format(field)),
        Some(s) => {
            let date = Date::parse(s, &Iso8601::DATE).map_err(|_| date_format(field))?;
            if !DATE_YEARS.contains(&date.year()) {
                return Err(AppError::invalid(
                    field,
                    InvalidReason::OutOfRange,
                    None,
                    format!("{field} must be between 1900 and 2200"),
                ));
            }
            Ok(Some(date))
        }
    }
}

/// 글자 수 상한을 넘었다(`limit` = 상한).
pub fn too_long(field: &str, max: usize) -> AppError {
    AppError::invalid(
        field,
        InvalidReason::TooLong,
        u64::try_from(max).ok(),
        format!("{field} must be at most {max} characters"),
    )
}

fn control_characters(field: &str) -> AppError {
    AppError::invalid(
        field,
        InvalidReason::ControlCharacters,
        None,
        format!("{field} must not contain control characters"),
    )
}

fn date_format(field: &str) -> AppError {
    AppError::invalid(
        field,
        InvalidReason::InvalidFormat,
        None,
        format!("{field} must be a date like 2026-10-17"),
    )
}

/// 날짜를 `YYYY-MM-DD`로.
pub fn format_date(date: Option<Date>) -> Option<String> {
    date.map(|d| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation() {
        assert_eq!(
            validate_email("  Alice@Example.TEST ").unwrap(),
            "alice@example.test"
        );
        assert!(validate_email("no-at-sign").is_err());
        assert!(validate_email("@example.test").is_err());
        assert!(validate_email("a@localhost").is_err());
        assert!(validate_email("a b@example.test").is_err());
    }

    #[test]
    fn dates_are_plain_calendar_dates() {
        let d = parse_date(Some("2026-10-17"), "due_date").unwrap();
        assert_eq!(format_date(d).as_deref(), Some("2026-10-17"));
        assert_eq!(parse_date(Some(" "), "due_date").unwrap(), None);
        assert!(parse_date(Some("2026-13-01"), "due_date").is_err());
        assert!(parse_date(Some("2026-10-17T09:00:00Z"), "due_date").is_err());
    }
}
