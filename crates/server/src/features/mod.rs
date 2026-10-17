//! 기능별 핸들러. 공통 경계(`http`, `db`)를 재사용하고 기능끼리 코드를 복사하지 않는다.

pub mod accounts;
pub mod audit;
pub mod comments;
pub mod courses;
pub mod documents;
pub mod events;
pub mod export;
pub mod files;
pub mod home;
pub mod members;
pub mod modules;
pub mod public_links;
pub mod recurrence;
pub mod references;
pub mod revisions;
pub mod schedule;
pub mod search_api;
pub mod search_text;
pub mod tasks;
pub mod time_tracking;
pub mod trash;
pub mod workspaces;
