//! fvoci 공통 규칙: 식별자 newtype, 접근 등급, 공통 오류 종류와 그 응답, 입력 칸 검증, 오류·요청 id를 로그에 남기는
//! 도우미, 비밀 값.

pub mod access;
pub mod error;
pub mod ids;
pub mod logging;
pub mod module;
pub mod recurrence;
pub mod secret;
pub mod trash;
pub mod validate;

pub mod marks;
