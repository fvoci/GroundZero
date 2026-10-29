//! 계정·조직·공간·멤버 도메인의 라우터와 OpenAPI 조각(D-131, #178).

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod accounts;
pub mod members;
pub mod organizations;
pub mod state;
pub mod workspaces;

use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_extractor_with_state;
use axum::routing::{delete, get, post, put};
use fvoci_http::session::{ExpectedUserIfSent, SessionState};
use utoipa::OpenApi;

pub use state::{AccountState, OrganizationsState};

/// 계정·조직·공간·멤버 API 계약 조각.
#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        accounts::me,
        accounts::set_settings,
        accounts::set_last_organization,
        workspaces::list,
        workspaces::create,
        workspaces::delete,
        workspaces::deleted,
        workspaces::restore,
        workspaces::purge,
        organizations::list_mine,
        organizations::get,
        organizations::members,
        organizations::set_member,
        organizations::removal_preview,
        organizations::remove_member,
        organizations::invite,
        organizations::invitations,
        organizations::settings::set_settings,
        organizations::settings::workspace_limits,
        organizations::settings::set_workspace_limits,
        organizations::workspaces,
        organizations::join_as_admin,
        organizations::restore_workspace,
        organizations::offer_workspace_ownership,
        organizations::server_organizations,
        organizations::create,
        organizations::server_admins,
        members::list_members,
        members::set_role,
        members::remove_member,
        members::leave,
        members::invite,
        members::list_invitations,
        members::revoke_invitation,
        members::my_invitations,
        members::accept_invitation,
        members::decline_invitation,
        members::offer_ownership,
        members::cancel_ownership_offer,
        members::accept_ownership,
        members::list_groups,
        members::create_group,
        members::delete_group,
        members::add_group_member,
        members::remove_group_member,
    ),
    components(schemas(
        members::Member,
        members::MembersView,
        members::SetRole,
        members::CreateInvitation,
        members::Invitation,
        members::ReceivedInvitation,
        members::Accepted,
        members::InvitationTarget,
        members::OfferOwnership,
        members::Group,
        members::CreateGroup,
        accounts::Me,
        accounts::InputLimits,
        accounts::MySettings,
        accounts::SetLastOrganization,
        workspaces::Workspace,
        workspaces::CreateWorkspace,
        workspaces::DeleteWorkspace,
        workspaces::WorkspaceDeleted,
        workspaces::DeletedWorkspace,
        organizations::OrgRole,
        organizations::OrganizationSummary,
        organizations::OrganizationLimits,
        organizations::Organization,
        organizations::OrganizationMember,
        organizations::SetOrganizationRole,
        organizations::OrganizationRemovalPreview,
        organizations::InviteOrganization,
        organizations::OrganizationInvitation,
        organizations::OrganizationSettingsPatch,
        organizations::OrganizationWorkspace,
        organizations::ServerOrganization,
        organizations::CreateOrganization,
        organizations::CreatedOrganization,
        organizations::ServerAdmin,
    ))
)]
pub struct OrganizationsApi;

/// 조립 상태에서 부분 상태를 꺼내는 경로(/api 아래). 보낸 기대 사용자는 모든 명령에서 비교한다(D-96).
/// 이미 필수인 조직 설정·공간 한도는 처리기의 ExpectedUser도 그대로 유지한다.
pub fn router<S>(state: &S) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    OrganizationsState: FromRef<S>,
    AccountState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/me", get(accounts::me))
        .route("/me/settings", put(accounts::set_settings))
        .route(
            "/me/last-organization",
            put(accounts::set_last_organization),
        )
        .route("/me/organizations", get(organizations::list_mine))
        .route("/organizations/{organization_id}", get(organizations::get))
        .route(
            "/organizations/{organization_id}/members",
            get(organizations::members),
        )
        .route(
            "/organizations/{organization_id}/members/{user_id}",
            put(organizations::set_member).delete(organizations::remove_member),
        )
        .route(
            "/organizations/{organization_id}/members/{user_id}/removal-preview",
            get(organizations::removal_preview),
        )
        .route(
            "/organizations/{organization_id}/invitations",
            get(organizations::invitations).post(organizations::invite),
        )
        .route(
            "/organizations/{organization_id}/settings",
            put(organizations::settings::set_settings),
        )
        .route(
            "/workspaces/{workspace_id}/limits",
            get(organizations::settings::workspace_limits)
                .put(organizations::settings::set_workspace_limits),
        )
        .route(
            "/organizations/{organization_id}/workspaces",
            get(organizations::workspaces),
        )
        .route(
            "/organizations/{organization_id}/workspaces/{workspace_id}/join-as-admin",
            post(organizations::join_as_admin),
        )
        .route(
            "/organizations/{organization_id}/workspaces/{workspace_id}/restore",
            post(organizations::restore_workspace),
        )
        .route(
            "/organizations/{organization_id}/workspaces/{workspace_id}/ownership-offer",
            post(organizations::offer_workspace_ownership),
        )
        .route(
            "/server/organizations",
            get(organizations::server_organizations).post(organizations::create),
        )
        .route("/server/admins", get(organizations::server_admins))
        .route(
            "/workspaces",
            get(workspaces::list).post(workspaces::create),
        )
        .route("/workspaces/{workspace_id}", delete(workspaces::delete))
        .route("/me/deleted-workspaces", get(workspaces::deleted))
        .route(
            "/me/deleted-workspaces/{workspace_id}",
            delete(workspaces::purge),
        )
        .route(
            "/me/deleted-workspaces/{workspace_id}/restore",
            post(workspaces::restore),
        )
        .merge(member_routes())
        .route_layer(from_extractor_with_state::<ExpectedUserIfSent, S>(
            state.clone(),
        ))
}

fn member_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    OrganizationsState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route(
            "/workspaces/{workspace_id}/members",
            get(members::list_members),
        )
        .route(
            "/workspaces/{workspace_id}/members/{user_id}",
            put(members::set_role).delete(members::remove_member),
        )
        .route("/workspaces/{workspace_id}/leave", post(members::leave))
        .route(
            "/workspaces/{workspace_id}/invitations",
            get(members::list_invitations).post(members::invite),
        )
        .route("/invitations", get(members::my_invitations))
        .route(
            "/invitations/{invitation_id}",
            delete(members::revoke_invitation),
        )
        .route(
            "/invitations/{invitation_id}/accept",
            post(members::accept_invitation),
        )
        .route(
            "/invitations/{invitation_id}/decline",
            post(members::decline_invitation),
        )
        .route(
            "/workspaces/{workspace_id}/ownership",
            post(members::offer_ownership).delete(members::cancel_ownership_offer),
        )
        .route(
            "/workspaces/{workspace_id}/ownership/accept",
            post(members::accept_ownership),
        )
        .route(
            "/workspaces/{workspace_id}/groups",
            get(members::list_groups).post(members::create_group),
        )
        .route("/groups/{group_id}", delete(members::delete_group))
        .route(
            "/groups/{group_id}/members/{user_id}",
            put(members::add_group_member).delete(members::remove_group_member),
        )
}
