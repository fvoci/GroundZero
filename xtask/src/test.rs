//! 통합 테스트. 격리된 Compose 프로젝트를 띄우고 역할·마이그레이션을 준비한 뒤 앱 역할로 테스트한다.
//!
//! OIDC 테스트(`oidc_` 접두어)는 실제 Keycloak이 필요하다. `--oidc` 없이는 `--skip oidc_`로 제외하고
//! "NOT RUN"을 출력한다. S3 저장소 테스트(`s3_`)도 시험용 SeaweedFS가 없으면(외부 서비스 모드에서 주소를 주지 않음)
//! 같은 방식으로 제외한다. 제외는 명령 선택이며 통과로 간주하지 않는다.

use std::path::Path;

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, Env};
use crate::supabase::Supabase;

fn run_tests(sh: &Shell, oidc: bool, s3: bool, supabase: bool) -> Result<()> {
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
    if skip.is_empty() {
        cmd!(sh, "cargo test --workspace --locked").run()?;
    } else {
        cmd!(sh, "cargo test --workspace --locked -- {skip...}").run()?;
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

pub fn run(root: &Path, keep: bool, oidc: bool, external_services: bool) -> Result<()> {
    if external_services {
        return run_external(root, oidc);
    }
    let mut compose = Compose::new(root, Env::Test);
    if oidc {
        compose = compose.with_profile("oidc");
    }
    let sh = compose.shell()?;
    println!("==> compose up ({})", compose.project());
    compose.up(&sh)?;

    let result = (|| -> Result<()> {
        let pg_port = compose.host_port(&sh, "postgres", 5432)?;
        println!("==> postgres on 127.0.0.1:{pg_port}");
        let urls = DbUrls::local(pg_port);
        compose::prepare_database(&sh, &urls)?;
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
        run_tests(&sh, oidc, true, false)
    })();

    if keep {
        println!(
            "==> containers kept (project {}); run `cargo xtask down` later",
            compose.project()
        );
    } else {
        println!("==> compose down");
        compose.down(&sh)?;
    }
    result
}

/// 셀프호스트 Supabase(공식 구성의 db·rest·supavisor, D-88)를 DB로 두고 같은 통합 테스트를 돌린다. 검색·S3(·Keycloak)은
/// 시험 Compose에서 PostgreSQL만 빼고 띄운다. 관리자는 superuser가 아닌 `postgres`다. `session_pooler`면 앱 역할
/// 연결을 Supavisor의 session 모드로 바꿔 한 번 더 돌린다(마이그레이션·관리·고정 자료 연결은 직접).
pub fn run_supabase(root: &Path, keep: bool, oidc: bool, session_pooler: bool) -> Result<()> {
    let supabase = Supabase::new(root)?;
    let mut services = Compose::new(root, Env::Test);
    if oidc {
        services = services.with_profile("oidc");
    }
    let sh = services.shell()?;
    let mut wanted = vec!["meilisearch", "seaweedfs"];
    if oidc {
        wanted.push("keycloak");
    }
    println!(
        "==> compose up ({} without postgres, {})",
        services.project(),
        supabase.project()
    );
    println!("==> kept Supavisor role-preparation regression (independent temporary stack)");
    let regression = cmd!(sh, "cargo test --package xtask --locked supabase::tests::kept_pooler_restarts_after_role_preparation -- --ignored --exact --test-threads=1 --nocapture")
        .run();
    let result = regression
        .map_err(anyhow::Error::from)
        .and_then(|()| services.up_services(&sh, &wanted))
        .and_then(|()| {
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
                println!("==> integration tests, direct connection");
                let _app = sh.push_env("DATABASE_URL", &urls.app);
                run_tests(&sh, oidc, true, true)?;
            }
            if session_pooler {
                let pooler = supabase.host_port(&sh, "supavisor", 5432)?;
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
    compose::prepare_database(&sh, &urls)?;
    let _env = sh.push_env("DATABASE_URL", &urls.app);
    let _fixture = sh.push_env("FVOCI_TEST_MIGRATION_URL", &urls.migration);
    let _admin = sh.push_env("FVOCI_TEST_ADMIN_URL", &urls.admin);
    let _search_key = sh.push_env("FVOCI_TEST_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    let _files = sh.push_env("FVOCI_STORAGE_DIR", root.join("target/test-files"));
    let s3 = std::env::var("FVOCI_TEST_S3_ENDPOINT").is_ok_and(|v| !v.trim().is_empty());
    let _s3_env = push_s3_credentials(&sh);
    run_tests(&sh, oidc, s3, false)
}

/// 시험용 SeaweedFS의 버킷과 자격 증명(compose.test.yaml과 같은 명백한 테스트 값).
fn push_s3_credentials(sh: &Shell) -> [xshell::PushEnv<'_>; 3] {
    [
        sh.push_env("FVOCI_TEST_S3_BUCKET", compose::S3_TEST_BUCKET),
        sh.push_env("FVOCI_TEST_S3_ACCESS_KEY", compose::S3_TEST_ACCESS_KEY),
        sh.push_env("FVOCI_TEST_S3_SECRET_KEY", compose::S3_TEST_SECRET_KEY),
    ]
}
