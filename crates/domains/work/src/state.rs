//! 업무 처리기가 받는 부분 상태. 조립 crate의 상태에서 `FromRef`로 꺼낸다(노르웨이숲 태비 초안 3: 처리기는 필요한
//! 부분 상태만 받는다).

use fvoci_db::Db;

/// 업무 처리기의 상태: 연결 풀과 제목 글자 수 상한(`FVOCI_MAX_TITLE_CHARS`).
#[derive(Debug, Clone)]
pub struct WorkState {
    pub db: Db,
    pub max_title_chars: usize,
}
