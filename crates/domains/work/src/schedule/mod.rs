//! 마일스톤·선행 관계·후속 재배치 미리보기 (D-31, D-33, D-91).
//!
//! 선행 관계는 같은 항목 안의 업무 쌍마다 하나이고 종류(FS·SS·FF·SF)와 지연일(0 이상, 달력 날짜)을 가진다. 날짜나
//! 관계를 바꾸기 전에 미리보기로 영향받는 후속 업무와 제안 날짜, 그 원인 관계를 계산해 보여 주고, 사용자가 승인한
//! 목록만 같은 트랜잭션에서 옮긴다. 제안은 바뀐 것(옮겨진 선행 또는 고친 관계)이 전보다 더 요구하는 만큼만 뒤로 미는
//! 최소 이동이며 기간(시작~마감)을 유지한다. 앞당긴 경우에는 제안하지 않는다. 승인할 때는 잠근 뒤 같은 계산을 다시
//! 해서 승인 목록과 다르면(그사이 관계·날짜·완료가 바뀜) 아무것도 바꾸지 않는다.
//! 관계의 종류·지연일과 앱 공급 UUIDv7(D-88)은
//! `crates/db/migrations/202610280005_work.sql`의 `app_set_task_dependency` 등이 처리한다.
//!
//! 나눔: `planner` 계산(DB 없음), `preview` 미리보기 읽기, `links` 관계 명령.

mod links;
mod planner;
mod preview;

pub use links::{
    __path_add_predecessor, __path_remove_predecessor, LinkBase, LinkInput, PredecessorConflict,
    SetPredecessor, TaskLink, add_predecessor, remove_predecessor,
};
pub(crate) use links::{approved_shifts, approved_still_planned, spans_before};
pub use planner::{
    Change, Link, LinkKind, MAX_LAG_DAYS, Plan, PlanBlockReason, Planned, Span, plan_shifts,
};
pub use preview::{
    __path_plan, __path_plan_link, PlanBlocked, SchedulePlan, SchedulePreviewInput, ShiftCause,
    ShiftProposal, UndatedTask, plan, plan_link,
};
