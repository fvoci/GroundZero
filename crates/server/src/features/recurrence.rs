//! 반복 업무·반복 일정 (D-31, D-50, D-57).
//!
//! 규칙은 단순한 형태(매일·매주(요일)·매달·매년, 간격, 끝 날짜 또는 횟수)이며 RFC 5545 전체가 아니다. 펼치기는
//! DB 함수 `app_recurrence_dates` 하나가 한다. 반복 업무의 회차는 쓰기 요청 안에서만 만든다(백그라운드 작업 구조
//! O-19 미결): 규칙을 정할 때, 반복 업무를 고칠 때, 쓰기 권한이 있는 사람이 항목을 열 때의 동기화 요청.
//! 읽기는 아직 없는 회차를 "예정"으로 계산해 보여 줄 뿐 만들지 않는다. 반복 일정은 달력에서 펼친다.
//! 처리는 `migrations/0014_recurrence.sql`.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::events::Event;
use crate::features::tasks::{TaskDetail, format_date, load_detail, parse_date};
use crate::http::AppState;
use crate::ids::ItemId;

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
    pub interval: u16,
    /// 매주 반복의 요일(ISO, 월=1 … 일=7). 비우면 기준일의 요일.
    pub weekdays: Vec<u8>,
    /// 이 날짜까지(포함). `count`와 함께 쓸 수 없다.
    pub until: Option<String>,
    /// 첫 회차를 포함한 횟수(1~1000).
    pub count: Option<u16>,
}

impl RecurrenceRule {
    /// 확인한 뒤 DB에 넘길 JSON.
    pub(crate) fn validated(&self) -> Result<serde_json::Value, AppError> {
        if !(1..=99).contains(&self.interval) {
            return Err(AppError::BadRequest("interval must be 1 to 99".into()));
        }
        let mut weekdays = self.weekdays.clone();
        weekdays.sort_unstable();
        weekdays.dedup();
        if weekdays.iter().any(|d| !(1..=7).contains(d)) {
            return Err(AppError::BadRequest(
                "weekdays must be 1 (Monday) to 7 (Sunday)".into(),
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
            return Err(AppError::BadRequest("count must be 1 to 1000".into()));
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
    pub(crate) fn from_db(value: serde_json::Value) -> Result<Self, AppError> {
        serde_json::from_value(value)
            .map_err(|e| AppError::Config(format!("stored recurrence rule is invalid: {e}")))
    }
}

/// 업무 반복 정하기. `rule`이 없으면 반복을 멈춘다(이미 만든 회차는 남는다).
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetTaskRecurrence {
    pub rule: Option<RecurrenceRule>,
    /// "오늘"을 정하는 시간대(IANA). 공동 반복은 지정한 시간대를 따른다(D-50).
    pub timezone: String,
}

/// 반복 업무 정하기·바꾸기·멈추기(쓰기 권한, 첫 회차에서). 처음 정하면 그 업무가 첫 회차가 되고, 오늘까지의
/// 회차와 다음 회차를 만든다. 이미 만든 회차는 바꾸지 않는다.
#[utoipa::path(put, path = "/api/tasks/{task_id}/recurrence", operation_id = "set_task_recurrence",
    params(("task_id" = String, Path, format = Uuid)), request_body = SetTaskRecurrence,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn set_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SetTaskRecurrence>,
) -> Result<Json<TaskDetail>, AppError> {
    let rule = input
        .rule
        .as_ref()
        .map(RecurrenceRule::validated)
        .transpose()?;
    let mut tx = state.db.begin_as(user).await?;
    let series: Option<Uuid> = sqlx::query_scalar("SELECT app_set_task_recurrence($1, $2, $3)")
        .bind(id)
        .bind(rule.as_ref())
        .bind(input.timezone.trim())
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    // 규칙은 이미 저장됐다. 회차 맞추기가 실패해도(잠금 충돌 등) 다음 쓰기 요청이나 항목 열기에서 이어 한다.
    if let Some(series) = series
        && let Err(e) = sync_series(&state, user, series).await
    {
        tracing::warn!(event = "recurrence.sync_failed", error_code = e.code(), detail = crate::logging::safe_error(&e), %series, "recurrence sync after rule change failed");
    }
    let mut tx = state.db.begin_as(user).await?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 한 반복의 회차를 만든다(별도 트랜잭션: 앞선 변경의 잠금과 섞이지 않게).
pub(crate) async fn sync_series(
    state: &AppState,
    user: crate::ids::UserId,
    series: Uuid,
) -> Result<i32, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let made: i32 = sqlx::query_scalar("SELECT app_sync_task_series($1)")
        .bind(series)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(made)
}

/// 동기화 결과.
#[derive(Debug, Serialize, ToSchema)]
pub struct SyncResult {
    /// 이번에 만든 회차 수. 한 번에 50개까지 만들며 남은 것은 다음 요청에서 이어 만든다.
    pub created: i32,
}

/// 항목의 반복 업무 회차 만들기(쓰기 권한). 화면이 쓰기 권한으로 항목을 열 때 보낸다. 독자는 403.
#[utoipa::path(post, path = "/api/items/{item_id}/recurrences/sync", operation_id = "sync_item_recurrences",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = SyncResult), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn sync_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<SyncResult>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let created: i32 = sqlx::query_scalar("SELECT app_sync_item_series($1)")
        .bind(item)
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(SyncResult { created }))
}

/// 일정 반복 정하기. `rule`이 없으면 반복을 멈춘다. 기준일은 일정 시간대의 시작 날짜.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetEventRecurrence {
    pub rule: Option<RecurrenceRule>,
}

/// 일정 반복 정하기·멈추기(쓰기 권한).
#[utoipa::path(put, path = "/api/events/{event_id}/recurrence", operation_id = "set_event_recurrence",
    params(("event_id" = String, Path, format = Uuid)), request_body = SetEventRecurrence,
    responses((status = 200, body = Event), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "events")]
pub async fn set_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SetEventRecurrence>,
) -> Result<Json<Event>, AppError> {
    let rule = input
        .rule
        .as_ref()
        .map(RecurrenceRule::validated)
        .transpose()?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_event_recurrence($1, $2)")
        .bind(id)
        .bind(rule.as_ref())
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let event = crate::features::events::load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(event))
}

/// 반복 일정의 회차 하나 빼기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExcludeOccurrence {
    /// 일정 시간대 기준의 회차 날짜.
    pub date: String,
}

/// 반복 일정의 회차 하나 빼기(쓰기 권한). 이미 뺐으면 그대로.
#[utoipa::path(post, path = "/api/events/{event_id}/exclusions", operation_id = "exclude_event_occurrence",
    params(("event_id" = String, Path, format = Uuid)), request_body = ExcludeOccurrence,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "events")]
pub async fn exclude_event(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ExcludeOccurrence>,
) -> Result<StatusCode, AppError> {
    let date = parse_date(Some(&input.date), "date")?
        .ok_or_else(|| AppError::BadRequest("date is required".into()))?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_exclude_event_occurrence($1, $2)")
        .bind(id)
        .bind(date)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
