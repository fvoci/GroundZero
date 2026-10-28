//! 모듈 기능 유형. 서버와 도메인이 같은 타입을 사용한다.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 모듈 기능 유형 (D-14). 만든 뒤 바꾸지 않는다 (D-16).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKind {
    Project,
    Research,
    Study,
    Course,
    Hobby,
}

impl ModuleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Research => "research",
            Self::Study => "study",
            Self::Course => "course",
            Self::Hobby => "hobby",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "project" => Some(Self::Project),
            "research" => Some(Self::Research),
            "study" => Some(Self::Study),
            "course" => Some(Self::Course),
            "hobby" => Some(Self::Hobby),
            _ => None,
        }
    }
}
