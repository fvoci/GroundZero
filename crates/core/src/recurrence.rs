//! 업무와 일정이 공유하는 반복 규칙. 회차 전개는 SQL 함수가 맡는다.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::{AppError, InvalidReason};
use crate::validate::{format_date, parse_date};

/// 반복 주기.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// 반복 규칙. 기준일이 첫 회차이고 `count`는 첫 회차를 포함한다. 매달 31일·2월 29일처럼 없는 날은 건너뛴다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct RecurrenceRule {
    pub freq: Frequency,
    /// 1~99.
    #[schema(minimum = 1, maximum = 99)]
    pub interval: u16,
    /// 매주 반복의 요일(ISO, 월=1 … 일=7). 비우면 기준일의 요일.
    #[schema(max_items = 7)]
    pub weekdays: Vec<u8>,
    /// 이 날짜까지(포함). `count`와 함께 쓸 수 없다.
    pub until: Option<String>,
    /// 첫 회차를 포함한 횟수(1~1000).
    #[schema(minimum = 1, maximum = 1000)]
    pub count: Option<u16>,
}

impl RecurrenceRule {
    /// 확인한 뒤 DB에 넘길 JSON.
    pub fn validated(&self) -> Result<serde_json::Value, AppError> {
        if !(1..=99).contains(&self.interval) {
            return Err(out_of_range("interval", 99, "interval must be 1 to 99"));
        }
        let mut weekdays = self.weekdays.clone();
        weekdays.sort_unstable();
        weekdays.dedup();
        if weekdays.iter().any(|d| !(1..=7).contains(d)) {
            return Err(out_of_range(
                "weekdays",
                7,
                "weekdays must be 1 (Monday) to 7 (Sunday)",
            ));
        }
        if self.freq != Frequency::Weekly && !weekdays.is_empty() {
            return Err(AppError::BadRequest(
                "weekdays are only for weekly rules".into(),
            ));
        }
        if self.until.is_some() && self.count.is_some() {
            return Err(AppError::BadRequest(
                "use either until or count, not both".into(),
            ));
        }
        if self.count.is_some_and(|c| !(1..=1000).contains(&c)) {
            return Err(out_of_range("count", 1000, "count must be 1 to 1000"));
        }
        let until = format_date(parse_date(self.until.as_deref(), "until")?);
        Ok(serde_json::json!({
            "freq": self.freq,
            "interval": self.interval,
            "weekdays": weekdays,
            "until": until,
            "count": self.count,
        }))
    }

    /// DB에 저장된 규칙을 읽는다.
    pub fn from_db(value: serde_json::Value) -> Result<Self, AppError> {
        serde_json::from_value(value)
            .map_err(|e| AppError::Config(format!("stored recurrence rule is invalid: {e}")))
    }
}

/// 반복 규칙 칸이 범위(1..=`max`) 밖이다.
fn out_of_range(field: &str, max: u64, message: &str) -> AppError {
    AppError::invalid(field, InvalidReason::OutOfRange, Some(max), message)
}
