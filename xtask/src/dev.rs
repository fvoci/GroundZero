//! 개발 실행. Compose를 띄우고 역할·마이그레이션을 준비한 뒤 웹(Vite)과 서버를 실행한다.
//!
//! - `--oidc`: 개발용 Keycloak을 함께 띄우고 서버에 `FVOCI_OIDC_*`를 넣는다. 테스트 realm의 가짜 사용자로 바로
//!   로그인할 수 있다: `test-user`는 초기 관리자(D-51)로 '개발' 워크스페이스의 소유자, `test-user-2`는 그 워크스페이스의
//!   작성자. 둘이 함께 쓰는 실시간 공동 편집 문서도 한 번만 만든다(`deploy/postgres/dev-seed.sql`).
//! - `--no-web`: 웹 개발 서버를 띄우지 않는다(API만). 기본은 `pnpm --filter @fvoci/web dev`를 함께 띄운다.
//! - 공개 URL 기본값은 Vite 원점 `http://127.0.0.1:5173`이며 `FVOCI_PUBLIC_URL`로 바꾼다. 쿠키·리다이렉트가 이
//!   원점에 맞춰진다.

use std::path::Path;
use std::process::{Child, Command};

use anyhow::{Context, Result};
use xshell::{Shell, cmd};

use crate::compose::{self, Compose, DbUrls, Env};

pub fn run(root: &Path, oidc: bool, web: bool) -> Result<()> {
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
    println!("open         {public}");
    println!(
        "stop         Ctrl-C (data stays; `cargo xtask down` removes this checkout's containers and data)"
    );

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
        seed(root, &compose, &sh, &issuer)?;
        println!("keycloak     {issuer}");
        // 비밀번호는 로그·코드에 두지 않는다. 값은 deploy/keycloak/realm-test.json의 테스트 계정에 있다.
        println!(
            "login        test-user (admin), test-user-2 — passwords in deploy/keycloak/realm-test.json"
        );
        let admin = std::env::var("FVOCI_INITIAL_ADMIN_SUBJECT")
            .unwrap_or_else(|_| compose::DEV_ADMIN_SUBJECT.to_owned());
        Some((
            sh.push_env("FVOCI_OIDC_ISSUER", issuer),
            sh.push_env("FVOCI_OIDC_CLIENT_ID", compose::OIDC_CLIENT_ID),
            sh.push_env("FVOCI_OIDC_CLIENT_SECRET", compose::OIDC_CLIENT_SECRET),
            sh.push_env("FVOCI_INITIAL_ADMIN_SUBJECT", admin),
        ))
    } else {
        None
    };
    let _web = if web {
        Some(WebServer::start(root, &bind)?)
    } else {
        println!(
            "web          (not started) VITE_API_URL=http://{bind} pnpm --filter @fvoci/web dev"
        );
        None
    };
    cmd!(sh, "cargo run --package fvoci-server --locked").run()?;
    Ok(())
}

/// 개발 전용 사용자·공동 작업 공간·공동 편집 문서를 한 번만 만든다(이미 있으면 아무것도 바꾸지 않음).
fn seed(root: &Path, compose: &Compose, sh: &Shell, issuer: &str) -> Result<()> {
    let sql = std::fs::read_to_string(root.join("deploy/postgres/dev-seed.sql"))
        .context("read deploy/postgres/dev-seed.sql")?;
    let command: Vec<String> = [
        "psql",
        "--quiet",
        "--no-psqlrc",
        "-U",
        "fvoci",
        "-d",
        "fvoci",
        "-v",
        "ON_ERROR_STOP=1",
        "-v",
        &format!("issuer={issuer}"),
        "-f",
        "-",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    println!("==> dev seed (shared workspace and document, once)");
    compose.exec(sh, "postgres", &command, &sql)
}

/// 웹 개발 서버. 같은 프로세스 그룹이라 Ctrl-C가 함께 멈춘다. 서버가 먼저 끝나면 여기서 멈춘다.
struct WebServer(Child);

impl WebServer {
    fn start(root: &Path, bind: &str) -> Result<Self> {
        let child = Command::new("pnpm")
            .args(["--filter", "@fvoci/web", "dev"])
            .env("VITE_API_URL", format!("http://{bind}"))
            .current_dir(root)
            .spawn()
            .context("start the web dev server (pnpm --filter @fvoci/web dev)")?;
        Ok(Self(child))
    }
}

impl Drop for WebServer {
    fn drop(&mut self) {
        // pnpm이 띄운 vite까지 멈춘다.
        // 이미 끝났으면(Ctrl-C) 실패해도 괜찮으므로 알리기만 한다.
        let pid = self.0.id().to_string();
        if let Err(e) = Command::new("pkill").args(["-TERM", "-P", &pid]).status() {
            eprintln!("web dev server: pkill: {e}");
        }
        if let Err(e) = self.0.kill() {
            eprintln!("web dev server: kill: {e}");
        }
        if let Err(e) = self.0.wait() {
            eprintln!("web dev server: wait: {e}");
        }
    }
}
