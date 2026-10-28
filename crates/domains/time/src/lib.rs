//! 개인 시간 기록 도메인. 라우터와 OpenAPI를 조립 crate에 제공한다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod commands;
pub mod comparison;
pub mod model;
pub mod range;
pub mod read;
pub mod summary;
pub mod targets;
pub mod timer;

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
        timer::current,
        timer::start,
        timer::action,
        read::list,
        commands::add,
        commands::update,
        commands::delete,
        read::revisions,
        summary::summary,
        comparison::task_time,
        comparison::item_time,
    ),
    components(schemas(
        model::TimeKind,
        targets::TargetKind,
        targets::TargetInput,
        targets::Target,
        model::TimeEntry,
        timer::StartTimer,
        timer::TimerStartMode,
        timer::StartedTimer,
        timer::TimerAction,
        timer::TimerActionInput,
        commands::EntryInput,
        commands::AddEntry,
        commands::UpdateEntry,
        commands::TimeEntryConflict,
        read::TimeEntryRevision,
        summary::SummaryGroup,
        summary::TimeSummaryRow,
        comparison::TaskTime,
        comparison::ItemTime,
        comparison::ItemTimeKind,
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
        .route("/me/timer", get(timer::current).post(timer::start))
        .route("/time-entries/{entry_id}/timer", post(timer::action))
        .route("/me/time-entries", get(read::list).post(commands::add))
        .route(
            "/time-entries/{entry_id}",
            put(commands::update).delete(commands::delete),
        )
        .route("/time-entries/{entry_id}/revisions", get(read::revisions))
        .route("/me/time-summary", get(summary::summary))
        .route("/tasks/{task_id}/time", get(comparison::task_time))
        .route("/items/{item_id}/time", get(comparison::item_time))
}
