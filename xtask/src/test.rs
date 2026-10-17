//! 통합 테스트. 격리된 Compose 프로젝트를 띄우고 역할·마이그레이션을 준비한 뒤 앱 역할로 테스트한다.
//!
//! OIDC 테스트(`oidc_` 접두어)는 실제 Keycloak이 필요하다. `--oidc` 없이는 `--skip oidc_`로 제외하고
//! "NOT RUN"을 출력한다. 제외는 명령 선택이며 통과로 간주하지 않는다.

use std::path::Path;

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, Env};

fn run_tests(sh: &Shell, oidc: bool) -> Result<()> {
    if oidc {
        cmd!(sh, "cargo test --workspace --locked").run()?;
    } else {
        cmd!(sh, "cargo test --workspace --locked -- --skip oidc_").run()?;
        println!("NOT RUN: OIDC integration tests (oidc_*). Use `cargo xtask test --oidc`.");
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
        run_tests(&sh, oidc)
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
    run_tests(&sh, oidc)
}
