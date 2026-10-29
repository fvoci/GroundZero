//! 서버에 남은 기능의 HTTP 배선. 도메인 라우터와 같은 API 경계에서 조립한다.

use super::AppState;
use crate::features::{accounts, item_profile, members, modules, organizations, workspaces};
use axum::routing::{delete, get, post, put};
use axum::{Router, middleware};

pub(super) fn api(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(fvoci_work::router(state))
        .merge(fvoci_records::router(state))
        .merge(fvoci_home::router(state))
        .merge(fvoci_search::router(state))
        .merge(fvoci_references::router(state))
        .merge(fvoci_documents::router(state))
        .merge(fvoci_system::router(state))
        .merge(fvoci_files::router(state))
        .merge(fvoci_calendar::router())
        .merge(fvoci_time::router(state))
        .merge(fvoci_activity::router(state))
        .merge(accounts_routes())
        .merge(organizations_routes())
        .merge(workspaces_routes())
        .merge(item_profile_routes())
        .merge(modules_routes())
        .merge(collab_routes())
        .merge(members_routes())
        .fallback(super::api_not_found)
        .layer(middleware::from_fn(super::json_rejections))
}

fn accounts_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(accounts::me))
        .route("/me/settings", put(accounts::set_settings))
        .route("/auth-info", get(accounts::auth_info))
}

fn organizations_routes() -> Router<AppState> {
    Router::new()
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
            put(organizations::set_settings),
        )
        .route(
            "/workspaces/{workspace_id}/limits",
            get(organizations::workspace_limits).put(organizations::set_workspace_limits),
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
}

fn workspaces_routes() -> Router<AppState> {
    Router::new()
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
}

fn item_profile_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/items/{item_id}/profile",
            get(item_profile::get).put(item_profile::set),
        )
        .route("/items/{item_id}/members", put(item_profile::set_members))
        .route("/items/{item_id}/copy", post(item_profile::copy))
        .route("/items/{item_id}/template", put(item_profile::set_template))
        .route(
            "/modules/{module_id}/icon",
            put(item_profile::set_module_icon),
        )
}

fn modules_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/modules/{module_id}/description",
            put(modules::set_description),
        )
        .route(
            "/workspaces/{workspace_id}/modules",
            get(modules::list_modules).post(modules::create_module),
        )
        .route(
            "/modules/{module_id}",
            get(modules::get_module).put(modules::update_module),
        )
        .route(
            "/modules/{module_id}/archived",
            put(modules::archive_module),
        )
        .route(
            "/modules/{module_id}/items",
            get(modules::list_items).post(modules::create_item),
        )
        .route(
            "/items/{item_id}",
            get(modules::get_item).put(modules::update_item),
        )
        .route("/items/{item_id}/archived", put(modules::archive_item))
}

fn collab_routes() -> Router<AppState> {
    Router::new().route(
        "/documents/{document_id}/collab",
        get(crate::collab::ws::connect),
    )
}

fn members_routes() -> Router<AppState> {
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
