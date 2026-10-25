//! 조직(D-111·D-112): 조직·서버 관리 API와 개인 화면의 "현재 조직" 문맥.
//!
//! 현재 조직은 서버에 저장하지 않는다. 화면의 조직 › 워크스페이스 전환기가 고른 조직을 요청마다 `organization_id`
//! 질의로 보내고(구현 해석, 사용자 결정 밖), 서버는 그 조직의 멤버인지만 확인한다(`app_access('organization', …)`).
//! 조직 관리(멤버·설정·한도·공개 링크 토글·관리자로 참여)와 서버 관리자 API는 이 모듈에 이어서 둔다. 모두 정의자 함수가
//! 권한·불변식·감사 기록을 처리하고 여기서는 입력 검증과 응답 변환만 한다.

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::access::{self, Kind};
use crate::db::{UserTx, needed_ids, new_ids};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::{CurrentUser, ExpectedUser};
use crate::http::AppState;
use crate::ids::{OrganizationId, Role, UserId, WorkspaceId};

/// 조직 역할(D-112). 조직 관리자는 메타데이터만 다루고 내용은 "관리자로 참여" 뒤에만 본다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    Admin,
    Member,
}

impl OrgRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    pub fn parse(raw: &str) -> Result<Self, AppError> {
        match raw {
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            other => Err(AppError::Config(format!(
                "unknown organization role in database: {other}"
            ))),
        }
    }
}

/// 내가 속한 조직 한 줄.
#[derive(Debug, Serialize, ToSchema)]
pub struct OrganizationSummary {
    pub id: OrganizationId,
    pub name: String,
    pub role: OrgRole,
    /// 비로그인 공개 링크를 쓸 수 있는지(조직 관리자가 끌 수 있다, D-114).
    pub public_links_enabled: bool,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// 조직·워크스페이스의 한도·보존 설정값(D-124). `null`은 해당 단계에서 미설정이라는 뜻이다.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct OrganizationLimits {
    #[schema(minimum = 1)]
    pub max_file_bytes: Option<i64>,
    #[schema(minimum = 0)]
    pub max_storage_bytes: Option<i64>,
    #[schema(minimum = 1)]
    pub max_concurrent_uploads: Option<i32>,
    /// 열만 두고 아직 적용하지 않는다(사용자 결정 대기: 적용 대상·채널·초과 처리).
    #[schema(minimum = 1)]
    pub notification_rate_per_hour: Option<i32>,
    #[schema(minimum = 1, maximum = 3650)]
    pub activity_retention_days: Option<i32>,
    #[schema(minimum = 1, maximum = 3650)]
    pub audit_retention_days: Option<i32>,
}

/// 조직 상세(멤버에게). 설정 변경은 관리자만.
#[derive(Debug, Serialize, ToSchema)]
pub struct Organization {
    pub id: OrganizationId,
    pub name: String,
    pub role: OrgRole,
    pub public_links_enabled: bool,
    pub limits: OrganizationLimits,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

#[derive(Debug, Deserialize)]
struct OrgQuery {
    organization_id: Option<OrganizationId>,
}

/// 개인 화면 요청의 현재 조직 문맥(D-111). 질의 `organization_id`가 없거나 UUID가 아니면 400, 내가 속하지 않은 조직이면
/// 존재를 숨기고 404. 사용자는 세션으로 정한다(`CurrentUser`와 같다).
#[derive(Debug, Clone, Copy)]
pub struct OrgScope {
    pub user: UserId,
    pub organization: OrganizationId,
}

impl FromRequestParts<AppState> for OrgScope {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        let Query(query) = Query::<OrgQuery>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::BadRequest("organization_id must be a uuid".to_owned()))?;
        let organization = query
            .organization_id
            .ok_or_else(|| AppError::BadRequest("organization_id is required".to_owned()))?;
        let mut tx = state.db.begin_as(user).await?;
        access::require_visible(&mut tx, Kind::Organization, organization).await?;
        tx.commit().await?;
        Ok(Self { user, organization })
    }
}

/// 내가 속한 조직. 전환기의 목록이다. 서버 관리자라도 속하지 않은 조직은 여기 없다.
#[utoipa::path(get, path = "/api/me/organizations", operation_id = "list_my_organizations",
    responses((status = 200, body = [OrganizationSummary]), (status = 401, body = ErrorBody)), tag = "organizations")]
pub async fn list_mine(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<OrganizationSummary>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(OrganizationId, String, String, bool, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, name, role, public_links_enabled, created_at FROM app_my_organizations()",
    )
    .fetch_all(tx.conn())
    .await?;
    tx.commit().await?;
    rows.into_iter()
        .map(|(id, name, role, public_links_enabled, created_at)| {
            Ok(OrganizationSummary {
                id,
                name,
                role: OrgRole::parse(&role)?,
                public_links_enabled,
                created_at,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(Json)
}

type OrganizationRow = (
    OrganizationId,
    String,
    bool,
    Option<i64>,
    Option<i64>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    Option<i32>,
    OffsetDateTime,
);

/// 조직 하나. 멤버가 아니면 404.
#[utoipa::path(get, path = "/api/organizations/{organization_id}", operation_id = "get_organization",
    params(("organization_id" = OrganizationId, Path, description = "조직 id")),
    responses((status = 200, body = Organization), (status = 401, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn get(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(organization): Path<OrganizationId>,
) -> Result<Json<Organization>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let result = read_organization(&mut tx, organization).await?;
    tx.commit().await?;
    Ok(Json(result))
}

async fn read_organization(
    tx: &mut UserTx,
    organization: OrganizationId,
) -> Result<Organization, AppError> {
    let grade = access::require_visible(tx, Kind::Organization, organization).await?;
    let row: OrganizationRow = sqlx::query_as(
        "SELECT id, name, public_links_enabled, max_file_bytes, max_storage_bytes, max_concurrent_uploads,
                notification_rate_per_hour, activity_retention_days, audit_retention_days, created_at
         FROM organizations WHERE id = $1",
    )
    .bind(organization)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let (
        id,
        name,
        public_links_enabled,
        max_file_bytes,
        max_storage_bytes,
        max_concurrent_uploads,
        notification_rate_per_hour,
        activity_retention_days,
        audit_retention_days,
        created_at,
    ) = row;
    Ok(Organization {
        id,
        name,
        // 조직 등급: 관리자는 owner, 멤버는 read(app_grade_organization).
        role: if grade.is_owner() {
            OrgRole::Admin
        } else {
            OrgRole::Member
        },
        public_links_enabled,
        limits: OrganizationLimits {
            max_file_bytes,
            max_storage_bytes,
            max_concurrent_uploads,
            notification_rate_per_hour,
            activity_retention_days,
            audit_retention_days,
        },
        created_at,
    })
}

/// 조직 멤버. 이메일은 SQL이 조직 관리자에게만 돌려준다.
#[derive(Debug, Serialize, ToSchema)]
pub struct OrganizationMember {
    pub user_id: UserId,
    pub display_name: String,
    pub email: Option<String>,
    pub role: OrgRole,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

#[utoipa::path(get, path = "/api/organizations/{organization_id}/members", operation_id = "organization_members",
    params(("organization_id" = OrganizationId, Path)),
    responses((status = 200, body = [OrganizationMember]), (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn members(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(org): Path<OrganizationId>,
) -> Result<Json<Vec<OrganizationMember>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let rows: Vec<(UserId, String, Option<String>, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT user_id, display_name, email, role, created_at FROM app_organization_members($1)",
    )
    .bind(org)
    .fetch_all(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    let members = rows
        .into_iter()
        .map(|(user_id, display_name, email, role, created_at)| {
            Ok(OrganizationMember {
                user_id,
                display_name,
                email,
                role: OrgRole::parse(&role)?,
                created_at,
            })
        })
        .collect::<Result<_, AppError>>()?;
    tx.commit().await?;
    Ok(Json(members))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetOrganizationRole {
    pub role: OrgRole,
}

#[utoipa::path(put, path = "/api/organizations/{organization_id}/members/{user_id}", operation_id = "set_organization_member",
    params(("organization_id" = OrganizationId, Path), ("user_id" = UserId, Path)), request_body = SetOrganizationRole,
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn set_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((org, member)): Path<(OrganizationId, UserId)>,
    Json(input): Json<SetOrganizationRole>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    sqlx::query("SELECT app_set_organization_member($1, $2, $3, $4)")
        .bind(org)
        .bind(member)
        .bind(input.role.as_str())
        .bind(new_ids(1))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct OrganizationRemovalPreview {
    pub workspaces: i32,
    pub shares: i32,
    pub assignments: i32,
}

#[utoipa::path(get, path = "/api/organizations/{organization_id}/members/{user_id}/removal-preview", operation_id = "organization_member_removal_preview",
    params(("organization_id" = OrganizationId, Path), ("user_id" = UserId, Path)),
    responses((status = 200, body = OrganizationRemovalPreview), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn removal_preview(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((org, member)): Path<(OrganizationId, UserId)>,
) -> Result<Json<OrganizationRemovalPreview>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let result = sqlx::query_as("SELECT workspaces, shares, assignments FROM app_remove_organization_member_preview($1, $2)")
        .bind(org).bind(member).fetch_one(tx.conn()).await.map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(result))
}

#[utoipa::path(delete, path = "/api/organizations/{organization_id}/members/{user_id}", operation_id = "remove_organization_member",
    params(("organization_id" = OrganizationId, Path), ("user_id" = UserId, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn remove_member(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((org, member)): Path<(OrganizationId, UserId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = needed_ids(
        tx.conn(),
        sqlx::query_scalar("SELECT app_remove_organization_member_ids($1, $2)")
            .bind(org)
            .bind(member),
    )
    .await?;
    sqlx::query("SELECT app_remove_organization_member($1, $2, $3)")
        .bind(org)
        .bind(member)
        .bind(ids)
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct InviteOrganization {
    pub email: String,
    pub role: OrgRole,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct OrganizationInvitation {
    pub id: Uuid,
    pub email: String,
    pub role: OrgRole,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
}

#[utoipa::path(post, path = "/api/organizations/{organization_id}/invitations", operation_id = "invite_organization",
    params(("organization_id" = OrganizationId, Path)), request_body = InviteOrganization,
    responses((status = 201, body = OrganizationInvitation), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn invite(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(org): Path<OrganizationId>,
    Json(input): Json<InviteOrganization>,
) -> Result<(StatusCode, Json<OrganizationInvitation>), AppError> {
    let email = super::members::validate_email(&input.email)?;
    let mut tx = state.db.begin_as(user).await?;
    let id: Uuid = sqlx::query_scalar("SELECT app_invite_organization($1, $2, $3, $4, $5)")
        .bind(org)
        .bind(email)
        .bind(input.role.as_str())
        .bind(14 * 24 * 3600_i32)
        .bind(new_ids(3))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    let (email, role, expires_at): (String, String, OffsetDateTime) =
        sqlx::query_as("SELECT email, role, expires_at FROM invitations WHERE id = $1")
            .bind(id)
            .fetch_one(tx.conn())
            .await?;
    let result = OrganizationInvitation {
        id,
        email,
        role: OrgRole::parse(&role)?,
        expires_at,
    };
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// 조직 초대 목록은 전용 함수가 없으므로 접근 등급과 invitations_select RLS를 쓴다.
/// 철회는 기존 DELETE /api/invitations/{invitation_id}가 조직 초대도 처리한다.
#[utoipa::path(get, path = "/api/organizations/{organization_id}/invitations", operation_id = "organization_invitations",
    params(("organization_id" = OrganizationId, Path)),
    responses((status = 200, body = [OrganizationInvitation]), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn invitations(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(org): Path<OrganizationId>,
) -> Result<Json<Vec<OrganizationInvitation>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    access::require_owner(&mut tx, Kind::Organization, org).await?;
    let rows: Vec<(Uuid, String, String, OffsetDateTime)> = sqlx::query_as(
        "SELECT id, email, role, expires_at FROM invitations WHERE organization_id = $1
         AND workspace_id IS NULL AND closed_at IS NULL AND expires_at > now() ORDER BY created_at, id",
    ).bind(org).fetch_all(tx.conn()).await?;
    let result = rows
        .into_iter()
        .map(|(id, email, role, expires_at)| {
            Ok(OrganizationInvitation {
                id,
                email,
                role: OrgRole::parse(&role)?,
                expires_at,
            })
        })
        .collect::<Result<_, AppError>>()?;
    tx.commit().await?;
    Ok(Json(result))
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
    State(state): State<AppState>,
    ExpectedUser(user): ExpectedUser,
    Path(org): Path<OrganizationId>,
    Json(patch): Json<Value>,
) -> Result<Json<Organization>, AppError> {
    let input: OrganizationSettingsPatch = serde_json::from_value(patch.clone())
        .map_err(|_| AppError::BadRequest("invalid organization settings".into()))?;
    if patch.get("name").is_some()
        && input
            .name
            .as_ref()
            .is_none_or(|name| name.trim_matches(' ').is_empty())
    {
        return Err(AppError::BadRequest("the organization needs a name".into()));
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
    State(state): State<AppState>,
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
    State(state): State<AppState>,
    ExpectedUser(user): ExpectedUser,
    Path(ws): Path<WorkspaceId>,
    Json(patch): Json<Value>,
) -> Result<Json<OrganizationLimits>, AppError> {
    let input: OrganizationLimits = serde_json::from_value(patch.clone())
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

#[derive(Debug, Serialize, ToSchema)]
pub struct OrganizationWorkspace {
    pub id: WorkspaceId,
    pub name: String,
    pub owner_id: UserId,
    pub owner_name: String,
    pub member_count: i32,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub deleted_at: Option<OffsetDateTime>,
    pub my_role: Option<Role>,
}

#[utoipa::path(get, path = "/api/organizations/{organization_id}/workspaces", operation_id = "organization_workspaces",
    params(("organization_id" = OrganizationId, Path)),
    responses((status = 200, body = [OrganizationWorkspace]), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn workspaces(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(org): Path<OrganizationId>,
) -> Result<Json<Vec<OrganizationWorkspace>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    type Row = (
        WorkspaceId,
        String,
        UserId,
        String,
        i32,
        OffsetDateTime,
        Option<OffsetDateTime>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as("SELECT id, name, owner_id, owner_name, member_count, created_at, deleted_at, my_role FROM app_organization_workspaces($1)")
        .bind(org).fetch_all(tx.conn()).await.map_err(AppError::from_db_function)?;
    let result = rows
        .into_iter()
        .map(
            |(id, name, owner_id, owner_name, member_count, created_at, deleted_at, my_role)| {
                let my_role = my_role
                    .map(|role| {
                        Role::parse(&role).ok_or_else(|| {
                            AppError::Config(format!("unknown workspace role: {role}"))
                        })
                    })
                    .transpose()?;
                Ok(OrganizationWorkspace {
                    id,
                    name,
                    owner_id,
                    owner_name,
                    member_count,
                    created_at,
                    deleted_at,
                    my_role,
                })
            },
        )
        .collect::<Result<_, AppError>>()?;
    tx.commit().await?;
    Ok(Json(result))
}

#[utoipa::path(post, path = "/api/organizations/{organization_id}/workspaces/{workspace_id}/join-as-admin", operation_id = "join_workspace_as_admin",
    params(("organization_id" = OrganizationId, Path), ("workspace_id" = WorkspaceId, Path)),
    responses((status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn join_as_admin(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path((org, ws)): Path<(OrganizationId, WorkspaceId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 내용에 접근할 필요 없이 관리자 메타데이터 함수로 URL의 부모·자식 관계를 확인한다.
    let belongs: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM app_organization_workspaces($1) WHERE id = $2)",
    )
    .bind(org)
    .bind(ws)
    .fetch_one(tx.conn())
    .await
    .map_err(AppError::from_db_function)?;
    if !belongs {
        return Err(AppError::NotFound);
    }
    sqlx::query("SELECT app_join_workspace_as_admin($1, $2)")
        .bind(ws)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ServerOrganization {
    pub id: OrganizationId,
    pub name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
    pub member_count: i32,
    pub workspace_count: i32,
}

#[utoipa::path(get, path = "/api/server/organizations", operation_id = "server_organizations",
    responses((status = 200, body = [ServerOrganization]), (status = 403, body = ErrorBody)), tag = "organizations")]
pub async fn server_organizations(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<ServerOrganization>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let result = sqlx::query_as("SELECT id, name, created_at, member_count, workspace_count FROM app_server_organizations()")
        .fetch_all(tx.conn()).await.map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(result))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateOrganization {
    pub request_id: Uuid,
    pub name: String,
    pub first_admin_user_id: UserId,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreatedOrganization {
    pub id: OrganizationId,
}

#[utoipa::path(post, path = "/api/server/organizations", operation_id = "create_organization",
    request_body = CreateOrganization,
    responses((status = 201, body = CreatedOrganization), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody)), tag = "organizations")]
pub async fn create(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateOrganization>,
) -> Result<(StatusCode, Json<CreatedOrganization>), AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let id = sqlx::query_scalar("SELECT app_create_organization($1, $2, $3, $4)")
        .bind(input.request_id)
        .bind(input.name)
        .bind(input.first_admin_user_id)
        .bind(new_ids(3))
        .fetch_one(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(CreatedOrganization { id })))
}

#[derive(Debug, Serialize, sqlx::FromRow, ToSchema)]
pub struct ServerAdmin {
    pub user_id: UserId,
    pub display_name: String,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

#[utoipa::path(get, path = "/api/server/admins", operation_id = "server_admins",
    responses((status = 200, body = [ServerAdmin]), (status = 403, body = ErrorBody)), tag = "organizations")]
pub async fn server_admins(
    State(state): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> Result<Json<Vec<ServerAdmin>>, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let result =
        sqlx::query_as("SELECT user_id, display_name, created_at FROM app_server_admins()")
            .fetch_all(tx.conn())
            .await
            .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(Json(result))
}
