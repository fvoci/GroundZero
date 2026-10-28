//! 파일 첨부·업로드·버전 도메인. 공동 편집 저장은 조립 crate가 주입한다.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod files;
pub mod router;
pub mod state;

pub use router::router;
pub use state::{FileLimits, FilesState, PersistWorkspace};
