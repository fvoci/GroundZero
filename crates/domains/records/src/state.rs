//! 기록·학습 처리기의 부분 상태.

use fvoci_db::Db;

#[derive(Debug, Clone)]
pub struct RecordsState {
    pub db: Db,
    pub zotero_api_url: url::Url,
}
