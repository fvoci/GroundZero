//! 소통·활동 처리기에 필요한 연결 풀.
use fvoci_db::Db;

/// 조립 crate의 상태에서 추출하는 소통·활동 상태.
#[derive(Debug, Clone)]
pub struct ActivityState {
    pub db: Db,
}
