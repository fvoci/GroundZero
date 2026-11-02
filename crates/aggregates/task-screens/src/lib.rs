//! Item task screens: authenticated, RLS-preserving aggregate reads (#291).
#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod model;

use axum::extract::{FromRef, Path, State};
use axum::routing::get;
use axum::{Json, Router};
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::UserId;
use fvoci_db::Db;
use fvoci_http::session::{CurrentUser, SessionState};
use model::{TaskBoardRead, TaskContextRead, TaskDetailOptionsRead, TaskDetailRead};
use serde::de::DeserializeOwned;
use utoipa::OpenApi;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TaskScreensState {
    pub db: Db,
}

#[derive(Debug, OpenApi)]
#[openapi(paths(context, board, detail, options), components(schemas(
    TaskContextRead, TaskBoardRead, TaskDetailRead, TaskDetailOptionsRead,
    model::ScreenItem, model::ScreenModule, model::ScreenDocument, model::ScreenTask,
    model::ScreenStatus, model::ScreenAssignee, model::ScreenDependency,
    model::ScreenSeries, model::ScreenChecklistItem, model::ScreenAssignable,
    model::ScreenStatusCategory, model::ScreenPriority, model::ScreenLinkKind, model::SectionError,
    model::ReadSection<Vec<model::ScreenStatus>>, model::ReadSection<Vec<model::ScreenAssignable>>,
)))]
pub struct TaskScreensApi;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    TaskScreensState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/items/{item_id}/task-context", get(context))
        .route("/items/{item_id}/task-board", get(board))
        .route("/tasks/{task_id}/detail-read", get(detail))
        .route("/items/{item_id}/task-detail-options", get(options))
}

async fn read<T: DeserializeOwned>(
    state: &TaskScreensState,
    user: UserId,
    id: Uuid,
    sql: &str,
) -> Result<Json<T>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let value: serde_json::Value = sqlx::query_scalar(sql)
        .bind(id)
        .fetch_one(tx.conn())
        .await
        .map_err(read_error)?;
    let resource = serde_json::from_value(value)
        .map_err(|error| AppError::Config(format!("task screen SQL response: {error}")))?;
    tx.commit().await?;
    Ok(Json(resource))
}

fn read_error(error: sqlx::Error) -> AppError {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code.starts_with("28"))
    {
        AppError::Unauthorized
    } else {
        AppError::from_db_function(error)
    }
}

#[utoipa::path(get, path="/api/items/{item_id}/task-context", operation_id="read_task_context",
    params(("item_id"=String,Path,format=Uuid)),
    responses((status=200,body=TaskContextRead),(status=401,body=ErrorBody),(status=404,body=ErrorBody)),tag="tasks")]
pub async fn context(
    State(state): State<TaskScreensState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskContextRead>, AppError> {
    read(&state, user, id, "SELECT fvoci.app_task_context_read($1)").await
}

#[utoipa::path(get, path="/api/items/{item_id}/task-board", operation_id="read_task_board",
    params(("item_id"=String,Path,format=Uuid)),
    responses((status=200,body=TaskBoardRead),(status=401,body=ErrorBody),(status=404,body=ErrorBody)),tag="tasks")]
pub async fn board(
    State(state): State<TaskScreensState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskBoardRead>, AppError> {
    read(&state, user, id, "SELECT fvoci.app_task_board_read($1)").await
}

#[utoipa::path(get, path="/api/tasks/{task_id}/detail-read", operation_id="read_task_detail",
    params(("task_id"=String,Path,format=Uuid)),
    responses((status=200,body=TaskDetailRead),(status=401,body=ErrorBody),(status=403,body=ErrorBody),(status=404,body=ErrorBody)),tag="tasks")]
pub async fn detail(
    State(state): State<TaskScreensState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskDetailRead>, AppError> {
    read(&state, user, id, "SELECT fvoci.app_task_detail_read($1)").await
}

#[utoipa::path(get, path="/api/items/{item_id}/task-detail-options", operation_id="read_task_detail_options",
    params(("item_id"=String,Path,format=Uuid)),
    responses((status=200,body=TaskDetailOptionsRead),(status=401,body=ErrorBody),(status=403,body=ErrorBody),(status=404,body=ErrorBody)),tag="tasks")]
pub async fn options(
    State(state): State<TaskScreensState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskDetailOptionsRead>, AppError> {
    read(
        &state,
        user,
        id,
        "SELECT fvoci.app_task_detail_options_read($1)",
    )
    .await
}
