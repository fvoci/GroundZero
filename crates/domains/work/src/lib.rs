//! 업무 도메인: 업무·하위 업무·담당자·체크리스트, 항목별 상태 정의, 선행 관계와 재배치 미리보기, 반복 업무·일정·저장한 보기
//! (노르웨이숲 태비 초안 2·5). 라우터와 OpenAPI 조각을 내고, 조립 crate(`fvoci-server`)가 합친다(초안 3).
//! 처리기는 부분 상태 [`WorkState`]를 조립 crate의 상태에서 `FromRef`로 꺼낸다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod event_recurrence;
pub mod events;
pub mod recurrence;
pub mod saved_views;
pub mod schedule;
pub mod state;
pub mod task_statuses;
pub mod tasks;

use axum::Router;
use axum::extract::FromRef;
use axum::middleware::from_extractor_with_state;
use axum::routing::{get, post, put};
use fvoci_http::session::{ExpectedUserIfSent, SessionState};
use utoipa::OpenApi;

pub use state::WorkState;

/// 업무 API 계약 조각.
#[derive(Debug, OpenApi)]
#[openapi(
    paths(
        events::list,
        events::get,
        events::create,
        events::update,
        events::delete,
        event_recurrence::set_event,
        event_recurrence::exclude_event,
        saved_views::list,
        saved_views::create,
        saved_views::update,
        saved_views::delete,
        task_statuses::list,
        task_statuses::create,
        task_statuses::update,
        task_statuses::delete,
        tasks::list,
        tasks::assignable,
        tasks::get,
        tasks::create,
        tasks::move_task,
        tasks::set_parent,
        tasks::add_checklist,
        tasks::update_checklist_item,
        tasks::delete_checklist_item,
        schedule::plan,
        schedule::plan_link,
        schedule::add_predecessor,
        schedule::remove_predecessor,
        recurrence::set_task,
        recurrence::sync_item,
        tasks::trash_task,
        tasks::patch,
        tasks::assignees,
        tasks::dependencies,
        tasks::series,
        tasks::checklist_items,
    ),
    components(schemas(
        event_recurrence::SetEventRecurrence,
        event_recurrence::ExcludeOccurrence,
        events::EventKind,
        events::Event,
        events::EventInput,
        events::CreateEvent,
        events::UpdateEvent,
        events::EventWriteConflict,
        saved_views::SavedView,
        saved_views::CreateView,
        saved_views::UpdateView,
        saved_views::ViewNameConflict,
        saved_views::ViewCreateConflict,
        saved_views::ViewWriteConflict,
        tasks::TaskPriority,
        tasks::Assignee,
        tasks::ChecklistItem,
        tasks::Task,
        tasks::TaskAssignee,
        tasks::TaskDependency,
        tasks::PatchTask,
        tasks::TaskPatchConflict,
        tasks::TaskFieldConflict,
        tasks::TaskConflictCurrent,
        tasks::CreateTask,
        tasks::MoveTask,
        tasks::SetTaskParent,
        tasks::NewChecklistItem,
        tasks::AddChecklist,
        tasks::UpdateChecklistItem,
        tasks::SuccessorShift,
        schedule::SchedulePreviewInput,
        schedule::ShiftProposal,
        schedule::ShiftCause,
        schedule::SchedulePlan,
        schedule::PlanBlocked,
        schedule::PlanBlockReason,
        schedule::UndatedTask,
        schedule::LinkKind,
        schedule::LinkInput,
        schedule::SetPredecessor,
        schedule::LinkBase,
        schedule::DependencyWriteConflict,
        recurrence::RuleWriteConflict,
        recurrence::Frequency,
        recurrence::RecurrenceRule,
        recurrence::SetTaskRecurrence,
        recurrence::SyncResult,
        tasks::TaskSeries,
        task_statuses::StatusCategory,
        task_statuses::TaskStatus,
        task_statuses::NewTaskStatus,
        task_statuses::UpdateTaskStatus,
        task_statuses::DeletedTaskStatus,
    ))
)]
pub struct WorkApi;

/// 업무 경로(`/api` 아래). 조립 crate의 상태 `S`에서 [`WorkState`]와 세션 상태를 꺼낸다. 모든 명령에 기대 사용자
/// 머리가 있으면 비교한다(D-96 2026-10-28 개정 (가)). 머리가 필수인 업무 쓰기는 처리기가 `ExpectedUser`로 따로 받는다.
pub fn router<S>(state: &S) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    WorkState: FromRef<S>,
    SessionState: FromRef<S>,
{
    Router::new()
        .route(
            "/items/{item_id}/statuses",
            get(task_statuses::list).post(task_statuses::create),
        )
        .route(
            "/items/{item_id}/statuses/{status_id}",
            put(task_statuses::update).delete(task_statuses::delete),
        )
        .route(
            "/items/{item_id}/tasks",
            get(tasks::list).post(tasks::create),
        )
        .route("/items/{item_id}/assignable", get(tasks::assignable))
        .route(
            "/tasks/{task_id}",
            get(tasks::get)
                .patch(tasks::patch)
                .delete(tasks::trash_task),
        )
        .route("/tasks/{task_id}/position", put(tasks::move_task))
        .route("/tasks/{task_id}/parent", put(tasks::set_parent))
        .route("/tasks/{task_id}/schedule-plan", post(schedule::plan))
        .route(
            "/tasks/{task_id}/predecessors/{predecessor_id}/schedule-plan",
            post(schedule::plan_link),
        )
        .route("/tasks/{task_id}/recurrence", put(recurrence::set_task))
        .route(
            "/items/{item_id}/recurrences/sync",
            post(recurrence::sync_item),
        )
        .route(
            "/tasks/{task_id}/predecessors/{predecessor_id}",
            put(schedule::add_predecessor).delete(schedule::remove_predecessor),
        )
        .route("/task-assignees", get(tasks::assignees))
        .route("/task-dependencies", get(tasks::dependencies))
        .route("/task-series", get(tasks::series))
        .route("/checklist-items", get(tasks::checklist_items))
        .route("/tasks/{task_id}/checklist", post(tasks::add_checklist))
        .route(
            "/checklist-items/{item_id}",
            put(tasks::update_checklist_item).delete(tasks::delete_checklist_item),
        )
        .route(
            "/items/{item_id}/views",
            get(saved_views::list).post(saved_views::create),
        )
        .route(
            "/views/{view_id}",
            put(saved_views::update).delete(saved_views::delete),
        )
        .route(
            "/items/{item_id}/events",
            get(events::list).post(events::create),
        )
        .route(
            "/events/{event_id}",
            get(events::get).put(events::update).delete(events::delete),
        )
        .route(
            "/events/{event_id}/recurrence",
            put(event_recurrence::set_event),
        )
        .route(
            "/events/{event_id}/exclusions",
            post(event_recurrence::exclude_event),
        )
        .route_layer(from_extractor_with_state::<ExpectedUserIfSent, S>(
            state.clone(),
        ))
}
