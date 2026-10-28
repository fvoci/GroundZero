//! 홈·내 업무 집계. 도메인 crate에 기대지 않고 DB와 공통 HTTP 문맥을 사용한다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod dashboard;
pub mod progress;
pub mod tasks;

use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_extractor_with_state;
use axum::routing::get;
use fvoci_db::Db;
use fvoci_http::session::{ExpectedUserIfSent, SessionState};
use utoipa::OpenApi;

/// 집계 읽기에 필요한 상태.
#[derive(Debug, Clone)]
pub struct HomeState {
    pub db: Db,
}

/// 홈·내 업무 API 계약.
#[derive(Debug, OpenApi)]
#[openapi(
    paths(dashboard::home, tasks::my_tasks),
    components(schemas(
        dashboard::DueBucket,
        dashboard::HomeTask,
        dashboard::HomeDocument,
        dashboard::HomeItem,
        dashboard::MarkTarget,
        dashboard::MarkedTarget,
        dashboard::Home,
        dashboard::UpNextCounts,
        progress::HomeProgress,
        progress::StudyAmount,
        progress::StudyUnitKind,
        tasks::MyTask,
        tasks::MyTaskCounts,
        tasks::WorkspaceTasks,
        tasks::MyTasks,
    ))
)]
pub struct HomeApi;

/// 집계 읽기 경로. 조립 crate가 상태를 주입한다.
pub fn router<S>(state: &S) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    HomeState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route("/me/home", get(dashboard::home))
        .route("/me/tasks", get(tasks::my_tasks))
        .route_layer(from_extractor_with_state::<ExpectedUserIfSent, S>(
            state.clone(),
        ))
}
