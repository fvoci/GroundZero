//! 휴지통·복원·영구 삭제 (D-39, D-41, D-57).
//!
//! 지우면 휴지통 묶음 하나가 생긴다(문서는 하위 문서까지, 업무는 하위 업무까지, 항목은 항목 하나, 일정은 하나).
//! 지운 것은 접근 함수가 없는 것으로 보므로 어디에도 나오지 않는다. 보존 기간(워크스페이스마다, 기본 30일)이
//! 지나면 되살릴 수 없고 "만료"로 남으며, 실제 삭제는 관리자(개인 노트는 소유자)가 직접 한다(자동 삭제는 O-19
//! 뒤). DB 정의는 `crates/db/migrations/202610280009_system.sql`의 `app_restore_batch`·`app_purge_batch` 등에 있다.

mod commands;
mod model;
mod read;

pub use commands::*;
pub use model::*;
pub use read::*;
