//! 통합 검색 집계와 색인 작업 본문. 일정과 텍스트 추출기는 조립 crate가 제공한다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod api;
mod client;
mod filters;
pub mod indexing;
mod visibility;

use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_extractor_with_state;
use axum::routing::get;
use fvoci_db::Db;
use fvoci_http::session::{ExpectedUserIfSent, SessionState};
use serde_json::Value;
use utoipa::OpenApi;

/// 검색 처리기가 사용하는 부분 상태.
#[derive(Debug, Clone)]
pub struct SearchState {
    pub db: Db,
    pub search: Option<indexing::SearchClient>,
}

/// 공통 추출기의 소유권은 조립 crate에 두고 색인 작업에 필요한 연산만 주입한다.
pub trait TextExtractor: Sync {
    fn plain_text(&self, body: &Value) -> String;
    fn file_text(
        &self,
        key: Option<&str>,
        name: Option<&str>,
        content_type: Option<&str>,
        size: Option<i64>,
    ) -> impl Future<Output = Option<String>> + Send;
}

/// 검색 API 계약 조각.
#[derive(Debug, OpenApi)]
#[openapi(paths(api::search), components(schemas(api::SearchResult)))]
pub struct SearchApi;

/// 검색 경로(`/api` 아래).
pub fn router<S>(state: &S) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    SearchState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/search", get(api::search))
        .route_layer(from_extractor_with_state::<ExpectedUserIfSent, S>(
            state.clone(),
        ))
}
