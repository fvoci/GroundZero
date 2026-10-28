use crate::{
    TimeState,
    model::TimeKind,
    range::{TimeRange, window},
};
use axum::{
    Json,
    extract::{Query, State},
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    validate::format_date,
};
use fvoci_http::session::CurrentUser;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::{IntoParams, ToSchema};

/// 집계 단위.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SummaryGroup {
    Day,
    Week,
    Month,
}

/// 집계 조건. 주는 `week_start`(`mon` 기본·`sun`)부터.
#[derive(Debug, Deserialize, IntoParams)]
pub struct SummaryQuery {
    pub from: String,
    pub to: String,
    pub tz: String,
    pub group: SummaryGroup,
    /// 한 주 시작(D-81): `mon`(기본)·`sun`.
    #[serde(default)]
    pub week_start: Option<String>,
}

/// 기간·종류별 합계.
#[derive(Debug, Serialize, ToSchema)]
pub struct TimeSummaryRow {
    /// 기간의 첫날(`YYYY-MM-DD`).
    pub period: String,
    pub kind: TimeKind,
    pub seconds: i64,
}

/// 내 시간 집계(일·주·월, 보는 사람 시간대). 기록 원본은 그대로 두고 합계만 그 시간대의 자정에서 나눈다(D-81).
/// 일시정지 시각은 저장하지 않으므로 일시정지 시간은 경과 비율로 나눈다. 나눈 초의 합은 기록의 초와 같다.
#[utoipa::path(get, path = "/api/me/time-summary", operation_id = "time_summary", params(SummaryQuery),
    responses((status = 200, body = [TimeSummaryRow]), (status = 400, body = ErrorBody)), tag = "time")]
pub async fn summary(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<SummaryQuery>,
) -> Result<Json<Vec<TimeSummaryRow>>, AppError> {
    let range = TimeRange {
        from: query.from,
        to: query.to,
        tz: query.tz,
    };
    let unit = match query.group {
        SummaryGroup::Day => "day",
        SummaryGroup::Week => "week",
        SummaryGroup::Month => "month",
    };
    let week_offset: i32 = match query.week_start.as_deref().unwrap_or("mon") {
        "mon" => 0,
        "sun" => 1,
        _ => return Err(AppError::BadRequest("week_start must be mon or sun".into())),
    };
    let mut tx = state.db.begin_as(user).await?;
    let w = window(&mut tx, &range).await?;
    // 기록을 보는 사람 시간대의 날짜마다 조각으로 나누고, 조각의 초는 누적 경과 비율의 내림 차로 정해 조각의 합이
    // 기록의 초(app_entry_seconds)와 정확히 같게 한다. 기간 경계도 그 시간대의 자정이다. 날짜는 기간 안에서만
    // 펼친다(조각의 초는 기록 전체의 s·f로 나누므로 결과는 같다). 길이 제한이 없는 기록 하나가 조회마다 수백만 날을
    // 펼치지 않게 한다(감사 S20).
    let rows: Vec<(Date, String, i64)> = sqlx::query_as(
        "WITH e AS (
           SELECT e.kind, e.started_at AS s, coalesce(e.ended_at, now()) AS f, app_entry_seconds(e)::bigint AS active
           FROM time_entries e
           WHERE e.deleted_at IS NULL AND e.started_at < $2 AND coalesce(e.ended_at, now()) >= $1
         ),
         seg AS (
           SELECT e.kind, e.active, e.s, e.f, g.day, greatest(e.s, g.lo) AS ss, least(e.f, g.hi) AS sf
           FROM e
           CROSS JOIN LATERAL (
             SELECT d::date AS day,
                    coalesce(app_local_instant(d::date::timestamp, $3), d::date::timestamp AT TIME ZONE $3) AS lo,
                    coalesce(app_local_instant((d::date + 1)::timestamp, $3),
                             (d::date + 1)::timestamp AT TIME ZONE $3) AS hi
             FROM generate_series((greatest(e.s, $1) AT TIME ZONE $3)::date, (least(e.f, $2) AT TIME ZONE $3)::date,
                                  interval '1 day') d
           ) g
         ),
         part AS (
           SELECT kind, day, ss,
                  CASE WHEN f = s THEN CASE WHEN ss = s THEN active ELSE 0 END
                       ELSE floor(active * extract(epoch FROM sf - s) / extract(epoch FROM f - s))::bigint
                          - floor(active * extract(epoch FROM ss - s) / extract(epoch FROM f - s))::bigint
                  END AS seconds
           FROM seg
           WHERE sf > ss OR f = s
         )
         SELECT CASE $4
                  WHEN 'day' THEN day
                  WHEN 'week' THEN (date_trunc('week', day + $5) - make_interval(days => $5))::date
                  ELSE date_trunc('month', day)::date
                END AS period,
                kind, sum(seconds)::bigint
         FROM part
         WHERE ss >= $1 AND ss < $2
         GROUP BY 1, 2 ORDER BY 1, 2",
    )
    .bind(w.from_at)
    .bind(w.to_at)
    .bind(range.tz.trim())
    .bind(unit)
    .bind(week_offset)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(period, kind, seconds)| {
            Ok(TimeSummaryRow {
                period: format_date(Some(period)).unwrap_or_default(),
                kind: TimeKind::parse(&kind)
                    .ok_or_else(|| AppError::Config(format!("unknown time kind {kind}")))?,
                seconds,
            })
        })
        .collect::<Result<_, AppError>>()
        .map(Json)
}
