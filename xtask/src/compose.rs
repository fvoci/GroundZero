//! Docker Compose 호출. 체크아웃마다 프로젝트명을 다르게 해 병렬 실행을 격리한다.
//!
//! 같은 체크아웃 안에서는 프로젝트가 하나라서 `cargo xtask test`·`browser`가 실행 잠금([`Compose::lock_run`], #130)으로
//! 한 번에 하나씩 돈다. 중단 신호는 [`watch_interrupts`]가 받아 실패한 단계로 바꾸고 정리 경로를 지나게 한다(#93).

use std::fs::{File, OpenOptions, TryLockError};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow, bail};
use xshell::{Shell, cmd};

/// Compose 파일 종류.
#[derive(Debug, Clone, Copy)]
pub enum Env {
    Dev,
    Test,
    /// 셀프호스트 Supabase 시험 구성(D-88). 공식 구성과 겹쳐 쓰므로 [`crate::supabase`]가 다룬다.
    Supabase,
    /// 실제 브라우저 회귀 시험(`cargo xtask browser`). 시험 Compose 파일을 쓰되 프로젝트를 따로 둬, 같은 체크아웃의
    /// `cargo xtask test`와 서로 정리하지 않는다.
    Browser,
}

impl Env {
    fn file(self) -> &'static str {
        match self {
            Self::Dev => "deploy/compose.dev.yaml",
            Self::Test => "deploy/compose.test.yaml",
            Self::Supabase => "deploy/supabase/compose.test.yaml",
            Self::Browser => "deploy/compose.test.yaml",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Test => "test",
            Self::Supabase => "supabase",
            Self::Browser => "browser",
        }
    }
}

/// 디렉터리 이름과 전체 경로 해시로 프로젝트명을 만든다. Compose 규칙: 소문자·숫자·`-`·`_`.
pub fn project_name(root: &Path, env: Env) -> String {
    let base = root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("fvoci")
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>();
    let mut hasher = DefaultHasher::new();
    root.to_string_lossy().hash(&mut hasher);
    let hash = hasher.finish() & 0xff_ffff;
    format!("fvoci-{base}-{hash:06x}-{}", env.suffix())
}

/// Compose 핸들.
#[derive(Debug)]
pub struct Compose {
    root: std::path::PathBuf,
    env: Env,
    project: String,
    profiles: Vec<String>,
}

impl Compose {
    pub fn new(root: &Path, env: Env) -> Self {
        Self {
            root: root.to_path_buf(),
            env,
            project: project_name(root, env),
            profiles: Vec::new(),
        }
    }

    /// 프로필을 추가한다(예: `oidc`).
    pub fn with_profile(mut self, profile: &str) -> Self {
        self.profiles.push(profile.to_owned());
        self
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    /// 이 프로젝트를 쓰는 실행을 체크아웃마다 하나로 묶는 `target/<종류>/run.lock`을 잡는다(#130). 나중 실행은 무엇을
    /// 지우거나 띄우기 전에 실패한다. 다른 종류(예: test와 browser)와 다른 체크아웃은 함께 돌 수 있다.
    pub fn lock_run(&self) -> Result<RunLock> {
        let suffix = self.env.suffix();
        RunLock::acquire(&self.root.join("target").join(suffix), suffix)
    }

    /// 강제로 끝난 실행(SIGKILL 등 정리 경로를 못 지난 경우)의 컨테이너만 멈추는 명령. 저장소 루트에서 돌린다. 이름이
    /// 체크아웃마다 정해져 있어 따로 기록하지 않는다. 볼륨은 지우지 않으며 시험 DB는 tmpfs라 컨테이너가 멈추면 내용이 사라진다.
    pub fn recovery_command(&self) -> String {
        format!(
            "docker {} down --remove-orphans",
            self.base_args().join(" ")
        )
    }

    fn base_args(&self) -> Vec<String> {
        let mut args = vec![
            "compose".to_owned(),
            "--project-name".to_owned(),
            self.project.clone(),
            "--file".to_owned(),
            self.env.file().to_owned(),
        ];
        for p in &self.profiles {
            args.push("--profile".to_owned());
            args.push(p.clone());
        }
        args
    }

    /// 서비스가 healthy 될 때까지 기다리며 띄운다.
    pub fn up(&self, sh: &Shell) -> Result<()> {
        let args = self.base_args();
        cmd!(sh, "docker {args...} up --detach --wait --remove-orphans")
            .run()
            .with_context(|| format!("docker compose up failed (project {})", self.project))?;
        Ok(())
    }

    /// 고른 서비스만 healthy 될 때까지 기다리며 띄운다(Supabase 시험에서 PostgreSQL 대신).
    pub fn up_services(&self, sh: &Shell, services: &[&str]) -> Result<()> {
        let args = self.base_args();
        cmd!(
            sh,
            "docker {args...} up --detach --wait --remove-orphans {services...}"
        )
        .run()
        .with_context(|| format!("docker compose up failed (project {})", self.project))?;
        Ok(())
    }

    /// 이 프로젝트의 컨테이너·네트워크·볼륨만 제거한다.
    pub fn down(&self, sh: &Shell) -> Result<()> {
        let args = self.base_args();
        cmd!(sh, "docker {args...} down --volumes --remove-orphans").run()?;
        Ok(())
    }

    /// 실행 중인 서비스 안에서 명령을 실행한다. `stdin`을 표준 입력으로 넘긴다.
    pub fn exec(&self, sh: &Shell, service: &str, command: &[String], stdin: &str) -> Result<()> {
        let args = self.base_args();
        cmd!(sh, "docker {args...} exec -T {service} {command...}")
            .stdin(stdin)
            .run()
            .with_context(|| format!("{service}: {} failed", command.join(" ")))?;
        Ok(())
    }

    /// 실행 중인 서비스 안에서 명령을 실행하고 표준 출력을 돌려준다(출력은 화면에 남기지 않는다).
    pub fn exec_read(&self, sh: &Shell, service: &str, command: &[String]) -> Result<String> {
        let args = self.base_args();
        cmd!(sh, "docker {args...} exec -T {service} {command...}")
            .quiet()
            .read()
            .with_context(|| format!("{service}: command failed"))
    }

    /// 서비스의 컨테이너 포트에 매핑된 호스트 포트를 돌려준다.
    pub fn host_port(&self, sh: &Shell, service: &str, container_port: u16) -> Result<u16> {
        let args = self.base_args();
        let port_str = container_port.to_string();
        let out = cmd!(sh, "docker {args...} port {service} {port_str}")
            .read()
            .with_context(|| format!("cannot resolve port for {service}:{container_port}"))?;
        let line = out.lines().next().unwrap_or_default();
        let Some((_, port)) = line.rsplit_once(':') else {
            bail!("unexpected `docker compose port` output: {out:?}");
        };
        port.trim()
            .parse()
            .with_context(|| format!("not a port: {port:?}"))
    }

    pub fn shell(&self) -> Result<Shell> {
        let sh = Shell::new()?;
        sh.change_dir(&self.root);
        Ok(sh)
    }
}

/// 같은 체크아웃의 실행을 직렬화하는 `run.lock`. 놓을 때 명시적으로 풀고, 프로세스가 끝나면(강제 종료 포함) 운영체제가
/// 푼다. 지운 경로를 다른 실행이 새로 만들어 따로 잠그는 경쟁이 생기므로 파일은 지우지 않는다. 남은 파일은 실행을 막지 않는다.
#[derive(Debug)]
pub struct RunLock {
    file: File,
    path: PathBuf,
}

impl RunLock {
    fn acquire(dir: &Path, command: &str) -> Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        let path = dir.join("run.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("open {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { file, path }),
            Err(TryLockError::WouldBlock) => bail!(
                "another `cargo xtask {command}` run in this checkout holds {}; wait for it to finish",
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
        if let Err(e) = self.file.unlock() {
            eprintln!("unlock {}: {e}", self.path.display());
        }
    }
}

/// 이 실행이 받은 첫 중단 신호의 이름(`INT`·`TERM`·`HUP`).
static INTERRUPTED: OnceLock<&'static str> = OnceLock::new();

/// SIGINT·SIGTERM·SIGHUP을 받아도 이 프로세스가 바로 끝나지 않게 하고, 받으면 진행 중인 자식 단계를 끝내 실패로 돌린다.
/// 그러면 각 명령의 정리 경로(서버 정지, 증거 공개, `--keep`이 아니면 이 실행의 Compose 프로젝트만 `down`)를 그대로 지난다.
///
/// 터미널의 Ctrl-C는 프로세스 그룹 전체에 가므로 자식도 이미 받는다. 이 프로세스에만 온 신호(`kill <pid>`)는 이 프로세스가
/// 그룹을 이끌 때(터미널·`setsid`로 띄운 경우)에만 같은 신호를 그룹(자식·손자)에 다시 보낸다. 이끌지 않으면(스크립트·CI)
/// 그 그룹에는 부모 셸 같은 남의 프로세스가 있어 보내지 않는다. 신호는 한 번만 전달하므로 그 뒤 정리 단계는 막지 않는다.
pub fn watch_interrupts() -> Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .build()
            .context("start the signal watcher")?;
        // 돌려주기 전에 처리기를 건다. 처리기는 프로세스가 끝날 때까지 남으므로 감시 스레드가 끝난 뒤의 신호는 무시된다.
        let (mut int, mut term, mut hup) = {
            let _enter = runtime.enter();
            (
                signal(SignalKind::interrupt())?,
                signal(SignalKind::terminate())?,
                signal(SignalKind::hangup())?,
            )
        };
        std::thread::spawn(move || {
            let name = runtime.block_on(async {
                tokio::select! {
                    _ = int.recv() => "INT",
                    _ = term.recv() => "TERM",
                    _ = hup.recv() => "HUP",
                }
            });
            let _first = INTERRUPTED.set(name);
            eprintln!(
                "==> SIG{name} received; stopping the current step, then cleaning up as after a failure"
            );
            forward_to_own_group(name);
        });
    }
    Ok(())
}

/// 그룹 id가 이 pid인 그룹은 이 프로세스가 이끌 때만 있으므로, 이끌지 않으면 `kill`이 실패하고 아무에게도 보내지 않는다.
/// 이 프로세스도 받지만 이미 처리기가 걸려 있어 끝나지 않는다. 슬림 이미지에 없을 수 있는 `kill` 실행 파일 대신 셸 내장 명령.
#[cfg(unix)]
fn forward_to_own_group(name: &str) {
    let pid = std::process::id().to_string();
    let forwarded = std::process::Command::new("sh")
        .args(["-c", "kill -s \"$0\" -- \"-$1\" 2>/dev/null", name, &pid])
        .stdin(std::process::Stdio::null())
        .status();
    if let Err(e) = forwarded {
        eprintln!("forward SIG{name} to this run's processes: {e}");
    }
}

/// 신호를 받은 뒤에는 남은 단계를 시작하지 않는다(신호가 단계 사이에 와서 끝낼 자식이 없었던 경우).
pub fn check_interrupt() -> Result<()> {
    match INTERRUPTED.get() {
        Some(name) => bail!("skipped the remaining steps after SIG{name}"),
        None => Ok(()),
    }
}

/// 중단된 실행은 단계가 모두 성공했어도 실패로 끝낸다. 실패한 단계의 오류 앞에 받은 신호를 붙인다.
pub fn with_interrupt(result: Result<()>) -> Result<()> {
    interrupt_outcome(INTERRUPTED.get().copied(), result)
}

fn interrupt_outcome(signal: Option<&str>, result: Result<()>) -> Result<()> {
    match (signal, result) {
        (None, result) => result,
        (Some(name), Ok(())) => Err(anyhow!("interrupted by SIG{name}")),
        (Some(name), Err(e)) => Err(e.context(format!("interrupted by SIG{name}"))),
    }
}

/// dev·test·browser 프로젝트를 모두 정리한다.
pub fn down_all(root: &Path) -> Result<()> {
    for env in [Env::Dev, Env::Test, Env::Browser] {
        let compose = Compose::new(root, env).with_profile("oidc");
        let sh = compose.shell()?;
        println!("==> down {}", compose.project());
        compose.down(&sh)?;
    }
    crate::supabase::down(root)
}

/// 개발·테스트용 DB 접속 정보. 모두 테스트 전용 값이며 운영에 쓰지 않는다.
/// 관리자(`fvoci`)는 compose 파일의 `POSTGRES_USER`, 나머지 두 역할은 `db bootstrap`이 만든다.
#[derive(Debug, Clone)]
pub struct DbUrls {
    pub admin: String,
    pub migration: String,
    pub app: String,
}

pub const TEST_MIGRATOR_PASSWORD: &str = "fvoci-test-migrator-password";
pub const TEST_APP_PASSWORD: &str = "fvoci-test-app-password";

impl DbUrls {
    pub fn local(port: u16) -> Self {
        Self::at(&format!("127.0.0.1:{port}"))
    }

    pub fn at(host_port: &str) -> Self {
        Self {
            admin: format!("postgres://fvoci:fvoci-test-password@{host_port}/fvoci"),
            migration: format!(
                "postgres://fvoci_migrator:{TEST_MIGRATOR_PASSWORD}@{host_port}/fvoci"
            ),
            app: format!("postgres://fvoci_app:{TEST_APP_PASSWORD}@{host_port}/fvoci"),
        }
    }

    /// 셀프호스트 Supabase 시험 구성(D-88): 관리자는 superuser가 아닌 `postgres`, DB는 `postgres`.
    pub fn supabase(port: u16) -> Self {
        let host_port = format!("127.0.0.1:{port}");
        let password = crate::supabase::POSTGRES_PASSWORD;
        Self {
            admin: format!("postgres://postgres:{password}@{host_port}/postgres"),
            migration: format!(
                "postgres://fvoci_migrator:{TEST_MIGRATOR_PASSWORD}@{host_port}/postgres"
            ),
            app: format!("postgres://fvoci_app:{TEST_APP_PASSWORD}@{host_port}/postgres"),
        }
    }

    /// 앱 역할로 Supavisor의 session 모드(공식 구성의 5432)를 거치는 주소. 사용자 이름에 테넌트를 붙인다.
    pub fn supabase_session_pooler(port: u16) -> String {
        let tenant = crate::supabase::POOLER_TENANT;
        format!("postgres://fvoci_app.{tenant}:{TEST_APP_PASSWORD}@127.0.0.1:{port}/postgres")
    }
}

/// 운영자 절차와 같은 경로(`db bootstrap` → `db migrate`)로 역할과 스키마를 준비한다.
pub fn prepare_database(sh: &Shell, urls: &DbUrls) -> Result<()> {
    let _a = sh.push_env("DATABASE_ADMIN_URL", &urls.admin);
    let _m = sh.push_env("DATABASE_MIGRATION_URL", &urls.migration);
    let _mp = sh.push_env("FVOCI_DB_MIGRATOR_PASSWORD", TEST_MIGRATOR_PASSWORD);
    let _ap = sh.push_env("FVOCI_DB_APP_PASSWORD", TEST_APP_PASSWORD);
    println!("==> db bootstrap / db migrate");
    cmd!(
        sh,
        "cargo run --quiet --package fvoci-server --locked -- db bootstrap"
    )
    .run()?;
    cmd!(
        sh,
        "cargo run --quiet --package fvoci-server --locked -- db migrate"
    )
    .run()?;
    Ok(())
}

/// Keycloak 개발·테스트 realm의 클라이언트 설정. `deploy/keycloak/realm-test.json`과 같다.
pub const OIDC_CLIENT_ID: &str = "fvoci-web";
pub const OIDC_CLIENT_SECRET: &str = "fvoci-test-client-secret";
pub const OIDC_REALM: &str = "fvoci-test";
/// 그 realm의 가짜 사용자 `test-user`의 고정 id(OIDC subject). 개발 환경의 초기 관리자(D-51)로 쓴다.
pub const DEV_ADMIN_SUBJECT: &str = "00000000-0000-4000-8000-00000000000a";
/// compose.dev·test.yaml과 CI 서비스의 Meilisearch 테스트 마스터 키(명백한 테스트 값).
pub const SEARCH_TEST_KEY: &str = "fvoci-test-meili-master-key-16chars";
/// compose.test.yaml의 SeaweedFS 버킷·자격 증명(명백한 테스트 값).
pub const S3_TEST_BUCKET: &str = "fvoci-test";
pub const S3_TEST_ACCESS_KEY: &str = "fvoci-test-s3-key";
pub const S3_TEST_SECRET_KEY: &str = "fvoci-test-s3-secret";

#[cfg(test)]
pub(crate) mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;

    /// 시험마다 저장소 `target` 아래 새 폴더를 가짜 체크아웃 루트로 쓴다. 실제 `target/<종류>/run.lock`은 건드리지 않는다
    /// (`cargo xtask test`가 이 시험을 돌리는 동안 진짜 잠금을 쥐고 있다).
    pub(crate) fn scratch_root() -> Result<PathBuf> {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .context("repository root")?
            .join("target/xtask-run-tests")
            .join(uuid::Uuid::now_v7().to_string());
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub(crate) fn assert_held<T: std::fmt::Debug>(acquired: Result<T>, command: &str) {
        let error = acquired.expect_err("the lock is held elsewhere");
        assert!(
            format!("{error:#}").contains(&format!("another `cargo xtask {command}` run")),
            "{error:#}"
        );
    }

    /// test와 browser는 잠금이 따로라 함께 돌고, 같은 종류만 막힌다.
    #[test]
    fn run_lock_is_per_checkout_and_kind() -> Result<()> {
        let root = scratch_root()?;
        let test = Compose::new(&root, Env::Test).lock_run()?;
        let browser = Compose::new(&root, Env::Browser)
            .with_profile("oidc")
            .lock_run()?;
        assert_held(
            Compose::new(&root, Env::Test)
                .with_profile("oidc")
                .lock_run(),
            "test",
        );
        assert_held(Compose::new(&root, Env::Browser).lock_run(), "browser");
        drop((test, browser));
        assert!(root.join("target/test/run.lock").exists());
        assert!(root.join("target/browser/run.lock").exists());
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// 다른 시험이 띄운 자식이 exec 전에 물려받은 서술자 사본을 `try_clone`(같은 열린 파일 설명)으로 흉내 낸다.
    /// 사본이 아직 열려 있어도 잠금을 놓은 즉시 다음 실행이 잡을 수 있어야 한다.
    #[test]
    fn dropped_lock_is_released_while_a_shared_descriptor_stays_open() -> Result<()> {
        let root = scratch_root()?;
        let compose = Compose::new(&root, Env::Browser);
        let holder = compose.lock_run()?;
        let inherited = holder.file.try_clone()?;
        drop(holder);
        let next = compose.lock_run();
        drop(inherited);
        drop(next?);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    const HOLDER_ENV: &str = "FVOCI_XTASK_LOCK_TEST_ROOT";
    const HOLDER_READY: &str = "run-lock-held";

    /// 다른 프로세스가 강제로 끝나면(Drop 없이) 운영체제가 잠금을 푼다. 남은 `run.lock` 파일은 실행을 막지 않는다.
    #[test]
    fn lock_is_released_when_holder_process_is_killed() -> Result<()> {
        let root = scratch_root()?;
        let mut holder = Command::new(std::env::current_exe()?)
            .args([
                "compose::tests::hold_lock_until_killed",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env(HOLDER_ENV, &root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = holder.stdout.take().context("holder stdout")?;
        let ready = BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok)
            .any(|line| line.contains(HOLDER_READY));
        let compose = Compose::new(&root, Env::Test);
        let while_held = compose.lock_run();
        holder.kill()?;
        holder.wait()?;

        assert!(ready, "holder process did not take the lock");
        assert_held(while_held, "test");
        drop(compose.lock_run()?);
        std::fs::remove_dir_all(&root)?;
        Ok(())
    }

    /// 위 시험이 띄우는 다른 프로세스. 환경 변수가 없으면 아무것도 하지 않는다.
    #[test]
    #[ignore = "helper process for lock_is_released_when_holder_process_is_killed"]
    fn hold_lock_until_killed() -> Result<()> {
        let Some(root) = std::env::var_os(HOLDER_ENV) else {
            return Ok(());
        };
        let _lock = Compose::new(Path::new(&root), Env::Test).lock_run()?;
        println!("{HOLDER_READY}");
        std::io::stdout().flush()?;
        std::thread::sleep(Duration::from_secs(60));
        Ok(())
    }

    #[test]
    fn interrupted_runs_fail_with_the_signal_first() {
        assert!(interrupt_outcome(None, Ok(())).is_ok());
        let failed = interrupt_outcome(None, Err(anyhow!("tests failed")));
        assert_eq!(format!("{:#}", failed.expect_err("failed")), "tests failed");
        let passed = interrupt_outcome(Some("TERM"), Ok(()));
        assert_eq!(
            format!("{:#}", passed.expect_err("interrupted")),
            "interrupted by SIGTERM"
        );
        let stopped = interrupt_outcome(Some("INT"), Err(anyhow!("cargo test failed")));
        assert_eq!(
            format!("{:#}", stopped.expect_err("interrupted")),
            "interrupted by SIGINT: cargo test failed"
        );
    }

    #[test]
    fn recovery_command_names_only_this_project() {
        let root = Path::new("/work/GroundZero");
        let compose = Compose::new(root, Env::Test).with_profile("oidc");
        assert_eq!(
            compose.recovery_command(),
            format!(
                "docker compose --project-name {} --file deploy/compose.test.yaml --profile oidc down --remove-orphans",
                project_name(root, Env::Test)
            )
        );
    }

    const SIGNAL_ENV: &str = "FVOCI_XTASK_SIGNAL_TEST";
    const SIGNAL_READY: &str = "signal-watcher-ready";

    /// 그룹을 이끄는 실행(터미널·`setsid`)의 pid에만 SIGTERM을 보내도 자식이 끝나고, 실행은 끝나지 않고 신호를 기록한다.
    /// 처리기가 없으면 실행이 바로 죽고 자식은 고아로 남아 계속 돈다(#93).
    #[cfg(unix)]
    #[test]
    fn termination_of_a_group_leader_stops_its_children_and_is_recorded() -> Result<()> {
        use std::os::unix::process::CommandExt;

        let mut runner = Command::new(std::env::current_exe()?)
            .args([
                "compose::tests::watch_signal_with_a_child",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env(SIGNAL_ENV, "1")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = runner.stdout.take().context("runner stdout")?;
        let mut lines = BufReader::new(stdout)
            .lines()
            .map_while(std::result::Result::ok);
        let ready = lines.by_ref().any(|line| line.contains(SIGNAL_READY));
        let sent = Command::new("sh")
            .args(["-c", "kill -s TERM \"$0\"", &runner.id().to_string()])
            .status()?;
        let report: Vec<String> = lines.collect();
        let status = runner.wait()?;

        assert!(ready, "runner did not start its child");
        assert!(sent.success());
        assert!(status.success(), "runner died: {status} {report:?}");
        assert!(report.iter().any(|l| l == "child signal 15"), "{report:?}");
        assert!(report.iter().any(|l| l == "interrupted TERM"), "{report:?}");
        Ok(())
    }

    /// 위 시험이 새 프로세스 그룹의 우두머리로 띄우는 실행. 환경 변수가 없으면 아무것도 하지 않는다.
    #[cfg(unix)]
    #[test]
    #[ignore = "helper process for termination_of_a_group_leader_stops_its_children_and_is_recorded"]
    fn watch_signal_with_a_child() -> Result<()> {
        use std::os::unix::process::ExitStatusExt;

        if std::env::var_os(SIGNAL_ENV).is_none() {
            return Ok(());
        }
        watch_interrupts()?;
        let mut child = Command::new("sleep").arg("30").spawn()?;
        println!("{SIGNAL_READY}");
        std::io::stdout().flush()?;
        let status = child.wait()?;
        println!("child signal {}", status.signal().unwrap_or(0));
        // 감시 스레드는 신호를 기록한 뒤에 그룹에 보내므로 자식이 끝났으면 기록돼 있다.
        println!(
            "interrupted {}",
            INTERRUPTED.get().copied().unwrap_or("none")
        );
        assert!(check_interrupt().is_err());
        Ok(())
    }
}
