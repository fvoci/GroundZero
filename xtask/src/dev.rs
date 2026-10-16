//! 개발 실행. Compose를 띄우고 서버를 전경에서 실행한다. 웹 개발 서버는 `pnpm --filter @fvoci/web dev`.

use std::path::Path;

use anyhow::Result;
use xshell::cmd;

use crate::compose::{self, Compose, Env};

pub fn run(root: &Path) -> Result<()> {
    let compose = Compose::new(root, Env::Dev);
    let sh = compose.shell()?;
    println!("==> compose up ({})", compose.project());
    compose.up(&sh)?;

    let pg_port = compose.host_port(&sh, "postgres", 5432)?;
    let meili_port = compose.host_port(&sh, "meilisearch", 7700)?;
    let database_url = compose::test_database_url(pg_port);
    let bind = std::env::var("FVOCI_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());

    println!("postgres     127.0.0.1:{pg_port}");
    println!("meilisearch  127.0.0.1:{meili_port}");
    println!("server       {bind}");
    println!("web          pnpm --filter @fvoci/web dev   (VITE_API_URL=http://{bind})");
    println!("stop         Ctrl-C, then `cargo xtask down` to remove containers");

    let _db = sh.push_env("DATABASE_URL", database_url);
    let _bind = sh.push_env("FVOCI_BIND", bind);
    cmd!(sh, "cargo run --package fvoci-server --locked").run()?;
    Ok(())
}
