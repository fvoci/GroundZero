//! 서버에 남은 기능의 HTTP 배선. 도메인 라우터와 같은 API 경계에서 조립한다.

use super::AppState;
use crate::features::{accounts, item_profile, modules};
use axum::middleware::from_extractor_with_state;
use axum::routing::{get, post, put};
use axum::{Router, middleware};
use fvoci_http::session::ExpectedUserIfSent;

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
        .merge(fvoci_task_screens::router())
        .merge(fvoci_search::router(state))
        .merge(fvoci_references::router(state))
        .merge(fvoci_documents::router(state))
        .merge(fvoci_system::router(state))
        .merge(fvoci_files::router(state))
        .merge(fvoci_calendar::router())
        .merge(fvoci_time::router(state))
        .merge(fvoci_activity::router(state))
        .merge(accounts_routes())
        .merge(item_profile_routes(state))
        .merge(modules_routes(state))
        .merge(collab_routes())
        .fallback(super::api_not_found)
        .layer(middleware::from_fn(super::json_rejections))
}

fn accounts_routes() -> Router<AppState> {
    Router::new().route("/auth-info", get(accounts::auth_info))
}

/// 항목 정보·구성원·복제·템플릿과 모듈 아이콘. 도메인 라우터처럼 보낸 기대 사용자를 모든 명령에서 비교한다(D-96, #307).
fn item_profile_routes(state: &AppState) -> Router<AppState> {
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
        .route_layer(expected_user_if_sent(state))
}

/// 모듈·항목 만들기·고치기·보관. 보낸 기대 사용자를 모든 명령에서 비교한다(D-96, #307).
fn modules_routes(state: &AppState) -> Router<AppState> {
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
        .route_layer(expected_user_if_sent(state))
}

/// 도메인 라우터와 같은 공통 처리: 안전하지 않은 메서드에 머리가 있으면 세션의 사용자와 비교하고(다르면 409
/// `account_changed`, 형식 오류 400), 없으면 아무것도 하지 않는다. 처리기보다 먼저 끝나 아무것도 바꾸지 않는다.
fn expected_user_if_sent(
    state: &AppState,
) -> middleware::FromExtractorLayer<ExpectedUserIfSent, AppState> {
    from_extractor_with_state::<ExpectedUserIfSent, AppState>(state.clone())
}

fn collab_routes() -> Router<AppState> {
    Router::new().route(
        "/documents/{document_id}/collab",
        get(crate::collab::ws::connect),
    )
}
