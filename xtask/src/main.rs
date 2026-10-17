//! 개발·검사·실행 명령 진입점. `cargo xtask <command>`.

mod check;
mod compose;
mod dev;
mod licenses;
mod notices;
mod openapi;
mod rhwp;
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
    Check {
        /// 라이선스 검사를 뺀다. CI는 라이선스를 따로 job(`cargo xtask licenses`)으로 돌려 결과를 나눠 보인다.
        #[arg(long)]
        skip_licenses: bool,
    },
    /// 테스트용 Compose를 띄우고 통합 테스트까지 실행한 뒤 정리한다.
    Test {
        /// 실행 후 컨테이너를 남긴다.
        #[arg(long)]
        keep: bool,
        /// OIDC 통합 테스트용 Keycloak도 함께 띄운다.
        #[arg(long)]
        oidc: bool,
        /// Compose를 띄우지 않고 환경 변수(`DATABASE_URL` 등)로 주어진 외부 서비스를 쓴다. CI 컨테이너 job용.
        #[arg(long)]
        external_services: bool,
    },
    /// 개발용 Compose를 띄우고 서버를 실행한다. 종료는 Ctrl-C.
    Dev {
        /// 개발용 Keycloak을 띄우고 OIDC 로그인을 켠다.
        #[arg(long)]
        oidc: bool,
    },
    /// 이 체크아웃이 만든 Compose 자원(dev·test)을 정리한다. 다른 체크아웃은 건드리지 않는다.
    Down,
    /// `contracts/openapi.json`을 서버 정의에서 다시 생성한다. `--check`는 최신 여부만 본다.
    Openapi {
        #[arg(long)]
        check: bool,
    },
    /// Cargo·pnpm 의존성 라이선스를 허용 목록과 비교한다.
    Licenses,
    /// HWP·HWPX 미리보기용 rhwp WASM을 만든다(D-76). 입력이 같으면 앞서 만든 결과를 다시 쓴다.
    Rhwp {
        /// 입력이 같아도 다시 빌드한다.
        #[arg(long)]
        force: bool,
        /// 빌드하지 않고, 앞서 만든 결과의 브라우저 그래프에 새 보안 권고가 있는지만 다시 본다.
        #[arg(long)]
        audit: bool,
    },
    /// 배포물(서버 바이너리·웹 번들)에 든 패키지의 제3자 고지 `THIRD_PARTY_NOTICES.txt`를 만든다.
    /// `--check`는 최신 여부만 본다.
    Notices {
        #[arg(long)]
        check: bool,
    },
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
        Command::Check { skip_licenses } => check::run(&root, skip_licenses),
        Command::Test {
            keep,
            oidc,
            external_services,
        } => test::run(&root, keep, oidc, external_services),
        Command::Dev { oidc } => dev::run(&root, oidc),
        Command::Down => compose::down_all(&root),
        Command::Openapi { check } => openapi::run(&root, check),
        Command::Licenses => licenses::run(&root),
        Command::Notices { check } => notices::run(&root, check),
        Command::Rhwp { force, audit } => {
            if audit {
                rhwp::audit(&root)
            } else {
                rhwp::run(&root, force)
            }
        }
    }
}
