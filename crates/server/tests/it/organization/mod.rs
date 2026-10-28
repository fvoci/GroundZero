#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests report fixture and request failures by panicking"
)]
//! D-20 역할별 공유 상한과 D-112 SR1-14 자기 참여 회수.

mod admin_recovery;
mod share_ceiling;
mod share_races;
