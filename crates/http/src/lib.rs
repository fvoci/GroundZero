//! axum 공통: 세션 추출기(`CurrentUser`·`ExpectedUser`)와 기대 사용자 공통 처리, Origin 검사, 판 충돌 409 공통 본문.
//! 상태는 조립 crate의 상태에서 `FromRef`로 꺼낸다.

pub mod conflict;
pub mod origin;
pub mod session;
