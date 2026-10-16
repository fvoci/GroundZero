//! 기능별 핸들러. 공통 경계(`http`, `db`)를 재사용하고 기능끼리 코드를 복사하지 않는다.

pub mod accounts;
pub mod documents;
pub mod workspaces;
