//! 접근 질의 도우미의 서버 경로. 구현은 공통 DB 계층에 있다.
pub use fvoci_core::access::{Grade, Kind, Target};
pub use fvoci_db::access::{grade, require_owner, require_visible, require_write};
