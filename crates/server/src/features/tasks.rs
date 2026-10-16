//! 업무·하위 업무·담당자·업무 체크리스트 (D-14, D-30, D-31, D-33, D-48, D-50, D-57).
//!
//! 업무는 모듈 항목 아래에 있고 모듈 번호열을 함께 쓴다. 권한은 항목을 따르며 DB 함수 `app_task_access`
//! 하나로 판정한다(보관 아래 읽기 전용). 필드 고치기는 `base_version`으로 충돌을 막고, 칸반 이동과 체크리스트
//! 체크는 버전 없이 따로 한다(같은 보드를 여럿이 써도 충돌하지 않게). 날짜는 시각이 없는 날짜로 저장한다(D-50).
//! 처리는 `migrations/0011_tasks.sql`의 정의자 함수가 한다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use time::format_description::well_known::Iso8601;
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::{UserTx, is_unique_violation};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::documents::Access;
use crate::features::workspaces::validate_title;
use crate::http::AppState;
use crate::ids::{ItemId, ModuleId, UserId, WorkspaceId};

/// 설명 글자 수 상한.
const MAX_DESCRIPTION_CHARS: usize = 10_000;
/// 태그 수·길이 상한.
const MAX_TAGS: usize = 20;
const MAX_TAG_CHARS: usize = 50;
/// 체크리스트 한 번에 덧붙일 수 있는 항목 수.
const MAX_CHECKLIST_BATCH: usize = 200;

/// 업무 상태. 완료 시각은 서버가 기록한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
}

impl TaskStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::InProgress => "in_progress",
            Self::Done => "done",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "todo" => Some(Self::Todo),
            "in_progress" => Some(Self::InProgress),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

/// 우선순위.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    None,
    Low,
    Medium,
    High,
    Urgent,
}

impl TaskPriority {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "none" => Some(Self::None),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "urgent" => Some(Self::Urgent),
            _ => None,
        }
    }
}

/// 담당자.
#[derive(Debug, Serialize, ToSchema)]
pub struct Assignee {
    pub user_id: UserId,
    pub display_name: String,
}

/// 체크리스트 항목.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChecklistItem {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub text: String,
    pub checked: bool,
}

/// 업무.
#[derive(Debug, Serialize, ToSchema)]
pub struct Task {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub module_id: ModuleId,
    pub workspace_id: WorkspaceId,
    /// 하위 업무면 상위 업무.
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
    /// 예: `DEV-5`.
    pub reference: Option<String>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    /// `YYYY-MM-DD`. 시각 없는 날짜.
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate_minutes: Option<i32>,
    pub tags: Vec<String>,
    /// 마일스톤이면 마감일 하나만 쓴다.
    pub milestone: bool,
    /// 같은 항목 안의 선행 업무(끝나야 시작).
    #[schema(value_type = Vec<String>)]
    pub predecessor_ids: Vec<Uuid>,
    pub assignees: Vec<Assignee>,
    pub checklist_done: i64,
    pub checklist_total: i64,
    /// 필드 고치기의 기준 버전.
    pub version: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub completed_at: Option<OffsetDateTime>,
    pub access: Access,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// 업무와 체크리스트.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    pub checklist: Vec<ChecklistItem>,
}

/// 업무 만들기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTask {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub title: String,
    /// 하위 업무로 만들 때 상위 업무(같은 항목, 한 단계).
    #[serde(default)]
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
}

/// 업무 고치기. `base_version`이 지났으면 409와 서버 최신본.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTask {
    pub base_version: i32,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate_minutes: Option<i32>,
    pub tags: Vec<String>,
    /// 담당자 전체 목록(빠지면 지우는 것이므로 생략할 수 없다).
    pub assignee_ids: Vec<UserId>,
    /// 마일스톤이면 시작일 없이 마감일만.
    pub milestone: bool,
    /// 미리보기에서 사용자가 승인한 후속 업무 재배치. 같은 트랜잭션에서 함께 옮기며, 하나라도 기준 버전이
    /// 지났으면 아무것도 바꾸지 않고 409(`shift_conflict`).
    #[serde(default)]
    pub successor_shifts: Vec<SuccessorShift>,
}

/// 승인한 후속 업무 재배치 한 건. 날짜가 있던 칸만 옮긴다.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SuccessorShift {
    #[schema(value_type = String, format = Uuid)]
    pub task_id: Uuid,
    pub base_version: i32,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 한 번에 옮길 수 있는 후속 업무 수.
const MAX_SHIFTS: usize = 500;

/// 칸반 이동: 열(상태)과 그 안에서 바로 뒤에 올 업무(없으면 맨 끝).
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveTask {
    pub status: TaskStatus,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub before_id: Option<Uuid>,
}

/// 상위 업무 바꾸기. 없으면 최상위.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetTaskParent {
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
}

/// 덧붙일 체크리스트 항목.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewChecklistItem {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub text: String,
    #[serde(default)]
    pub checked: bool,
}

/// 체크리스트 덧붙이기(Markdown에서 펼친 항목을 사용자가 확인한 뒤 한 번에).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddChecklist {
    pub items: Vec<NewChecklistItem>,
}

/// 체크리스트 항목 고치기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateChecklistItem {
    pub text: String,
    pub checked: bool,
}

/// 409 응답: 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskConflict {
    pub error: &'static str,
    pub current: TaskDetail,
}

pub(crate) fn parse_date(raw: Option<&str>, field: &str) -> Result<Option<Date>, AppError> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        // 시각이 붙은 값은 받지 않는다(날짜만 있는 일정은 날짜 그대로, D-50).
        Some(s) if s.len() != 10 => Err(AppError::BadRequest(format!(
            "{field} must be a date like 2026-10-17"
        ))),
        Some(s) => Date::parse(s, &Iso8601::DATE)
            .map(Some)
            .map_err(|_| AppError::BadRequest(format!("{field} must be a date like 2026-10-17"))),
    }
}

pub(crate) fn format_date(date: Option<Date>) -> Option<String> {
    date.map(|d| format!("{:04}-{:02}-{:02}", d.year(), u8::from(d.month()), d.day()))
}

pub(crate) fn validate_text(raw: &str, max: usize, field: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.chars().count() > max {
        return Err(AppError::BadRequest(format!(
            "{field} must be at most {max} characters"
        )));
    }
    if trimmed
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(AppError::BadRequest(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(trimmed.to_owned())
}

fn validate_tags(tags: &[String]) -> Result<Vec<String>, AppError> {
    if tags.len() > MAX_TAGS {
        return Err(AppError::BadRequest(format!("at most {MAX_TAGS} tags")));
    }
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = validate_title(tag, MAX_TAG_CHARS, "tag")?;
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 읽기
// ---------------------------------------------------------------------------

#[derive(Debug, sqlx::FromRow)]
struct TaskRow {
    id: Uuid,
    item_id: ItemId,
    module_id: ModuleId,
    workspace_id: WorkspaceId,
    parent_id: Option<Uuid>,
    reference: Option<String>,
    title: String,
    description: String,
    status: String,
    priority: String,
    start_date: Option<Date>,
    due_date: Option<Date>,
    estimate_minutes: Option<i32>,
    tags: Vec<String>,
    milestone: bool,
    predecessor_ids: Vec<Uuid>,
    checklist_done: i64,
    checklist_total: i64,
    version: i32,
    completed_at: Option<OffsetDateTime>,
    access: String,
    updated_at: OffsetDateTime,
}

const TASK_SELECT: &str = "SELECT t.id, t.item_id, i.module_id, t.workspace_id, t.parent_id,
        app_task_reference(t.id) AS reference, t.title, t.description, t.status, t.priority,
        t.start_date, t.due_date, t.estimate_minutes, t.tags, t.milestone,
        ARRAY(SELECT d.predecessor_id FROM task_dependencies d WHERE d.successor_id = t.id
              ORDER BY d.created_at, d.predecessor_id) AS predecessor_ids,
        c.done AS checklist_done, c.total AS checklist_total,
        t.version, t.completed_at, app_task_access(t.id) AS access, t.updated_at
 FROM tasks t JOIN items i ON i.id = t.item_id
 CROSS JOIN LATERAL app_task_checklist_counts(t.id) c";

/// 업무 순서: 칸반 열(상태) 안의 위치·번호 순. 목록 보기의 상위·하위 묶음은 화면에서 만든다.
const TASK_ORDER: &str =
    "array_position(ARRAY['todo', 'in_progress', 'done'], t.status), t.position, t.number";

async fn assignees_of(tx: &mut UserTx, ids: &[Uuid]) -> Result<Vec<(Uuid, Assignee)>, AppError> {
    // 담당자 이름은 같은 항목을 볼 수 있는 사람이므로 워크스페이스를 함께 쓰는 사람이다(users RLS).
    let rows: Vec<(Uuid, UserId, Option<String>)> = sqlx::query_as(
        "SELECT a.task_id, a.user_id, u.display_name
         FROM task_assignees a LEFT JOIN users u ON u.id = a.user_id
         WHERE a.task_id = ANY($1) ORDER BY u.display_name, a.user_id",
    )
    .bind(ids)
    .fetch_all(tx.conn())
    .await?;
    Ok(rows
        .into_iter()
        .map(|(task, user_id, name)| {
            (
                task,
                Assignee {
                    user_id,
                    display_name: name.unwrap_or_default(),
                },
            )
        })
        .collect())
}

async fn to_tasks(tx: &mut UserTx, rows: Vec<TaskRow>) -> Result<Vec<Task>, AppError> {
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut assignees = assignees_of(tx, &ids).await?;
    rows.into_iter()
        .map(|row| {
            let TaskRow {
                id,
                item_id,
                module_id,
                workspace_id,
                parent_id,
                reference,
                title,
                description,
                status,
                priority,
                start_date,
                due_date,
                estimate_minutes,
                tags,
                milestone,
                predecessor_ids,
                checklist_done,
                checklist_total,
                version,
                completed_at,
                access,
                updated_at,
            } = row;
            let mine: Vec<Assignee> = assignees
                .extract_if(.., |(task, _)| *task == id)
                .map(|(_, a)| a)
                .collect();
            Ok(Task {
                id,
                item_id,
                module_id,
                workspace_id,
                parent_id,
                reference,
                title,
                description,
                status: TaskStatus::parse(&status)
                    .ok_or_else(|| AppError::Config(format!("unknown task status {status}")))?,
                priority: TaskPriority::parse(&priority)
                    .ok_or_else(|| AppError::Config(format!("unknown priority {priority}")))?,
                start_date: format_date(start_date),
                due_date: format_date(due_date),
                estimate_minutes,
                tags,
                milestone,
                predecessor_ids,
                assignees: mine,
                checklist_done,
                checklist_total,
                version,
                completed_at,
                access: Access::parse(&access)
                    .ok_or_else(|| AppError::Config(format!("unknown access {access}")))?,
                updated_at,
            })
        })
        .collect()
}

pub(crate) async fn load_detail(tx: &mut UserTx, id: Uuid) -> Result<TaskDetail, AppError> {
    let row: Option<TaskRow> = sqlx::query_as(&format!("{TASK_SELECT} WHERE t.id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    let row = row.ok_or(AppError::NotFound)?;
    let task = to_tasks(tx, vec![row])
        .await?
        .pop()
        .ok_or(AppError::NotFound)?;
    let checklist: Vec<(Uuid, String, bool)> = sqlx::query_as(
        "SELECT id, text, checked FROM checklist_items WHERE task_id = $1 ORDER BY position, created_at",
    )
    .bind(id)
    .fetch_all(tx.conn())
    .await?;
    Ok(TaskDetail {
        task,
        checklist: checklist
            .into_iter()
            .map(|(id, text, checked)| ChecklistItem { id, text, checked })
            .collect(),
    })
}

/// 항목의 업무 목록(칸반 열 순서, 열 안은 위치 순). 항목을 볼 수 없으면 404.
#[utoipa::path(get, path = "/api/items/{item_id}/tasks", operation_id = "list_tasks",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Task]), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn list(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Task>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_item_access($1) IS NOT NULL")
        .bind(item)
        .fetch_one(tx.conn())
        .await?;
    if !visible {
        return Err(AppError::NotFound);
    }
    let rows: Vec<TaskRow> = sqlx::query_as(&format!(
        "{TASK_SELECT} WHERE t.item_id = $1 ORDER BY {TASK_ORDER}"
    ))
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    let tasks = to_tasks(&mut tx, rows).await?;
    tx.commit().await?;
    Ok(Json(tasks))
}

/// 담당자로 지정할 수 있는 사람(그 항목을 볼 수 있는 멤버).
#[utoipa::path(get, path = "/api/items/{item_id}/assignable",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 200, body = [Assignee]), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn assignable(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Assignee>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(UserId, String)> =
        sqlx::query_as("SELECT user_id, display_name FROM app_item_assignable($1)")
            .bind(item)
            .fetch_all(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(user_id, display_name)| Assignee {
                user_id,
                display_name,
            })
            .collect(),
    ))
}

/// 업무 하나와 체크리스트.
#[utoipa::path(get, path = "/api/tasks/{task_id}", operation_id = "get_task",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = TaskDetail), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

// ---------------------------------------------------------------------------
// 쓰기
// ---------------------------------------------------------------------------

/// 업무 만들기(항목 쓰기 권한). 같은 `request_id`는 같은 업무를 돌려준다.
#[utoipa::path(post, path = "/api/items/{item_id}/tasks", operation_id = "create_task",
    params(("item_id" = String, Path, format = Uuid)), request_body = CreateTask,
    responses((status = 201, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "tasks")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
    Json(input): Json<CreateTask>,
) -> Result<(StatusCode, Json<TaskDetail>), AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let run = || async {
        let mut tx = state.db.begin_as(user).await?;
        let id: Uuid = sqlx::query_scalar("SELECT app_create_task($1, $2, $3, $4)")
            .bind(item)
            .bind(input.request_id)
            .bind(&title)
            .bind(input.parent_id)
            .fetch_one(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
        let detail = load_detail(&mut tx, id).await?;
        tx.commit().await?;
        Ok(detail)
    };
    match run().await {
        Ok(detail) => Ok((StatusCode::CREATED, Json(detail))),
        Err(AppError::Database(e)) if is_unique_violation(&e) => {
            Ok((StatusCode::CREATED, Json(run().await?)))
        }
        Err(e) => Err(e),
    }
}

/// 업무 고치기(쓰기 권한). 기준 버전이 지났으면 409와 서버 최신본(입력은 클라이언트가 보존).
#[utoipa::path(put, path = "/api/tasks/{task_id}", operation_id = "update_task",
    params(("task_id" = String, Path, format = Uuid)), request_body = UpdateTask,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = TaskConflict)), tag = "tasks")]
pub async fn update(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateTask>,
) -> Result<Response, AppError> {
    let title = validate_title(&input.title, state.config.limits.max_title_chars, "title")?;
    let description = validate_text(&input.description, MAX_DESCRIPTION_CHARS, "description")?;
    let start = parse_date(input.start_date.as_deref(), "start_date")?;
    let due = parse_date(input.due_date.as_deref(), "due_date")?;
    if input.estimate_minutes.is_some_and(|m| m < 0) {
        return Err(AppError::BadRequest(
            "estimate_minutes must not be negative".into(),
        ));
    }
    let tags = validate_tags(&input.tags)?;
    if input.successor_shifts.len() > MAX_SHIFTS {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_SHIFTS} successor shifts"
        )));
    }
    let mut shifts = Vec::with_capacity(input.successor_shifts.len());
    for shift in &input.successor_shifts {
        // 날짜 형식은 업무 날짜와 같은 규칙으로 확인한 뒤 DB에 넘긴다.
        shifts.push(serde_json::json!({
            "task_id": shift.task_id,
            "base_version": shift.base_version,
            "start_date": format_date(parse_date(shift.start_date.as_deref(), "start_date")?),
            "due_date": format_date(parse_date(shift.due_date.as_deref(), "due_date")?),
        }));
    }
    let mut tx = state.db.begin_as(user).await?;
    let outcome: String = sqlx::query_scalar(
        "SELECT app_update_task($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(id)
    .bind(input.base_version)
    .bind(&title)
    .bind(&description)
    .bind(input.status.as_str())
    .bind(input.priority.as_str())
    .bind(start)
    .bind(due)
    .bind(input.estimate_minutes)
    .bind(&tags)
    .bind(&input.assignee_ids)
    .bind(input.milestone)
    .bind(serde_json::Value::Array(shifts))
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    let error = match outcome.as_str() {
        "conflict" => "conflict",
        // 승인할 때 본 후속 업무가 그사이 바뀌었다. 아무것도 바꾸지 않았으니 다시 미리 본다.
        "shift_conflict" => "shift_conflict",
        _ => return Ok(Json(detail).into_response()),
    };
    Ok((
        StatusCode::CONFLICT,
        Json(TaskConflict {
            error,
            current: detail,
        }),
    )
        .into_response())
}

/// 칸반 이동(쓰기 권한). 버전과 상관없이 바로 반영한다.
#[utoipa::path(put, path = "/api/tasks/{task_id}/position",
    params(("task_id" = String, Path, format = Uuid)), request_body = MoveTask,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn move_task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<MoveTask>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_move_task($1, $2, $3)")
        .bind(id)
        .bind(input.status.as_str())
        .bind(input.before_id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 상위 업무 바꾸기(같은 항목, 한 단계).
#[utoipa::path(put, path = "/api/tasks/{task_id}/parent", operation_id = "set_task_parent",
    params(("task_id" = String, Path, format = Uuid)), request_body = SetTaskParent,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn set_parent(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<SetTaskParent>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_task_parent($1, $2)")
        .bind(id)
        .bind(input.parent_id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 체크리스트 덧붙이기(쓰기 권한). 항목별 `request_id`로 재시도해도 중복되지 않는다.
#[utoipa::path(post, path = "/api/tasks/{task_id}/checklist",
    params(("task_id" = String, Path, format = Uuid)), request_body = AddChecklist,
    responses((status = 200, body = TaskDetail), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn add_checklist(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<AddChecklist>,
) -> Result<Json<TaskDetail>, AppError> {
    if input.items.len() > MAX_CHECKLIST_BATCH {
        return Err(AppError::BadRequest(format!(
            "at most {MAX_CHECKLIST_BATCH} items at once"
        )));
    }
    let mut items = Vec::with_capacity(input.items.len());
    for item in input.items {
        let text = validate_title(&item.text, state.config.limits.max_title_chars, "text")?;
        items.push(NewChecklistItem { text, ..item });
    }
    let payload = serde_json::to_value(&items)
        .map_err(|e| AppError::BadRequest(format!("invalid items: {e}")))?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_add_checklist_items($1, $2)")
        .bind(id)
        .bind(payload)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}

/// 체크리스트 항목 고치기(쓰기 권한). 버전 없이 바로 반영한다.
#[utoipa::path(put, path = "/api/checklist-items/{item_id}",
    params(("item_id" = String, Path, format = Uuid)), request_body = UpdateChecklistItem,
    responses((status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn update_checklist_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateChecklistItem>,
) -> Result<StatusCode, AppError> {
    let text = validate_title(&input.text, state.config.limits.max_title_chars, "text")?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_update_checklist_item($1, $2, $3)")
        .bind(id)
        .bind(&text)
        .bind(input.checked)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 체크리스트 항목 지우기(쓰기 권한).
#[utoipa::path(delete, path = "/api/checklist-items/{item_id}",
    params(("item_id" = String, Path, format = Uuid)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn delete_checklist_item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_delete_checklist_item($1)")
        .bind(id)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_plain_calendar_dates() {
        let d = parse_date(Some("2026-10-17"), "due_date").unwrap();
        assert_eq!(format_date(d).as_deref(), Some("2026-10-17"));
        assert_eq!(parse_date(Some(" "), "due_date").unwrap(), None);
        assert!(parse_date(Some("2026-13-01"), "due_date").is_err());
        assert!(parse_date(Some("2026-10-17T09:00:00Z"), "due_date").is_err());
    }

    #[test]
    fn tags_are_trimmed_deduplicated_and_limited() {
        let tags = validate_tags(&[" 설계 ".into(), "설계".into(), "검토".into()]).unwrap();
        assert_eq!(tags, vec!["설계", "검토"]);
        let many: Vec<String> = (0..=MAX_TAGS).map(|i| i.to_string()).collect();
        assert!(validate_tags(&many).is_err());
    }

    #[test]
    fn status_and_priority_round_trip() {
        for s in [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] {
            assert_eq!(TaskStatus::parse(s.as_str()), Some(s));
        }
        for p in [TaskPriority::None, TaskPriority::Urgent] {
            assert_eq!(TaskPriority::parse(p.as_str()), Some(p));
        }
    }
}
