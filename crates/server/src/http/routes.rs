//! 서버에 남은 기능의 HTTP 배선. 도메인 라우터와 같은 API 경계에서 조립한다.

use super::AppState;
use crate::features::{accounts, item_profile, modules};
use axum::routing::{get, post, put};
use axum::{Router, middleware};

pub(super) fn api(state: &AppState) -> Router<AppState> {
    Router::new()
        .merge(fvoci_organizations::router(state).layer(axum::Extension(
            fvoci_organizations::runtime::OrganizationRooms(std::sync::Arc::new(
                crate::collab::organizations::OrganizationCollab {
                    rooms: state.collab.clone(),
                    limits: state.config.limits,
                    prepare_timeout: state.config.collab_recheck,
                },
            )),
        )))
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
        .merge(item_profile_routes())
        .merge(modules_routes())
        .merge(collab_routes())
        .fallback(super::api_not_found)
        .layer(middleware::from_fn(super::json_rejections))
}

fn accounts_routes() -> Router<AppState> {
    Router::new().route("/auth-info", get(accounts::auth_info))
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
