//! 컨테이너 시험의 호스트 소유권과 이 체크아웃 Compose 자원 검증(D-10).

use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, ensure};
use xshell::{Shell, cmd};

use crate::compose::{Compose, Env};

pub const OWNERSHIP_PASSED: &str = "==> host ownership check passed";
pub const RESOURCES_PASSED: &str = "==> Compose resource check passed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerMode {
    Rootful,
    Rootless,
    UsernsRemap,
}

impl DockerMode {
    pub fn from_options(options: &[String]) -> Self {
        // rootless 데몬 자체가 만든 사용자 네임스페이스에서는 0이 호출자다. 일반 userns 표시보다 우선한다.
        if options.iter().any(|option| option == "name=rootless") {
            Self::Rootless
        } else if options.iter().any(|option| option == "name=userns") {
            Self::UsernsRemap
        } else {
            Self::Rootful
        }
    }
}

pub fn docker_mode(sh: &Shell) -> Result<DockerMode> {
    let format = "{{json .SecurityOptions}}";
    let raw = cmd!(sh, "docker info --format {format}")
        .quiet()
        .read()
        .context("read Docker security options before selecting the test runner")?;
    println!("==> Docker security options: {raw}");
    let options: Option<Vec<String>> =
        serde_json::from_str(&raw).context("parse Docker security options")?;
    Ok(DockerMode::from_options(&options.unwrap_or_default()))
}

#[derive(Debug, Clone, Copy)]
pub enum Requirement {
    Any,
    Rootless,
}

/// CI 사전 단언과 로컬 수동 검증은 같은 판정을 쓴다. 다른 체크아웃의 자원은 조회하지 않는다.
pub fn run(root: &Path, requirement: Requirement) -> Result<()> {
    let compose = Compose::new(root, Env::Test);
    let _lock = compose.lock_run()?;
    let sh = compose.shell()?;
    let mode = docker_mode(&sh)?;
    if let Requirement::Rootless = requirement {
        ensure!(
            mode == DockerMode::Rootless,
            "Docker must report name=rootless"
        );
    }
    let ownership = ownership(root, &sh);
    let resources = resources_absent(&sh, compose.project());
    for error in [&ownership, &resources]
        .into_iter()
        .filter_map(|r| r.as_ref().err())
    {
        eprintln!("{error:#}");
    }
    ownership.and(resources)
}

pub fn ownership(root: &Path, sh: &Shell) -> Result<()> {
    let uid: u32 = cmd!(sh, "id -u")
        .quiet()
        .read()?
        .parse()
        .context("parse host UID")?;
    let started = Instant::now();
    check_ownership(root, uid)?;
    println!(
        "{OWNERSHIP_PASSED} (UID {uid}, {:.3}s)",
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// symlink는 그 자체만 검사한다. target을 포함한 작업 트리를 훑되 밖의 경로는 따라가지 않는다.
#[cfg(unix)]
fn check_ownership(root: &Path, uid: u32) -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let mut pending = vec![root.to_path_buf()];
    let mut foreign = Vec::new();
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)
            .with_context(|| format!("inspect ownership of {}", path.display()))?;
        if metadata.uid() != uid {
            foreign.push(path.display().to_string());
            if foreign.len() == 5 {
                break;
            }
        }
        if metadata.is_dir() {
            for entry in std::fs::read_dir(&path)
                .with_context(|| format!("read ownership directory {}", path.display()))?
            {
                pending.push(entry?.path());
            }
        }
    }
    ensure!(
        foreign.is_empty(),
        "paths not owned by host UID {uid} (first up to 5):\n{}",
        foreign.join("\n")
    );
    Ok(())
}

#[cfg(not(unix))]
fn check_ownership(_root: &Path, _uid: u32) -> Result<()> {
    anyhow::bail!("host ownership verification requires Unix metadata")
}

pub fn resources_absent(sh: &Shell, project: &str) -> Result<()> {
    let label = format!("label=com.docker.compose.project={project}");
    let mut remaining = Vec::new();
    for kind in ["container", "network", "volume"] {
        let all: Vec<&str> = if kind == "container" {
            vec!["--all"]
        } else {
            Vec::new()
        };
        let ids = cmd!(sh, "docker {kind} ls {all...} --quiet --filter {label}")
            .quiet()
            .read()
            .with_context(|| format!("check remaining {kind} for {project}"))?;
        for id in ids.lines().filter(|line| !line.trim().is_empty()).take(5) {
            remaining.push(format!("{kind}: {id}"));
        }
    }
    ensure!(
        remaining.is_empty(),
        "Compose resources remain for {project}:\n{}",
        remaining.join("\n")
    );
    println!("{RESOURCES_PASSED} ({project})");
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    use super::*;
    use crate::compose::tests::scratch_root;

    #[test]
    fn review_regression_gid_only_mismatch_is_rejected() -> Result<()> {
        let root = scratch_root()?;
        std::fs::write(root.join("group-only-file"), "fixture")?;
        let metadata = std::fs::metadata(&root)?;
        let uid = metadata.uid();
        let gid = metadata.gid().wrapping_add(1);
        let id = root.join("id");
        std::fs::write(
            &id,
            format!(
                "#!/bin/sh\ncase \"$1\" in -u) echo {uid};; -g) echo {gid};; *) exit 42;; esac\n"
            ),
        )?;
        std::fs::set_permissions(&id, std::fs::Permissions::from_mode(0o755))?;
        let sh = Shell::new()?;
        let _path = sh.push_env("PATH", &root);
        let result = ownership(&root, &sh);
        std::fs::remove_dir_all(&root)?;
        let error = result.expect_err("correct UID with a different GID must fail");
        let message = format!("{error:#}");
        assert!(
            message.contains(&format!("UID/GID {uid}:{gid}")),
            "{message}"
        );
        assert!(message.contains("group-only-file"), "{message}");
        Ok(())
    }

    #[test]
    fn owned_tree_passes_without_following_a_symlink_loop() -> Result<()> {
        let root = scratch_root()?;
        std::fs::create_dir_all(root.join("target/test-container"))?;
        std::fs::write(root.join("target/test-container/file"), "owned")?;
        symlink(&root, root.join("loop"))?;
        check_ownership(&root, std::fs::metadata(&root)?.uid())?;
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn different_owner_fails_with_the_file_path() -> Result<()> {
        let root = scratch_root()?;
        let path = root.join("foreign-file");
        std::fs::write(&path, "fixture")?;
        let other = std::fs::metadata(&path)?.uid().wrapping_add(1);
        let error = check_ownership(&root, other).expect_err("a different owner must fail");
        assert!(format!("{error:#}").contains(&path.display().to_string()));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn resource_check_includes_stopped_containers_and_scopes_every_query() -> Result<()> {
        let root = scratch_root()?;
        let docker = root.join("docker");
        std::fs::write(
            &docker,
            r#"#!/bin/sh
case "$*" in
  'container ls --all --quiet --filter label=com.docker.compose.project=owned') ;;
  'network ls --quiet --filter label=com.docker.compose.project=owned') ;;
  'volume ls --quiet --filter label=com.docker.compose.project=owned')
    if [ -f "leftover" ]; then echo left-volume; fi ;;
  *) exit 42 ;;
esac
"#,
        )?;
        std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755))?;
        let sh = Shell::new()?;
        sh.change_dir(&root);
        let _path = sh.push_env("PATH", &root);
        resources_absent(&sh, "owned")?;
        std::fs::write(root.join("leftover"), "")?;
        let error = resources_absent(&sh, "owned").expect_err("leftover volume must fail");
        assert!(format!("{error:#}").contains("volume: left-volume"));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
