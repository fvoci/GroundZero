//! DB 전체 주기·보존 작업을 부르는 소통·활동 시험의 독립 실행 파일.
mod common;
#[path = "it/activity/feed_jobs.rs"]
mod feed_jobs;
mod sql_support;
