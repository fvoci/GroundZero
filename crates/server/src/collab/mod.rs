//! 실시간 공동 편집(D-70): Yjs 문서 변환, 방(연결 묶음), 웹소켓 동기화.

pub mod convert;
mod documents;
mod file_refs;
mod lease;
pub mod organizations;
pub mod room;
pub mod visibility;
pub mod ws;

pub use room::Rooms;
