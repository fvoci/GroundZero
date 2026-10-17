//! Docker Compose 호출. 체크아웃마다 프로젝트명을 다르게 해 병렬 실행을 격리한다.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;

use anyhow::{Context, Result, bail};
use xshell::{Shell, cmd};

/// Compose 파일 종류.
#[derive(Debug, Clone, Copy)]
pub enum Env {
    Dev,
    Test,
}

impl Env {
    fn file(self) -> &'static str {
        match self {
            Self::Dev => "deploy/compose.dev.yaml",
            Self::Test => "deploy/compose.test.yaml",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Test => "test",
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

    /// 이 프로젝트의 컨테이너·네트워크·볼륨만 제거한다.
    pub fn down(&self, sh: &Shell) -> Result<()> {
        let args = self.base_args();
        cmd!(sh, "docker {args...} down --volumes --remove-orphans").run()?;
        Ok(())
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

/// dev·test 프로젝트를 모두 정리한다.
pub fn down_all(root: &Path) -> Result<()> {
    for env in [Env::Dev, Env::Test] {
        let compose = Compose::new(root, env).with_profile("oidc");
        let sh = compose.shell()?;
        println!("==> down {}", compose.project());
        compose.down(&sh)?;
    }
    Ok(())
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
/// compose.dev·test.yaml과 CI 서비스의 Meilisearch 테스트 마스터 키(명백한 테스트 값).
pub const SEARCH_TEST_KEY: &str = "fvoci-test-meili-master-key-16chars";
/// compose.test.yaml의 SeaweedFS 버킷·자격 증명(명백한 테스트 값).
pub const S3_TEST_BUCKET: &str = "fvoci-test";
pub const S3_TEST_ACCESS_KEY: &str = "fvoci-test-s3-key";
pub const S3_TEST_SECRET_KEY: &str = "fvoci-test-s3-secret";
