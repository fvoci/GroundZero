//! 통합 테스트. 격리된 Compose 프로젝트를 띄우고 `DATABASE_URL`을 주입해 전체 테스트를 돌린다.

use std::path::Path;

use anyhow::Result;
use xshell::cmd;

use crate::compose::{self, Compose, Env};

pub fn run(root: &Path, keep: bool, oidc: bool, external_services: bool) -> Result<()> {
    if external_services {
        return run_external(root);
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
        let database_url = compose::test_database_url(pg_port);
        println!("==> postgres on 127.0.0.1:{pg_port}");
        let _env = sh.push_env("DATABASE_URL", &database_url);
        let _env2 = oidc.then(|| {
            let kc = compose.host_port(&sh, "keycloak", 8080).unwrap_or(0);
            println!("==> keycloak on 127.0.0.1:{kc}");
            sh.push_env(
                "FVOCI_TEST_OIDC_ISSUER",
                format!("http://127.0.0.1:{kc}/realms/fvoci-test"),
            )
        });
        cmd!(sh, "cargo test --workspace --locked").run()?;
        Ok(())
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

/// 외부에서 준비된 서비스로 테스트한다. 필요한 환경 변수가 없으면 건너뛰지 않고 실패한다.
fn run_external(root: &Path) -> Result<()> {
    let sh = xshell::Shell::new()?;
    sh.change_dir(root);
    if std::env::var("DATABASE_URL")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        anyhow::bail!("--external-services requires DATABASE_URL to be set");
    }
    println!("==> using external services from environment");
    cmd!(sh, "cargo test --workspace --locked").run()?;
    Ok(())
}
