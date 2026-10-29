//! 조직·공간 관리와 계정 처리기가 필요한 부분 상태.

use crate::accounts::InputLimits;
use fvoci_db::Db;

/// 조직·공간·멤버 처리기의 DB와 이름 상한.
#[derive(Debug, Clone)]
pub struct OrganizationsState {
    pub db: Db,
    pub max_title_chars: usize,
}

/// 계정 처리기의 DB와 화면에 공개하는 입력 상한.
#[derive(Debug, Clone)]
pub struct AccountState {
    pub db: Db,
    pub limits: InputLimits,
}
