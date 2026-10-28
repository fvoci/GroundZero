//! 학습 자원과 명령 입력.

use fvoci_core::ids::UserId;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;
time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 내 진행.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyUnitProgress {
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    /// 한 분량(단위의 분량과 같은 단위).
    pub done_amount: i32,
    /// 내 계획일(없으면 공통 계획일).
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub completed_on: Option<Date>,
    /// 개인 메모(나만 봄).
    pub note: String,
}

/// 계획 단위.
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyUnit {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub number: i32,
    pub title: String,
    /// 교육과정·챕터·단계.
    pub chapter: String,
    /// `pages`·`problems`·`minutes`·`items`.
    pub unit: String,
    pub amount: i32,
    /// 공통 계획일.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    pub mine: MyUnitProgress,
    /// 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409).
    pub version: i32,
}

/// 스터디 계획.
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyPlan {
    pub units: Vec<StudyUnit>,
    /// 단위를 고칠 수 있는지(진행은 볼 수 있으면 누구나 자기 것).
    pub can_write: bool,
}

/// 추가할 단위.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewStudyUnit {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub number: Option<i32>,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    #[serde(default)]
    pub amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

fn default_unit() -> String {
    "pages".into()
}

/// 단위 여러 개 추가(최대 1000개).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddStudyUnits {
    pub units: Vec<NewStudyUnit>,
}

/// 추가한 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnitsAdded {
    pub added: i32,
}

/// 단위 고치기. `version`은 읽은 판.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateStudyUnit {
    pub version: i32,
    pub number: i32,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    pub unit: String,
    #[serde(default)]
    pub amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 내 진행 바꾸기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetUnitProgress {
    pub status: String,
    #[serde(default)]
    pub done_amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    #[serde(default)]
    pub note: String,
    /// 사용자 시간대의 오늘(완료한 날).
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub today: Option<Date>,
}

/// 내 계획일 하나.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct PlanEntry {
    #[schema(value_type = String, format = Uuid)]
    pub unit_id: Uuid,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 내 계획일 여러 개(밀린 단위 재배치). 날짜는 사용자가 정한다.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PlanUnits {
    pub entries: Vec<PlanEntry>,
}

/// 바꾼 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnitsPlanned {
    pub planned: i32,
}

/// 함께 하는 사람의 진행 집계(메모·개인 날짜는 없음).
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyParticipant {
    pub user_id: UserId,
    pub display_name: String,
    pub done: i32,
    pub in_progress: i32,
    pub overdue: i32,
    pub done_amount: i64,
}
