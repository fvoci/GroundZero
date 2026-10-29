//! 조직 역할 변경의 가변 감사·정리 id 할당(D-88·D-112).

use super::OrgRole;
use fvoci_core::error::AppError;
use fvoci_core::ids::{OrganizationId, UserId};
use fvoci_db::needed_ids;
use sqlx::PgConnection;
use uuid::Uuid;

pub(super) async fn for_change(
    conn: &mut PgConnection,
    organization: OrganizationId,
    member: UserId,
    role: OrgRole,
) -> Result<Vec<Uuid>, AppError> {
    needed_ids(
        conn,
        sqlx::query_scalar("SELECT app_set_organization_member_ids($1, $2, $3)")
            .bind(organization)
            .bind(member)
            .bind(role.as_str()),
    )
    .await
}
