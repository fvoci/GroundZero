//! 달력 입력 검증과 SQL 집계 응답의 변환(D-80).

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use time::Date;
use utoipa::IntoParams;
use uuid::Uuid;

use crate::CalendarState;
use crate::model::Calendar;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::WorkspaceId;
use fvoci_core::validate::parse_date;
use fvoci_http::session::CurrentUser;

/// 달력 한 번 조회의 최대 기간(일).
const MAX_RANGE_DAYS: i64 = 62;

/// 달력 기간. `from`·`to`는 날짜(끝 포함), `tz`는 보는 사람의 시간대(시각 있는 일정의 날짜 경계).
/// `module_id`·`item_id`로 범위를 그 모듈·항목으로 좁힌다(D-80, 같은 데이터를 거를 뿐 사본은 없다).
#[derive(Debug, Deserialize, IntoParams)]
pub struct CalendarRange {
    pub from: String,
    pub to: String,
    pub tz: String,
    /// 이 모듈의 항목만.
    #[serde(default)]
    #[param(value_type = Option<String>, format = Uuid)]
    pub module_id: Option<Uuid>,
    /// 이 항목만.
    #[serde(default)]
    #[param(value_type = Option<String>, format = Uuid)]
    pub item_id: Option<Uuid>,
}

impl CalendarRange {
    fn dates(&self) -> Result<(Date, Date), AppError> {
        let from = parse_date(Some(&self.from), "from")?
            .ok_or_else(|| AppError::BadRequest("from is required".into()))?;
        let to = parse_date(Some(&self.to), "to")?
            .ok_or_else(|| AppError::BadRequest("to is required".into()))?;
        if to < from || (to - from).whole_days() >= MAX_RANGE_DAYS {
            return Err(AppError::BadRequest(format!(
                "the range must be 1 to {MAX_RANGE_DAYS} days"
            )));
        }
        Ok((from, to))
    }
}

/// 워크스페이스 달력(멤버). 볼 수 있는 업무·예정 회차·일정 회차만 나온다. 기간은 62일까지. 읽기 전용이다.
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/calendar", operation_id = "workspace_calendar",
    params(("workspace_id" = String, Path, format = Uuid), CalendarRange),
    responses((status = 200, body = Calendar), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "events")]
pub async fn calendar(
    State(state): State<CalendarState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
    Query(range): Query<CalendarRange>,
) -> Result<Json<Calendar>, AppError> {
    let (from, to) = range.dates()?;
    let mut tx = state.db.begin_as(user).await?;
    let value: serde_json::Value =
        sqlx::query_scalar("SELECT app_calendar($1, $2, $3, $4, $5, $6)")
            .bind(ws)
            .bind(from)
            .bind(to)
            .bind(range.tz.trim())
            .bind(range.module_id)
            .bind(range.item_id)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    serde_json::from_value(value)
        .map(Json)
        .map_err(|error| AppError::Config(format!("invalid calendar aggregate: {error}")))
}
