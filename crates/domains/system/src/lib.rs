//! 휴지통·ZIP 반출·자동 정리 도메인. 일정 실행은 조립 crate가 맡는다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod cleanup;
pub mod export;
pub mod state;
pub mod trash;

use axum::middleware::from_extractor_with_state;
use axum::{
    Router,
    extract::FromRef,
    routing::{delete, get, post, put},
};
use fvoci_http::session::{ExpectedUserIfSent, SessionState};
pub use state::SystemState;
use utoipa::OpenApi;

#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        trash::trash_document,
        trash::trash_item,
        trash::trash_module,
        trash::list,
        trash::retention,
        trash::restore,
        trash::purge,
        trash::purge_expired,
        trash::set_retention,
        trash::set_version_retention,
        export::workspace,
        export::mine,
    ),
    components(schemas(
        trash::Trashed,
        trash::TrashKind,
        trash::TrashEntry,
        trash::RetentionSettings,
        trash::TrashExpiry,
        trash::Purged,
        trash::SetRetention,
        trash::SetVersionRetention,
    ))
)]
pub struct SystemApi;

/// 시스템 경로. 모든 명령에서 보낸 기대 사용자를 비교한다(D-96).
pub fn router<S>(state: &S) -> Router<S>
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
            "/workspaces/{workspace_id}/retention",
            get(trash::retention),
        )
        .route("/trash-batches", get(trash::list))
        .route("/trash-batches/{batch_id}/restore", post(trash::restore))
        .route("/trash-batches/{batch_id}", delete(trash::purge))
        .route("/trash-batches/purge-expired", post(trash::purge_expired))
        .route(
            "/workspaces/{workspace_id}/retention/trash",
            put(trash::set_retention),
        )
        .route(
            "/workspaces/{workspace_id}/retention/file-versions",
            put(trash::set_version_retention),
        )
        .route("/workspaces/{workspace_id}/export", get(export::workspace))
        .route("/me/export", get(export::mine))
        .route_layer(from_extractor_with_state::<ExpectedUserIfSent, S>(
            state.clone(),
        ))
}
