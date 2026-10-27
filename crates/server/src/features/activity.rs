//! 협업 활동(D-119). 감사로그와 별개이며 SQL이 대상 접근을 확인한다.
use crate::access::{self, Kind};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::features::notifications::CursorQuery;
use crate::features::organizations::OrgScope;
use crate::http::AppState;
use crate::ids::{OrganizationId, UserId};
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct Activity {
    pub id: Uuid,
    pub kind: String,
    pub actor_id: Option<Uuid>,
    pub actor_name: Option<String>,
    /// 최근 활동에는 포함하고 대상별 활동에는 없다(SQL 반환 계약).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    pub target_kind: String,
    pub target_id: Uuid,
    pub item_id: Option<Uuid>,
    pub detail: serde_json::Value,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ActivityPage {
    pub activity: Vec<Activity>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub next_before: Option<OffsetDateTime>,
    pub next_before_id: Option<Uuid>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct RecentActivityQuery {
    pub limit: Option<i32>,
}

#[utoipa::path(get, path = "/api/me/activity", operation_id = "my_recent_activity",
    params(RecentActivityQuery, ("organization_id" = OrganizationId, Query)),
    responses((status = 200, body = [Activity]), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "activity")]
pub async fn recent(
    State(state): State<AppState>,
    OrgScope { user, organization }: OrgScope,
    Query(query): Query<RecentActivityQuery>,
) -> Result<Json<Vec<Activity>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let activity = sqlx::query_as("SELECT * FROM app_recent_activity($1, $2)")
        .bind(organization)
        .bind(query.limit)
        .fetch_all(tx.conn())
        .await?;
    tx.commit().await?;
    Ok(Json(activity))
}

async fn target(
    state: &AppState,
    user: UserId,
    kind: Kind,
    id: Uuid,
    query: CursorQuery,
) -> Result<Json<ActivityPage>, AppError> {
    query.validate()?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let mut tx = state.db.begin_as(user).await?;
    access::require_visible(&mut tx, kind, id).await?;
    let activity: Vec<Activity> = sqlx::query_as(
        "SELECT a.*, NULL::uuid AS workspace_id FROM app_target_activity($1, $2, $3, $4, $5) a",
    )
    .bind(kind.as_str())
    .bind(id)
    .bind(limit)
    .bind(query.before)
    .bind(query.before_id)
    .fetch_all(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    let next = activity
        .last()
        .filter(|_| i32::try_from(activity.len()).is_ok_and(|n| n == limit));
    let (next_before, next_before_id) = (next.map(|a| a.created_at), next.map(|a| a.id));
    Ok(Json(ActivityPage {
        activity,
        next_before,
        next_before_id,
    }))
}

#[utoipa::path(get, path = "/api/items/{item_id}/activity", operation_id = "item_activity",
    params(("item_id" = Uuid, Path), CursorQuery),
    responses((status = 200, body = ActivityPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "activity")]
pub async fn item(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<ActivityPage>, AppError> {
    target(&state, user, Kind::Item, id, query).await
}

#[utoipa::path(get, path = "/api/documents/{document_id}/activity", operation_id = "document_activity",
    params(("document_id" = Uuid, Path), CursorQuery),
    responses((status = 200, body = ActivityPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "activity")]
pub async fn document(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<ActivityPage>, AppError> {
    target(&state, user, Kind::Document, id, query).await
}

#[utoipa::path(get, path = "/api/tasks/{task_id}/activity", operation_id = "task_activity",
    params(("task_id" = Uuid, Path), CursorQuery),
    responses((status = 200, body = ActivityPage), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "activity")]
pub async fn task(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<Uuid>,
    Query(query): Query<CursorQuery>,
) -> Result<Json<ActivityPage>, AppError> {
    target(&state, user, Kind::Task, id, query).await
}
