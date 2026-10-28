//! 소통·알림·활동 시험. DB 전체 보존·주기 작업을 부르는 tests/comments.rs와 tests/activity.rs는 별도 실행 파일이다.
mod audit;
mod comment_archive_barrier;
mod comment_archive_race;
mod comment_flags;
mod home_marks;
mod notifications;
