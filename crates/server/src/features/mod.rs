//! 기능별 핸들러. 공통 경계(`http`, `db`)를 재사용하고 기능끼리 코드를 복사하지 않는다.

pub mod accounts;
pub mod courses;
mod document_locks;
pub mod documents;
pub mod export;
pub mod file_text;
pub mod item_profile;
pub mod members;
pub mod modules;
pub mod organizations;
pub mod public_links;
pub mod records;
pub mod references;
pub mod revisions;
pub mod search_text;
pub mod study;
pub mod trash;
pub mod workspaces;
pub mod zotero;
