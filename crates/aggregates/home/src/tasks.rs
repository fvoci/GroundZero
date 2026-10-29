//! 현재 조직에서 내가 담당한 업무의 집계 읽기.

use crate::HomeState;
use axum::Json;
use axum::extract::{Query, State};
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{ItemId, OrganizationId, WorkspaceId};
use fvoci_http::organization::OrgScope;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Deserialize, IntoParams)]
pub struct MyTasksQuery {
    /// YYYY-MM-DD. 없으면 저장된 사용자 시간대의 오늘(미설정은 UTC).
    pub today: Option<String>,
}

/// SQL bucket: overdue, today, upcoming, none(날짜 없음), done.
#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct MyTask {
    pub id: Uuid,
    pub item_id: ItemId,
    pub reference: Option<String>,
    pub title: String,
    pub status: String,
    pub status_category: String,
    pub priority: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub bucket: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, sqlx::FromRow)]
pub struct MyTaskCounts {
    pub overdue: i32,
    pub today: i32,
    pub upcoming: i32,
    pub undated: i32,
    pub done: i32,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WorkspaceTasks {
    pub workspace_id: WorkspaceId,
    pub workspace_name: String,
    pub tasks: Vec<MyTask>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MyTasks {
    pub counts: MyTaskCounts,
    pub workspaces: Vec<WorkspaceTasks>,
}

#[derive(sqlx::FromRow)]
struct MyTaskRow {
    workspace_id: WorkspaceId,
    workspace_name: String,
    #[sqlx(flatten)]
    task: MyTask,
}

#[utoipa::path(get, path = "/api/me/tasks", operation_id = "my_tasks",
    params(MyTasksQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = MyTasks), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "home")]
pub async fn my_tasks(
    State(state): State<HomeState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<MyTasksQuery>,
) -> Result<Json<MyTasks>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let today = match query.today {
        Some(raw) => Date::parse(&raw, time::macros::format_description!("[year]-[month]-[day]"))
            .map_err(|_| AppError::invalid("today", InvalidReason::InvalidFormat, None, "today must be YYYY-MM-DD"))?,
        None => sqlx::query_scalar(
            "SELECT (now() AT TIME ZONE coalesce(nullif((SELECT time_zone FROM user_settings WHERE user_id = app_user_id()), ''), 'UTC'))::date",
        ).fetch_one(tx.conn()).await?,
    };
    let rows: Vec<MyTaskRow> = sqlx::query_as(
        "SELECT workspace_id, workspace_name, task_id AS id, item_id, reference, title, status,
                status_category, priority, start_date::text, due_date::text, bucket, updated_at
         FROM app_my_tasks($1, $2)",
    )
    .bind(organization)
    .bind(today)
    .fetch_all(tx.conn())
    .await?;
    let counts = sqlx::query_as("SELECT * FROM app_my_task_counts($1, $2)")
        .bind(organization)
        .bind(today)
        .fetch_one(tx.conn())
        .await?;
    tx.commit().await?;
    let mut workspaces: Vec<WorkspaceTasks> = Vec::new();
    for row in rows {
        if let Some(group) = workspaces
            .last_mut()
            .filter(|g| g.workspace_id == row.workspace_id)
        {
            group.tasks.push(row.task);
        } else {
            workspaces.push(WorkspaceTasks {
                workspace_id: row.workspace_id,
                workspace_name: row.workspace_name,
                tasks: vec![row.task],
            });
        }
    }
    Ok(Json(MyTasks { counts, workspaces }))
}
