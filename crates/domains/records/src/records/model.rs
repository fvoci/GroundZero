//! 기록 자원과 명령 입력.

use fvoci_core::ids::UserId;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{Date, OffsetDateTime};
use utoipa::ToSchema;
use uuid::Uuid;
time::serde::format_description!(day, Date, "[year]-[month]-[day]");

/// 연결한 파일 버전.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct RecordFile {
    pub role: String,
    /// 파일이 영구 삭제되었으면 없다(이름·버전·SHA-256은 남는다).
    #[schema(value_type = Option<String>, format = Uuid)]
    pub file_id: Option<Uuid>,
    pub file_name: String,
    pub version: i32,
    pub sha256: String,
    pub size: i64,
    /// 그 버전을 지금도 열 수 있는지.
    pub available: bool,
}

/// 항목 기록.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct Record {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub kind: String,
    #[schema(min_length = 1, max_length = 300)]
    pub title: String,
    #[serde(with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub occurred_on: Option<Date>,
    pub body: String,
    #[schema(value_type = Object)]
    pub fields: Value,
    pub files: Vec<RecordFile>,
    pub created_by: UserId,
    pub created_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: OffsetDateTime,
    pub version: i32,
    pub can_edit: bool,
    pub can_delete: bool,
}

/// 판 충돌은 현재 기록, 그 밖의 409는 공통 오류 본문이다.
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub enum RecordWriteConflict {
    Stale(Box<fvoci_http::conflict::StaleWrite<Record>>),
    State(fvoci_core::error::ErrorBody),
}

/// 연결할 파일 버전. 버전이 없으면 지금 버전.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct RecordFileInput {
    #[schema(value_type = String, format = Uuid)]
    pub file_id: Uuid,
    pub version: Option<i32>,
    pub role: String,
}

/// 기록 만들기.
#[derive(Debug, Deserialize, ToSchema)]
pub struct NewRecord {
    #[schema(value_type = String, format = Uuid)]
    pub request_id: Uuid,
    pub kind: String,
    #[schema(min_length = 1, max_length = 300)]
    pub title: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub occurred_on: Option<Date>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    #[schema(max_items = 50)]
    pub files: Vec<RecordFileInput>,
}

/// 기록 고치기. `version`은 읽은 판.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct EditRecord {
    pub version: i32,
    #[schema(min_length = 1, max_length = 300)]
    pub title: String,
    #[serde(default, with = "day::option")]
    #[schema(value_type = Option<String>, format = Date)]
    pub occurred_on: Option<Date>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub fields: Map<String, Value>,
    #[serde(default)]
    #[schema(max_items = 50)]
    pub files: Vec<RecordFileInput>,
}

/// 새 판.
#[derive(Debug, Serialize, ToSchema)]
pub struct RecordVersion {
    pub version: i32,
}
