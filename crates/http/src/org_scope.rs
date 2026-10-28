//! 개인 화면의 현재 조직 문맥(D-111). 세션과 SQL 접근 등급으로 확인한다.
use crate::session::{CurrentUser, SessionState};
use axum::extract::{FromRef, FromRequestParts, Query};
use axum::http::request::Parts;
use fvoci_core::error::AppError;
use fvoci_core::ids::{OrganizationId, UserId};
use fvoci_db::access::{self, Kind};
use serde::Deserialize;

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

impl<S> FromRequestParts<S> for OrgScope
where
    S: Send + Sync,
    SessionState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AppError> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        let Query(query) = Query::<OrgQuery>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::BadRequest("organization_id must be a uuid".to_owned()))?;
        let organization = query
            .organization_id
            .ok_or_else(|| AppError::BadRequest("organization_id is required".to_owned()))?;
        let mut tx = SessionState::from_ref(state).db.begin_as(user).await?;
        access::require_visible(&mut tx, Kind::Organization, organization).await?;
        tx.commit().await?;
        Ok(Self { user, organization })
    }
}
