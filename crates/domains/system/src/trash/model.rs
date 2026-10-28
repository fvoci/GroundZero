use fvoci_core::ids::{UserId, WorkspaceId};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::{IntoParams, ToSchema};
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
    /// 만료 여부와 참조 보존을 하나의 상태로 표현한다(D-56·D-69).
    pub expiry: TrashExpiry,
    /// 영구 삭제할 수 있는지(관리자·개인 노트 소유자).
    pub can_purge: bool,
}

/// 휴지통 만료 상태. 참조 보존은 만료된 묶음에서만 가능하다(D-56·D-69).
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TrashExpiry {
    Active,
    Expired,
    KeptForReferences,
}

/// 공간 보존 설정 자원. 목록과 별도로 읽고 화면에서 조합한다.
#[derive(Debug, Serialize, ToSchema)]
pub struct RetentionSettings {
    pub retention_days: i32,
    /// null이면 이전 파일 버전을 영구 보존한다(D-56).
    pub version_retention_days: Option<i32>,
}

/// 공간 id가 없으면 현재 사용자의 개인 휴지통이다.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct TrashScope {
    pub workspace_id: Option<WorkspaceId>,
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
    #[schema(minimum = 1, maximum = 3650)]
    pub days: i32,
}

/// 이전 파일 버전 보존 기간.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SetVersionRetention {
    /// 1~3650일. 없으면 영구 보존.
    #[schema(minimum = 1, maximum = 3650)]
    pub days: Option<i32>,
}
