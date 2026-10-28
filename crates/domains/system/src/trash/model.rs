use fvoci_core::ids::UserId;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

pub use fvoci_core::trash::Trashed;

/// 휴지통 묶음의 맨 위 대상 종류.
#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrashKind {
    Item,
    Document,
    Task,
    Event,
    /// 첨부 파일(모든 버전).
    File,
    /// 모듈(그 아래 항목은 모듈과 함께 보이지 않음, D-78).
    Module,
}

impl TrashKind {
    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "task" => Some(Self::Task),
            "event" => Some(Self::Event),
            "file" => Some(Self::File),
            "module" => Some(Self::Module),
            _ => None,
        }
    }
}

/// 휴지통 묶음.
#[derive(Debug, Serialize, ToSchema)]
pub struct TrashEntry {
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub kind: TrashKind,
    #[schema(value_type = String, format = Uuid)]
    pub root_id: Uuid,
    pub title: String,
    /// 함께 지워진 수(하위 문서·하위 업무 포함).
    pub member_count: i32,
    pub deleted_by: UserId,
    pub deleted_by_name: Option<String>,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub deleted_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339")]
    #[schema(value_type = String, format = DateTime)]
    pub expires_at: OffsetDateTime,
    /// 보존 기간이 지나 되살릴 수 없다(영구 삭제 대기).
    pub expired: bool,
    /// 만료됐지만 살아 있는 기록·문서가 참조하는 파일 버전이 있어 자동으로 영구 삭제하지 않는다(D-56).
    pub kept_for_references: bool,
    /// 영구 삭제할 수 있는지(관리자·개인 노트 소유자).
    pub can_purge: bool,
}

/// 휴지통.
#[derive(Debug, Serialize, ToSchema)]
pub struct Trash {
    pub retention_days: i32,
    /// 이전 파일 버전 보존 기간(일, D-56). 없으면 영구 보존. 개인 휴지통에서는 없다.
    pub version_retention_days: Option<i32>,
    pub entries: Vec<TrashEntry>,
}

/// 비운 수.
#[derive(Debug, Serialize, ToSchema)]
pub struct Purged {
    pub purged: i32,
}

/// 휴지통 보존 기간.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetRetention {
    /// 1~3650일.
    pub days: i32,
}

/// 이전 파일 버전 보존 기간.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetVersionRetention {
    /// 1~3650일. 없으면 영구 보존.
    pub days: Option<i32>,
}
