//! 조직(D-111·D-112): 조직·서버 관리 API와 개인 화면의 "현재 조직" 문맥.
//!
//! 마지막으로 고른 조직은 개인 설정에 저장한다. 화면의 조직 › 워크스페이스 전환기가 고른 조직을 요청마다
//! `organization_id` 질의로 보내고, 서버는 그 조직의 멤버인지만 확인한다(`app_access('organization', …)`).
//! 조직 관리(멤버·설정·한도·공개 링크 토글·관리자로 참여)와 서버 관리자 API는 이 모듈에 이어서 둔다. 모두 정의자 함수가
//! 권한·불변식·감사 기록을 처리하고 여기서는 입력 검증과 응답 변환만 한다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::OrganizationsState;
use crate::members::OfferOwnership;
use fvoci_core::error::{AppError, ErrorBody};
use fvoci_core::ids::{OrganizationId, Role, UserId, WorkspaceId};
use fvoci_core::validate::validate_title;
use fvoci_db::access::{self, Kind};
use fvoci_db::{UserTx, needed_ids, new_ids};
use fvoci_http::session::CurrentUser;

mod role_ids;
pub mod settings;

pub use settings::{OrganizationLimits, OrganizationSettingsPatch};

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

pub use fvoci_http::organization::OrgScope;

/// 내가 속한 조직. 전환기의 목록이다. 서버 관리자라도 속하지 않은 조직은 여기 없다.
#[utoipa::path(get, path = "/api/me/organizations", operation_id = "list_my_organizations",
    responses((status = 200, body = [OrganizationSummary]), (status = 401, body = ErrorBody)), tag = "organizations")]
pub async fn list_mine(
    State(state): State<OrganizationsState>,
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
    State(state): State<OrganizationsState>,
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
    State(state): State<OrganizationsState>,
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
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("organization_id" = OrganizationId, Path), ("user_id" = UserId, Path)), request_body = SetOrganizationRole,
    responses((status = 400, body = ErrorBody), (status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn set_member(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path((org, member)): Path<(OrganizationId, UserId)>,
    Json(input): Json<SetOrganizationRole>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    let ids = role_ids::for_change(tx.conn(), org, member, input.role).await?;
    sqlx::query("SELECT app_set_organization_member($1, $2, $3, $4)")
        .bind(org)
        .bind(member)
        .bind(input.role.as_str())
        .bind(ids)
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
    State(state): State<OrganizationsState>,
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
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("organization_id" = OrganizationId, Path), ("user_id" = UserId, Path)),
    responses((status = 400, body = ErrorBody), (status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn remove_member(
    State(state): State<OrganizationsState>,
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
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("organization_id" = OrganizationId, Path)), request_body = InviteOrganization,
    responses((status = 201, body = OrganizationInvitation), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn invite(
    State(state): State<OrganizationsState>,
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
    State(state): State<OrganizationsState>,
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
    State(state): State<OrganizationsState>,
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
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("organization_id" = OrganizationId, Path), ("workspace_id" = WorkspaceId, Path)),
    responses((status = 409, body = ErrorBody), (status = 400, body = ErrorBody), (status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn join_as_admin(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path((org, ws)): Path<(OrganizationId, WorkspaceId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    // 내용에 접근할 필요 없이 관리자 메타데이터 함수로 URL의 부모·자식 관계를 확인한다.
    require_organization_workspace(&mut tx, org, ws).await?;
    sqlx::query("SELECT app_join_workspace_as_admin($1, $2)")
        .bind(ws)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_organization_workspace(
    tx: &mut UserTx,
    org: OrganizationId,
    ws: WorkspaceId,
) -> Result<(), AppError> {
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
    Ok(())
}

/// 조직 관리자가 보존 기간 안의 삭제된 워크스페이스를 복원한다.
#[utoipa::path(post, path = "/api/organizations/{organization_id}/workspaces/{workspace_id}/restore", operation_id = "restore_organization_workspace",
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("organization_id" = OrganizationId, Path), ("workspace_id" = WorkspaceId, Path)),
    responses((status = 400, body = ErrorBody), (status = 204), (status = 403, body = ErrorBody), (status = 404, body = ErrorBody),
              (status = 409, body = ErrorBody)), tag = "organizations")]
pub async fn restore_workspace(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path((org, ws)): Path<(OrganizationId, WorkspaceId)>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    require_organization_workspace(&mut tx, org, ws).await?;
    sqlx::query("SELECT app_restore_workspace($1, $2)")
        .bind(ws)
        .bind(new_ids(2))
        .execute(tx.conn())
        .await
        .map_err(AppError::from_db_function)?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 조직 관리자가 워크스페이스 멤버에게 소유권 인계를 제안한다.
#[utoipa::path(post, path = "/api/organizations/{organization_id}/workspaces/{workspace_id}/ownership-offer", operation_id = "offer_organization_workspace_ownership",
    params(("x-fvoci-expected-user" = Option<UserId>, Header), ("organization_id" = OrganizationId, Path), ("workspace_id" = WorkspaceId, Path)),
    request_body = OfferOwnership,
    responses((status = 409, body = ErrorBody), (status = 204), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody),
              (status = 404, body = ErrorBody)), tag = "organizations")]
pub async fn offer_workspace_ownership(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Path((org, ws)): Path<(OrganizationId, WorkspaceId)>,
    Json(input): Json<OfferOwnership>,
) -> Result<StatusCode, AppError> {
    let mut tx = state.db.begin_as(user).await?;
    require_organization_workspace(&mut tx, org, ws).await?;
    sqlx::query("SELECT app_offer_ownership($1, $2, $3)")
        .bind(ws)
        .bind(input.to_user)
        .bind(new_ids(1))
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
    State(state): State<OrganizationsState>,
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

#[utoipa::path(post, params(("x-fvoci-expected-user" = Option<UserId>, Header)), path = "/api/server/organizations", operation_id = "create_organization",
    request_body = CreateOrganization,
    responses((status = 409, body = ErrorBody), (status = 201, body = CreatedOrganization), (status = 400, body = ErrorBody),
              (status = 403, body = ErrorBody)), tag = "organizations")]
pub async fn create(
    State(state): State<OrganizationsState>,
    CurrentUser(user): CurrentUser,
    Json(input): Json<CreateOrganization>,
) -> Result<(StatusCode, Json<CreatedOrganization>), AppError> {
    let name = validate_title(&input.name, state.max_title_chars, "name")?;
    let mut tx = state.db.begin_as(user).await?;
    let id = sqlx::query_scalar("SELECT app_create_organization($1, $2, $3, $4)")
        .bind(input.request_id)
        .bind(name)
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
    State(state): State<OrganizationsState>,
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
