//! 달력 집계. 업무·일정·학습·기록의 날짜를 읽으며 도메인 crate에 의존하지 않는다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

mod calendar;
mod events;
pub mod model;
mod response;
mod scope;
mod sources;

use axum::Router;
use axum::extract::FromRef;
use axum::routing::get;
use fvoci_db::Db;
use fvoci_http::session::SessionState;
use utoipa::OpenApi;

/// 달력 읽기에 필요한 연결 풀.
#[derive(Debug, Clone)]
pub struct CalendarState {
    pub db: Db,
}

/// 달력 API 계약 조각.
#[derive(Debug, OpenApi)]
#[openapi(
    paths(calendar::calendar),
    components(schemas(
        model::CalendarTask,
        model::CalendarItem,
        model::CalendarDate,
        model::CalendarDateKind,
        model::Calendar,
        model::EventOccurrence,
        model::ProjectedTask,
        model::Event,
        model::EventKind,
    ))
)]
pub struct CalendarApi;

/// 달력 경로(`/api` 아래).
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    CalendarState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new().route(
        "/workspaces/{workspace_id}/calendar",
        get(calendar::calendar),
    )
}
