//! 업무 읽기: 항목의 업무 목록, 업무 하나, 담당 후보.

use axum::Json;
use axum::extract::{Path, State};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::model::{Assignee, ChecklistItem, Task, TaskDetail, TaskPriority, TaskSeries};
use crate::recurrence::RecurrenceRule;
use crate::schedule::{LinkKind, TaskLink};
use crate::state::WorkState;
use fvoci_core::access::Grade as Access;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{ItemId, ModuleId, UserId, WorkspaceId};
use fvoci_core::validate::format_date;
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;

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
    status_category: String,
    priority: String,
    start_date: Option<Date>,
    due_date: Option<Date>,
    estimate_minutes: Option<i32>,
    tags: Vec<String>,
    milestone: bool,
    predecessor_ids: Vec<Uuid>,
    predecessor_kinds: Vec<String>,
    predecessor_lags: Vec<i32>,
    series_id: Option<Uuid>,
    series_root: Option<Uuid>,
    series_rule: Option<serde_json::Value>,
    series_timezone: Option<String>,
    occurrence_date: Option<Date>,
    checklist_done: i64,
    checklist_total: i64,
    version: i32,
    move_version: i32,
    completed_at: Option<OffsetDateTime>,
    access: String,
    updated_at: OffsetDateTime,
}

const TASK_SELECT: &str = "SELECT t.id, t.item_id, i.module_id, t.workspace_id, t.parent_id,
        app_task_reference(t.id) AS reference, t.title, t.description, s.name AS status, s.category AS status_category, t.priority,
        t.start_date, t.due_date, t.estimate_minutes, t.tags, t.milestone,
        ARRAY(SELECT d.predecessor_id FROM task_dependencies d WHERE d.successor_id = t.id
              ORDER BY d.created_at, d.predecessor_id) AS predecessor_ids,
        ARRAY(SELECT d.kind FROM task_dependencies d WHERE d.successor_id = t.id
              ORDER BY d.created_at, d.predecessor_id) AS predecessor_kinds,
        ARRAY(SELECT d.lag_days FROM task_dependencies d WHERE d.successor_id = t.id
              ORDER BY d.created_at, d.predecessor_id) AS predecessor_lags,
        series.id AS series_id, series.root_task_id AS series_root, series.rule AS series_rule, series.timezone AS series_timezone,
        t.occurrence_date,
        c.done AS checklist_done, c.total AS checklist_total,
        t.version, t.move_version, t.completed_at, app_access('task', t.id) AS access, t.updated_at
 FROM tasks t JOIN items i ON i.id = t.item_id
 JOIN task_statuses s ON s.id = t.status_id
 LEFT JOIN task_series series ON series.id = t.series_id
 CROSS JOIN LATERAL app_task_checklist_counts(t.id) c";

/// 업무 순서: 칸반 열(상태) 안의 위치·번호 순. 목록 보기의 상위·하위 묶음은 화면에서 만든다.
const TASK_ORDER: &str = "s.position, t.position, t.number";

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
                status_category,
                priority,
                start_date,
                due_date,
                estimate_minutes,
                tags,
                milestone,
                predecessor_ids,
                predecessor_kinds,
                predecessor_lags,
                series_id,
                series_root,
                series_rule,
                series_timezone,
                occurrence_date,
                checklist_done,
                checklist_total,
                version,
                move_version,
                completed_at,
                access,
                updated_at,
            } = row;
            let mine: Vec<Assignee> = assignees
                .extract_if(.., |(task, _)| *task == id)
                .map(|(_, a)| a)
                .collect();
            let predecessors = predecessor_ids
                .iter()
                .zip(&predecessor_kinds)
                .zip(&predecessor_lags)
                .map(|((&task_id, kind), &lag_days)| {
                    Ok(TaskLink {
                        task_id,
                        kind: LinkKind::parse(kind).ok_or_else(|| {
                            AppError::Config(format!("unknown dependency kind {kind}"))
                        })?,
                        lag_days,
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?;
            Ok(Task {
                id,
                item_id,
                module_id,
                workspace_id,
                parent_id,
                reference,
                title,
                description,
                status,
                status_category,
                priority: TaskPriority::parse(&priority)
                    .ok_or_else(|| AppError::Config(format!("unknown priority {priority}")))?,
                start_date: format_date(start_date),
                due_date: format_date(due_date),
                estimate_minutes,
                tags,
                milestone,
                predecessors,
                series: match (series_id, series_root, series_timezone) {
                    (Some(id), Some(root_task_id), Some(timezone)) => Some(TaskSeries {
                        id,
                        root_task_id,
                        rule: series_rule.map(RecurrenceRule::from_db).transpose()?,
                        timezone,
                    }),
                    _ => None,
                },
                occurrence_date: format_date(occurrence_date),
                assignees: mine,
                checklist_done,
                checklist_total,
                version,
                move_version,
                completed_at,
                access: Access::parse_present(&access)?,
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
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(item): Path<ItemId>,
) -> Result<Json<Vec<Task>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = sqlx::query_scalar("SELECT app_access('item', $1) <> 'none'")
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
    State(state): State<WorkState>,
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
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskDetail>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let detail = load_detail(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}
