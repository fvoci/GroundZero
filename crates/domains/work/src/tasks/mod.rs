//! 업무·하위 업무·담당자·업무 체크리스트 (D-14, D-30, D-31, D-33, D-48, D-50, D-57).
//!
//! 업무는 모듈 항목 아래에 있고 모듈 번호열을 함께 쓴다. 권한은 항목을 따르며 DB 함수 `app_access`
//! 하나로 판정한다(보관 아래 읽기 전용). 필드 고치기는 `base_version`으로 충돌을 막고, 칸반 이동과 체크리스트
//! 체크는 버전 없이 따로 한다(같은 보드를 여럿이 써도 충돌하지 않게). 날짜는 시각이 없는 날짜로 저장한다(D-50).
//! 앱 공급 UUIDv7(D-88)과 모든 칸을 한 번에 받는 만들기(D-105)의 SQL 정본은 `crates/db/migrations/`에 있다.
//!
//! 나눔: `resource` 업무 자원, `relations` 관계 모음 읽기, `patch` 고친 칸만 보내는 고치기 명령, `edit` 고치기의 저장
//! 한 번, `model` 명령 모양과 칸 검증, `read` 자원 읽기, `write` 업무 명령, `checklist` 체크리스트 명령.

mod checklist;
mod edit;
mod model;
mod patch;
mod read;
mod relations;
mod resource;
mod write;

pub use checklist::*;
pub use model::*;
pub use patch::*;
pub use read::*;
pub use relations::*;
pub use resource::{Task, TaskConflictCurrent};
pub use write::*;

pub(crate) use relations::{load_dependency, load_series};
