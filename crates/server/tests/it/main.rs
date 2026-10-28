//! 서버 통합 시험 실행 파일. 주제별 시험 파일을 모듈로 모아 한 번만 컴파일·링크한다(#158). 시험 이름은
//! `<파일>::<시험>`이라 `--skip oidc_` 같은 이름 조각 거르기는 그대로 맞는다.
//!
//! 여기 모듈의 시험은 한 프로세스에서 서로 다른 파일의 시험과 함께 돈다. 그래서 공유 시험 DB 전체를 보는 작업(주기 작업
//! `worker::run_all_now`·`run_once`·`run_jobs`, 검색 대기열 `search::drain`·`app_system_search_claim`, 기간이 지난 행을
//! DB 전체에서 지우는 `app_system_purge_*`)을 부르는 파일과, 프로세스 전역 상태를 쓰는 파일(`logging.rs`의 전역 로그
//! 수집)은 여기 넣지 않고 `tests/` 바로 아래에 각자의 실행 파일로 둔다. 실행 파일은 차례로 돌므로 그 파일들은 전처럼
//! 다른 파일의 시험과 겹치지 않는다. 새 DB를 만들어 그 안에서만 그런 작업을 하는 시험(`shutdown`·`backup` 등)은 여기 둔다.

#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "shared integration fixtures panic when setup or response decoding fails"
)]
#[path = "../common/mod.rs"]
mod common;
#[expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "shared integration fixtures panic when setup or response decoding fails"
)]
#[path = "../sql_support/mod.rs"]
mod sql_support;

mod access_matrix;
mod archived_management;
mod audit;
mod backup;
mod cache;
mod calendar;
mod comment_archive_barrier;
mod comment_archive_race;
mod comment_flags;
mod conflict_contracts;
mod courses;
mod db_context;
mod db_errors;
mod expected_user;
mod expected_user_writes;
mod export;
mod first_flow;
mod health;
mod home;
mod home_marks;
mod http_denials;
mod id_bounds;
mod item_profile;
mod limits;
mod limits_contract;
mod members;
mod migration_baseline;
mod modules;
mod my_tasks;
mod notifications;
mod oidc;
mod openapi_review;
mod organization_removal_lock_order;
mod organizations_api;
mod personal_scope;
mod progress_privacy;
mod public_links;
mod references;
mod restore_lock_order;
mod restore_measure;
mod revisions;
mod room_memory_measure;
mod settings_contract;
mod sharing;
mod shutdown;
mod source_organization;
mod storage;
mod study;
mod supabase;
mod templates;
mod time_tracking;
mod timer_contention;
mod timer_http;
mod timer_switch;
mod trash;
mod tree;
mod unicode_boundaries;
mod work;
mod workspace_document_lock_order;
mod zotero;
