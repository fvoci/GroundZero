//! 통합 테스트. 격리된 Compose 프로젝트를 띄우고 역할·마이그레이션을 준비한 뒤 앱 역할로 테스트한다.
//!
//! OIDC 테스트(`oidc_` 접두어)는 실제 Keycloak이 필요하다. `--oidc` 없이는 `--skip oidc_`로 제외하고
//! "NOT RUN"을 출력한다. S3 저장소 테스트(`s3_`)도 시험용 SeaweedFS가 없으면(외부 서비스 모드에서 주소를 주지 않음)
//! 같은 방식으로 제외한다. 제외는 명령 선택이며 통과로 간주하지 않는다.
//!
//! 통합 테스트 실행 때 개발 준비 SQL 시험([`crate::dev::check_seeds`])을 같은 PostgreSQL 서버의 새 DB에서 돈다(#168).
//! 기본 PostgreSQL 경로는 역할 설치 전에 Cloud 재생성 fixture를 검증한다. 컨테이너 경로의 개발 seed 검사는 그 뒤에 돈다.
//! `--external-services`는 그 시험을 돌 수 없어(컨테이너 안 psql이 필요) "NOT RUN"을 출력한다.
//!
//! 시험 Compose 프로젝트는 체크아웃마다 하나라서(`--supabase`도 검색·S3에 같은 프로젝트를 쓴다) 같은 체크아웃의 실행은
//! `target/test/run.lock`으로 한 번에 하나씩 돈다(#130). 잠그지 않으면 나중 실행이 앞 실행의 DB에 `db bootstrap`·
//! `db migrate`·시험을 겹쳐 돌리고, 먼저 끝난 쪽의 `down`이 다른 쪽 DB를 지운다. SIGINT·SIGTERM·SIGHUP을 받으면 진행 중인
//! 단계를 실패로 끝내고 실패와 같은 정리를 한다(#93, [`compose::watch_interrupts`]). `--external-services`는 Compose를
//! 쓰지 않으므로 잠그지 않는다.
//!
//! amd64·arm64 Docker의 기본 실행은 ci-base 컨테이너에서 Compose 서비스 이름에 직접 연결한다.
//! 일반 Docker는 호스트 UID:GID, rootless는 0:0을 쓴다. 그 밖의 아키텍처는 호스트 경로를 쓴다.
//! userns-remap은 `--host`를 안내하며 거절한다.
//! 컨테이너 빌드의 HOME·Cargo 캐시·target·임시 파일은 `target/test-container` 안에서 호스트 빌드와 분리한다.

use std::ffi::OsString;
use std::path::Path;

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, Env};
use crate::supabase::Supabase;
use crate::test_verify::{self, DockerMode};

#[path = "test_platform.rs"]
pub(crate) mod platform;

#[cfg(all(test, unix))]
#[path = "test_rootful.rs"]
mod rootful_tests;

fn ci_base_image(digest: &str) -> Result<String> {
    let hash = digest.strip_prefix("sha256:").unwrap_or_default();
    anyhow::ensure!(
        hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "FVOCI_TEST_CI_BASE_IMAGE_DIGEST must be a sha256 digest (64 hex digits)"
    );
    Ok(format!("ghcr.io/fvoci/groundzero-ci-base@{digest}"))
}

fn container_args(root: &Path, project: &str, user: &str) -> Vec<OsString> {
    let mut volume = root.as_os_str().to_os_string();
    volume.push(":/work");
    let mut args: Vec<OsString> = ["run", "--rm", "--init", "--name"]
        .into_iter()
        .map(OsString::from)
        .collect();
    args.push(format!("{project}-runner").into());
    // Compose는 config-hash 라벨이 있는 컨테이너만 조회한다. 정의하지 않은 서비스로 표시해
    // 중단된 실행기도 이 프로젝트의 up/down --remove-orphans로 정리한다.
    for (option, value) in [
        ("--label", format!("com.docker.compose.project={project}")),
        (
            "--label",
            "com.docker.compose.service=test-runner".to_owned(),
        ),
        (
            "--label",
            "com.docker.compose.config-hash=xtask-test-runner".to_owned(),
        ),
        ("--label", "com.docker.compose.oneoff=False".to_owned()),
        ("--network", format!("{project}_default")),
        ("--user", user.to_owned()),
    ] {
        args.extend([option.into(), value.into()]);
    }
    args.extend([
        "--volume".into(),
        volume,
        "--workdir".into(),
        "/work".into(),
    ]);
    for value in [
        "HOME=/work/target/test-container/home",
        "CARGO_HOME=/work/target/test-container/cargo",
        "CARGO_TARGET_DIR=/work/target/test-container/build",
        "TMPDIR=/work/target/test-container/tmp",
        "FVOCI_TEST_PG_HOST_PORT=postgres:5432",
        "FVOCI_TEST_SEARCH_URL=http://meilisearch:7700",
        "FVOCI_TEST_S3_ENDPOINT=http://seaweedfs:8333",
    ] {
        args.extend(["--env".into(), value.into()]);
    }
    args
}

#[derive(Debug)]
struct TestContainer {
    args: Vec<OsString>,
    image: String,
}

impl TestContainer {
    fn new(
        root: &Path,
        compose: &Compose,
        sh: &Shell,
        identity: ContainerUser,
        platform: &str,
    ) -> Result<Self> {
        let digest = match std::env::var("FVOCI_TEST_CI_BASE_IMAGE_DIGEST") {
            Ok(digest) => digest,
            Err(std::env::VarError::NotPresent) => cmd!(
                sh,
                "gh api repos/fvoci/GroundZero/actions/variables/CI_BASE_IMAGE_DIGEST --jq .value"
            )
            .quiet()
            .read()
            .context(
                "read CI_BASE_IMAGE_DIGEST with existing gh access, or set FVOCI_TEST_CI_BASE_IMAGE_DIGEST explicitly",
            )?,
            Err(error) => return Err(error).context("read FVOCI_TEST_CI_BASE_IMAGE_DIGEST"),
        };
        let image = ci_base_image(digest.trim())?;
        println!("==> test runner image: {image}");
        platform::require_image(sh, &image, platform)?;
        let uid = cmd!(sh, "id -u").quiet().read()?;
        let gid = cmd!(sh, "id -g").quiet().read()?;
        let user = identity.user(&uid, &gid);
        println!("==> test runner container user: {user}");
        for dir in ["home", "cargo", "build", "tmp"] {
            std::fs::create_dir_all(root.join("target/test-container").join(dir))?;
        }
        let mut args = container_args(root, compose.project(), &user);
        args.extend(["--platform".into(), platform.into()]);
        // 빌드 자원 제한과 사용자가 명시한 시험 동시성만 전달한다. 호스트의 경로·빌드 래퍼는 전달하지 않는다.
        for key in ["CARGO_BUILD_JOBS", "RUST_TEST_THREADS"] {
            if let Ok(value) = sh.var(key) {
                args.extend(["--env".into(), format!("{key}={value}").into()]);
            }
        }
        Ok(Self { args, image })
    }

    fn run(&self, sh: &Shell, env: &[String], command: &[&str]) -> Result<()> {
        let mut args = self.args.clone();
        for value in env {
            args.extend(["--env".into(), value.into()]);
        }
        let image = &self.image;
        compose::check_interrupt()?;
        cmd!(sh, "docker {args...} {image} {command...}").run()?;
        Ok(())
    }

    fn prepare_database(&self, sh: &Shell, urls: &DbUrls) -> Result<()> {
        let env = [
            format!("DATABASE_ADMIN_URL={}", urls.admin),
            format!("DATABASE_MIGRATION_URL={}", urls.migration),
            format!(
                "FVOCI_DB_MIGRATOR_PASSWORD={}",
                compose::TEST_MIGRATOR_PASSWORD
            ),
            format!("FVOCI_DB_APP_PASSWORD={}", compose::TEST_APP_PASSWORD),
        ];
        for operation in ["bootstrap", "migrate"] {
            self.run(
                sh,
                &env,
                &[
                    "cargo",
                    "run",
                    "--quiet",
                    "--package",
                    "fvoci-server",
                    "--locked",
                    "--",
                    "db",
                    operation,
                ],
            )?;
        }
        Ok(())
    }

    fn test(&self, sh: &Shell, oidc: bool) -> Result<()> {
        let mut command = vec!["cargo", "xtask", "test", "--external-services"];
        let mut env = Vec::new();
        if oidc {
            command.push("--oidc");
            env.push(format!(
                "FVOCI_TEST_OIDC_ISSUER=http://keycloak:8080/realms/{}",
                compose::OIDC_REALM
            ));
        }
        self.run(sh, &env, &command)
    }
}

fn run_tests(sh: &Shell, oidc: bool, s3: bool, supabase: bool) -> Result<()> {
    // 실행 인자·프로필·기능·Shell 환경을 목록 빌드와 공유한다. doctest도 기존 cargo test가 그대로 실행한다.
    let cargo_args = ["test", "--workspace", "--locked", "--color", "never"];
    let mut skip: Vec<&str> = Vec::new();
    if !oidc {
        skip.extend(["--skip", "oidc_"]);
    }
    if !s3 {
        skip.extend(["--skip", "s3_"]);
    }
    if !supabase {
        skip.extend(["--skip", "supabase_"]);
    }
    crate::test_executables::emit_inventory(sh, &cargo_args, &skip)?;
    if skip.is_empty() {
        cmd!(sh, "cargo {cargo_args...}").run()?;
    } else {
        cmd!(sh, "cargo {cargo_args...} -- {skip...}").run()?;
    }
    if !oidc {
        println!("NOT RUN: OIDC integration tests (oidc_*). Use `cargo xtask test --oidc`.");
    }
    if !s3 {
        println!(
            "NOT RUN: S3 storage tests (s3_*). Set FVOCI_TEST_S3_ENDPOINT for external services."
        );
    }
    if !supabase {
        println!(
            "NOT RUN: self-hosted Supabase tests (supabase_*). Use `cargo xtask test --supabase`."
        );
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub enum Execution {
    Container,
    Host,
}

#[derive(Debug, PartialEq, Eq)]
enum ExecutionChoice {
    Container(ContainerUser),
    Host(&'static str),
    Unsupported(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainerUser {
    Host,
    Rootless,
}

impl ContainerUser {
    fn user(self, uid: &str, gid: &str) -> String {
        match self {
            Self::Host => format!("{}:{}", uid.trim(), gid.trim()),
            Self::Rootless => "0:0".to_owned(),
        }
    }
}

fn select_execution(architecture: &str, mode: DockerMode) -> ExecutionChoice {
    if mode == DockerMode::UsernsRemap {
        ExecutionChoice::Unsupported(
            "userns-remap Docker는 아직 지원하지 않습니다. --host로 실행하세요",
        )
    } else if matches!(architecture, "amd64" | "x86_64" | "arm64" | "aarch64") {
        ExecutionChoice::Container(if mode == DockerMode::Rootless {
            ContainerUser::Rootless
        } else {
            ContainerUser::Host
        })
    } else {
        ExecutionChoice::Host("ci-base 이미지가 amd64·arm64만 지원하므로 호스트 경로로 실행합니다")
    }
}

fn prepare_test_container(
    root: &Path,
    compose: &Compose,
    sh: &Shell,
    execution: Execution,
) -> Result<Option<TestContainer>> {
    if let Execution::Host = execution {
        return Ok(None);
    }
    let platform = platform::daemon(sh)?;
    let architecture = platform
        .strip_prefix("linux/")
        .context("Docker must report a Linux platform")?;
    match select_execution(architecture, test_verify::docker_mode(sh)?) {
        ExecutionChoice::Container(identity) => Ok(Some(TestContainer::new(
            root, compose, sh, identity, &platform,
        )?)),
        ExecutionChoice::Host(reason) => {
            println!("==> {reason}");
            Ok(None)
        }
        ExecutionChoice::Unsupported(reason) => anyhow::bail!("{reason}"),
    }
}

pub fn run(
    root: &Path,
    keep: bool,
    oidc: bool,
    external_services: bool,
    execution: Execution,
) -> Result<()> {
    if external_services {
        return run_external(root, oidc);
    }
    let mut compose = Compose::new(root, Env::Test);
    if oidc {
        compose = compose.with_profile("oidc");
    }
    let _lock = compose.lock_run()?;
    compose::watch_interrupts()?;
    let sh = compose.shell()?;
    let runner = prepare_test_container(root, &compose, &sh, execution)?;
    println!("==> compose up ({})", compose.project());
    print_recovery(&compose);

    // 기동이 중간에 실패하거나 중단돼도 일부 뜬 컨테이너를 아래에서 정리한다.
    let result = compose::check_interrupt()
        .and_then(|()| compose.up(&sh))
        .and_then(|()| {
            if let Some(runner) = &runner {
                runner.test(&sh, oidc)?;
                crate::dev::check_seeds_with(root, &compose, &sh, |database| {
                    runner.prepare_database(&sh, &DbUrls::at_database("postgres:5432", database))
                })?;
                return Ok(());
            }
            let pg_port = compose.host_port(&sh, "postgres", 5432)?;
            println!("==> postgres on 127.0.0.1:{pg_port}");
            let urls = DbUrls::local(pg_port);
            compose::check_interrupt()?;
            crate::supabase_cloud_rebuild::fixture::external(
                root,
                &format!("127.0.0.1:{pg_port}"),
            )?;
            compose::prepare_database(&sh, &urls)?;
            compose::check_interrupt()?;
            crate::dev::check_seeds(root, &compose, &sh, pg_port)?;
            let _env = sh.push_env("DATABASE_URL", &urls.app);
            // 시험 데이터의 시각을 옮기는 일(예: 휴지통 만료)에만 쓰는 테스트 전용 마이그레이션 역할 주소.
            let _fixture = sh.push_env("FVOCI_TEST_MIGRATION_URL", &urls.migration);
            let _admin = sh.push_env("FVOCI_TEST_ADMIN_URL", &urls.admin);
            let meili = compose.host_port(&sh, "meilisearch", 7700)?;
            println!("==> meilisearch on 127.0.0.1:{meili}");
            let _search = sh.push_env("FVOCI_TEST_SEARCH_URL", format!("http://127.0.0.1:{meili}"));
            let _files = sh.push_env("FVOCI_STORAGE_DIR", root.join("target/test-files"));
            let _search_key = sh.push_env("FVOCI_TEST_SEARCH_KEY", compose::SEARCH_TEST_KEY);
            let s3 = compose.host_port(&sh, "seaweedfs", 8333)?;
            println!("==> seaweedfs s3 on 127.0.0.1:{s3}");
            let _s3 = sh.push_env("FVOCI_TEST_S3_ENDPOINT", format!("http://127.0.0.1:{s3}"));
            let _s3_env = push_s3_credentials(&sh);
            let _env2 = if oidc {
                let kc = compose.host_port(&sh, "keycloak", 8080)?;
                println!("==> keycloak on 127.0.0.1:{kc}");
                Some(sh.push_env(
                    "FVOCI_TEST_OIDC_ISSUER",
                    format!("http://127.0.0.1:{kc}/realms/{}", compose::OIDC_REALM),
                ))
            } else {
                None
            };
            compose::check_interrupt()?;
            run_tests(&sh, oidc, true, false)
        });

    finish_run(root, &compose, &sh, keep, result)
}

fn finish_run(
    root: &Path,
    compose: &Compose,
    sh: &Shell,
    keep: bool,
    result: Result<()>,
) -> Result<()> {
    let cleanup = if keep {
        println!(
            "==> containers kept (project {}); run `cargo xtask down` later",
            compose.project()
        );
        Ok(())
    } else {
        println!("==> compose down");
        compose.down(sh)
    };
    // 실패·중단 뒤에도 두 검사를 모두 실행한다. 원래 오류를 우선하되 추가 실패는 로그에 남긴다.
    let ownership = test_verify::ownership(root, sh);
    let resources = if keep {
        Ok(())
    } else {
        test_verify::resources_absent(sh, compose.project())
    };
    for error in [&cleanup, &ownership, &resources]
        .into_iter()
        .filter_map(|r| r.as_ref().err())
    {
        eprintln!(
            "verification/cleanup of {} failed: {error:#}",
            compose.project()
        );
    }
    compose::with_interrupt(result.and(cleanup).and(ownership).and(resources))
}

fn print_recovery(compose: &Compose) {
    println!(
        "==> if this run is killed, stop only its containers (tmpfs DBs are lost) from the repository root: {}",
        compose.recovery_command()
    );
}

/// 셀프호스트 Supabase(공식 구성의 db·rest·supavisor, D-88)를 DB로 두고 같은 통합 테스트를 돌린다. 검색·S3(·Keycloak)은
/// 시험 Compose에서 띄운다. 관리자는 superuser가 아닌 `postgres`다. `session_pooler`면 앱 역할
/// 연결을 Supavisor의 session 모드로 바꿔 한 번 더 돌린다(마이그레이션·관리·고정 자료 연결은 직접).
///
/// 시험 Compose의 PostgreSQL은 통합 테스트에 쓰지 않고 개발 준비 SQL 시험([`crate::dev::check_seeds`])에만 쓴다. 개발
/// 실행처럼 그 컨테이너 안의 psql로 준비 SQL을 넣어야 하는데, CI의 `integration`·`oidc` job은 베이스 이미지 안에서 돌아
/// psql도 Docker도 없으므로 CI에서는 이 경로(`supabase` job)가 그 시험을 돈다(#168).
pub fn run_supabase(root: &Path, keep: bool, oidc: bool, session_pooler: bool) -> Result<()> {
    let mut services = Compose::new(root, Env::Test);
    if oidc {
        services = services.with_profile("oidc");
    }
    let _lock = services.lock_run()?;
    compose::watch_interrupts()?;
    compose::with_interrupt(supabase_stack(root, &services, keep, oidc, session_pooler))
}

fn supabase_stack(
    root: &Path,
    services: &Compose,
    keep: bool,
    oidc: bool,
    session_pooler: bool,
) -> Result<()> {
    let supabase = Supabase::new(root)?;
    let sh = services.shell()?;
    let mut wanted = vec!["postgres", "meilisearch", "seaweedfs"];
    if oidc {
        wanted.push("keycloak");
    }
    if session_pooler {
        compose::check_interrupt()?;
        println!("==> kept Supavisor role-preparation regression (independent temporary stack)");
        cmd!(sh, "cargo test --package xtask --locked supabase::tests::kept_pooler_restarts_after_role_preparation -- --ignored --exact --test-threads=1 --nocapture")
            .run()
            .context("independent Supavisor regression failed before main stack setup")?;
    }
    println!(
        "==> compose up ({}, {})",
        services.project(),
        supabase.project()
    );
    print_recovery(services);
    // 이 실행은 Supabase 프로젝트도 띄우므로 그 복구 명령도 알린다(#93).
    println!(
        "==> and this run's Supabase project: {}",
        supabase.recovery_command()
    );
    // 선택 회귀 시험에서 `?`로 나가면 아직 아무것도 띄우지 않았으므로 정리할 것이 없다.
    compose::check_interrupt()?;
    let result = services
        .up_services(&sh, &wanted)
        .and_then(|()| {
            let pg_port = services.host_port(&sh, "postgres", 5432)?;
            compose::check_interrupt()?;
            crate::dev::check_seeds(root, services, &sh, pg_port)?;
            compose::check_interrupt()?;
            let urls = supabase.prepare_database(&sh)?;
            let db = supabase.host_port(&sh, "db", 5432)?;
            let rest = supabase.host_port(&sh, "rest", 3000)?;
            println!("==> supabase db on 127.0.0.1:{db}, rest on 127.0.0.1:{rest}");
            let _fixture = sh.push_env("FVOCI_TEST_MIGRATION_URL", &urls.migration);
            let _admin = sh.push_env("FVOCI_TEST_ADMIN_URL", &urls.admin);
            let _rest = sh.push_env(
                "FVOCI_TEST_SUPABASE_REST_URL",
                format!("http://127.0.0.1:{rest}"),
            );
            let meili = services.host_port(&sh, "meilisearch", 7700)?;
            let _search = sh.push_env("FVOCI_TEST_SEARCH_URL", format!("http://127.0.0.1:{meili}"));
            let _search_key = sh.push_env("FVOCI_TEST_SEARCH_KEY", compose::SEARCH_TEST_KEY);
            let _files = sh.push_env("FVOCI_STORAGE_DIR", root.join("target/test-files"));
            let s3 = services.host_port(&sh, "seaweedfs", 8333)?;
            let _s3 = sh.push_env("FVOCI_TEST_S3_ENDPOINT", format!("http://127.0.0.1:{s3}"));
            let _s3_env = push_s3_credentials(&sh);
            let _oidc = if oidc {
                let kc = services.host_port(&sh, "keycloak", 8080)?;
                Some(sh.push_env(
                    "FVOCI_TEST_OIDC_ISSUER",
                    format!("http://127.0.0.1:{kc}/realms/{}", compose::OIDC_REALM),
                ))
            } else {
                None
            };
            // 시험마다 앱 상태(연결 풀 최대 16)를 따로 만드므로 한 바이너리에서 동시에 도는 시험을 4개로 묶는다. session
            // 모드는 클라이언트 연결마다 서버 연결을 잡고, 풀(`POOLER_DEFAULT_POOL_SIZE` 64)을 넘는 클라이언트는 기다리지 않고
            // 거절한다(`EMAXCONNSESSION`, 시험에는 500 `XX000`): 64 / 16 = 4. 직접 연결도 db의 `max_connections` 100에서
            // superuser 몫 3과 Supabase 자체 서비스 몫(약 10)을 빼면 남는 것이 적다. 기본값(코어 수)이면 32코어 기계에서 시험이
            // 많은 바이너리(collab)가 session 단계에서 풀과 db 연결을 다 썼고 직접 연결 단계에서도 97개 가운데 94개까지 썼다.
            // CI 러너는 4코어라 그대로다.
            let _threads = sh.push_env("RUST_TEST_THREADS", "4");
            {
                compose::check_interrupt()?;
                println!("==> integration tests, direct connection");
                let _app = sh.push_env("DATABASE_URL", &urls.app);
                run_tests(&sh, oidc, true, true)?;
            }
            if session_pooler {
                let pooler = supabase.host_port(&sh, "supavisor", 5432)?;
                compose::check_interrupt()?;
                println!("==> integration tests, app role through the Supavisor session pooler on 127.0.0.1:{pooler}");
                let _app = sh.push_env("DATABASE_URL", DbUrls::supabase_session_pooler(pooler));
                // 테넌트는 `postgres` DB 하나만 받는다. 새 DB를 만드는 테스트(백업·검색 대기열 등)는 직접 연결로.
                let _direct = sh.push_env("FVOCI_TEST_DIRECT_APP_URL", &urls.app);
                run_tests(&sh, oidc, true, true)?;
            } else {
                println!("NOT RUN: tests through the Supavisor session pooler. Add --session-pooler.");
            }
            Ok(())
        });
    if keep {
        println!(
            "==> containers kept (projects {}, {}); run `cargo xtask down` later",
            services.project(),
            supabase.project()
        );
    } else {
        println!("==> compose down");
        // 두 프로젝트를 모두 정리하고, 원래 기동·테스트 오류를 먼저 돌려준다. 정리 실패도 버리지 않고 알린다.
        let cleanup = [
            (supabase.project().to_owned(), supabase.down(&sh)),
            (services.project().to_owned(), services.down(&sh)),
        ];
        for (project, outcome) in &cleanup {
            if let Err(err) = outcome {
                eprintln!("cleanup of {project} failed: {err:#}");
            }
        }
        result?;
        for (_, outcome) in cleanup {
            outcome?;
        }
        return Ok(());
    }
    result
}

/// 외부에서 준비된 서비스로 테스트한다(CI job). `FVOCI_TEST_PG_HOST_PORT`(PostgreSQL `호스트:포트`)와
/// `FVOCI_TEST_SEARCH_URL`, `--oidc`면 `FVOCI_TEST_OIDC_ISSUER`도 없으면 실패한다. 세 역할 URL과 `DATABASE_URL`은
/// 그 호스트·포트와 테스트 자격 증명으로 만든다. `FVOCI_TEST_S3_ENDPOINT`가 없으면 S3 테스트를 뺀다.
fn run_external(root: &Path, oidc: bool) -> Result<()> {
    let sh = Shell::new()?;
    sh.change_dir(root);
    let host_port = std::env::var("FVOCI_TEST_PG_HOST_PORT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .context("--external-services requires FVOCI_TEST_PG_HOST_PORT (e.g. postgres:5432)")?;
    if oidc
        && std::env::var("FVOCI_TEST_OIDC_ISSUER")
            .map(|v| v.trim().is_empty())
            .unwrap_or(true)
    {
        anyhow::bail!("--oidc with --external-services requires FVOCI_TEST_OIDC_ISSUER");
    }
    if std::env::var("FVOCI_TEST_SEARCH_URL")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        anyhow::bail!(
            "--external-services requires FVOCI_TEST_SEARCH_URL (e.g. http://meilisearch:7700)"
        );
    }
    println!("==> using external services at {host_port}");
    let urls = DbUrls::at(&host_port);
    crate::supabase_cloud_rebuild::fixture::external(root, &host_port)?;
    compose::prepare_database(&sh, &urls)?;
    let _env = sh.push_env("DATABASE_URL", &urls.app);
    let _fixture = sh.push_env("FVOCI_TEST_MIGRATION_URL", &urls.migration);
    let _admin = sh.push_env("FVOCI_TEST_ADMIN_URL", &urls.admin);
    let _search_key = sh.push_env("FVOCI_TEST_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    let _files = sh.push_env("FVOCI_STORAGE_DIR", root.join("target/test-files"));
    let s3 = std::env::var("FVOCI_TEST_S3_ENDPOINT").is_ok_and(|v| !v.trim().is_empty());
    let _s3_env = push_s3_credentials(&sh);
    run_tests(&sh, oidc, s3, false)?;
    println!(
        "NOT RUN: dev seed check inside --external-services (checked by the outer local Compose runner or the CI supabase job)."
    );
    Ok(())
}

/// 시험용 SeaweedFS의 버킷과 자격 증명(compose.test.yaml과 같은 명백한 테스트 값).
fn push_s3_credentials(sh: &Shell) -> [xshell::PushEnv<'_>; 3] {
    [
        sh.push_env("FVOCI_TEST_S3_BUCKET", compose::S3_TEST_BUCKET),
        sh.push_env("FVOCI_TEST_S3_ACCESS_KEY", compose::S3_TEST_ACCESS_KEY),
        sh.push_env("FVOCI_TEST_S3_SECRET_KEY", compose::S3_TEST_SECRET_KEY),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::tests::{assert_held, scratch_root};

    #[test]
    fn rootless_amd64_uses_the_container() {
        let mode = DockerMode::from_options(&["name=rootless".to_owned()]);
        assert_eq!(
            select_execution("amd64", mode),
            ExecutionChoice::Container(ContainerUser::Rootless)
        );
    }

    #[test]
    fn architecture_and_docker_mode_select_the_runner_and_identity() {
        for architecture in ["amd64", "x86_64", "arm64", "aarch64"] {
            for mode in [DockerMode::Rootful, DockerMode::Rootless] {
                let choice = select_execution(architecture, mode);
                let ExecutionChoice::Container(identity) = choice else {
                    panic!("{architecture} {mode:?}: {choice:?}");
                };
                let expected = if mode == DockerMode::Rootless {
                    "0:0"
                } else {
                    "1234:5678"
                };
                let args = container_args(
                    Path::new("/work"),
                    "example",
                    &identity.user("1234\n", "5678\n"),
                );
                assert!(
                    args.windows(2)
                        .any(|pair| pair[0] == "--user" && pair[1] == expected),
                    "{architecture} {mode:?}"
                );
            }
            assert_eq!(
                select_execution(architecture, DockerMode::UsernsRemap),
                ExecutionChoice::Unsupported(
                    "userns-remap Docker는 아직 지원하지 않습니다. --host로 실행하세요"
                ),
                "{architecture}"
            );
        }
    }

    #[test]
    fn other_architectures_use_the_host_unless_userns_remapped() {
        for architecture in ["riscv64", "s390x", "arm", "unknown"] {
            for mode in [DockerMode::Rootful, DockerMode::Rootless] {
                assert!(
                    matches!(select_execution(architecture, mode), ExecutionChoice::Host(reason) if !reason.is_empty()),
                    "{architecture} {mode:?}"
                );
            }
            assert!(matches!(
                select_execution(architecture, DockerMode::UsernsRemap),
                ExecutionChoice::Unsupported(_)
            ));
        }
    }

    #[test]
    fn security_options_distinguish_rootless_userns_and_both_orders() {
        for (options, expected) in [
            (vec![], DockerMode::Rootful),
            (
                vec!["name=seccomp,profile=builtin", "name=cgroupns"],
                DockerMode::Rootful,
            ),
            (vec!["name=rootless"], DockerMode::Rootless),
            (vec!["name=userns"], DockerMode::UsernsRemap),
            (vec!["name=rootless", "name=userns"], DockerMode::Rootless),
            (vec!["name=userns", "name=rootless"], DockerMode::Rootless),
            (
                vec!["name=rootless-extra", "name=userns-extra"],
                DockerMode::Rootful,
            ),
        ] {
            let options: Vec<String> = options.into_iter().map(str::to_owned).collect();
            assert_eq!(DockerMode::from_options(&options), expected, "{options:?}");
        }
    }

    #[test]
    fn ci_base_image_requires_a_sha256_digest() -> Result<()> {
        let digest = format!("sha256:{}", "a".repeat(64));
        assert_eq!(
            ci_base_image(&digest)?,
            format!("ghcr.io/fvoci/groundzero-ci-base@{digest}")
        );
        for invalid in [
            "latest",
            "",
            "sha256:abc",
            &format!("sha256:{}", "g".repeat(64)),
        ] {
            assert!(ci_base_image(invalid).is_err(), "accepted {invalid:?}");
        }
        Ok(())
    }

    #[test]
    fn container_uses_its_network_paths_and_the_callers_identity() {
        let root = Path::new("/checkout with spaces/$literal");
        let args = container_args(root, "fvoci-example-test", "1234:5678");
        let pair = |option: &str, value: &str| {
            args.windows(2)
                .any(|pair| pair[0] == option && pair[1] == value)
        };
        assert!(pair("--network", "fvoci-example-test_default"));
        assert!(pair("--user", "1234:5678"));
        assert!(pair("--volume", "/checkout with spaces/$literal:/work"));
        assert!(pair("--env", "HOME=/work/target/test-container/home"));
        assert!(pair(
            "--env",
            "CARGO_HOME=/work/target/test-container/cargo"
        ));
        assert!(pair(
            "--env",
            "CARGO_TARGET_DIR=/work/target/test-container/build"
        ));
        assert!(pair("--env", "TMPDIR=/work/target/test-container/tmp"));
        assert!(pair("--env", "FVOCI_TEST_PG_HOST_PORT=postgres:5432"));
        assert!(pair(
            "--env",
            "FVOCI_TEST_SEARCH_URL=http://meilisearch:7700"
        ));
        assert!(pair(
            "--env",
            "FVOCI_TEST_S3_ENDPOINT=http://seaweedfs:8333"
        ));
        assert!(pair(
            "--label",
            "com.docker.compose.project=fvoci-example-test"
        ));
        assert!(pair(
            "--label",
            "com.docker.compose.config-hash=xtask-test-runner"
        ));
        assert!(pair("--label", "com.docker.compose.oneoff=False"));
        assert!(pair("--name", "fvoci-example-test-runner"));
        assert!(
            args.iter()
                .all(|arg| !arg.to_string_lossy().contains("docker.sock"))
        );
        assert!(
            args.iter()
                .all(|arg| !arg.to_string_lossy().contains("RUST_TEST_THREADS"))
        );
    }

    /// 같은 체크아웃의 시험 실행(기본·`--supabase`)은 앞 실행이 잠금을 쥐는 동안 Compose를 건드리기 전에 실패한다.
    /// 잠금이 막지 못하면 Compose 파일이 없는 가짜 체크아웃에서 `docker compose` 오류로 실패한다.
    #[test]
    fn held_lock_rejects_test_runs_before_compose() -> Result<()> {
        let root = scratch_root()?;
        let holder = Compose::new(&root, Env::Test).lock_run()?;
        for execution in [Execution::Container, Execution::Host] {
            assert_held(run(&root, false, false, false, execution), "test");
            assert_held(run(&root, true, true, false, execution), "test");
        }
        assert_held(run_supabase(&root, false, false, false), "test");
        drop(holder);
        drop(Compose::new(&root, Env::Test).lock_run()?);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }
}
