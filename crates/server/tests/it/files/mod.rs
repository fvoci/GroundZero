#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test module: every item is test code; failures must panic"
)]
//! 파일 HTTP·저장소 시험. 공유 DB 전체 작업과 DDL 시험은 tests/files.rs에서 따로 돈다.
mod api_contracts;
mod completion;
mod fixtures;
mod moving;
mod storage;
