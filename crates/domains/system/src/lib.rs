//! 휴지통·ZIP 반출·자동 정리 도메인. 일정 실행은 조립 crate가 맡는다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod cleanup;
pub mod export;
pub mod state;
pub mod trash;

use axum::{
    Router,
    extract::FromRef,
    routing::{delete, get, post, put},
};
use fvoci_http::session::SessionState;
pub use state::SystemState;
use utoipa::OpenApi;

#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        trash::trash_document,
        trash::trash_item,
        trash::trash_module,
        trash::workspace_trash,
        trash::my_trash,
        trash::restore,
        trash::purge,
        trash::purge_expired,
        trash::purge_my_expired,
        trash::set_retention,
        trash::set_version_retention,
        export::workspace,
        export::mine,
    ),
    components(schemas(
        trash::Trashed,
        trash::TrashKind,
        trash::TrashEntry,
        trash::Trash,
        trash::Purged,
        trash::SetRetention,
        trash::SetVersionRetention,
    ))
)]
pub struct SystemApi;

/// 시스템 경로. 이 단계는 기존 HTTP 계약을 그대로 옮긴다.
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    SystemState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/documents/{document_id}", delete(trash::trash_document))
        .route("/items/{item_id}", delete(trash::trash_item))
        .route("/modules/{module_id}", delete(trash::trash_module))
        .route(
            "/workspaces/{workspace_id}/trash",
            get(trash::workspace_trash),
        )
        .route("/me/trash", get(trash::my_trash))
        .route("/trash/{batch_id}/restore", post(trash::restore))
        .route("/trash/{batch_id}", delete(trash::purge))
        .route(
            "/workspaces/{workspace_id}/trash/purge-expired",
            post(trash::purge_expired),
        )
        .route("/me/trash/purge-expired", post(trash::purge_my_expired))
        .route(
            "/workspaces/{workspace_id}/trash-retention",
            put(trash::set_retention),
        )
        .route(
            "/workspaces/{workspace_id}/version-retention",
            put(trash::set_version_retention),
        )
        .route("/workspaces/{workspace_id}/export", get(export::workspace))
        .route("/me/export", get(export::mine))
}
