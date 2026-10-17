//! 통합 테스트. 격리된 Compose 프로젝트를 띄우고 역할·마이그레이션을 준비한 뒤 앱 역할로 테스트한다.
//!
//! OIDC 테스트(`oidc_` 접두어)는 실제 Keycloak이 필요하다. `--oidc` 없이는 `--skip oidc_`로 제외하고
//! "NOT RUN"을 출력한다. S3 저장소 테스트(`s3_`)도 시험용 SeaweedFS가 없으면(외부 서비스 모드에서 주소를 주지 않음)
//! 같은 방식으로 제외한다. 제외는 명령 선택이며 통과로 간주하지 않는다.

use std::path::Path;

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, Env};

fn run_tests(sh: &Shell, oidc: bool, s3: bool) -> Result<()> {
    let mut skip: Vec<&str> = Vec::new();
    if !oidc {
        skip.extend(["--skip", "oidc_"]);
    }
    if !s3 {
        skip.extend(["--skip", "s3_"]);
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
        let meili = compose.host_port(&sh, "meilisearch", 7700)?;
        println!("==> meilisearch on 127.0.0.1:{meili}");
        let _search = sh.push_env("FVOCI_TEST_SEARCH_URL", format!("http://127.0.0.1:{meili}"));
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
        run_tests(&sh, oidc, true)
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

/// 외부에서 준비된 서비스로 테스트한다(CI 컨테이너 job). 필요한 환경 변수가 없으면 실패한다.
/// `DATABASE_ADMIN_URL`의 호스트로 세 역할 URL을 만든다.
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
    let _search_key = sh.push_env("FVOCI_TEST_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    let s3 = std::env::var("FVOCI_TEST_S3_ENDPOINT").is_ok_and(|v| !v.trim().is_empty());
    let _s3_env = push_s3_credentials(&sh);
    run_tests(&sh, oidc, s3)
}

/// 시험용 SeaweedFS의 버킷과 자격 증명(compose.test.yaml과 같은 명백한 테스트 값).
fn push_s3_credentials(sh: &Shell) -> [xshell::PushEnv<'_>; 3] {
    [
        sh.push_env("FVOCI_TEST_S3_BUCKET", compose::S3_TEST_BUCKET),
        sh.push_env("FVOCI_TEST_S3_ACCESS_KEY", compose::S3_TEST_ACCESS_KEY),
        sh.push_env("FVOCI_TEST_S3_SECRET_KEY", compose::S3_TEST_SECRET_KEY),
    ]
}
