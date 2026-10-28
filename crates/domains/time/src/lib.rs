//! 개인 시간 기록 도메인. 라우터와 OpenAPI를 조립 crate에 제공한다.

pub mod time_tracking;

use axum::routing::{get, post, put};
use axum::{Router, extract::FromRef};
use fvoci_http::session::SessionState;
use utoipa::OpenApi;

/// 시간 처리기에 필요한 DB 상태.
#[derive(Debug, Clone)]
pub struct TimeState {
    pub db: fvoci_db::Db,
}

#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        time_tracking::current,
        time_tracking::start,
        time_tracking::action,
        time_tracking::list,
        time_tracking::add,
        time_tracking::update,
        time_tracking::delete,
        time_tracking::revisions,
        time_tracking::summary,
        time_tracking::task_time,
        time_tracking::item_time,
    ),
    components(schemas(
        time_tracking::TimeKind,
        time_tracking::TargetKind,
        time_tracking::TargetInput,
        time_tracking::Target,
        time_tracking::TimeEntry,
        time_tracking::StartTimer,
        time_tracking::TimerStartMode,
        time_tracking::StartedTimer,
        time_tracking::TimerAction,
        time_tracking::TimerActionInput,
        time_tracking::EntryInput,
        time_tracking::AddEntry,
        time_tracking::UpdateEntry,
        time_tracking::TimeEntryConflict,
        time_tracking::TimeEntryRevision,
        time_tracking::SummaryGroup,
        time_tracking::TimeSummaryRow,
        time_tracking::TaskTime,
        time_tracking::ItemTime,
        time_tracking::ItemTimeKind,
    ))
)]
pub struct TimeApi;

/// 시간 경로(/api 아래).
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    TimeState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route(
            "/me/timer",
            get(time_tracking::current).post(time_tracking::start),
        )
        .route(
            "/time-entries/{entry_id}/timer",
            post(time_tracking::action),
        )
        .route(
            "/me/time-entries",
            get(time_tracking::list).post(time_tracking::add),
        )
        .route(
            "/time-entries/{entry_id}",
            put(time_tracking::update).delete(time_tracking::delete),
        )
        .route(
            "/time-entries/{entry_id}/revisions",
            get(time_tracking::revisions),
        )
        .route("/me/time-summary", get(time_tracking::summary))
        .route("/tasks/{task_id}/time", get(time_tracking::task_time))
        .route("/items/{item_id}/time", get(time_tracking::item_time))
}
