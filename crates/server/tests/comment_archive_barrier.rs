//! 마감 알림의 DB 전체 작업을 부르는 #86 장벽 시험은 독립 실행 파일에서 돈다.
#[path = "it/activity/comment_archive_barrier.rs"]
mod comment_archive_barrier;
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "shared integration fixtures panic when setup or response decoding fails"
)]
mod common;
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "shared integration fixtures panic when setup or response decoding fails"
)]
mod sql_support;
