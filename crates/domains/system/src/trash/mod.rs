//! 휴지통·복원·영구 삭제 (D-39, D-41, D-57).
//!
//! 지우면 휴지통 묶음 하나가 생긴다(문서는 하위 문서까지, 업무는 하위 업무까지, 항목은 항목 하나, 일정은 하나).
//! 지운 것은 접근 함수가 없는 것으로 보므로 어디에도 나오지 않는다. 공간별 보존 기간은 기본 30일이다(D-41).
//! 만료된 묶음은 자동 정리하되 살아 있는 기록·문서가 참조하는 버전은 보존하고 되살릴 수 있다(D-56·D-69).
//! 관리자의 수동 영구 삭제 정책은 그대로다. DB 정의는 `crates/db/migrations/202610280009_system.sql`의
//! `app_restore_batch`·`app_purge_batch` 등에 있다.

mod commands;
mod model;
mod read;

pub use commands::*;
pub use model::*;
pub use read::*;
