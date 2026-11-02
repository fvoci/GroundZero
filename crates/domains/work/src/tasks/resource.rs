//! 업무 자원(노르웨이숲 태비 초안 1·2): 업무 하나의 칸만 싣고 다른 자원은 id로만 가리킨다. 담당·선행 관계·반복 계열·
//! 체크리스트는 관계 모음(`relations`)으로 따로 읽고, 등급은 항목에서 한 번 읽는다(업무 등급은 항목 아래 등급, D-113).

use serde::Serialize;
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use super::model::TaskPriority;
use crate::task_statuses::StatusCategory;
use fvoci_core::error::AppError;
use fvoci_core::ids::{ItemId, ModuleId, UserId, WorkspaceId};
use fvoci_core::validate::format_date;
use fvoci_db::UserTx;

/// 업무.
#[derive(Debug, Clone, Serialize, ToSchema)]
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
    /// 상태 이름. 명령(고치기·이동)이 상태를 항목 안에서 겹치지 않는 이름으로 가리키므로(D-116) 이름으로 싣는다.
    pub status: String,
    /// 상태의 범주(완료 판정). 상태 정의(`GET /api/items/{item_id}/statuses`)와 같은 값이다.
    pub status_category: StatusCategory,
    pub priority: TaskPriority,
    /// `YYYY-MM-DD`. 시각 없는 날짜(D-50).
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate_minutes: Option<i32>,
    pub tags: Vec<String>,
    /// 마일스톤이면 마감일 하나만 쓴다.
    pub milestone: bool,
    /// 반복 업무의 회차면 그 반복 계열(`GET /api/task-series`).
    #[schema(value_type = Option<String>, format = Uuid)]
    pub series_id: Option<Uuid>,
    /// 반복 회차의 날짜(규칙 기준).
    pub occurrence_date: Option<String>,
    /// 칸 고치기의 기준 판(D-94).
    pub version: i32,
    /// 이동 판(D-89): 열이나 열 안 위치가 바뀔 때마다 오른다. 조건부 이동(`base_move_version`)의 기준.
    pub move_version: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub completed_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// PATCH 판 충돌의 비교용 현재 값. 일반 업무 자원·목록에는 담당을 추가하지 않는다(#180).
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskConflictCurrent {
    #[serde(flatten)]
    pub task: Task,
    /// 같은 statement snapshot에서 읽은 담당자 전체 집합(id 순).
    pub assignee_ids: Vec<UserId>,
}

#[derive(Debug, sqlx::FromRow)]
struct ConflictRecord {
    #[sqlx(flatten)]
    task: TaskRecord,
    assignee_ids: Vec<UserId>,
}

#[derive(Debug, sqlx::FromRow)]
struct TaskRecord {
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
    series_id: Option<Uuid>,
    occurrence_date: Option<Date>,
    version: i32,
    move_version: i32,
    completed_at: Option<OffsetDateTime>,
    updated_at: OffsetDateTime,
}

/// 업무 자원 읽기. 사용자 권한(RLS)으로 읽으므로 볼 수 없는 업무는 나오지 않는다.
const TASK_SELECT: &str = "SELECT t.id, t.item_id, i.module_id, t.workspace_id, t.parent_id,
        app_task_reference(t.id) AS reference, t.title, t.description, s.name AS status,
        s.category AS status_category, t.priority, t.start_date, t.due_date, t.estimate_minutes,
        t.tags, t.milestone, t.series_id, t.occurrence_date, t.version, t.move_version,
        t.completed_at, t.updated_at
 FROM tasks t JOIN items i ON i.id = t.item_id JOIN task_statuses s ON s.id = t.status_id";

impl TryFrom<TaskRecord> for Task {
    type Error = AppError;

    fn try_from(row: TaskRecord) -> Result<Self, AppError> {
        Ok(Self {
            id: row.id,
            item_id: row.item_id,
            module_id: row.module_id,
            workspace_id: row.workspace_id,
            parent_id: row.parent_id,
            reference: row.reference,
            title: row.title,
            description: row.description,
            status: row.status,
            status_category: StatusCategory::parse(&row.status_category)?,
            priority: TaskPriority::parse(&row.priority)
                .ok_or_else(|| AppError::Config(format!("unknown priority {}", row.priority)))?,
            start_date: format_date(row.start_date),
            due_date: format_date(row.due_date),
            estimate_minutes: row.estimate_minutes,
            tags: row.tags,
            milestone: row.milestone,
            series_id: row.series_id,
            occurrence_date: format_date(row.occurrence_date),
            version: row.version,
            move_version: row.move_version,
            completed_at: row.completed_at,
            updated_at: row.updated_at,
        })
    }
}

/// 업무 하나. 볼 수 없으면 404.
pub(crate) async fn load_task(tx: &mut UserTx, id: Uuid) -> Result<Task, AppError> {
    let row: Option<TaskRecord> = sqlx::query_as(&format!("{TASK_SELECT} WHERE t.id = $1"))
        .bind(id)
        .fetch_optional(tx.conn())
        .await?;
    row.ok_or(AppError::NotFound)?.try_into()
}

/// 업무 칸·판·담당을 한 문장으로 읽는다. 바깥 업무와 담당 관계 모두 같은 앱 역할의 RLS를 따른다.
pub(super) async fn load_conflict(
    tx: &mut UserTx,
    id: Uuid,
) -> Result<TaskConflictCurrent, AppError> {
    let row: Option<ConflictRecord> = sqlx::query_as(&format!(
        "SELECT task.*, ARRAY(SELECT a.user_id FROM task_assignees a
         WHERE a.task_id = task.id ORDER BY a.user_id) AS assignee_ids
         FROM ({TASK_SELECT} WHERE t.id = $1) task"
    ))
    .bind(id)
    .fetch_optional(tx.conn())
    .await?;
    let row = row.ok_or(AppError::NotFound)?;
    Ok(TaskConflictCurrent {
        task: row.task.try_into()?,
        assignee_ids: row.assignee_ids,
    })
}

/// 항목의 업무를 칸반 열·위치·번호 순서로 읽는다.
pub(super) async fn load_tasks(tx: &mut UserTx, item: ItemId) -> Result<Vec<Task>, AppError> {
    let rows: Vec<TaskRecord> = sqlx::query_as(&format!(
        "{TASK_SELECT} WHERE t.item_id = $1 ORDER BY s.position, t.position, t.number"
    ))
    .bind(item)
    .fetch_all(tx.conn())
    .await?;
    rows.into_iter().map(Task::try_from).collect()
}
