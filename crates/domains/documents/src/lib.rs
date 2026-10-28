//! 문서 자원·한도와 이동·복원 runtime 접점.

#![warn(clippy::too_many_lines, clippy::cognitive_complexity)]

pub mod model;
pub mod state;

pub use state::{DocumentLimits, DocumentsState};
