//! DB 전체 주기·보존 작업을 부르는 소통·활동 시험의 독립 실행 파일.
#[path = "it/activity/comment_jobs.rs"]
mod comment_jobs;
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "shared integration fixtures panic when setup or response decoding fails"
)]
mod common;
