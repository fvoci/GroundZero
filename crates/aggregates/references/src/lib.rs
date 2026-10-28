//! 여러 영역의 참조 후보와 일괄 해석을 모으는 집계. 도메인 crate에 기대지 않는다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod handlers;
pub mod state;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::{get, post};
use fvoci_http::session::SessionState;
use utoipa::OpenApi;

pub use state::ReferencesState;

#[derive(Debug, OpenApi)]
#[openapi(
    paths(handlers::candidates, handlers::resolve),
    components(schemas(
        handlers::ReferenceKind,
        handlers::ReferenceCandidate,
        handlers::ReferenceInput,
        handlers::ResolveReferences,
        handlers::ResolvedReference,
    ))
)]
pub struct ReferencesApi;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    ReferencesState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/reference-candidates", get(handlers::candidates))
        .route("/references/resolve", post(handlers::resolve))
}
