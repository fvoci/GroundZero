//! 조직(D-111·D-112) API 가운데 1단계 HEAD의 검증에 필요한 최소 부분. 2단계(`fvoci/overhaul-server`)의 구현을 이름·동작
//! 그대로 가져왔다(D-126 순서의 예외로 PR #139 본문에 기록): 1단계 규칙 "워크스페이스 초대 수락은 조직 멤버만"(D-114)
//! 때문에 조직 초대를 만드는 경로 없이는 두 사용자 브라우저 시험이 통과할 수 없다. 조직 목록·조회·멤버·설정·한도·서버
//! 관리자 API는 2단계에 그대로 있다. 정의자 함수가 권한·불변식·감사 기록을 처리하고 여기서는 입력 검증과 응답 변환만 한다.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::db::new_ids;
use crate::error::{AppError, ErrorBody};
use crate::features::accounts::CurrentUser;
use crate::http::AppState;
use crate::ids::OrganizationId;

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
