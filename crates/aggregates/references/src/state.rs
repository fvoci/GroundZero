//! 참조 후보·일괄 해석 집계가 받는 부분 상태.

use fvoci_db::Db;

#[derive(Debug, Clone)]
pub struct ReferencesState {
    pub db: Db,
    pub max_title_chars: usize,
}
