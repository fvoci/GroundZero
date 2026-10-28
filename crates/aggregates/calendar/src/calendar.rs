//! 검증·날짜 경계와 출처별 달력 읽기의 조립.

use crate::{CalendarState, model::Calendar, response::CalendarRows};
use axum::Json;
use axum::extract::{Path, Query, State};
use fvoci_core::access::Grade as Access;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::WorkspaceId;
use fvoci_core::validate::parse_date;
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;
use serde::Deserialize;
use time::{Date, OffsetDateTime};
use utoipa::IntoParams;
use uuid::Uuid;

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

async fn boundaries(
    tx: &mut UserTx,
    ws: WorkspaceId,
    from: Date,
    to: Date,
    tz: &str,
) -> Result<(OffsetDateTime, OffsetDateTime), AppError> {
    let access: String = sqlx::query_scalar("SELECT app_access('workspace', $1)")
        .bind(ws)
        .fetch_one(tx.conn())
        .await?;
    Access::parse(&access)?.ok_or(AppError::NotFound)?;
    // 보는 사람의 시간대: 브라우저가 아는 이름만(일정 시간대와 같은 규칙).
    sqlx::query("SELECT app_check_timezone($1)")
        .bind(tz)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    // 보는 사람의 날짜 경계. 하루의 시작은 그날 0시가 처음 오는 시점이다(0시가 두 번 오는 날은 앞의 0시, 없는 날은
    // 건너뛴 직후). 시간 기록·검색의 날 경계(`time_tracking.rs`, `search_api.rs`, 감사 S21)와 같은 식.
    let (from_at, to_at): (OffsetDateTime, OffsetDateTime) = sqlx::query_as(
        "SELECT coalesce(app_local_instant($1::date::timestamp, $3), $1::date::timestamp AT TIME ZONE $3),
                coalesce(app_local_instant(($2::date + 1)::timestamp, $3), ($2::date + 1)::timestamp AT TIME ZONE $3)",
    )
    .bind(from)
    .bind(to)
    .bind(tz)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;

    Ok((from_at, to_at))
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
    let (from_at, to_at) = boundaries(&mut tx, ws, from, to, range.tz.trim()).await?;
    let task_rows = crate::sources::tasks(&mut tx, ws, from, to).await?;
    let projected_rows = crate::sources::projected(&mut tx, ws, from, to).await?;
    let occurrence_rows =
        crate::sources::occurrences(&mut tx, ws, from, to, from_at, to_at).await?;
    let event_rows = crate::events::load(&mut tx, &occurrence_rows).await?;
    let date_rows = crate::sources::dates(&mut tx, ws, from, to).await?;
    let mut rows = CalendarRows {
        task_rows,
        projected_rows,
        occurrence_rows,
        event_rows,
        date_rows,
    };
    if let Some(scope) = crate::scope::calendar_scope(&mut tx, ws, &range).await? {
        rows.retain(&scope);
    }
    let items = rows.items(&mut tx).await?;
    tx.commit().await?;
    rows.into_calendar(items).map(Json)
}
