//! 항목 기록: 종류 정의, 자원 읽기, 명령과 검증.

mod conflict;
mod kinds;
mod model;
mod read;
mod validation;
mod write;

pub use kinds::{__path_kinds, ChoiceDef, FieldDef, FieldType, RecordKindDef, kinds, record_kinds};
pub use model::RecordWriteConflict;
pub use model::{EditRecord, NewRecord, Record, RecordFile, RecordFileInput, RecordVersion};
pub use read::{__path_list, list};
pub(crate) use validation::validate;
pub use write::{__path_create, __path_delete, __path_update, create, delete, update};
