use fvoci_core::ids::WorkspaceId;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// 연결 대상 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Task,
    Document,
    Item,
    /// 강의 회차(D-32).
    Episode,
    /// 스터디 단위(D-32 공부).
    StudyUnit,
}

impl TargetKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Document => "document",
            Self::Item => "item",
            Self::Episode => "episode",
            Self::StudyUnit => "study_unit",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "task" => Some(Self::Task),
            "document" => Some(Self::Document),
            "item" => Some(Self::Item),
            "episode" => Some(Self::Episode),
            "study_unit" => Some(Self::StudyUnit),
            _ => None,
        }
    }
}

/// 연결 대상(보낼 때).
#[derive(Debug, Clone, Copy, Deserialize, ToSchema)]
pub struct TargetInput {
    pub kind: TargetKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
}

/// 연결 대상(받을 때). 지금 볼 수 없으면 `label`이 없다.
#[derive(Debug, Serialize, ToSchema)]
pub struct Target {
    pub kind: TargetKind,
    #[schema(value_type = String, format = Uuid)]
    pub id: Uuid,
    pub label: Option<String>,
    pub workspace_id: Option<WorkspaceId>,
}

pub(crate) fn split_target(target: Option<TargetInput>) -> (Option<&'static str>, Option<Uuid>) {
    target.map_or((None, None), |t| (Some(t.kind.as_str()), Some(t.id)))
}
