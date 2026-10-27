//! 셀프호스트 Supabase 시험 구성(D-88). 공식 구성(supabase/supabase 저장소의 `docker/`)을 고정 revision으로
//! `target/supabase/<rev>/`에 받고, `deploy/supabase/compose.test.yaml`을 겹쳐 db·rest·supavisor만 띄운다.
//! 공식 구성은 저장소에 넣지 않는다(받아서 쓰기만 한다). 프로젝트명·포트·데이터는 체크아웃마다 따로다.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use xshell::{Shell, cmd};

use crate::compose::{self, Env};

#[cfg(test)]
mod tests;

/// 공식 셀프호스트 구성의 revision(2026-10-16 "update 2026-10-16 - 0.8.3"). 바꾸면 compose.test.yaml의 이미지
/// digest와 docs/self-hosting-supabase.md도 함께 확인한다.
pub const REVISION: &str = "bc27d2661edd09a5d0b8d76ce33da6d4071a756e";
const REPOSITORY: &str = "https://github.com/supabase/supabase.git";
/// 공식 구성이 쓰는 DB 이미지 태그(받은 구성이 이 태그인지 확인한다).
pub const DB_IMAGE_TAG: &str = "supabase/postgres:17.6.1.136";
/// `deploy/supabase/test.env`의 시험 값.
pub const POSTGRES_PASSWORD: &str = "fvoci-test-supabase-postgres-password";
pub const POOLER_TENANT: &str = "fvoci-test";

/// 받은 공식 구성의 `docker/` 폴더. 없으면 고정 revision을 얕게 받는다(그 폴더만).
pub fn upstream(root: &Path) -> Result<PathBuf> {
    let dir = root.join("target/supabase").join(REVISION);
    let docker = dir.join("docker");
    let sh = Shell::new()?;
    if !docker.join("docker-compose.yml").exists() {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).with_context(|| format!("remove {}", dir.display()))?;
        }
        std::fs::create_dir_all(&dir)?;
        sh.change_dir(&dir);
        println!("==> fetch supabase/supabase docker/ at {REVISION}");
        cmd!(sh, "git init --quiet").run()?;
        cmd!(sh, "git remote add origin {REPOSITORY}").run()?;
        cmd!(sh, "git sparse-checkout set --no-cone /docker").run()?;
        cmd!(
            sh,
            "git fetch --quiet --depth 1 --filter=blob:none origin {REVISION}"
        )
        .run()?;
        cmd!(sh, "git checkout --quiet FETCH_HEAD").run()?;
    }
    sh.change_dir(&dir);
    let head = cmd!(sh, "git rev-parse HEAD").read()?;
    if head.trim() != REVISION {
        bail!(
            "{} is at {head}, not {REVISION}; remove it and run again",
            dir.display()
        );
    }
    let file = std::fs::read_to_string(docker.join("docker-compose.yml"))?;
    if !file.contains(&format!("image: {DB_IMAGE_TAG}")) {
        bail!("the upstream compose file does not use {DB_IMAGE_TAG}");
    }
    Ok(docker)
}

/// POSIX 셸이 인자 하나로 읽도록 인용한다. 안전한 글자만 있으면 그대로 두고, 아니면 작은따옴표로 감싸며 안의
/// 작은따옴표는 `'\''`로 바꾼다.
fn shell_quote(arg: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c);
    if !arg.is_empty() && arg.chars().all(safe) {
        arg.to_owned()
    } else {
        format!("'{}'", arg.replace('\'', r"'\''"))
    }
}

/// 시험 구성 핸들.
#[derive(Debug)]
pub struct Supabase {
    root: PathBuf,
    upstream: PathBuf,
    project: String,
}

impl Supabase {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            upstream: upstream(root)?,
            project: compose::project_name(root, Env::Supabase),
        })
    }

    pub fn project(&self) -> &str {
        &self.project
    }

    /// 강제로 끝난 실행의 이 Supabase 프로젝트 컨테이너만 멈추는 명령([`compose::Compose::recovery_command`]와 같은
    /// 방식). 볼륨은 지우지 않으며 DB는 tmpfs라 컨테이너가 멈추면 내용이 사라진다.
    ///
    /// 경로는 체크아웃 위치의 절대 경로라 공백이나 따옴표가 있을 수 있으므로 인자마다 POSIX 셸 규칙으로 인용한다
    /// (붙여 넣은 명령에서 경로가 여러 인자로 갈라지면 `down`이 실패한다).
    pub fn recovery_command(&self) -> String {
        let args: Vec<String> = self.args().iter().map(|a| shell_quote(a)).collect();
        format!("docker {} down --remove-orphans", args.join(" "))
    }

    fn args(&self) -> Vec<String> {
        let up = self.upstream.to_string_lossy().into_owned();
        vec![
            "compose".into(),
            "--project-name".into(),
            self.project.clone(),
            "--project-directory".into(),
            up.clone(),
            "--env-file".into(),
            self.root
                .join("deploy/supabase/test.env")
                .to_string_lossy()
                .into_owned(),
            "--file".into(),
            format!("{up}/docker-compose.yml"),
            "--file".into(),
            self.root
                .join("deploy/supabase/compose.test.yaml")
                .to_string_lossy()
                .into_owned(),
        ]
    }

    /// A kept pooler can retain SCRAM secrets and authenticated backends across bootstrap's
    /// same-password reset. Stop only this project's pooler before changing roles, then start
    /// it after migration; HTTP health alone does not establish app-role authentication.
    pub fn prepare_database(&self, sh: &Shell) -> Result<compose::DbUrls> {
        let args = self.args();
        let args = &args;
        let result: Result<compose::DbUrls> = (|| {
            cmd!(sh, "docker {args...} stop supavisor")
                .run()
                .context("stop Supavisor before role preparation")?;
            cmd!(sh, "docker {args...} up --detach --wait db rest")
                .run()
                .context("start Supabase database and REST")?;
            let port = self.host_port(sh, "db", 5432)?;
            let urls = compose::DbUrls::supabase(port);
            compose::prepare_database(sh, &urls)?;
            cmd!(sh, "docker {args...} up --detach --wait supavisor")
                .run()
                .context("start Supavisor after role preparation")?;
            Ok(urls)
        })();
        if result.is_err() {
            // Preserve diagnostics before the caller cleans up, including bootstrap failures.
            self.diagnose(sh);
        }
        result.with_context(|| format!("Supabase preparation failed (project {})", self.project))
    }

    /// 서비스마다 상태·종료 코드·재시작 횟수·OOM·healthcheck 기록·최근 로그를 출력한다. 시험 env의 비밀 값은 가린다.
    pub fn diagnose(&self, sh: &Shell) {
        let secrets = self.secrets();
        let redact = |text: &str| {
            secrets.iter().fold(text.to_owned(), |acc, secret| {
                acc.replace(secret.as_str(), "***")
            })
        };
        let args = self.args();
        let state = "status={{.State.Status}} exit={{.State.ExitCode}} restarts={{.RestartCount}} \
                     oom={{.State.OOMKilled}} error={{.State.Error}} \
                     health={{if .State.Health}}{{.State.Health.Status}}{{end}}";
        let probes = "{{if .State.Health}}{{range .State.Health.Log}}[exit {{.ExitCode}}] {{.Output}}\n{{end}}{{end}}";
        for service in ["db", "rest", "supavisor"] {
            println!("==> diagnose {service} ({})", self.project);
            let args = args.clone();
            let id = cmd!(sh, "docker {args...} ps --all --quiet {service}")
                .quiet()
                .ignore_status()
                .read()
                .unwrap_or_default();
            let Some(id) = id.lines().next().filter(|l| !l.trim().is_empty()) else {
                println!("no container");
                continue;
            };
            for format in [state, probes] {
                if let Ok(out) = cmd!(sh, "docker inspect --format {format} {id}")
                    .quiet()
                    .ignore_status()
                    .output()
                {
                    print!("{}", redact(&String::from_utf8_lossy(&out.stdout)));
                }
            }
            println!();
            if let Ok(out) = cmd!(sh, "docker logs --tail 120 {id}")
                .quiet()
                .ignore_status()
                .output()
            {
                print!("{}", redact(&String::from_utf8_lossy(&out.stdout)));
                print!("{}", redact(&String::from_utf8_lossy(&out.stderr)));
            }
        }
    }

    /// 시험 env에서 가릴 값(비밀번호·키·시크릿).
    fn secrets(&self) -> Vec<String> {
        let env =
            std::fs::read_to_string(self.root.join("deploy/supabase/test.env")).unwrap_or_default();
        env.lines()
            .filter_map(|line| line.split_once('='))
            .filter(|(key, value)| {
                ["PASSWORD", "SECRET", "KEY"]
                    .iter()
                    .any(|k| key.contains(k))
                    && value.len() >= 8
            })
            .map(|(_, value)| value.trim().to_owned())
            .collect()
    }

    pub fn down(&self, sh: &Shell) -> Result<()> {
        let args = self.args();
        cmd!(sh, "docker {args...} down --volumes --remove-orphans").run()?;
        Ok(())
    }

    pub fn host_port(&self, sh: &Shell, service: &str, container_port: u16) -> Result<u16> {
        let args = self.args();
        let port = container_port.to_string();
        let out = cmd!(sh, "docker {args...} port {service} {port}").read()?;
        let line = out.lines().next().unwrap_or_default();
        let Some((_, port)) = line.rsplit_once(':') else {
            bail!("unexpected `docker compose port` output: {out:?}");
        };
        port.trim()
            .parse()
            .with_context(|| format!("not a port: {port:?}"))
    }
}

/// 이 체크아웃의 Supabase 시험 프로젝트를 정리한다(받아 둔 공식 구성이 없으면 할 일이 없다).
pub fn down(root: &Path) -> Result<()> {
    let dir = root.join("target/supabase").join(REVISION).join("docker");
    if !dir.join("docker-compose.yml").exists() {
        return Ok(());
    }
    let supabase = Supabase::new(root)?;
    let sh = Shell::new()?;
    sh.change_dir(root);
    println!("==> down {}", supabase.project());
    supabase.down(&sh)
}
