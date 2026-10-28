//! 개인 화면 요청의 현재 조직 문맥.

use crate::session::{CurrentUser, SessionState};
use axum::extract::{FromRef, FromRequestParts, Query};
use axum::http::request::Parts;
use fvoci_core::access::Grade;
use fvoci_core::error::AppError;
use fvoci_core::ids::{OrganizationId, UserId};
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
        let state = SessionState::from_ref(state);
        let mut tx = state.db.begin_as(user).await?;
        let raw: String = sqlx::query_scalar("SELECT app_access('organization', $1)")
            .bind(organization)
            .fetch_one(tx.conn())
            .await?;
        Grade::parse(&raw)?.ok_or(AppError::NotFound)?;
        tx.commit().await?;
        Ok(Self { user, organization })
    }
}
