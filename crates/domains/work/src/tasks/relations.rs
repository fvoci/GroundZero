//! 업무의 관계 모음 읽기(노르웨이숲 태비 초안 1·2): 담당, 선행 관계, 반복 계열, 체크리스트 항목. 모음은 항목(`?item=`)
//! 또는 업무(`?task=`) 범위로 한 번에 읽어 요청 수가 업무 수에 비례하지 않게 한다. 화면은 업무 자원과 이 모음을 함께
//! 읽어 조합하고, 모르는 id는 버린다(나눈 읽기는 시점이 조금씩 다를 수 있다. 정합성은 명령의 판 조건이 지킨다).
//! 사용자 권한(RLS)으로 읽으므로 볼 수 없는 업무의 행은 나오지 않는다. 범위를 볼 수 없으면 404.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use super::model::{ChecklistItem, TaskSeries};
use crate::recurrence::RecurrenceRule;
use crate::schedule::LinkKind;
use crate::state::WorkState;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{ItemId, UserId};
use fvoci_db::UserTx;
use fvoci_http::session::CurrentUser;

/// 모음의 범위. `item`과 `task` 가운데 꼭 하나만 보낸다.
#[derive(Debug, Deserialize, IntoParams)]
pub struct RelationScope {
    /// 이 항목의 모든 업무.
    #[param(value_type = Option<String>, format = Uuid)]
    pub item: Option<ItemId>,
    /// 이 업무 하나.
    #[param(value_type = Option<String>, format = Uuid)]
    pub task: Option<Uuid>,
}

#[derive(Debug, Clone, Copy)]
enum Scope {
    Item(ItemId),
    Task(Uuid),
}

impl RelationScope {
    fn scope(&self) -> Result<Scope, AppError> {
        match (self.item, self.task) {
            (Some(item), None) => Ok(Scope::Item(item)),
            (None, Some(task)) => Ok(Scope::Task(task)),
            _ => Err(AppError::invalid(
                "item",
                InvalidReason::Required,
                None,
                "send exactly one of item or task",
            )),
        }
    }
}

/// 범위를 볼 수 있는지 본다. 없으면 존재를 숨기고 404.
async fn open_scope(
    state: &WorkState,
    user: UserId,
    query: &RelationScope,
) -> Result<(UserTx, Scope), AppError> {
    let scope = query.scope()?;
    let mut tx = state.db.begin_as(user).await?;
    let visible: bool = match scope {
        Scope::Item(item) => {
            sqlx::query_scalar("SELECT app_access('item', $1) <> 'none'")
                .bind(item)
                .fetch_one(tx.conn())
                .await?
        }
        Scope::Task(task) => {
            sqlx::query_scalar("SELECT app_access('task', $1) <> 'none'")
                .bind(task)
                .fetch_one(tx.conn())
                .await?
        }
    };
    if !visible {
        return Err(AppError::NotFound);
    }
    Ok((tx, scope))
}

/// 범위의 id(항목 또는 업무)와 그 범위에 맞는 조건 하나를 고른다.
fn pick(scope: Scope, by_item: &'static str, by_task: &'static str) -> (Uuid, &'static str) {
    match scope {
        Scope::Item(item) => (item.0, by_item),
        Scope::Task(task) => (task, by_task),
    }
}

/// 업무 담당 한 줄.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskAssignee {
    #[schema(value_type = String, format = Uuid)]
    pub task_id: Uuid,
    pub user_id: UserId,
    pub display_name: String,
}

/// 담당 모음(이름 순).
#[utoipa::path(get, path = "/api/task-assignees", operation_id = "list_task_assignees",
    params(RelationScope),
    responses((status = 200, body = [TaskAssignee]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn assignees(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<RelationScope>,
) -> Result<Json<Vec<TaskAssignee>>, AppError> {
    let (mut tx, scope) = open_scope(&state, user, &query).await?;
    let (target, filter) = pick(scope, "t.item_id = $1", "a.task_id = $1");
    // 담당자 이름은 같은 항목을 볼 수 있는 사람이므로 워크스페이스를 함께 쓰는 사람이다(users RLS).
    let rows: Vec<(Uuid, UserId, Option<String>)> = sqlx::query_as(&format!(
        "SELECT a.task_id, a.user_id, u.display_name
         FROM task_assignees a JOIN tasks t ON t.id = a.task_id LEFT JOIN users u ON u.id = a.user_id
         WHERE {filter} ORDER BY a.task_id, u.display_name, a.user_id"
    ))
    .bind(target)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(task_id, user_id, name)| TaskAssignee {
                task_id,
                user_id,
                display_name: name.unwrap_or_default(),
            })
            .collect(),
    ))
}

/// 선행 관계 한 줄(D-31·D-91): 선행 업무가 끝나거나 시작한 뒤 후속 업무가 어떻게 이어지는지.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskDependency {
    #[schema(value_type = String, format = Uuid)]
    pub predecessor_id: Uuid,
    #[schema(value_type = String, format = Uuid)]
    pub successor_id: Uuid,
    pub kind: LinkKind,
    #[schema(minimum = 0, maximum = 3650)]
    pub lag_days: i32,
}

/// 선행 관계 모음(만든 순). 업무 범위면 그 업무가 선행이거나 후속인 관계 모두다.
#[utoipa::path(get, path = "/api/task-dependencies", operation_id = "list_task_dependencies",
    params(RelationScope),
    responses((status = 200, body = [TaskDependency]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn dependencies(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<RelationScope>,
) -> Result<Json<Vec<TaskDependency>>, AppError> {
    let (mut tx, scope) = open_scope(&state, user, &query).await?;
    let (target, filter) = pick(
        scope,
        "item_id = $1",
        "(successor_id = $1 OR predecessor_id = $1)",
    );
    let rows: Vec<(Uuid, Uuid, String, i32)> = sqlx::query_as(&format!(
        "SELECT predecessor_id, successor_id, kind, lag_days FROM task_dependencies
         WHERE {filter} ORDER BY created_at, predecessor_id, successor_id"
    ))
    .bind(target)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(predecessor_id, successor_id, kind, lag_days)| {
            Ok(TaskDependency {
                predecessor_id,
                successor_id,
                kind: LinkKind::parse(&kind)
                    .ok_or_else(|| AppError::Config(format!("unknown dependency kind {kind}")))?,
                lag_days,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(Json)
}

/// 반복 계열 모음. 업무 범위면 그 업무가 속한 계열(없으면 빈 목록)이다.
#[utoipa::path(get, path = "/api/task-series", operation_id = "list_task_series",
    params(RelationScope),
    responses((status = 200, body = [TaskSeries]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn series(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<RelationScope>,
) -> Result<Json<Vec<TaskSeries>>, AppError> {
    let (mut tx, scope) = open_scope(&state, user, &query).await?;
    let (target, filter) = pick(
        scope,
        "s.item_id = $1",
        "s.id = (SELECT series_id FROM tasks WHERE id = $1)",
    );
    let rows: Vec<(Uuid, Uuid, Option<serde_json::Value>, String)> = sqlx::query_as(&format!(
        "SELECT s.id, s.root_task_id, s.rule, s.timezone FROM task_series s WHERE {filter} ORDER BY s.id"
    ))
    .bind(target)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(id, root_task_id, rule, timezone)| {
            Ok(TaskSeries {
                id,
                root_task_id,
                rule: rule.map(RecurrenceRule::from_db).transpose()?,
                timezone,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(Json)
}

/// 체크리스트 항목 모음(업무별 순서). 항목 범위로 읽어 화면이 업무마다 완료 수를 센다(부조종사 결정 2026-10-28 Q2 (나)).
#[utoipa::path(get, path = "/api/checklist-items", operation_id = "list_checklist_items",
    params(RelationScope),
    responses((status = 200, body = [ChecklistItem]), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)),
    tag = "tasks")]
pub async fn checklist_items(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Query(query): Query<RelationScope>,
) -> Result<Json<Vec<ChecklistItem>>, AppError> {
    let (mut tx, scope) = open_scope(&state, user, &query).await?;
    let (target, filter) = pick(scope, "t.item_id = $1", "c.task_id = $1");
    let rows: Vec<(Uuid, Uuid, String, bool)> = sqlx::query_as(&format!(
        "SELECT c.id, c.task_id, c.text, c.checked
         FROM checklist_items c JOIN tasks t ON t.id = c.task_id
         WHERE {filter} ORDER BY c.task_id, c.position, c.created_at"
    ))
    .bind(target)
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    Ok(Json(
        rows.into_iter()
            .map(|(id, task_id, text, checked)| ChecklistItem {
                id,
                task_id,
                text,
                checked,
            })
            .collect(),
    ))
}

/// 업무의 체크리스트(명령 응답과 읽기가 같은 자원을 쓴다).
pub(crate) async fn load_checklist(
    tx: &mut UserTx,
    task: Uuid,
) -> Result<Vec<ChecklistItem>, AppError> {
    Ok(sqlx::query_as("SELECT id, task_id, text, checked FROM checklist_items WHERE task_id = $1 ORDER BY position, created_at")
        .bind(task).fetch_all(tx.conn()).await?)
}

/// 업무에 걸린 반복 계열. 반복이 없으면 없다.
pub(crate) async fn load_series(
    tx: &mut UserTx,
    task: Uuid,
) -> Result<Option<TaskSeries>, AppError> {
    let row: Option<(Uuid, Uuid, Option<serde_json::Value>, String)> = sqlx::query_as(
        "SELECT s.id, s.root_task_id, s.rule, s.timezone FROM task_series s JOIN tasks t ON t.series_id = s.id WHERE t.id = $1")
        .bind(task).fetch_optional(tx.conn()).await?;
    row.map(|(id, root_task_id, rule, timezone)| {
        Ok(TaskSeries {
            id,
            root_task_id,
            rule: rule.map(RecurrenceRule::from_db).transpose()?,
            timezone,
        })
    })
    .transpose()
}

/// 한 선행 관계. 충돌 때 이미 지워진 관계면 없다.
pub(crate) async fn load_dependency(
    tx: &mut UserTx,
    predecessor: Uuid,
    successor: Uuid,
) -> Result<Option<TaskDependency>, AppError> {
    let row: Option<(String, i32)> = sqlx::query_as(
        "SELECT kind, lag_days FROM task_dependencies WHERE predecessor_id = $1 AND successor_id = $2")
        .bind(predecessor).bind(successor).fetch_optional(tx.conn()).await?;
    row.map(|(kind, lag_days)| {
        Ok(TaskDependency {
            predecessor_id: predecessor,
            successor_id: successor,
            kind: LinkKind::parse(&kind)
                .ok_or_else(|| AppError::Config(format!("unknown dependency kind {kind}")))?,
            lag_days,
        })
    })
    .transpose()
}
