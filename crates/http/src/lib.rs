//! axum 공통: 세션 추출기(`CurrentUser`·`ExpectedUser`)와 Origin 검사. 상태는 조립 crate의 상태에서 `FromRef`로 꺼낸다.

pub mod origin;
pub mod session;
