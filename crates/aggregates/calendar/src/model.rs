//! 달력 집계의 응답 타입.

use fvoci_core::access::Grade as Access;
use fvoci_core::ids::{ItemId, WorkspaceId};
use fvoci_core::recurrence::RecurrenceRule;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

/// 일정 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Meeting,
    Presentation,
    Event,
}

impl EventKind {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "meeting" => Some(Self::Meeting),
            "presentation" => Some(Self::Presentation),
            "event" => Some(Self::Event),
            _ => None,
        }
    }
}

/// 일정.
#[derive(Debug, Serialize, ToSchema)]
pub struct Event {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub workspace_id: WorkspaceId,
    pub kind: EventKind,
    pub title: String,
    pub description: String,
    /// 종일이면 `start_date`·`end_date`(끝 포함), 아니면 `starts_at`·`ends_at`.
    pub all_day: bool,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub starts_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ends_at: Option<OffsetDateTime>,
    /// 일정에 지정한 시간대(IANA). 반복 계산의 기준이다.
    pub timezone: String,
    /// 반복 규칙(없으면 한 번). 회차는 달력에서 펼친다.
    pub recurrence: Option<RecurrenceRule>,
    /// 뺀 회차(일정 시간대 기준 날짜).
    pub excluded_dates: Vec<String>,
    pub version: i32,
    pub access: Access,
}

/// 업무·일정 밖의 날짜 출처(D-80). 원래 자리의 날짜를 그대로 읽는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CalendarDateKind {
    /// 강의 회차 수강 예정일(내 계획일, 없으면 공통 계획일).
    Episode,
    /// 스터디 계획 단위 계획일(내 계획일, 없으면 공통 계획일).
    StudyUnit,
    /// 연구 투고 제출 예정일.
    Submission,
    /// 연구 투고 수정 마감일.
    RevisionDue,
    /// 프로젝트 산출물 마감일.
    Deliverable,
}

impl CalendarDateKind {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "episode" => Some(Self::Episode),
            "study_unit" => Some(Self::StudyUnit),
            "submission" => Some(Self::Submission),
            "revision_due" => Some(Self::RevisionDue),
            "deliverable" => Some(Self::Deliverable),
            _ => None,
        }
    }
}

/// 달력의 날짜 하나(D-80).
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarDate {
    pub kind: CalendarDateKind,
    /// 원래 대상(회차·스터디 단위·기록)의 id.
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub title: String,
    pub date: String,
    /// 내 계획일이면 참(본인만 보는 개인 일정), 공통 계획일·기록 날짜면 거짓.
    pub personal: bool,
    /// 끝난 것(내가 이수·완료, 게재·거절·승인·완료 상태).
    pub done: bool,
}

/// 달력의 업무(시작일~마감일).
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarTask {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub reference: Option<String>,
    pub title: String,
    pub status: String,
    pub status_category: String,
    pub milestone: bool,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 달력에 나온 항목의 이름(업무·일정이 어디에 속하는지 보이기 위해).
#[derive(Debug, Serialize, ToSchema)]
pub struct CalendarItem {
    pub id: ItemId,
    pub reference: Option<String>,
    pub title: String,
}

/// 일정의 회차 하나(반복이 아니면 그 일정 자체). 종일이면 날짜, 아니면 UTC 시각.
#[derive(Debug, Serialize, ToSchema)]
pub struct EventOccurrence {
    #[schema(value_type = String, format = Uuid)]
    pub event_id: Uuid,
    /// 일정 시간대 기준의 회차 날짜(회차 빼기에 쓴다).
    pub occurrence: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub starts_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub ends_at: Option<OffsetDateTime>,
    /// 반복 회차의 현지 시각이 그 시간대에 없어(여름시간 시작) 건너뛴 회차(D-81). 시각은 없다.
    pub skipped: bool,
}

/// 아직 만들지 않은 반복 업무 회차(예정). 읽기 전용 계산이며 쓰기 권한이 있는 사람이 항목을 열면 실제로 만든다.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProjectedTask {
    #[schema(value_type = String, format = Uuid)]
    pub series_id: Uuid,
    /// 반복의 첫 회차(틀).
    #[schema(value_type = String, format = Uuid)]
    pub root_task_id: Uuid,
    pub item_id: ItemId,
    pub title: String,
    pub milestone: bool,
    pub occurrence: String,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 기간 안의 볼 수 있는 업무·일정.
#[derive(Debug, Serialize, ToSchema)]
pub struct Calendar {
    pub tasks: Vec<CalendarTask>,
    pub projected_tasks: Vec<ProjectedTask>,
    /// 기간 안에 회차가 있는 일정의 정의.
    pub events: Vec<Event>,
    pub occurrences: Vec<EventOccurrence>,
    /// 강의·스터디 계획일, 투고·산출물 날짜(D-80).
    pub dates: Vec<CalendarDate>,
    pub items: Vec<CalendarItem>,
}
