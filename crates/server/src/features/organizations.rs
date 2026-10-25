//! 조직(D-111·D-112): 내가 속한 조직 목록, 조직 조회, 개인 화면의 "현재 조직" 문맥.
//!
//! 현재 조직은 서버에 저장하지 않는다. 화면의 조직 › 워크스페이스 전환기가 고른 조직을 요청마다 `organization_id`
//! 질의로 보내고(구현 해석, 사용자 결정 밖), 서버는 그 조직의 멤버인지만 확인한다(`app_access('organization', …)`).
//! 조직 관리(멤버·설정·한도·공개 링크 토글·관리자로 참여)와 서버 관리자 API는 이 모듈에 이어서 둔다. 모두 정의자 함수가
//! 권한·불변식·감사 기록을 처리하고 여기서는 입력 검증과 응답 변환만 한다.

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::access::{self, Kind};
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::{OrganizationId, UserId};

/// 조직 역할(D-112). 조직 관리자는 메타데이터만 다루고 내용은 "관리자로 참여" 뒤에만 본다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    Admin,
    Member,
}

impl OrgRole {
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
    #[schema(value_type = String, format = DateTime)]
    pub created_at: OffsetDateTime,
}

/// 조직 단계 한도·보존 설정(D-124). `null`은 미설정(서버 기본값 또는 워크스페이스 설정을 따름).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct OrganizationLimits {
    pub max_file_bytes: Option<i64>,
    pub max_storage_bytes: Option<i64>,
    pub max_concurrent_uploads: Option<i32>,
    /// 열만 두고 아직 적용하지 않는다(사용자 결정 대기: 적용 대상·채널·초과 처리).
    pub notification_rate_per_hour: Option<i32>,
    pub activity_retention_days: Option<i32>,
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
    let grade = access::require_visible(&mut tx, Kind::Organization, organization).await?;
    let row: OrganizationRow = sqlx::query_as(
        "SELECT id, name, public_links_enabled, max_file_bytes, max_storage_bytes, max_concurrent_uploads,
                notification_rate_per_hour, activity_retention_days, audit_retention_days, created_at
         FROM organizations WHERE id = $1",
    )
    .bind(organization)
    .fetch_optional(tx.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    tx.commit().await?;
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
    Ok(Json(Organization {
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
    }))
}
