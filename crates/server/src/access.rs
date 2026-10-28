//! 접근 등급 질의 도우미는 공통 DB crate에서 제공한다.

pub use fvoci_db::access::{
    Grade, Kind, Target, grade, require_owner, require_visible, require_write,
};
