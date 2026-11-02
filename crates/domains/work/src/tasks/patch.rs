//! 업무 고치기 명령 `PATCH /api/tasks/{task_id}`(노르웨이숲 태비 초안 2): 고친 칸만 싣고 기준 판(`base_version`)을
//! 붙인다. 상세의 "저장" 한 번이 명령 하나이고, 날짜를 바꿔 뒤 업무가 밀리면 승인한 이동 목록을 함께 싣는다(D-94,
//! D-91·D-125). 보내지 않은 칸은 지금 값 그대로 저장한다. 판이 지났으면 아무것도 바꾸지 않고 공통 409 본문
//! (`StaleWrite`, #180 서버 부분)으로 지금 업무와 바뀐 칸을 돌려준다.

use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;

use super::edit::{Edit, Saved, save_edit};
use super::model::{
    EXPECTED_TASK_USER_DOC, MAX_DESCRIPTION_CHARS, SuccessorShift, TaskPriority, check_estimate,
    validate_status, validate_tags,
};
use super::resource::{Task, TaskConflictCurrent, load_conflict, load_task};
use crate::state::WorkState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::UserId;
use fvoci_core::validate::{format_date, parse_date, validate_text, validate_title};
use fvoci_http::conflict::StaleWrite;
use fvoci_http::session::ExpectedUser;

/// 업무 고치기. 보낸 칸만 바꾼다(빠진 칸은 그대로). 날짜·예상 시간은 `null`을 보내면 지운다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PatchTask {
    /// 편집을 시작할 때 읽은 업무의 판(`Task.version`).
    pub base_version: i32,
    /// 제목. 글자 수 상한은 서버 설정이다(`GET /api/me`의 `limits.max_title_chars`). 넘으면 400 `reason: too_long`과 `limit`.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub description: Option<String>,
    /// 상태 이름(항목이 정의한 것).
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<TaskPriority>,
    /// `YYYY-MM-DD`, `null`이면 지운다.
    #[serde(default, deserialize_with = "sent")]
    #[schema(value_type = Option<String>, nullable)]
    pub start_date: Option<Option<String>>,
    /// `YYYY-MM-DD`, `null`이면 지운다.
    #[serde(default, deserialize_with = "sent")]
    #[schema(value_type = Option<String>, nullable)]
    pub due_date: Option<Option<String>>,
    /// 분, `null`이면 지운다.
    #[serde(default, deserialize_with = "sent")]
    #[schema(value_type = Option<i32>, nullable, minimum = 0)]
    pub estimate_minutes: Option<Option<i32>>,
    #[serde(default)]
    #[schema(max_items = 20)]
    pub tags: Option<Vec<String>>,
    /// 담당자 전체 목록(그 항목을 볼 수 있는 멤버).
    #[serde(default)]
    pub assignee_ids: Option<Vec<UserId>>,
    /// 마일스톤이면 시작일 없이 마감일만.
    #[serde(default)]
    pub milestone: Option<bool>,
    /// 미리보기에서 승인한 후속 업무 재배치. 같은 트랜잭션에서 함께 옮기며, 그사이 맞지 않게 됐으면 아무것도 바꾸지 않고
    /// 409 `shift_conflict`.
    #[serde(default)]
    #[schema(max_items = 500)]
    pub successor_shifts: Vec<SuccessorShift>,
}

/// 칸이 왔는지(값이 `null`이어도)를 남긴다: 없음 = 그대로, `Some(None)` = 지우기.
fn sent<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `PATCH`의 409 본문(계약 문서용). 판 충돌이면 `StaleWrite<Task>`, 기대 사용자 불일치·보관·동시 변경이면 `ErrorBody`.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum TaskPatchConflict {
    Stale(Box<StaleWrite<Task>>),
    State(ErrorBody),
}

/// PATCH 전용 409. 이동 충돌은 기존 `TaskPatchConflict`와 일반 Task를 유지한다.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum TaskFieldConflict {
    Stale(Box<StaleWrite<TaskConflictCurrent>>),
    State(ErrorBody),
}

/// 보낸 칸을 확인하고 지금 업무 위에 얹는다. 보내지 않은 칸은 다시 확인하지 않는다(그사이 설정 상한이 바뀌어도
/// 고치지 않은 제목이 거절되지 않게). 돌려주는 JSON은 보낸 칸의 정리한 값(공통 409의 `changed` 계산용)이다.
fn merge(
    state: &WorkState,
    input: &PatchTask,
    current: &Task,
    assignees: Vec<UserId>,
) -> Result<(Edit, Value), AppError> {
    let mut sent = Map::new();
    let mut note = |name: &str, value: Value| {
        sent.insert(name.to_owned(), value);
    };
    let title = match &input.title {
        Some(raw) => validate_title(raw, state.max_title_chars, "title")?,
        None => current.title.clone(),
    };
    let description = match &input.description {
        Some(raw) => validate_text(raw, MAX_DESCRIPTION_CHARS, "description")?,
        None => current.description.clone(),
    };
    let status = match &input.status {
        Some(raw) => validate_status(raw)?,
        None => current.status.clone(),
    };
    let start = date_of(
        input.start_date.as_ref(),
        current.start_date.as_deref(),
        "start_date",
    )?;
    let due = date_of(
        input.due_date.as_ref(),
        current.due_date.as_deref(),
        "due_date",
    )?;
    let estimate_minutes = match input.estimate_minutes {
        Some(value) => check_estimate(value)?,
        None => current.estimate_minutes,
    };
    let tags = match &input.tags {
        Some(raw) => validate_tags(raw)?,
        None => current.tags.clone(),
    };
    if input.title.is_some() {
        note("title", Value::from(title.as_str()));
    }
    if input.description.is_some() {
        note("description", Value::from(description.as_str()));
    }
    if input.status.is_some() {
        note("status", Value::from(status.as_str()));
    }
    if let Some(priority) = input.priority {
        note("priority", Value::from(priority.as_str()));
    }
    if input.start_date.is_some() {
        note("start_date", format_date(start).into());
    }
    if input.due_date.is_some() {
        note("due_date", format_date(due).into());
    }
    if input.estimate_minutes.is_some() {
        note("estimate_minutes", estimate_minutes.into());
    }
    if input.tags.is_some() {
        note("tags", tags.clone().into());
    }
    if let Some(milestone) = input.milestone {
        note("milestone", milestone.into());
    }
    if let Some(ids) = &input.assignee_ids {
        let mut ids = ids.clone();
        ids.sort_by_key(|id| id.0);
        ids.dedup();
        note(
            "assignee_ids",
            Value::Array(
                ids.into_iter()
                    .map(|id| Value::from(id.0.to_string()))
                    .collect(),
            ),
        );
    }
    let edit = Edit {
        title,
        description,
        status,
        priority: input.priority.unwrap_or(current.priority),
        start,
        due,
        estimate_minutes,
        tags,
        assignee_ids: input.assignee_ids.clone().unwrap_or(assignees),
        milestone: input.milestone.unwrap_or(current.milestone),
    };
    Ok((edit, Value::Object(sent)))
}

/// 보낸 날짜(지우기 포함), 아니면 지금 날짜.
fn date_of(
    input: Option<&Option<String>>,
    current: Option<&str>,
    field: &str,
) -> Result<Option<Date>, AppError> {
    match input {
        Some(value) => parse_date(value.as_deref(), field),
        None => parse_date(current, field),
    }
}

/// 업무 고치기(쓰기 권한). 보낸 칸만 바꾸고, 기준 판이 지났으면 409 `conflict`, 승인한 재배치가 맞지 않게 됐으면 409
/// `shift_conflict`다(둘 다 아무것도 바꾸지 않고 지금 업무와 바뀐 칸을 준다).
#[utoipa::path(patch, path = "/api/tasks/{task_id}", operation_id = "patch_task",
    params(("task_id" = String, Path, format = Uuid),
           ("x-fvoci-expected-user" = String, Header, format = Uuid, description = EXPECTED_TASK_USER_DOC)), request_body = PatchTask,
    responses((status = 200, body = Task), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody),
              (status = 409, body = TaskFieldConflict, description = "conflict·shift_conflict면 StaleWrite(같은 스냅샷의 업무·판·담당 ID와 바뀐 칸); account_changed·invalid_state면 ErrorBody. 아무것도 바꾸지 않음")),
    tag = "tasks")]
pub async fn patch(
    State(state): State<WorkState>,
    ExpectedUser(user): ExpectedUser,
    Path(id): Path<Uuid>,
    Json(input): Json<PatchTask>,
) -> Result<Response, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let current = load_task(&mut tx, id).await?;
    let assignees: Vec<UserId> = if input.assignee_ids.is_some() {
        Vec::new()
    } else {
        sqlx::query_scalar("SELECT user_id FROM task_assignees WHERE task_id = $1 ORDER BY user_id")
            .bind(id)
            .fetch_all(tx.conn())
            .await?
    };
    tx.commit().await?;
    // 읽은 뒤 다른 사람이 바꿨으면 저장 함수가 기준 판으로 막는다(합친 값은 기준 판일 때의 값이어야만 쓰인다).
    let (edit, sent) = merge(&state, &input, &current, assignees)?;
    let saved = save_edit(
        &state,
        user,
        id,
        input.base_version,
        &edit,
        &input.successor_shifts,
    )
    .await?;
    let mut tx = state.db.begin_as(user).await?;
    let (code, message) = match saved {
        Saved::Updated => {
            let latest = load_task(&mut tx, id).await?;
            tx.commit().await?;
            return Ok(Json(latest).into_response());
        }
        Saved::ShiftConflict => (
            "shift_conflict",
            "the approved successor shifts no longer match; nothing was saved",
        ),
        Saved::Conflict => (
            "conflict",
            "the task changed after this edit started; nothing was saved",
        ),
    };
    let latest = load_conflict(&mut tx, id).await?;
    tx.commit().await?;
    Ok(StaleWrite::new(code, message, latest, &sent).into_response())
}
