#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test crate: every item is test code; failures must panic"
)]
//! 실제 색인을 준비하는 검색 HTTP 시험. 공유 DB 전체 큐를 비우므로 it/main.rs와 실행을 분리한다.
mod common;

#[path = "it/search/indexed/mod.rs"]
mod indexed;
