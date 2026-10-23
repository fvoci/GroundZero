//! 개발·검사·실행 명령 진입점. `cargo xtask <command>`.

mod check;
mod compose;
mod dev;
mod i18n;
mod licenses;
mod notices;
mod openapi;
mod rhwp;
mod sql_compare;
mod supabase;
mod supabase_cloud_precheck;
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
    /// Compare a fixed historical SQL revision with product migrations in isolated Compose projects.
    SqlCompare {
        /// Full immutable commit SHA supplying the previous cumulative migrations.
        #[arg(long, default_value = sql_compare::PREVIOUS_SQL_SHA)]
        baseline_sha: String,
        /// Apply the historical source to both sides for an A/A control.
        #[arg(long)]
        control: bool,
        /// Evidence directory (must not already exist).
        #[arg(long)]
        output: PathBuf,
        /// Also run the existing integration suite against both databases.
        #[arg(long)]
        behavior: bool,
        /// Select all existing tests or only the listed app-role capability tests.
        #[arg(long, value_enum, requires = "behavior")]
        behavior_scope: Option<sql_compare::BehaviorScope>,
        /// Also start the test Keycloak (`oidc` profile) and run the OIDC tests in the full suite.
        #[arg(long, requires = "behavior")]
        oidc: bool,
        /// Use a local PostgreSQL installer with CREATEROLE and target-DB privileges only.
        #[arg(long, conflicts_with = "supabase")]
        restricted_admin: bool,
        /// Standard PostgreSQL major; Supabase uses its own pinned fixture.
        #[arg(long, value_enum, default_value = "17", conflicts_with = "supabase")]
        postgres_major: compose::PostgresMajor,
        /// Use the pinned self-hosted Supabase PostgreSQL and session pooler.
        #[arg(long)]
        supabase: bool,
    },
    /// 전용 Supabase Cloud 대상의 읽기 전용 사전 점검. 기본값은 연결하지 않는다.
    SupabaseCloudPrecheck {
        /// 승인된 대상에 한 번 연결한다. 수동 workflow는 검토된 main에서만 실행한다.
        #[arg(long)]
        connect: bool,
    },
    /// 승인된 Cloud 재생성 대상을 읽기 전용으로 조사한다. 행 내용이나 시크릿은 출력하지 않는다.
    SupabaseCloudInventory {
        /// 기존 테스트 자격 증명으로 한 번 연결한다. 쓰기·초기화는 수행하지 않는다.
        #[arg(long)]
        connect: bool,
    },
    /// Approved single-project first installation; evidence branch only.
    SupabaseCloudFirstInstall {
        #[arg(long)]
        connect: bool,
    },
    /// Read-only diagnosis after the failed first-install attempt; never calls bootstrap.
    SupabaseCloudFirstInstallInspect {
        #[arg(long)]
        connect: bool,
    },
    /// Verify the already-installed approved Cloud target. Never bootstraps or migrates.
    SupabaseCloudVerify {
        #[arg(long)]
        connect: bool,
    },
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
        /// Compose를 띄우지 않고 환경 변수(`FVOCI_TEST_PG_HOST_PORT`·`FVOCI_TEST_SEARCH_URL`, `--oidc`면
        /// `FVOCI_TEST_OIDC_ISSUER`)로 주어진 외부 서비스를 쓴다. CI job용.
        #[arg(long)]
        external_services: bool,
        /// PostgreSQL 대신 셀프호스트 Supabase 공식 구성(고정 revision의 db·rest·supavisor)을 DB로 쓴다(D-88).
        #[arg(long, conflicts_with = "external_services")]
        supabase: bool,
        /// `--supabase`에서 앱 역할 연결을 Supavisor session 모드로 바꿔 한 번 더 돌린다.
        #[arg(long, requires = "supabase")]
        session_pooler: bool,
    },
    /// 개발용 Compose를 띄우고 서버를 실행한다. 종료는 Ctrl-C.
    Dev {
        /// 개발용 Keycloak을 띄우고 OIDC 로그인을 켠다(테스트 계정·공동 작업 공간 준비 포함).
        #[arg(long)]
        oidc: bool,
        /// 웹 개발 서버(Vite)를 띄우지 않는다(API만).
        #[arg(long)]
        no_web: bool,
    },
    /// 이 체크아웃이 만든 Compose 자원(dev·test)을 정리한다. 다른 체크아웃은 건드리지 않는다.
    Down,
    /// `contracts/openapi.json`을 서버 정의에서 다시 생성한다. `--check`는 그 파일과 거기서 생성한 TS client
    /// (`packages/api-client/src/generated/schema.ts`)의 최신 여부만 본다.
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
    let cli = Cli::try_parse().unwrap_or_else(|error| {
        // clap의 일반 오류에는 사용자가 잘못 붙인 URI가 포함될 수 있다.
        // 이 명령의 잘못된 입력은 고정 코드만 보고하고 환경도 읽지 않는다.
        if std::env::args_os().nth(1).is_some_and(|arg| {
            arg == "supabase-cloud-precheck"
                || arg == "supabase-cloud-inventory"
                || arg == "supabase-cloud-first-install"
                || arg == "supabase-cloud-first-install-inspect"
                || arg == "supabase-cloud-verify"
        }) && !matches!(
            error.kind(),
            clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
        ) {
            std::process::exit(supabase_cloud_precheck::rejected_arguments());
        }
        error.exit()
    });
    let root = repo_root()?;
    match cli.command {
        Command::SqlCompare {
            baseline_sha,
            control,
            output,
            behavior,
            behavior_scope,
            oidc,
            restricted_admin,
            postgres_major,
            supabase,
        } => sql_compare::run(
            &root,
            sql_compare::Baseline {
                sha: &baseline_sha,
                control,
            },
            &output,
            behavior.then(|| sql_compare::Behavior {
                scope: behavior_scope.unwrap_or(sql_compare::BehaviorScope::Full),
                oidc,
            }),
            supabase,
            restricted_admin,
            postgres_major,
        ),
        Command::SupabaseCloudPrecheck { connect } => {
            std::process::exit(supabase_cloud_precheck::run(connect))
        }
        Command::SupabaseCloudInventory { connect } => {
            std::process::exit(supabase_cloud_precheck::inventory::run(connect))
        }
        Command::SupabaseCloudFirstInstall { connect } => {
            std::process::exit(supabase_cloud_precheck::first_install::run(
                connect,
                supabase_cloud_precheck::first_install::Operation::Install,
            ))
        }
        Command::SupabaseCloudFirstInstallInspect { connect } => {
            std::process::exit(supabase_cloud_precheck::first_install::run(
                connect,
                supabase_cloud_precheck::first_install::Operation::Inspect,
            ))
        }
        Command::SupabaseCloudVerify { connect } => {
            std::process::exit(supabase_cloud_precheck::first_install::run(
                connect,
                supabase_cloud_precheck::first_install::Operation::Verify,
            ))
        }
        Command::Check { skip_licenses } => check::run(&root, skip_licenses),
        Command::Test {
            keep,
            oidc,
            supabase: true,
            session_pooler,
            ..
        } => test::run_supabase(&root, keep, oidc, session_pooler),
        Command::Test {
            keep,
            oidc,
            external_services,
            ..
        } => test::run(&root, keep, oidc, external_services),
        Command::Dev { oidc, no_web } => dev::run(&root, oidc, !no_web),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sql_compare(args: &[&str]) -> Result<Command, clap::Error> {
        let base = ["xtask", "sql-compare", "--output", "target/x"];
        Cli::try_parse_from(base.iter().chain(args)).map(|cli| cli.command)
    }

    #[test]
    fn sql_compare_requires_behavior_for_oidc() {
        let error = sql_compare(&["--oidc"]).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
        for args in [
            &["--behavior", "--oidc"][..],
            &["--behavior", "--oidc", "--postgres-major", "18"],
            &["--behavior", "--oidc", "--supabase"],
        ] {
            assert!(matches!(
                sql_compare(args),
                Ok(Command::SqlCompare {
                    behavior: true,
                    oidc: true,
                    ..
                })
            ));
        }
        assert!(matches!(
            sql_compare(&["--behavior"]),
            Ok(Command::SqlCompare { oidc: false, .. })
        ));
    }
}
