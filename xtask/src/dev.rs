//! 개발 실행. Compose를 띄우고 역할·마이그레이션을 준비한 뒤 서버를 전경에서 실행한다.
//!
//! - `--oidc`: 개발용 Keycloak을 함께 띄우고 서버에 `FVOCI_OIDC_*`를 넣는다. 로그인 화면이 필요할 때.
//! - 웹은 별도 터미널에서 `pnpm --filter @fvoci/web dev`. 공개 URL 기본값은 Vite 원점
//!   `http://127.0.0.1:5173`이며 `FVOCI_PUBLIC_URL`로 바꾼다. 쿠키·리다이렉트가 이 원점에 맞춰진다.

use std::path::Path;

use anyhow::Result;
use xshell::cmd;

use crate::compose::{self, Compose, DbUrls, Env};

pub fn run(root: &Path, oidc: bool) -> Result<()> {
    let mut compose = Compose::new(root, Env::Dev);
    if oidc {
        compose = compose.with_profile("oidc");
    }
    let sh = compose.shell()?;
    println!("==> compose up ({})", compose.project());
    compose.up(&sh)?;

    let pg_port = compose.host_port(&sh, "postgres", 5432)?;
    let meili_port = compose.host_port(&sh, "meilisearch", 7700)?;
    let urls = DbUrls::local(pg_port);
    compose::prepare_database(&sh, &urls)?;

    let bind = std::env::var("FVOCI_BIND").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());
    let public =
        std::env::var("FVOCI_PUBLIC_URL").unwrap_or_else(|_| "http://127.0.0.1:5173".to_owned());

    println!("postgres     127.0.0.1:{pg_port}");
    println!("meilisearch  127.0.0.1:{meili_port}");
    println!("server       {bind}");
    println!("public url   {public}");
    println!(
        "web          VITE_API_URL=http://{bind} pnpm --filter @fvoci/web dev --host 127.0.0.1"
    );
    println!("stop         Ctrl-C, then `cargo xtask down` to remove containers");

    let _db = sh.push_env("DATABASE_URL", &urls.app);
    let _bind = sh.push_env("FVOCI_BIND", &bind);
    let _public = sh.push_env("FVOCI_PUBLIC_URL", &public);
    let _search = sh.push_env("FVOCI_SEARCH_URL", format!("http://127.0.0.1:{meili_port}"));
    let _search_key = sh.push_env("FVOCI_SEARCH_KEY", compose::SEARCH_TEST_KEY);
    // 개발 중에는 검색 색인·반복 생성이 빨리 보이도록 짧게.
    let interval = std::env::var("FVOCI_WORKER_INTERVAL_SECS").unwrap_or_else(|_| "5".to_owned());
    let _interval = sh.push_env("FVOCI_WORKER_INTERVAL_SECS", interval);
    // 개발 파일은 빌드 산출물 옆에 둔다(저장소에 섞이지 않게).
    let files =
        std::env::var("FVOCI_STORAGE_DIR").unwrap_or_else(|_| "target/dev-files".to_owned());
    let _files = sh.push_env("FVOCI_STORAGE_DIR", files);
    let _oidc = if oidc {
        let kc = compose.host_port(&sh, "keycloak", 8080)?;
        let issuer = format!("http://127.0.0.1:{kc}/realms/{}", compose::OIDC_REALM);
        println!(
            "keycloak     {issuer}  (users: test-user / test-user-2, see docs/development.md)"
        );
        Some((
            sh.push_env("FVOCI_OIDC_ISSUER", issuer),
            sh.push_env("FVOCI_OIDC_CLIENT_ID", compose::OIDC_CLIENT_ID),
            sh.push_env("FVOCI_OIDC_CLIENT_SECRET", compose::OIDC_CLIENT_SECRET),
        ))
    } else {
        None
    };
    cmd!(sh, "cargo run --package fvoci-server --locked").run()?;
    Ok(())
}
