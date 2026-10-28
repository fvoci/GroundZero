//! 업무 읽기: 항목의 업무 목록, 업무 하나, 담당 후보.

use axum::Json;
use axum::extract::{Path, State};
use uuid::Uuid;

use super::model::Assignee;
use super::resource::{Task, load_task, load_tasks};
use crate::state::WorkState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{ItemId, UserId};
use fvoci_http::session::CurrentUser;

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
    let tasks = load_tasks(&mut tx, item).await?;
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

/// 업무 하나의 칸. 관계와 체크리스트는 별도 모음으로 읽는다.
#[utoipa::path(get, path = "/api/tasks/{task_id}", operation_id = "get_task",
    params(("task_id" = String, Path, format = Uuid)),
    responses((status = 200, body = Task), (status = 404, body = ErrorBody)), tag = "tasks")]
pub async fn get(
    State(state): State<WorkState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Task>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let detail = load_task(&mut tx, id).await?;
    tx.commit().await?;
    Ok(Json(detail))
}
