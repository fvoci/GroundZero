use crate::{
    TimeState,
    model::{TimeEntry, TimeKind, load},
    targets::{TargetInput, split_target},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use fvoci_core::{
    error::{AppError, ErrorBody, InvalidReason},
    validate::validate_text,
};
use fvoci_db::{is_unique_violation, new_ids};
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::CurrentUser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use utoipa::ToSchema;
use uuid::Uuid;
pub(crate) const MAX_MEMO_CHARS: usize = 2_000;

fn parse_instant(raw: &str, field: &str) -> Result<OffsetDateTime, AppError> {
    OffsetDateTime::parse(raw.trim(), &Rfc3339).map_err(|_| {
        AppError::invalid(
            field,
            InvalidReason::InvalidFormat,
            None,
            format!("{field} must be a time like 2026-10-17T09:00:00+09:00"),
        )
    })
}

/// 기록 내용(수동 추가·고치기 공통).
#[derive(Debug, Deserialize, ToSchema)]
pub struct EntryInput {
    /// RFC 3339.
    pub started_at: String,
    /// RFC 3339. 진행 중인 타이머를 고칠 때만 없다.
    pub ended_at: Option<String>,
    #[schema(minimum = 0)]
    pub paused_seconds: i32,
    pub kind: TimeKind,
    #[schema(max_length = 2000)]
    pub memo: String,
    pub target: Option<TargetInput>,
}

/// 수동 기록 추가.
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddEntry {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    #[serde(flatten)]
    pub entry: EntryInput,
}

/// 기록 고치기. `base_version`이 지났으면 409와 서버 최신본.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEntry {
    pub base_version: i32,
    #[serde(flatten)]
    pub entry: EntryInput,
}

/// 409 응답: 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum TimeEntryWriteConflict {
    Stale(Box<StaleWrite<TimeEntry>>),
    State(ErrorBody),
}

struct ValidEntry {
    started: OffsetDateTime,
    ended: Option<OffsetDateTime>,
    memo: String,
}

fn validate_entry(input: &EntryInput) -> Result<ValidEntry, AppError> {
    let started = parse_instant(&input.started_at, "started_at")?;
    let ended = input
        .ended_at
        .as_deref()
        .map(|s| parse_instant(s, "ended_at"))
        .transpose()?;
    if input.paused_seconds < 0 {
        return Err(AppError::invalid(
            "paused_seconds",
            InvalidReason::OutOfRange,
            None,
            "paused_seconds must not be negative",
        ));
    }
    Ok(ValidEntry {
        started,
        ended,
        memo: validate_text(&input.memo, MAX_MEMO_CHARS, "memo")?,
    })
}

/// 수동 기록 추가(끝난 기록). 같은 `request_id`면 같은 기록.
#[utoipa::path(post, path = "/api/me/time-entries", operation_id = "add_time_entry", request_body = AddEntry,
    params(("x-fvoci-expected-user" = Option<String>, Header, description = "If sent, must match the signed-in user (D-96).")),
    responses((status = 201, body = TimeEntry), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "time")]
pub async fn add(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<AddEntry>,
) -> Result<(StatusCode, Json<TimeEntry>), AppError> {
    let valid = validate_entry(&input.entry)?;
    let ended = valid.ended.ok_or_else(|| {
        AppError::invalid(
            "ended_at",
            InvalidReason::Required,
            None,
            "ended_at is required",
        )
    })?;
    let (target_kind, target_id) = split_target(input.entry.target);
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: Uuid =
            sqlx::query_scalar("SELECT app_add_time_entry($1, $2, $3, $4, $5, $6, $7, $8, $9)")
                .bind(input.request_id)
                .bind(valid.started)
                .bind(ended)
                .bind(input.entry.paused_seconds)
                .bind(input.entry.kind.as_str())
                .bind(&valid.memo)
                .bind(target_kind)
                .bind(target_id)
                .bind(new_ids(1))
                .fetch_one(tx.conn())
                .await
                .map_err(|e| entry_error(e, &input.entry, &valid, None))?;
        let entry = load(&mut tx, id).await?;
        tx.commit().await?;
        Ok(entry)
    };
    match run().await {
        Ok(entry) => Ok((StatusCode::CREATED, Json(entry))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 기록 고치기(본인). 이전 값은 수정 이력에 남는다. 진행 중이면 시작 시각·종류·메모·대상만 바꿀 수 있다.
#[utoipa::path(put, path = "/api/time-entries/{entry_id}", operation_id = "update_time_entry",
    params(("entry_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, description = "If sent, must match the signed-in user (D-96).")), request_body = UpdateEntry,
    responses((status = 200, body = TimeEntry), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = TimeEntryWriteConflict)), tag = "time")]
pub async fn update(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateEntry>,
) -> Result<Response, AppError> {
    let valid = validate_entry(&input.entry)?;
    let (target_kind, target_id) = split_target(input.entry.target);
    let mut tx = state.db.begin_as(user).await?;
    let before = load(&mut tx, id).await?;
    let outcome: String =
        sqlx::query_scalar("SELECT app_update_time_entry($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)")
            .bind(id)
            .bind(input.base_version)
            .bind(valid.started)
            .bind(valid.ended)
            .bind(input.entry.paused_seconds)
            .bind(input.entry.kind.as_str())
            .bind(&valid.memo)
            .bind(target_kind)
            .bind(target_id)
            .bind(new_ids(1))
            .fetch_one(tx.conn())
            .await
            .map_err(|e| entry_error(e, &input.entry, &valid, Some(&before)))?;
    let entry = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(if outcome == "conflict" {
        let sent = sent_fields(&input.entry, &valid)?;
        StaleWrite::new("conflict", "the time entry changed", entry, &sent).into_response()
    } else {
        Json(entry).into_response()
    })
}

/// 기록 지우기(본인).
#[utoipa::path(delete, path = "/api/time-entries/{entry_id}", operation_id = "delete_time_entry",
    params(("entry_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, description = "If sent, must match the signed-in user (D-96).")),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "time")]
pub async fn delete(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_time_entry($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

fn sent_fields(input: &EntryInput, valid: &ValidEntry) -> Result<Value, AppError> {
    let instant = |at: OffsetDateTime| {
        at.to_offset(time::UtcOffset::UTC)
            .format(&Rfc3339)
            .map_err(|e| AppError::Config(e.to_string()))
    };
    Ok(json!({
        "started_at": instant(valid.started)?,
        "ended_at": valid.ended.map(instant).transpose()?,
        "paused_seconds": input.paused_seconds,
        "kind": input.kind,
        "memo": valid.memo,
        "target": input.target,
    }))
}

/// SQL의 불변식·기존 긴 기록 허용은 그대로 두고 칸 거절의 진단만 공통 계약으로 바꾼다.
fn entry_error(
    error: sqlx::Error,
    input: &EntryInput,
    valid: &ValidEntry,
    current: Option<&TimeEntry>,
) -> AppError {
    let message = match AppError::from_db_function(error) {
        AppError::BadRequest(message) => message,
        other => return other,
    };
    let field = match message.as_str() {
        "invalid time range" if valid.ended.is_none_or(|end| end < valid.started) => "ended_at",
        "invalid time range" => "paused_seconds",
        "a running timer only changes its start, kind, memo and target"
            if input.ended_at.is_some() =>
        {
            "ended_at"
        }
        "a running timer only changes its start, kind, memo and target"
            if current.is_some_and(|entry| entry.paused_seconds != input.paused_seconds) =>
        {
            "paused_seconds"
        }
        "a running timer only changes its start, kind, memo and target" => "started_at",
        "the start leaves less time than the timer was paused" => "started_at",
        _ => return AppError::BadRequest(message),
    };
    let limit = (field == "paused_seconds")
        .then(|| {
            valid
                .ended
                .and_then(|end| u64::try_from((end - valid.started).whole_seconds()).ok())
        })
        .flatten();
    AppError::invalid(field, InvalidReason::OutOfRange, limit, message)
}
