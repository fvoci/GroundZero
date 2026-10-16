//! 개발·검사·실행 명령 진입점. `cargo xtask <command>`.

mod check;
mod compose;
mod dev;
mod licenses;
mod openapi;
mod test;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// fvoci 개발 작업 명령.
#[derive(Debug, Parser)]
#[command(name = "xtask", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 포맷·lint·타입·단위 테스트·라이선스·계약 최신 여부를 검사한다. 외부 서비스는 띄우지 않는다.
    Check,
    /// 테스트용 Compose를 띄우고 통합 테스트까지 실행한 뒤 정리한다.
    Test {
        /// 실행 후 컨테이너를 남긴다.
        #[arg(long)]
        keep: bool,
        /// OIDC 통합 테스트용 Keycloak도 함께 띄운다.
        #[arg(long)]
        oidc: bool,
    },
    /// 개발용 Compose를 띄우고 서버를 실행한다. 종료는 Ctrl-C.
    Dev,
    /// 이 체크아웃이 만든 Compose 자원(dev·test)을 정리한다. 다른 체크아웃은 건드리지 않는다.
    Down,
    /// `contracts/openapi.json`을 서버 정의에서 다시 생성한다. `--check`는 최신 여부만 본다.
    Openapi {
        #[arg(long)]
        check: bool,
    },
    /// Cargo·pnpm 의존성 라이선스를 허용 목록과 비교한다.
    Licenses,
}

/// 저장소 루트. xtask는 항상 루트에서 실행된다는 가정을 두지 않는다.
fn repo_root() -> Result<PathBuf> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .map(std::path::Path::to_path_buf)
        .context("xtask manifest has no parent directory")
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = repo_root()?;
    match cli.command {
        Command::Check => check::run(&root),
        Command::Test { keep, oidc } => test::run(&root, keep, oidc),
        Command::Dev => dev::run(&root),
        Command::Down => compose::down_all(&root),
        Command::Openapi { check } => openapi::run(&root, check),
        Command::Licenses => licenses::run(&root),
    }
}
