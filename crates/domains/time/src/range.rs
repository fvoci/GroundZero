use fvoci_core::{
    error::{AppError, InvalidReason},
    validate::parse_date,
};
use fvoci_db::UserTx;
use serde::Deserialize;
use time::OffsetDateTime;
use utoipa::IntoParams;
const MAX_RANGE_DAYS: i64 = 400;

/// 기간(보는 사람 시간대의 날짜, 끝 포함).
#[derive(Debug, Deserialize, IntoParams)]
pub struct TimeRange {
    pub from: String,
    pub to: String,
    pub tz: String,
}

pub(crate) struct Window {
    pub(crate) from_at: OffsetDateTime,
    pub(crate) to_at: OffsetDateTime,
}

pub(crate) async fn window(tx: &mut UserTx, range: &TimeRange) -> Result<Window, AppError> {
    let from = parse_date(Some(&range.from), "from")?.ok_or_else(|| {
        AppError::invalid("from", InvalidReason::Required, None, "from is required")
    })?;
    let to = parse_date(Some(&range.to), "to")?
        .ok_or_else(|| AppError::invalid("to", InvalidReason::Required, None, "to is required"))?;
    if to < from || (to - from).whole_days() >= MAX_RANGE_DAYS {
        return Err(AppError::invalid(
            "to",
            InvalidReason::OutOfRange,
            Some(400),
            format!("the range must be 1 to {MAX_RANGE_DAYS} days"),
        ));
    }
    // 하루의 시작은 그 시간대에서 그날 0시가 처음 오는 시점이다. 0시가 두 번 오는 시간대(아바나·아소르스의 가을)에서
    // 기본 해석은 뒤의 0시라 앞의 0시~1시가 전날로 밀리고, 집계와 목록이 서로 다른 날에 두었다(감사 S21). 0시가 없는
    // 날(봄에 0시를 건너뜀)은 기본 해석(건너뛴 직후)을 쓴다. 집계의 날 경계(`summary`)도 같은 식이다.
    let (from_at, to_at): (OffsetDateTime, OffsetDateTime) = sqlx::query_as(
        "SELECT coalesce(app_local_instant($1::date::timestamp, $3), $1::date::timestamp AT TIME ZONE $3),
                coalesce(app_local_instant(($2::date + 1)::timestamp, $3), ($2::date + 1)::timestamp AT TIME ZONE $3)",
    )
    .bind(from)
    .bind(to)
    .bind(range.tz.trim())
    .fetch_one(tx.conn())
    .await
    .map_err(|e| match AppError::from_db_function(e) {
        AppError::BadRequest(message) => AppError::invalid("tz", InvalidReason::InvalidFormat, None, message),
        other => other,
    })?;
    Ok(Window { from_at, to_at })
}
