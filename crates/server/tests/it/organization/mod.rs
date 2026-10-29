#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration tests report fixture and request failures by panicking"
)]
//! D-20 역할별 공유 상한과 D-112 SR1-14 자기 참여 회수.

mod account_contracts;
mod admin_recovery;
mod admin_security;
mod expected_user;
mod field_contracts;
mod ids_races;
mod last_admin;
mod members;
mod organization_removal_lock_order;
mod organizations_api;
mod recurrence_security;
mod role_sources;
mod share_ceiling;
mod share_display;
mod share_races;
mod share_resources;
mod source_organization;
