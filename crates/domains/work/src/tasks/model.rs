//! 업무 자원과 명령의 모양, 칸 검증(만들기·고치기가 함께 쓰는 규칙).

use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::recurrence::RecurrenceRule;
use crate::schedule::TaskLink;
use crate::state::WorkState;
use fvoci_core::access::Grade as Access;
use fvoci_core::error::{AppError, ErrorBody, InvalidReason};
use fvoci_core::ids::{ItemId, ModuleId, UserId, WorkspaceId};
use fvoci_core::validate::{parse_date, validate_text, validate_title};

/// 설명 글자 수 상한.
const MAX_DESCRIPTION_CHARS: usize = 10_000;
/// 태그 수·길이 상한.
const MAX_TAGS: usize = 20;
const MAX_TAG_CHARS: usize = 50;
/// 체크리스트 한 번에 덧붙일 수 있는 항목 수.
pub(super) const MAX_CHECKLIST_BATCH: usize = 200;

/// 업무 쓰기의 기대 사용자 머리 계약(OpenAPI).
pub(crate) const EXPECTED_TASK_USER_DOC: &str = "이 업무 편집을 시작한 사용자 id(필수). 세션의 사용자와 다르면 아무것도 바꾸지 않고 409 account_changed, 없거나 형식이 틀리면 400. 신원과 권한은 세션으로만 정한다";

/// 상태 이름은 앞뒤 공백을 제거하고 1..50자로 제한한다.
pub(crate) fn validate_status(raw: &str) -> Result<String, AppError> {
    validate_title(raw, 50, "status")
}

/// 우선순위.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    #[default]
    None,
    Low,
    Medium,
    High,
    Urgent,
}

impl TaskPriority {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Urgent => "urgent",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "none" => Some(Self::None),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "urgent" => Some(Self::Urgent),
            _ => None,
        }
    }
}

/// 담당자.
#[derive(Debug, Serialize, ToSchema)]
pub struct Assignee {
    pub user_id: UserId,
    pub display_name: String,
}

/// 체크리스트 항목.
#[derive(Debug, Serialize, ToSchema)]
pub struct ChecklistItem {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub text: String,
    pub checked: bool,
}

/// 업무.
#[derive(Debug, Serialize, ToSchema)]
pub struct Task {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub item_id: ItemId,
    pub module_id: ModuleId,
    pub workspace_id: WorkspaceId,
    /// 하위 업무면 상위 업무.
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
    /// 예: `DEV-5`.
    pub reference: Option<String>,
    pub title: String,
    pub description: String,
    pub status: String,
    pub status_category: String,
    pub priority: TaskPriority,
    /// `YYYY-MM-DD`. 시각 없는 날짜.
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    pub estimate_minutes: Option<i32>,
    pub tags: Vec<String>,
    /// 마일스톤이면 마감일 하나만 쓴다.
    pub milestone: bool,
    /// 선행 관계: 선행 업무, 종류(FS·SS·FF·SF), 지연일(D-91). 후속 업무는 다른 업무의 이 목록을 거꾸로 찾는다.
    pub predecessors: Vec<TaskLink>,
    /// 반복 업무의 회차면 그 반복.
    pub series: Option<TaskSeries>,
    /// 반복 회차의 날짜(규칙 기준).
    pub occurrence_date: Option<String>,
    pub assignees: Vec<Assignee>,
    pub checklist_done: i64,
    pub checklist_total: i64,
    /// 필드 고치기의 기준 버전.
    pub version: i32,
    /// 이동 판(D-89): 열이나 열 안 위치가 바뀔 때마다 오른다. 조건부 이동(`base_move_version`)의 기준.
    pub move_version: i32,
    #[serde(with = "time::serde::rfc3339::option")]
    #[schema(value_type = Option<String>, format = DateTime)]
    pub completed_at: Option<OffsetDateTime>,
    pub access: Access,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
}

/// 반복 업무 정보.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskSeries {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    /// 첫 회차(규칙은 여기서 바꾼다).
    #[schema(value_type = String, format = Uuid)]
    pub root_task_id: Uuid,
    /// 멈췄으면 없다.
    pub rule: Option<RecurrenceRule>,
    pub timezone: String,
}

/// 업무와 체크리스트.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: Task,
    pub checklist: Vec<ChecklistItem>,
}

/// 업무 만들기. 제목 밖의 칸은 고치기(`UpdateTask`)와 같고 같은 규칙으로 확인해 한 트랜잭션에서 넣는다(D-105): 하나라도
/// 거절되면 업무도 담당도 남지 않는다. 빠진 칸은 새 업무의 기본값(설명 없음, 할 일, 우선순위 없음, 날짜·예상·태그·담당자
/// 없음, 이정표 아님)이다. 같은 `request_id`는 처음 만든 업무를 지금 상태로 돌려준다(보낸 칸이 달라도 다시 쓰지 않는다).
/// 그 업무가 휴지통에 있으면 409 `task_deleted`(새 요청 id로 보낸다).
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateTask {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    /// 제목. 글자 수 상한은 서버 설정이다(`GET /api/me`의 `limits.max_title_chars`). 넘으면 400 `reason: too_long`과 `limit`.
    pub title: String,
    /// 하위 업무로 만들 때 상위 업무(같은 항목, 한 단계).
    #[serde(default)]
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
    #[serde(default)]
    #[schema(max_length = 10000)]
    pub description: String,
    /// 상태 이름(항목이 정의한 문자열). 비우면 항목의 기본 상태(todo 범주 첫 상태)다(개정 2026-10-26 ⑪: 기본 이름은 서버가 정하지 않는다).
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: TaskPriority,
    #[serde(default)]
    pub start_date: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    #[schema(minimum = 0)]
    pub estimate_minutes: Option<i32>,
    #[serde(default)]
    #[schema(max_items = 20)]
    pub tags: Vec<String>,
    /// 담당자(그 항목을 볼 수 있는 멤버).
    #[serde(default)]
    pub assignee_ids: Vec<UserId>,
    /// 마일스톤이면 시작일 없이 마감일만.
    #[serde(default)]
    pub milestone: bool,
}

/// 업무 고치기. `base_version`이 지났으면 409와 서버 최신본.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateTask {
    pub base_version: i32,
    /// 제목. 글자 수 상한은 서버 설정이다(`GET /api/me`의 `limits.max_title_chars`). 넘으면 400 `reason: too_long`과 `limit`.
    pub title: String,
    #[schema(max_length = 10000)]
    pub description: String,
    pub status: String,
    pub priority: TaskPriority,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
    #[schema(minimum = 0)]
    pub estimate_minutes: Option<i32>,
    #[schema(max_items = 20)]
    pub tags: Vec<String>,
    /// 담당자 전체 목록(빠지면 지우는 것이므로 생략할 수 없다).
    pub assignee_ids: Vec<UserId>,
    /// 마일스톤이면 시작일 없이 마감일만.
    pub milestone: bool,
    /// 미리보기에서 사용자가 승인한 후속 업무 재배치. 같은 트랜잭션에서 함께 옮기며, 하나라도 기준 버전이
    /// 지났으면 아무것도 바꾸지 않고 409(`shift_conflict`).
    #[serde(default)]
    #[schema(max_items = 500)]
    pub successor_shifts: Vec<SuccessorShift>,
}

/// 승인한 후속 업무 재배치 한 건. 날짜가 있던 칸만 옮긴다.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct SuccessorShift {
    #[schema(value_type = String, format = Uuid)]
    pub task_id: Uuid,
    pub base_version: i32,
    pub start_date: Option<String>,
    pub due_date: Option<String>,
}

/// 한 번에 옮길 수 있는 후속 업무 수.
pub(crate) const MAX_SHIFTS: usize = 500;

/// 칸반 이동: 열(상태)과 그 안에서 바로 뒤에 올 업무(없으면 맨 끝).
#[derive(Debug, Deserialize, ToSchema)]
pub struct MoveTask {
    pub status: String,
    #[schema(value_type = Option<String>, format = Uuid)]
    pub before_id: Option<Uuid>,
    /// 이 이동 판(`move_version`)에서 읽은 업무만 옮긴다(D-89). 그사이 열·위치가 바뀌었으면 아무것도 바꾸지 않고
    /// 409 `move_conflict`. 없으면 판과 상관없이 옮긴다(D-79).
    #[serde(default)]
    pub base_move_version: Option<i32>,
}

/// 상위 업무 바꾸기. 없으면 최상위.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetTaskParent {
    #[schema(value_type = Option<String>, format = Uuid)]
    pub parent_id: Option<Uuid>,
}

/// 덧붙일 체크리스트 항목.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewChecklistItem {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    /// 항목 글. 상한은 제목과 같다(`limits.max_title_chars`).
    pub text: String,
    #[serde(default)]
    pub checked: bool,
}

/// 체크리스트 덧붙이기(Markdown에서 펼친 항목을 사용자가 확인한 뒤 한 번에).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddChecklist {
    #[schema(max_items = 200)]
    pub items: Vec<NewChecklistItem>,
}

/// 체크리스트 항목 고치기. 보낸 칸만 바꾼다(빠진 칸은 그대로). 둘 다 없으면 400.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateChecklistItem {
    #[serde(default)]
    /// 항목 글. 상한은 제목과 같다(`limits.max_title_chars`).
    pub text: Option<String>,
    #[serde(default)]
    pub checked: Option<bool>,
}

/// 409 응답: 서버 최신본.
#[derive(Debug, Serialize, ToSchema)]
pub struct TaskConflict {
    pub error: &'static str,
    pub current: TaskDetail,
}

/// 업무 쓰기의 409 본문: 최신 업무를 포함한 충돌 또는 계정 변경 등 일반 오류.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum TaskWriteConflict {
    /// 업무·이동·반복 규칙의 기준이 오래됨.
    Stale(Box<TaskConflict>),
    /// 기대 사용자가 다르거나 현재 상태에서 쓰기를 할 수 없음.
    State(ErrorBody),
}

/// 만들기·고치기가 함께 쓰는 칸 확인(같은 규칙). 상태·우선순위 값, 시작일 ≤ 마감일, 이정표, 담당자는 DB 함수가 본다.
pub(super) struct CheckedFields {
    pub(super) title: String,
    pub(super) description: String,
    pub(super) start: Option<Date>,
    pub(super) due: Option<Date>,
    pub(super) tags: Vec<String>,
}

pub(super) fn check_fields(
    state: &WorkState,
    title: &str,
    description: &str,
    start_date: Option<&str>,
    due_date: Option<&str>,
    estimate_minutes: Option<i32>,
    tags: &[String],
) -> Result<CheckedFields, AppError> {
    let title = validate_title(title, state.max_title_chars, "title")?;
    let description = validate_text(description, MAX_DESCRIPTION_CHARS, "description")?;
    let start = parse_date(start_date, "start_date")?;
    let due = parse_date(due_date, "due_date")?;
    if estimate_minutes.is_some_and(|m| m < 0) {
        return Err(AppError::invalid(
            "estimate_minutes",
            InvalidReason::OutOfRange,
            None,
            "estimate_minutes must not be negative",
        ));
    }
    let tags = validate_tags(tags)?;
    Ok(CheckedFields {
        title,
        description,
        start,
        due,
        tags,
    })
}

fn validate_tags(tags: &[String]) -> Result<Vec<String>, AppError> {
    if tags.len() > MAX_TAGS {
        return Err(AppError::invalid(
            "tags",
            InvalidReason::TooMany,
            Some(MAX_TAGS as u64),
            format!("at most {MAX_TAGS} tags"),
        ));
    }
    let mut out: Vec<String> = Vec::new();
    for tag in tags {
        let tag = validate_title(tag, MAX_TAG_CHARS, "tag")?;
        if !out.contains(&tag) {
            out.push(tag);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_trimmed_deduplicated_and_limited() {
        let tags = validate_tags(&[" 설계 ".into(), "설계".into(), "검토".into()]).unwrap();
        assert_eq!(tags, vec!["설계", "검토"]);
        let many: Vec<String> = (0..=MAX_TAGS).map(|i| i.to_string()).collect();
        assert!(validate_tags(&many).is_err());
    }

    #[test]
    fn status_and_priority_round_trip() {
        for s in ["todo", "in_progress", "done", "검토 중"] {
            assert_eq!(validate_status(&format!(" {s} ")).unwrap(), s);
        }
        assert!(validate_status(" ").is_err());
        assert!(validate_status(&"가".repeat(51)).is_err());
        for p in [TaskPriority::None, TaskPriority::Urgent] {
            assert_eq!(TaskPriority::parse(p.as_str()), Some(p));
        }
    }
}
