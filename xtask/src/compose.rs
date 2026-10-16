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

/// 테스트 DB 접속 문자열. 자격 증명은 compose 파일의 테스트 전용 값과 같다.
pub fn test_database_url(port: u16) -> String {
    format!("postgres://fvoci:fvoci-test-password@127.0.0.1:{port}/fvoci")
}
