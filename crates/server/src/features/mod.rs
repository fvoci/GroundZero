//! 기능별 핸들러. 공통 경계(`http`, `db`)를 재사용하고 기능끼리 코드를 복사하지 않는다.

pub mod accounts;
pub mod courses;
mod document_locks;
pub mod documents;
pub mod event_recurrence;
pub mod events;
pub mod export;
pub mod file_text;
pub mod files;
pub mod home;
pub mod item_profile;
pub use fvoci_organizations::members;
pub mod modules;
pub use fvoci_organizations::organizations;
pub mod public_links;
pub mod records;
pub mod references;
pub mod revisions;
pub mod saved_views;
pub mod search_api;
pub mod search_text;
pub mod study;
pub mod trash;
pub mod workspaces;
pub mod zotero;
