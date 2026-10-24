//! 실제 브라우저 회귀 시험(#118). 실제 서버·DB·테스트 OIDC(Keycloak)에 Chromium(Playwright)으로 접속한다.
//!
//! 1. 이 체크아웃의 `browser` Compose 프로젝트(시험 Compose 파일의 PostgreSQL·Keycloak, 호스트 포트는 동적)를 띄우고
//!    `db bootstrap` → `db migrate`로 준비한다.
//! 2. 웹을 빌드하고, 빈 루프백 포트 하나에 서버를 띄워 API와 웹을 같은 원점에서 내보낸다(`FVOCI_WEB_DIR`).
//! 3. `apps/web/e2e`의 Playwright 시험을 돌린다. `--` 뒤의 인자는 그대로 Playwright에 넘긴다(예: 파일·`--grep`).
//! 4. 서버를 멈추고, 실패 증거를 검사한 뒤에만 공개 폴더로 옮기고, Compose를 정리한다(`--keep`이면 컨테이너를 남긴다).
//!
//! 출력 위치(모두 `target/browser` 아래, 이 명령만 쓴다):
//! - `runs/<실행 ID>/`: 실행마다 새로 만드는 원시 출력(시험 결과·trace·서버 로그·sensitive.txt). 세션 쿠키 등이 남을 수
//!   있어 올리지 않는다. 지우지 않으므로 첫 실패가 다음 실행에 덮이지 않는다. 필요 없으면 직접 지운다.
//! - `files/`: 서버 파일 저장소. `--keep`으로 남긴 DB의 첨부 행이 가리키므로 실행마다 같은 곳을 쓰고 지우지 않는다.
//! - `artifacts/`: 공개 폴더. 실행을 시작할 때 비운다. `e2e/publish.ts`가 이번 실행의 원시 출력 중 검사를 통과한
//!   파일만 옮기고 마지막에 `SAFE_TO_UPLOAD`를 쓴다. 표시가 없으면 올리지 않는다.

use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use xshell::cmd;

use crate::compose::{self, Compose, DbUrls, Env};

/// 시험에 쓰는 Compose 서비스. 검색·S3는 이 시험이 쓰지 않는다(서버는 검색이 없으면 DB에서 제목만 찾는다).
const SERVICES: &[&str] = &["postgres", "keycloak"];

pub fn run(root: &Path, keep: bool, playwright_args: &[String]) -> Result<()> {
    let out = root.join("target/browser");
    let artifacts = out.join("artifacts");
    // 지난 실행의 공개 표시·파일이 이번 결과로 올라가지 않게 무엇보다 먼저 비운다(원시 출력은 지우지 않는다).
    if artifacts.exists() {
        std::fs::remove_dir_all(&artifacts)
            .with_context(|| format!("remove {}", artifacts.display()))?;
    }
    let raw = out.join("runs").join(uuid::Uuid::now_v7().to_string());
    std::fs::create_dir_all(&raw).with_context(|| format!("create {}", raw.display()))?;
    let files = out.join("files");
    println!("==> raw output {}", raw.display());

    let compose = Compose::new(root, Env::Browser).with_profile("oidc");
    let sh = compose.shell()?;
    println!("==> compose up ({})", compose.project());
    let tested = compose
        .up_services(&sh, SERVICES)
        .and_then(|()| run_against(root, &compose, &raw, &files, playwright_args));

    // 시험이 실패해도 공개 폴더는 만든다(실패 증거가 목적이다). 검사에 걸리면 표시 없이 끝난다.
    println!("==> publish failure evidence (target/browser/artifacts)");
    let published = cmd!(
        sh,
        "pnpm --filter @fvoci/web exec node e2e/publish.ts {raw} {artifacts}"
    )
    .run()
    .context("publish browser evidence");

    let cleanup = if keep {
        println!(
            "==> containers kept (project {}); run `cargo xtask down` later",
            compose.project()
        );
        Ok(())
    } else {
        println!("==> compose down");
        compose.down(&sh)
    };
    // 시험 결과를 먼저, 공개·정리 실패도 버리지 않고 함께 알린다.
    let errors: Vec<String> = [
        ("browser tests", tested),
        ("evidence publishing", published),
        ("cleanup", cleanup),
    ]
    .into_iter()
    .filter_map(|(step, outcome)| outcome.err().map(|e| format!("{step}: {e:#}")))
    .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        bail!("{} (raw output {})", errors.join("; "), raw.display())
    }
}

fn run_against(
    root: &Path,
    compose: &Compose,
    raw: &Path,
    files: &Path,
    args: &[String],
) -> Result<()> {
    let sh = compose.shell()?;
    let pg = compose.host_port(&sh, "postgres", 5432)?;
    let kc = compose.host_port(&sh, "keycloak", 8080)?;
    let urls = DbUrls::local(pg);
    compose::prepare_database(&sh, &urls)?;

    println!("==> build web and server");
    cmd!(sh, "pnpm --filter @fvoci/web run build").run()?;
    cmd!(sh, "cargo build --package fvoci-server --locked").run()?;

    let port = free_port()?;
    let origin = format!("http://127.0.0.1:{port}");
    let issuer = format!("http://127.0.0.1:{kc}/realms/{}", compose::OIDC_REALM);
    println!("==> server on {origin} (postgres 127.0.0.1:{pg}, keycloak {issuer})");
    let log = raw.join("server.log");
    let mut server = Server::start(root, files, &log, port, &origin, &issuer, &urls.app)?;
    server.wait_ready(port)?;

    println!("==> playwright (Chromium)");
    let _base = sh.push_env("FVOCI_BROWSER_BASE_URL", &origin);
    let _raw = sh.push_env("FVOCI_BROWSER_RAW", raw);
    let result = cmd!(
        sh,
        "pnpm --filter @fvoci/web exec playwright test {args...}"
    )
    .run()
    .context("browser tests failed (first result; no retries)");
    server.stop();
    result
}

/// 빈 루프백 포트를 받는다. 닫은 뒤 서버가 열기까지의 짧은 틈에 다른 프로세스가 가져가면 서버가 시작하지 못하고 실패한다.
fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("find a free loopback port")?;
    Ok(listener.local_addr()?.port())
}

/// 시험용 서버 프로세스. 로그는 원시 출력 폴더의 파일로만 남긴다.
struct Server(Child);

impl Server {
    fn start(
        root: &Path,
        files: &Path,
        log: &Path,
        port: u16,
        origin: &str,
        issuer: &str,
        database: &str,
    ) -> Result<Self> {
        let out = File::create(log).with_context(|| format!("create {}", log.display()))?;
        let err = out.try_clone()?;
        let mut command = Command::new(root.join("target/debug/fvoci-server"));
        // 호출한 셸의 서버 설정(외부 DB·검색·S3 등)을 물려받지 않는다. 이 시험이 정한 값만 넣는다.
        for (key, _) in std::env::vars_os() {
            let key_text = key.to_string_lossy();
            if key_text.starts_with("FVOCI_") || key_text.starts_with("DATABASE_") {
                command.env_remove(&key);
            }
        }
        let child = command
            .current_dir(root)
            .env("DATABASE_URL", database)
            .env("FVOCI_BIND", format!("127.0.0.1:{port}"))
            .env("FVOCI_PUBLIC_URL", origin)
            .env("FVOCI_WEB_DIR", root.join("apps/web/dist"))
            .env("FVOCI_STORAGE", "local")
            .env("FVOCI_STORAGE_DIR", files)
            .env("FVOCI_OIDC_ISSUER", issuer)
            .env("FVOCI_OIDC_CLIENT_ID", compose::OIDC_CLIENT_ID)
            .env("FVOCI_OIDC_CLIENT_SECRET", compose::OIDC_CLIENT_SECRET)
            .env("FVOCI_INITIAL_ADMIN_SUBJECT", compose::DEV_ADMIN_SUBJECT)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .spawn()
            .context("start fvoci-server")?;
        Ok(Self(child))
    }

    /// `/healthz`가 200을 돌려줄 때까지 기다린다. 서버가 먼저 끝나면 기다리지 않고 실패한다.
    fn wait_ready(&mut self, port: u16) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(60);
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        loop {
            if let Some(status) = self.0.try_wait()? {
                bail!(
                    "fvoci-server exited before it was ready ({status}); see server.log in the raw output"
                );
            }
            if healthz(addr).unwrap_or(false) {
                return Ok(());
            }
            if Instant::now() > deadline {
                bail!("fvoci-server was not ready within 60s; see server.log in the raw output");
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    fn stop(&mut self) {
        // 이미 끝났으면 실패해도 괜찮으므로 알리기만 한다.
        if let Err(e) = self.0.kill() {
            eprintln!("fvoci-server: kill: {e}");
        }
        if let Err(e) = self.0.wait() {
            eprintln!("fvoci-server: wait: {e}");
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            self.stop();
        }
    }
}

fn healthz(addr: SocketAddr) -> std::io::Result<bool> {
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(1))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(b"GET /healthz HTTP/1.0\r\nHost: 127.0.0.1\r\n\r\n")?;
    let mut head = [0_u8; 12];
    stream.read_exact(&mut head)?;
    Ok(head.starts_with(b"HTTP/1.1 200") || head.starts_with(b"HTTP/1.0 200"))
}
