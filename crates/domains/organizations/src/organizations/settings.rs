//! 조직 설정과 공간 한도(D-114·D-124).

use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use super::{Organization, read_organization};
use crate::OrganizationsState;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{OrganizationId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_db::access::{self, Kind};
use fvoci_db::{UserTx, new_ids};
use fvoci_http::session::{CurrentUser, ExpectedUser};

/// 조직·워크스페이스의 한도·보존 설정값(D-124). `null`은 해당 단계에서 미설정이라는 뜻이다.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OrganizationLimits {
    #[schema(minimum = 1)]
    pub max_file_bytes: Option<i64>,
    #[schema(minimum = 0)]
    pub max_storage_bytes: Option<i64>,
    #[schema(minimum = 1)]
    pub max_concurrent_uploads: Option<i32>,
    /// 이메일 등 외부 채널만 적용하는 1시간 창의 발송 한도. 초과분은 다음 시간대로 지연하고 인앱 알림은 항상 만든다.
    /// 현재 외부 채널은 구현되어 있지 않다.
    #[schema(minimum = 1)]
    pub notification_rate_per_hour: Option<i32>,
    #[schema(minimum = 1, maximum = 3650)]
    pub activity_retention_days: Option<i32>,
    #[schema(minimum = 1, maximum = 3650)]
    pub audit_retention_days: Option<i32>,
}

/// 키 생략은 유지, 한도의 null은 미설정으로 되돌린다. 원본 JSON을 SQL에 전달한다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct OrganizationSettingsPatch {
    #[schema(nullable = false)]
    pub name: Option<String>,
    pub public_links_enabled: Option<bool>,
    #[serde(flatten)]
    pub limits: OrganizationLimits,
}

/// SQL의 열 CHECK와 같은 입력 범위. SQL의 23514/22P02는 내부 오류이므로 HTTP 입력에서 검증한다.
fn validate_limits(limits: &OrganizationLimits) -> Result<(), AppError> {
    if limits.max_file_bytes.is_some_and(|v| v < 1)
        || limits.max_storage_bytes.is_some_and(|v| v < 0)
        || limits.max_concurrent_uploads.is_some_and(|v| v < 1)
        || limits.notification_rate_per_hour.is_some_and(|v| v < 1)
        || limits
            .activity_retention_days
            .is_some_and(|v| !(1..=3650).contains(&v))
        || limits
            .audit_retention_days
            .is_some_and(|v| !(1..=3650).contains(&v))
    {
        return Err(AppError::BadRequest(
            "limit is outside its allowed range".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(put, path = "/api/organizations/{organization_id}/settings", operation_id = "set_organization_settings",
    params(("organization_id" = OrganizationId, Path), ("x-fvoci-expected-user" = UserId, Header)),
    request_body = OrganizationSettingsPatch,
    responses((status = 200, body = Organization), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn set_settings(
    State(state): State<OrganizationsState>,
    ExpectedUser(user): ExpectedUser,
    Path(org): Path<OrganizationId>,
    Json(mut patch): Json<Value>,
) -> Result<Json<Organization>, AppError> {
    let input = OrganizationSettingsPatch::deserialize(&patch)
        .map_err(|_| AppError::BadRequest("invalid organization settings".into()))?;
    if patch.get("name").is_some() {
        let name = input
            .name
            .as_deref()
            .ok_or_else(|| AppError::BadRequest("the organization needs a name".into()))?;
        let name = validate_title(name, state.max_title_chars, "name")?;
        patch["name"] = Value::String(name);
    }
    validate_limits(&input.limits)?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_organization_settings($1, $2, $3)")
        .bind(org)
        .bind(patch)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let result = read_organization(&mut tx, org).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn read_workspace_limits(
    tx: &mut UserTx,
    ws: WorkspaceId,
) -> Result<OrganizationLimits, AppError> {
    access::require_owner(tx, Kind::Workspace, ws).await?;
    sqlx::query_as("SELECT max_file_bytes, max_storage_bytes, max_concurrent_uploads,
        notification_rate_per_hour, activity_retention_days, audit_retention_days FROM workspaces WHERE id = $1")
        .bind(ws).fetch_optional(tx.conn()).await?.ok_or(AppError::NotFound)
}

/// 관리자가 편집하는 설정값만 반환한다(실효 한도는 계산하지 않는다).
#[utoipa::path(get, path = "/api/workspaces/{workspace_id}/limits", operation_id = "workspace_limits",
    params(("workspace_id" = WorkspaceId, Path)),
    responses((status = 200, body = OrganizationLimits), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn workspace_limits(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path(ws): Path<WorkspaceId>,
) -> Result<Json<OrganizationLimits>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let result = read_workspace_limits(&mut tx, ws).await?;
    tx.commit().await?;
    Ok(Json(result))
}

#[utoipa::path(put, path = "/api/workspaces/{workspace_id}/limits", operation_id = "set_workspace_limits",
    params(("workspace_id" = WorkspaceId, Path), ("x-fvoci-expected-user" = UserId, Header)),
    request_body = OrganizationLimits,
    responses((status = 200, body = OrganizationLimits), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn set_workspace_limits(
    State(state): State<OrganizationsState>,
    ExpectedUser(user): ExpectedUser,
    Path(ws): Path<WorkspaceId>,
    Json(patch): Json<Value>,
) -> Result<Json<OrganizationLimits>, AppError> {
    let input = OrganizationLimits::deserialize(&patch)
        .map_err(|_| AppError::BadRequest("invalid workspace limits".into()))?;
    validate_limits(&input)?;
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_workspace_limits($1, $2, $3)")
        .bind(ws)
        .bind(patch)
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let result = read_workspace_limits(&mut tx, ws).await?;
    tx.commit().await?;
    Ok(Json(result))
}
