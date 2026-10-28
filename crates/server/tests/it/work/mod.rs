//! 업무 도메인(`fvoci-work`) 통합 시험. 조립된 라우터가 있어야 하므로 서버 시험 실행 파일 안에 도메인별 폴더로 둔다
//! (노르웨이숲 태비 초안 3). 공유 DB 전체 작업을 부르는 업무 시험(`tests/recurrence.rs`의 회차 맞추기 작업 등)은
//! 실행 파일 머리 주석의 규칙대로 `tests/` 바로 아래에 남는다.

mod patch;
mod relations;
mod schedule;
mod statuses;
mod tasks;

mod events;
mod saved_views;

mod event_conflict_contracts;
mod event_recurrence;
mod expected_event_user;
