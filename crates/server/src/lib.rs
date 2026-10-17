//! fvoci 서버 라이브러리. 실행 진입점은 `main.rs`, 구성·시작·종료는 `app`.

pub mod app;
pub mod backup;
pub mod collab;
pub mod config;
pub mod db;
pub mod error;
pub mod features;
pub mod http;
pub mod ids;
pub mod search;
pub mod storage;
pub mod worker;
