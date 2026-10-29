//! 기능별 핸들러. 공통 경계(`http`, `db`)를 재사용하고 기능끼리 코드를 복사하지 않는다.

pub mod accounts;
pub mod file_text;
pub mod item_profile;
pub use fvoci_organizations::members;
pub mod modules;
pub use fvoci_organizations::organizations;
pub mod search_text;

pub use fvoci_documents::{documents, public_links, references, revisions};
