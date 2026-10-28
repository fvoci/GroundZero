//! 반복 일정의 규칙 정하기와 회차 빼기(D-31, D-50, D-57). 규칙 모양과 펼치기는 업무 반복과 같다(`recurrence`).
//! 일정은 일정 조각에서 옮긴다(노르웨이숲 태비 초안 2·6의 5).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::WorkState;
use crate::events::{Event, EventWriteConflict};
use crate::recurrence::RecurrenceRule;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::validate::parse_date;
use fvoci_db::new_ids;
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::CurrentUser;

/// 일정 반복 정하기. `rule`이 없으면 반복을 멈춘다. 기준일은 일정 시간대의 시작 날짜. `base`는 편집을 시작한
/// 규칙이다(반복이 없었으면 null). 그사이 규칙이 바뀌었으면 아무것도 바꾸지 않고 409 `rule_conflict`(최신 일정)다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetEventRecurrence {
    pub rule: Option<RecurrenceRule>,
    pub base: Option<RecurrenceRule>,
}

/// 일정 반복 정하기·멈추기(쓰기 권한). 기준 규칙과 지금 규칙이 다르면 409 `rule_conflict`(감사 W21: 기준 없이 저장해
/// 그사이 다른 사람이 바꾼 규칙을 옛 규칙으로 말없이 덮었다).
#[utoipa::path(put, path = "/api/events/{event_id}/recurrence", operation_id = "set_event_recurrence",
    params(("event_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "있으면 세션 사용자와 비교한다(D-96).")), request_body = SetEventRecurrence,
    responses((status = 200, body = Event), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = EventWriteConflict)), tag = "events")]
pub async fn set_event(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SetEventRecurrence>,
) -> Result<Response, AppError> {
    let rule = input
        .rule
        .as_ref()
        .map(RecurrenceRule::validated)
        .transpose()?;
    // 기준은 서버가 돌려준 규칙 그대로라 다시 확인하지 않고 같은 모양으로 바꾸기만 한다.
    let base = input
        .base
        .as_ref()
        .map(serde_json::to_value)
        .transpose()
        .map_err(|e| AppError::Config(format!("encode the base rule: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    let outcome: String = sqlx::query_scalar("SELECT app_set_event_recurrence($1, $2, $3, $4)")
        .bind(id)
        .bind(rule.as_ref())
        .bind(base.as_ref())
        .bind(new_ids(1))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let event = crate::events::load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        recurrence_conflict(event, rule).into_response()
    } else {
        Json(event).into_response()
    })
}

fn recurrence_conflict(current: Event, rule: Option<serde_json::Value>) -> StaleWrite<Event> {
    StaleWrite::new(
        "rule_conflict",
        "the recurrence rule changed",
        current,
        &serde_json::json!({ "recurrence": rule }),
    )
}

/// 반복 일정의 회차 하나 빼기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ExcludeOccurrence {
    /// 일정 시간대 기준의 회차 날짜.
    pub date: String,
}

/// 반복 일정의 회차 하나 빼기(쓰기 권한). 이미 뺐으면 그대로.
#[utoipa::path(post, path = "/api/events/{event_id}/exclusions", operation_id = "exclude_event_occurrence",
    params(("event_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, format = Uuid, description = "있으면 세션 사용자와 비교한다(D-96).")), request_body = ExcludeOccurrence,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "events")]
pub async fn exclude_event(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<ExcludeOccurrence>,
) -> Result<StatusCode, AppError> {
    let date = parse_date(Some(&input.date), "date")?.ok_or_else(|| {
        AppError::invalid("date", InvalidReason::Required, None, "date is required")
    })?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_exclude_event_occurrence($1, $2, $3)")
        .bind(id)
        .bind(date)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}
