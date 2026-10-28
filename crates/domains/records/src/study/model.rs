//! 학습 자원과 명령 입력.

use crate::validation;
use fvoci_core::error::AppError;
use fvoci_core::ids::UserId;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::ToSchema;
use uuid::Uuid;
time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 내 진행.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct MyUnitProgress {
    #[schema(value_type = String, format = Uuid)]
    pub unit_id: Uuid,
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    /// 한 분량(단위의 분량과 같은 단위).
    #[schema(minimum = 0, maximum = 1000000)]
    pub done_amount: i32,
    /// 내 계획일(없으면 공통 계획일).
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub completed_on: Option<Date>,
    /// 개인 메모(나만 봄).
    #[schema(max_length = 20000)]
    pub note: String,
}

/// 계획 단위.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct StudyUnit {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub number: i32,
    #[schema(min_length = 1, max_length = 300)]
    pub title: String,
    /// 교육과정·챕터·단계.
    #[schema(max_length = 300)]
    pub chapter: String,
    /// `pages`·`problems`·`minutes`·`items`.
    pub unit: String,
    #[schema(minimum = 0, maximum = 1000000)]
    pub amount: i32,
    /// 공통 계획일.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    /// 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409).
    pub version: i32,
}

/// 스터디의 접근 상태.
#[derive(Debug, Serialize, ToSchema)]
pub struct StudyPlan {
    pub access: crate::learning::LearningAccess,
}

/// 판 충돌은 현재 단위, 그 밖의 409는 공통 오류 본문이다.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum UnitWriteConflict {
    Stale(Box<fvoci_http::conflict::StaleWrite<StudyUnit>>),
    State(fvoci_core::error::ErrorBody),
}

/// 추가할 단위.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewStudyUnit {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub number: Option<i32>,
    #[schema(min_length = 1, max_length = 300)]
    pub title: String,
    #[serde(default)]
    #[schema(max_length = 300)]
    pub chapter: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    #[serde(default)]
    #[schema(minimum = 0, maximum = 1000000)]
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
    #[schema(min_items = 1, max_items = 1000)]
    pub units: Vec<NewStudyUnit>,
}

/// 추가한 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct UnitsAdded {
    pub added: i32,
}

/// 단위 고치기. `version`은 읽은 판.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct UpdateStudyUnit {
    pub version: i32,
    pub number: i32,
    #[schema(min_length = 1, max_length = 300)]
    pub title: String,
    #[serde(default)]
    #[schema(max_length = 300)]
    pub chapter: String,
    pub unit: String,
    #[serde(default)]
    #[schema(minimum = 0, maximum = 1000000)]
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
    #[schema(minimum = 0, maximum = 1000000)]
    pub done_amount: i32,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    #[serde(default)]
    #[schema(max_length = 20000)]
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
    #[schema(max_items = 1000)]
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

fn unit_fields(
    number: Option<i32>,
    title: &str,
    chapter: &str,
    unit: &str,
    amount: i32,
) -> Result<(), AppError> {
    validation::title(title)?;
    validation::length(chapter.trim(), "chapter", 300)?;
    validation::range(amount, "amount", 0, 1_000_000)?;
    if let Some(number) = number {
        validation::range(number, "number", 1, i32::MAX)?;
    }
    if !matches!(unit, "pages" | "problems" | "minutes" | "items") {
        return Err(validation::invalid("unit", "invalid study unit kind"));
    }
    Ok(())
}

impl AddStudyUnits {
    pub(super) fn validate(&self) -> Result<(), AppError> {
        validation::count(self.units.len(), "units", 1, 1_000)?;
        for unit in &self.units {
            unit_fields(
                unit.number,
                &unit.title,
                &unit.chapter,
                &unit.unit,
                unit.amount,
            )?;
        }
        Ok(())
    }
}

impl UpdateStudyUnit {
    pub(super) fn validate(&self) -> Result<(), AppError> {
        unit_fields(
            Some(self.number),
            &self.title,
            &self.chapter,
            &self.unit,
            self.amount,
        )
    }
}

impl SetUnitProgress {
    pub(super) fn validate(&self) -> Result<(), AppError> {
        validation::progress(&self.status, &self.note)?;
        validation::range(self.done_amount, "done_amount", 0, 1_000_000)
    }
}

impl PlanUnits {
    pub(super) fn validate(&self) -> Result<(), AppError> {
        validation::count(self.entries.len(), "entries", 0, 1_000)
    }
}
