//! 개발·검사·실행 명령 진입점. `cargo xtask <command>`.

mod browser;
mod check;
mod ci_base_manifest;
mod ci_plan;
mod compose;
mod dev;
mod fonts;
mod i18n;
mod licenses;
mod notices;
mod openapi;
mod rhwp;
mod supabase;
mod supabase_cloud_precheck;
mod supabase_cloud_rebuild;
mod test;
mod test_executables;
#[cfg(all(test, unix))]
mod test_fixture;
mod test_report;
mod test_verify;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

#[cfg(test)]
mod manifest_cli_tests {
    use super::*;

    #[test]
    fn manifest_publication_is_an_xtask_command() {
        let cli = Cli::try_parse_from([
            "xtask",
            "ci-base-manifest",
            "--image",
            "ghcr.io/fvoci/groundzero-ci-base",
            "--tag",
            "source-sha",
            "--digests-dir",
            "digests",
        ]);
        assert!(cli.is_ok(), "{cli:?}");
    }
}

/// fvoci 개발 작업 명령.
#[derive(Debug, Parser)]
#[command(name = "xtask", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 두 플랫폼 digest를 검증해 ci-base manifest를 게시하고 최종 digest만 출력한다.
    CiBaseManifest {
        #[arg(long)]
        image: String,
        #[arg(long)]
        tag: String,
        #[arg(long)]
        digests_dir: PathBuf,
    },
    /// 승인된 Cloud 시험 DB의 조사·재설치·확인. 실제 연결은 Actions main에서만 허용한다.
    SupabaseCloudRebuild {
        #[arg(value_enum)]
        phase: supabase_cloud_rebuild::Phase,
        #[arg(long)]
        connect: bool,
    },
    /// 시험 Compose의 PostgreSQL 하나로 Cloud 재생성의 실패·성공 경로를 검증한다.
    SupabaseCloudRebuildTest,
    /// 전용 Supabase Cloud 대상의 읽기 전용 사전 점검. 기본값은 연결하지 않는다.
    SupabaseCloudPrecheck {
        /// 승인된 대상에 한 번 연결한다. 수동 workflow는 검토된 main에서만 실행한다.
        #[arg(long)]
        connect: bool,
    },
    /// 포맷·lint·타입·단위 테스트·라이선스·계약 최신 여부를 검사한다. 외부 서비스는 띄우지 않는다.
    Check {
        /// 라이선스 검사를 뺀다. CI는 라이선스를 따로 job(`cargo xtask licenses`)으로 돌려 결과를 나눠 보인다.
        #[arg(long)]
        skip_licenses: bool,
        /// 문서 서식 검사(`pnpm format:check`)만 돌리고 나머지 단계는 건너뜀으로 보인다. CI가 문서 전용 PR에 쓴다(#146).
        #[arg(long)]
        docs_only: bool,
    },
    /// CI가 이번 실행에서 돌 job을 변경 경로로 정한다(#146·#155). `changes` job이 부른다.
    CiPlan {
        /// `$GITHUB_EVENT_NAME`. `pull_request`만 변경 경로로 좁히고 나머지는 전체를 돈다.
        #[arg(long)]
        event: String,
    },
    /// 시험 데몬 요구 사항·호스트 소유권·이 체크아웃의 Compose 자원 부재를 검증한다.
    TestVerify {
        #[arg(long)]
        require_rootless: bool,
    },
    /// 기본 Compose 시험 로그를 집계하고 소유권·정리·추가 시험의 PASS 증거를 검증한다.
    TestReport {
        /// cargo xtask test의 stdout 전용 파일(시험 이름·그룹·결과).
        #[arg(long)]
        stdout_log: PathBuf,
        /// 같은 실행의 stderr 전용 파일(Cargo Running·Doc-tests 헤더).
        #[arg(long)]
        stderr_log: PathBuf,
        #[arg(long, value_enum)]
        outcome: test_report::Outcome,
    },
    /// 테스트용 Compose를 띄우고 통합 테스트까지 실행한 뒤 정리한다.
    Test {
        /// 실행 후 컨테이너를 남긴다.
        #[arg(long)]
        keep: bool,
        /// OIDC 통합 테스트용 Keycloak도 함께 띄운다.
        #[arg(long)]
        oidc: bool,
        /// 시험을 호스트에서 실행하고 Compose 게시 포트에 연결한다(디버깅용). 기본값은 ci-base 컨테이너 실행.
        #[arg(long, conflicts_with_all = ["external_services", "supabase"])]
        host: bool,
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
    /// 실제 서버·DB·테스트 OIDC에 Chromium으로 접속하는 브라우저 회귀 시험(Playwright). 전용 Compose 프로젝트를 띄우고
    /// 정리한다. 원시 출력은 실행마다 `target/browser/runs/<실행 ID>`에 남기고 지우지 않는다. 검사를 통과한 실패 증거만
    /// `target/browser/artifacts`로 옮긴다(실행마다 비운다).
    Browser {
        /// 실행 후 Compose 컨테이너(PostgreSQL·Keycloak)를 남긴다. 서버는 늘 멈춘다. 원시 출력·`target/browser/files`는
        /// 이 옵션과 상관없이 남는다.
        #[arg(long)]
        keep: bool,
        /// Playwright에 그대로 넘길 인자(예: `-- e2e/layout.spec.ts`, `-- --grep 칸반`).
        #[arg(last = true)]
        playwright: Vec<String>,
    },
    /// 이 체크아웃이 만든 Compose 자원(dev·test·browser)을 정리한다. 다른 체크아웃은 건드리지 않는다.
    Down,
    /// `contracts/openapi.json`을 서버 정의에서 다시 생성한다. `--check`는 그 파일과 거기서 생성한 TS client
    /// (`packages/api-client/src/generated/schema.ts`)의 최신 여부만 본다.
    Openapi {
        #[arg(long)]
        check: bool,
    },
    /// Cargo·pnpm 의존성 라이선스를 허용 목록과 비교한다.
    Licenses,
    /// D-86: 실제 서체의 표·cmap·이체자·저작권·CSS 범위를 검사한다.
    FontsVerify,
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

fn parse_cli() -> Cli {
    Cli::try_parse().unwrap_or_else(|error| {
        // clap의 일반 오류에는 사용자가 잘못 붙인 URI가 포함될 수 있다.
        // 이 명령의 잘못된 입력은 고정 코드만 보고하고 환경도 읽지 않는다.
        if std::env::args_os()
            .nth(1)
            .is_some_and(|arg| arg == "supabase-cloud-precheck" || arg == "supabase-cloud-rebuild")
            && !matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            )
        {
            if std::env::args_os()
                .nth(1)
                .is_some_and(|arg| arg == "supabase-cloud-rebuild")
            {
                std::process::exit(supabase_cloud_rebuild::rejected_arguments());
            }
            std::process::exit(supabase_cloud_precheck::rejected_arguments());
        }
        error.exit()
    })
}

fn main() -> Result<()> {
    let cli = parse_cli();
    let root = repo_root()?;
    match cli.command {
        Command::CiBaseManifest {
            image,
            tag,
            digests_dir,
        } => ci_base_manifest::run(&image, &tag, &digests_dir),
        Command::SupabaseCloudRebuild { phase, connect } => {
            std::process::exit(supabase_cloud_rebuild::run(&root, phase, connect))
        }
        Command::SupabaseCloudRebuildTest => supabase_cloud_rebuild::fixture::local(&root),
        Command::SupabaseCloudPrecheck { connect } => {
            std::process::exit(supabase_cloud_precheck::run(connect))
        }
        Command::Check {
            skip_licenses,
            docs_only,
        } => check::run(&root, skip_licenses, docs_only),
        Command::CiPlan { event } => ci_plan::run(&root, &event),
        Command::TestVerify { require_rootless } => test_verify::run(
            &root,
            if require_rootless {
                test_verify::Requirement::Rootless
            } else {
                test_verify::Requirement::Any
            },
        ),
        Command::TestReport {
            stdout_log,
            stderr_log,
            outcome,
        } => test_report::run(&root, &stdout_log, &stderr_log, outcome),
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
            host,
            ..
        } => test::run(
            &root,
            keep,
            oidc,
            external_services,
            if host {
                test::Execution::Host
            } else {
                test::Execution::Container
            },
        ),
        Command::Dev { oidc, no_web } => dev::run(&root, oidc, !no_web),
        Command::Browser { keep, playwright } => browser::run(&root, keep, &playwright),
        Command::Down => compose::down_all(&root),
        Command::Openapi { check } => openapi::run(&root, check),
        Command::Licenses => licenses::run(&root),
        Command::FontsVerify => fonts::run(&root),
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
mod test_report_stream_tests {
    use super::*;

    #[test]
    fn review_regression_header_inserted_inside_a_test_line() -> Result<()> {
        // 41208793159: stderr 헤더가 stdout의 "... "와 "ok" 사이에 끼어든다.
        let log = "==> expected test executable: admin-a\n\
==> expected test executable: documents-b\n\
==> expected test names: {\"executable\":\"admin-a\",\"tests\":[\"admin::tests::minimum_postgres_version_has_no_upper_bound\"]}\n\
==> expected test names: {\"executable\":\"documents-b\",\"tests\":[\"documents::round_trip\"]}\n\
==> test executable inventory complete: 2\n\
Running unittests src/lib.rs (/work/admin-a)\n\
running 1 test\n\
test admin::tests::minimum_postgres_version_has_no_upper_bound ...      Running unittests src/lib.rs (/work/documents-b)\n\
ok\n\
test result: ok. 1 passed;\n\
running 1 test\n\
test documents::round_trip ... ok\n\
test result: ok. 1 passed;\n";
        let admin_header = "Running unittests src/lib.rs (/work/admin-a)\n";
        let documents_header = "     Running unittests src/lib.rs (/work/documents-b)\n";
        // tee로 합치기 전 각 스트림의 바이트. stdout에는 중간 개행 없이 "... ok"가 남는다.
        let stdout = log.replace(admin_header, "").replace(documents_header, "");
        let stderr = format!("{admin_header}{documents_header}");
        let mut coverage = test_executables::Coverage::default();
        for line in stdout.lines() {
            coverage.observe_stdout(line)?;
        }
        for line in stderr.lines() {
            coverage.observe_stderr(line)?;
        }
        coverage.validate()?;
        let mut mixed = test_executables::Coverage::default();
        assert!(
            log.lines()
                .try_for_each(|line| mixed.observe(line))
                .and_then(|()| mixed.validate())
                .is_err(),
            "combined bytes cannot prove the executable header"
        );
        Ok(())
    }
}
