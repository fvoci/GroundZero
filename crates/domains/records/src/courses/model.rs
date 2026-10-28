//! 학습 자원과 명령 입력.

use fvoci_core::ids::UserId;
use serde::{Deserialize, Serialize};
use time::Date;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 강좌 정보.
#[derive(Debug, Serialize, Deserialize, ToSchema, Default)]
pub struct CourseDetails {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub instructor: String,
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub textbook: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub starts_on: Option<Date>,
    /// 수강권 만료일.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub expires_on: Option<Date>,
    /// 목표 완료일.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub target_on: Option<Date>,
}

/// 내 진도.
#[derive(Debug, Serialize, ToSchema)]
pub struct MyProgress {
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    /// 내 계획일(없으면 공통 계획일).
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    /// 처음 완료한 날.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub completed_on: Option<Date>,
    /// 완료로 표시한 횟수(다시 보기 포함). 완료 회차 수는 한 번만 센다.
    pub watch_count: i32,
    pub last_position_seconds: Option<i32>,
    /// 개인 메모(나만 봄).
    pub note: String,
}

/// 회차.
#[derive(Debug, Serialize, ToSchema)]
pub struct Episode {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub number: i32,
    pub title: String,
    pub chapter: String,
    pub duration_seconds: i32,
    pub url: String,
    pub materials: String,
    /// 공통 계획일.
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    pub mine: MyProgress,
    /// 공통 정보의 판. 고칠 때 읽은 판을 보낸다(그 사이 다른 사람이 고쳤으면 409).
    pub version: i32,
}

/// 강좌: 정보·회차(번호순)·내 진도.
#[derive(Debug, Serialize, ToSchema)]
pub struct Course {
    pub details: CourseDetails,
    pub episodes: Vec<Episode>,
    /// 강좌 정보·회차를 고칠 수 있는지(진도는 볼 수 있으면 누구나 자기 것).
    pub can_write: bool,
}

/// 추가할 회차.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct NewEpisode {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    /// 없으면 마지막 다음 번호.
    pub number: Option<i32>,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub duration_seconds: i32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub materials: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 회차 여러 개 추가(직접 입력·CSV 일괄 등록, 최대 1000개).
#[derive(Debug, Deserialize, ToSchema)]
pub struct AddEpisodes {
    pub episodes: Vec<NewEpisode>,
}

/// 추가한 수(같은 요청 id로 이미 있던 것은 빠짐).
#[derive(Debug, Serialize, ToSchema)]
pub struct EpisodesAdded {
    pub added: i32,
}

/// 회차 고치기. `version`은 읽은 판.
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateEpisode {
    pub version: i32,
    pub number: i32,
    pub title: String,
    #[serde(default)]
    pub chapter: String,
    #[serde(default)]
    pub duration_seconds: i32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub materials: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
}

/// 내 진도 바꾸기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetProgress {
    /// `not_started`·`in_progress`·`done`.
    pub status: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub planned_on: Option<Date>,
    pub last_position_seconds: Option<i32>,
    #[serde(default)]
    pub note: String,
    /// 사용자 시간대의 오늘(완료한 날). 없으면 서버 날짜.
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub today: Option<Date>,
}

/// 함께 수강하는 사람의 진도 집계(메모·위치·시간은 없음).
#[derive(Debug, Serialize, ToSchema)]
pub struct Participant {
    pub user_id: UserId,
    pub display_name: String,
    pub done: i32,
    pub in_progress: i32,
    /// 계획일이 지났는데 완료하지 않은 회차 수.
    pub overdue: i32,
    pub done_seconds: i64,
}

/// 오늘(사용자 시간대의 날짜). 서버 날짜에서 하루 넘게 떨어지면 400.
#[derive(Debug, Deserialize, IntoParams)]
pub struct TodayQuery {
    #[param(value_type = String, format = Date)]
    #[serde(with = "day")]
    pub today: Date,
}
