//! 홈과 고정·즐겨찾기가 공유하는 대상 종류.
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 고정·즐겨찾기 대상.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MarkTarget {
    Item,
    Document,
    Task,
}

impl MarkTarget {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Document => "document",
            Self::Task => "task",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "item" => Some(Self::Item),
            "document" => Some(Self::Document),
            "task" => Some(Self::Task),
            _ => None,
        }
    }
}
