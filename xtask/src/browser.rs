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
//! - `run.lock`: 같은 체크아웃의 실행을 한 번에 하나로 묶는 운영체제 파일 잠금(#130). 위 경로와 Compose 프로젝트는
//!   체크아웃마다 하나라서, 나중 실행은 무엇을 지우거나 띄우기 전에 실패한다. 잠금은 프로세스가 끝나면(강제 종료 포함)
//!   풀리므로 파일이 남아 있어도 지울 필요가 없다. 다른 워크트리는 경로가 달라 함께 돌 수 있다.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use xshell::cmd;

use crate::compose::{self, Compose, DbUrls, Env};

/// 시험에 쓰는 Compose 서비스. 검색·S3는 이 시험이 쓰지 않는다(서버는 검색이 없으면 DB에서 제목만 찾는다).
const SERVICES: &[&str] = &["postgres", "keycloak"];

pub fn run(root: &Path, keep: bool, playwright_args: &[String]) -> Result<()> {
    let out = root.join("target/browser");
    // 공개 폴더 비우기·빌드·Compose보다 먼저 잡고 정리까지 쥔다.
    let _lock = RunLock::acquire(&out)?;
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
    with_server(&mut server, port, || {
        println!("==> playwright (Chromium)");
        let _base = sh.push_env("FVOCI_BROWSER_BASE_URL", &origin);
        let _raw = sh.push_env("FVOCI_BROWSER_RAW", raw);
        cmd!(
            sh,
            "pnpm --filter @fvoci/web exec playwright test {args...}"
        )
        .run()
        .context("browser tests failed (first result; no retries)")
    })
}

/// 서버가 준비되면 `tests`를 돌린다. 준비·시험이 실패해도 서버를 멈추고, 멈추지 못하면 첫 오류 뒤에 덧붙여 알린다.
fn with_server(server: &mut Server, port: u16, tests: impl FnOnce() -> Result<()>) -> Result<()> {
    let result = server.wait_ready(port).and_then(|()| tests());
    with_shutdown(result, server.stop())
}

/// 준비·시험 결과를 먼저 두고 서버 종료 실패를 함께 알린다(`.context`는 순서를 뒤집으므로 쓰지 않는다).
fn with_shutdown(primary: Result<()>, shutdown: Result<()>) -> Result<()> {
    match (primary, shutdown) {
        (Err(primary), Err(shutdown)) => Err(anyhow!(
            "{primary:#}; also failed to stop fvoci-server: {shutdown:#}"
        )),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), shutdown) => shutdown,
    }
}

/// 같은 체크아웃의 실행을 직렬화하는 `target/browser/run.lock`. 놓을 때 명시적으로 풀고, 프로세스가 끝나면 운영체제가 푼다.
/// 지운 경로를 다른 실행이 새로 만들어 따로 잠그는 경쟁이 생기므로 파일은 지우지 않는다.
#[derive(Debug)]
struct RunLock(File);

impl RunLock {
    fn acquire(out: &Path) -> Result<Self> {
        std::fs::create_dir_all(out).with_context(|| format!("create {}", out.display()))?;
        let path = out.join("run.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self(file)),
            Err(TryLockError::WouldBlock) => bail!(
                "another `cargo xtask browser` run in this checkout holds {}; wait for it to finish",
                path.display()
            ),
            Err(TryLockError::Error(e)) => {
                Err(e).with_context(|| format!("lock {}", path.display()))
            }
        }
    }
}

/// 닫기만 하면 같은 파일 서술자를 물려받은 프로세스(exec 전의 fork 자식 등)가 닫을 때까지 잠금이 남는다.
/// 잠금은 공유된 열린 파일 설명에 걸리므로 여기서 풀면 사본이 남아 있어도 바로 풀린다.
impl Drop for RunLock {
    fn drop(&mut self) {
        if let Err(e) = self.0.unlock() {
            eprintln!("unlock target/browser/run.lock: {e}");
        }
    }
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

    /// 서버를 죽이고 거둔다. 이미 끝난 서버는 `kill`이 성공으로 돌려준다. 죽이지 못하면 기다리지 않는다(끝없이 멈출 수 있다).
    fn stop(&mut self) -> Result<()> {
        self.0.kill().context("kill fvoci-server")?;
        self.0.wait().context("wait for fvoci-server")?;
        Ok(())
    }
}

/// 오류로 일찍 빠져나간 경로의 마지막 보루다. 알릴 곳이 없으므로 실패는 출력만 한다.
impl Drop for Server {
    fn drop(&mut self) {
        if !matches!(self.0.try_wait(), Ok(Some(_)))
            && let Err(e) = self.stop()
        {
            eprintln!("fvoci-server: {e:#}");
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

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::path::PathBuf;

    use super::*;

    /// 시험마다 저장소 `target` 아래 새 폴더를 쓴다. 실제 `target/browser`는 건드리지 않는다.
    fn scratch() -> Result<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("repository root")?
            .join("target/browser-lock-tests")
            .join(uuid::Uuid::now_v7().to_string());
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    fn assert_held<T: std::fmt::Debug>(acquired: Result<T>) {
        let error = acquired.expect_err("the lock is held elsewhere");
        assert!(
            format!("{error:#}").contains("another `cargo xtask browser` run"),
            "{error:#}"
        );
    }

    /// 나중 실행은 지난 공개 표시·원시 출력·파일 저장소를 건드리기 전에 실패하고, 풀린 뒤에는 다시 돈다.
    #[test]
    fn held_lock_rejects_run_before_touching_outputs() -> Result<()> {
        let root = scratch()?;
        let out = root.join("target/browser");
        let sentinel = out.join("artifacts/SAFE_TO_UPLOAD");
        std::fs::create_dir_all(sentinel.parent().context("artifacts")?)?;
        std::fs::write(&sentinel, "earlier run")?;

        let holder = RunLock::acquire(&out)?;
        // 잠금이 막지 못하면 공개 폴더를 지운 뒤 Compose 파일이 없는 가짜 체크아웃에서 실패한다.
        assert_held(run(&root, false, &[]));
        assert_eq!(std::fs::read_to_string(&sentinel)?, "earlier run");
        assert!(!out.join("runs").exists());
        assert!(!out.join("files").exists());
        assert_held(RunLock::acquire(&out));

        // 정상으로 놓은 뒤, 그리고 잠근 채 오류로 빠져나간 뒤에도 다시 잡을 수 있다.
        drop(holder);
        let failed = (|| -> Result<()> {
            let _lock = RunLock::acquire(&out)?;
            bail!("deliberate failure")
        })();
        assert!(failed.is_err());
        drop(RunLock::acquire(&out)?);
        assert!(out.join("run.lock").exists());
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// 다른 시험이 띄운 자식이 exec 전에 물려받은 서술자 사본을 `try_clone`(같은 열린 파일 설명)으로 흉내 낸다.
    /// 사본이 아직 열려 있어도 잠금을 놓은 즉시 다음 실행이 잡을 수 있어야 한다.
    #[test]
    fn dropped_lock_is_released_while_a_shared_descriptor_stays_open() -> Result<()> {
        let root = scratch()?;
        let out = root.join("target/browser");
        let holder = RunLock::acquire(&out)?;
        let inherited = holder.0.try_clone()?;
        drop(holder);
        let next = RunLock::acquire(&out);
        drop(inherited);
        drop(next?);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    const HOLDER_ENV: &str = "FVOCI_BROWSER_LOCK_TEST_HOLDER";
    const HOLDER_READY: &str = "browser-lock-held";

    /// 다른 프로세스가 강제로 끝나면(Drop 없이) 운영체제가 잠금을 푼다. 남은 `run.lock` 파일은 실행을 막지 않는다.
    #[test]
    fn lock_is_released_when_holder_process_is_killed() -> Result<()> {
        let root = scratch()?;
        let out = root.join("target/browser");
        let mut holder = Command::new(std::env::current_exe()?)
            .args([
                "browser::tests::hold_lock_until_killed",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env(HOLDER_ENV, &out)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = holder.stdout.take().context("holder stdout")?;
        let ready = BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
            .any(|line| line.contains(HOLDER_READY));
        let while_held = RunLock::acquire(&out);
        holder.kill()?;
        holder.wait()?;

        assert!(ready, "holder process did not take the lock");
        assert_held(while_held);
        drop(RunLock::acquire(&out)?);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// 위 시험이 띄우는 다른 프로세스. 환경 변수가 없으면 아무것도 하지 않는다.
    #[test]
    #[ignore = "helper process for lock_is_released_when_holder_process_is_killed"]
    fn hold_lock_until_killed() -> Result<()> {
        let Some(out) = std::env::var_os(HOLDER_ENV) else {
            return Ok(());
        };
        let _lock = RunLock::acquire(Path::new(&out))?;
        println!("{HOLDER_READY}");
        std::io::stdout().flush()?;
        std::thread::sleep(Duration::from_secs(60));
        Ok(())
    }

    #[test]
    fn shutdown_failure_follows_primary_error() {
        assert!(with_shutdown(Ok(()), Ok(())).is_ok());
        let only_primary = with_shutdown(Err(anyhow!("tests failed")), Ok(()));
        assert_eq!(
            format!("{:#}", only_primary.expect_err("primary")),
            "tests failed"
        );
        let only_shutdown = with_shutdown(Ok(()), Err(anyhow!("kill failed")));
        assert_eq!(
            format!("{:#}", only_shutdown.expect_err("shutdown")),
            "kill failed"
        );
        let both = with_shutdown(Err(anyhow!("not ready")), Err(anyhow!("kill failed")));
        assert_eq!(
            format!("{:#}", both.expect_err("both")),
            "not ready; also failed to stop fvoci-server: kill failed"
        );
    }

    #[cfg(unix)]
    fn pid_alive(pid: u32) -> bool {
        // 좀비도 살아 있다고 나온다. 슬림 이미지에 없을 수 있는 `kill` 실행 파일 대신 셸 내장 명령을 쓴다.
        Command::new("sh")
            .args(["-c", "kill -0 \"$0\"", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// 시험 실패 경로에서도 실제 자식 프로세스를 죽이고 거둔다. 다시 멈추거나 Drop해도 문제가 없다.
    #[cfg(unix)]
    #[test]
    fn failed_tests_still_stop_and_reap_server() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let port = listener.local_addr()?.port();
        let health = std::thread::spawn(move || -> std::io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut request = [0_u8; 64];
            let _read = stream.read(&mut request)?;
            stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
        });
        let mut server = Server(Command::new("sleep").arg("30").spawn()?);
        let pid = server.0.id();
        assert!(pid_alive(pid));

        let result = with_server(&mut server, port, || bail!("deliberate test failure"));
        assert_eq!(
            format!("{:#}", result.expect_err("tests failed")),
            "deliberate test failure"
        );
        let status = server.0.try_wait()?.context("server was reaped")?;
        assert!(!status.success());
        // 거둔 뒤에는 좀비로도 남지 않는다.
        assert!(!pid_alive(pid));
        assert!(health.join().is_ok_and(|served| served.is_ok()));
        server.stop()?;
        drop(server);
        Ok(())
    }

    /// 준비 전에 끝난 서버는 준비 오류만 알리고, 멈추기는 성공한다.
    #[cfg(unix)]
    #[test]
    fn readiness_failure_is_kept_when_server_already_exited() -> Result<()> {
        let mut server = Server(Command::new("sh").args(["-c", "exit 3"]).spawn()?);
        let port = free_port()?;
        let result = with_server(&mut server, port, || {
            unreachable!("server never became ready")
        });
        let error = format!("{:#}", result.expect_err("not ready"));
        assert!(
            error.starts_with("fvoci-server exited before it was ready"),
            "{error}"
        );
        assert!(!error.contains("failed to stop"), "{error}");
        assert_eq!(server.0.try_wait()?.and_then(|s| s.code()), Some(3));
        Ok(())
    }
}
