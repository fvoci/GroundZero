use crate::commands::MAX_MEMO_CHARS;
use crate::{
    TimeState,
    model::{ENTRY_SELECT, EntryRow, TimeEntry, TimeKind, load, to_entry},
    targets::{TargetInput, split_target},
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use fvoci_core::{
    error::{AppError, ErrorBody},
    validate::validate_text,
};
use fvoci_db::{is_unique_violation, new_ids};
use fvoci_http::session::CurrentUser;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 진행 중인 타이머(없으면 null).
#[utoipa::path(get, path = "/api/me/timer", operation_id = "current_timer",
    responses((status = 200, body = Option<TimeEntry>)), tag = "time")]
pub async fn current(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Option<TimeEntry>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let row: Option<EntryRow> = sqlx::query_as(&format!(
        "{ENTRY_SELECT} WHERE e.ended_at IS NULL AND e.deleted_at IS NULL"
    ))
    .fetch_optional(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(row.map(to_entry).transpose()?))
}

/// 같은 대상에서 실행 중인 타이머를 다시 시작할 때의 선택.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerStartMode {
    /// 실행 중인 기록을 그대로 이어간다.
    Continue,
    /// 실행 중인 기록을 끝내고 새 기록을 시작한다.
    New,
}

impl TimerStartMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::New => "new",
        }
    }
}

/// 타이머 시작.
#[derive(Debug, Deserialize, ToSchema)]
pub struct StartTimer {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub kind: TimeKind,
    #[schema(max_length = 2000)]
    pub memo: String,
    pub target: Option<TargetInput>,
    /// 같은 대상이 실행 중이면 필수. 생략 시 409 `timer_running`.
    pub mode: Option<TimerStartMode>,
}

/// 새 타이머와 자동으로 끝낸 이전 기록.
#[derive(Debug, Serialize, ToSchema)]
pub struct StartedTimer {
    #[serde(flatten)]
    pub entry: TimeEntry,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub stopped_entry_id: Option<Uuid>,
}

/// 타이머 시작. 다른 대상의 기록은 자동으로 끝내며 같은 대상은 명시한 선택을 따른다.
#[utoipa::path(post, path = "/api/me/timer", operation_id = "start_timer", request_body = StartTimer,
    params(("x-fvoci-expected-user" = Option<String>, Header, description = "If sent, must match the signed-in user (D-96).")),
    responses((status = 201, body = StartedTimer), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, description = "같은 대상 실행 중: timer_running, 그 밖의 상태 충돌: invalid_state", body = ErrorBody)), tag = "time")]
pub async fn start(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<StartTimer>,
) -> Result<(StatusCode, Json<StartedTimer>), AppError> {
    let memo = validate_text(&input.memo, MAX_MEMO_CHARS, "memo")?;
    let (target_kind, target_id) = split_target(input.target);
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let (id, stopped_entry_id): (Uuid, Option<Uuid>) = sqlx::query_as(
            "SELECT entry_id, stopped_entry_id FROM app_start_timer($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(input.request_id)
        .bind(input.kind.as_str())
        .bind(&memo)
        .bind(target_kind)
        .bind(target_id)
        .bind(new_ids(1))
        .bind(input.mode.map(TimerStartMode::as_str))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
        let entry = load(&mut tx, id).await?;
        tx.commit().await?;
        Ok(StartedTimer {
            entry,
            stopped_entry_id,
        })
    };
    match run().await {
        Ok(entry) => Ok((StatusCode::CREATED, Json(entry))),
        // 같은 요청의 동시 재시도이거나 다른 타이머가 막 시작됐다. 다시 보면 둘 중 하나로 정해진다.
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 타이머 동작.
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerAction {
    Pause,
    Resume,
    Stop,
}

/// 타이머 일시정지·재개·종료 요청.
#[derive(Debug, Deserialize, ToSchema)]
pub struct TimerActionInput {
    pub action: TimerAction,
}

/// 타이머 일시정지·재개·종료(본인). 이미 그 상태면 그대로.
#[utoipa::path(post, path = "/api/time-entries/{entry_id}/timer", operation_id = "timer_action",
    params(("entry_id" = String, Path, format = Uuid), ("x-fvoci-expected-user" = Option<String>, Header, description = "If sent, must match the signed-in user (D-96).")), request_body = TimerActionInput,
    responses((status = 200, body = TimeEntry), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)),
    tag = "time")]
pub async fn action(
    State(state): State<TimeState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<TimerActionInput>,
) -> Result<Json<TimeEntry>, AppError> {
    let action = match input.action {
        TimerAction::Pause => "pause",
        TimerAction::Resume => "resume",
        TimerAction::Stop => "stop",
    };
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_timer_action($1, $2)")
        .bind(id)
        .bind(action)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let entry = load(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(entry))
}
