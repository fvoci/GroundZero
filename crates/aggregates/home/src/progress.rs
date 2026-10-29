//! 서로 다른 의미·단위를 섞지 않는 홈 진행률(D-19, D-36·D-37·D-78).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 항목 종류에 맞는 진행률. 업무는 공유 집계, 강의·스터디는 요청한 사용자의 진행이다.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HomeProgress {
    Tasks { done: i64, total: i64 },
    Course { done: i64, total: i64 },
    Study { amounts: Vec<StudyAmount> },
}

/// 스터디 목표의 단위. 서로 다른 단위의 분량을 더하지 않는다.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum StudyUnitKind {
    Pages,
    Problems,
    Minutes,
    Items,
}

/// 같은 단위 안에서만 합한 목표와 실적.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StudyAmount {
    pub unit: StudyUnitKind,
    pub done: i64,
    pub target: i64,
}
