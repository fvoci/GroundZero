//! 컨테이너 준비 경계와 실제 rootful Docker의 마운트 쓰기·호스트 소유권을 확인한다.

use super::{Execution, finish_run, prepare_test_container};
use crate::compose::tests::scratch_root;
use crate::compose::{Compose, Env};
use crate::test_verify::{self, DockerMode};
use anyhow::{Context, Result};
use xshell::cmd;

fn platform_fixture(daemon: &str, manifest: &str) -> Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    let root = scratch_root()?;
    std::fs::write(root.join("daemon-platform"), daemon)?;
    std::fs::write(root.join("manifest.json"), manifest)?;
    std::fs::write(root.join("security-options"), "[]")?;
    for (name, script) in [
        (
            "docker",
            r#"#!/bin/sh
echo "$*" >> calls
case "$1 $2" in
  'info --format')
    case "$3" in
      *SecurityOptions*) cat security-options ;;
      *) cat daemon-platform ;;
    esac ;;
  'manifest inspect')
    [ ! -f manifest-failure ] || exit 19
    cat manifest.json ;;
  'run --rm') exit 0 ;;
  *) exit 97 ;;
esac
"#,
        ),
        ("gh", "#!/bin/sh\nprintf 'sha256:%064d\\n' 0\n"),
    ] {
        let path = root.join(name);
        std::fs::write(&path, script)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(root)
}

#[test]
fn missing_daemon_platform_fails_before_starting_services_or_runner() -> Result<()> {
    let manifest = r#"[{"Descriptor":{"platform":{"os":"linux","architecture":"amd64"}}}]"#;
    let root = platform_fixture("linux/aarch64", manifest)?;
    let sh = Compose::new(&root, Env::Test).shell()?;
    let _path = sh.push_env("PATH", format!("{}:{}", root.display(), sh.var("PATH")?));
    let result = prepare_test_container(
        &root,
        &Compose::new(&root, Env::Test),
        &sh,
        Execution::Container,
    );
    let error = result.expect_err("an amd64-only digest must reject an ARM64 daemon");
    let message = format!("{error:#}");
    for expected in ["linux/arm64", "CI_BASE_IMAGE_DIGEST", "--host"] {
        assert!(message.contains(expected), "missing {expected}: {message}");
    }
    let calls = std::fs::read_to_string(root.join("calls"))?;
    assert!(
        !calls
            .lines()
            .any(|line| line.starts_with("run ") || line.starts_with("compose ")),
        "{calls}"
    );
    assert!(!root.join("target/test-container").exists());
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn available_daemon_platform_is_pinned_for_rootful_and_rootless() -> Result<()> {
    let host = xshell::Shell::new()?;
    let uid = cmd!(host, "id -u").read()?;
    let gid = cmd!(host, "id -g").read()?;
    let host_user = format!("{uid}:{gid}");
    let manifest = r#"[
      {"Descriptor":{"platform":{"os":"linux","architecture":"amd64"}}},
      {"Descriptor":{"platform":{"os":"linux","architecture":"arm64","variant":"v8"}}},
      {"Descriptor":{"platform":{"os":"unknown","architecture":"unknown"}}}
    ]"#;
    for (daemon, platform) in [
        ("linux/x86_64", "linux/amd64"),
        ("linux/aarch64", "linux/arm64"),
    ] {
        for (options, user) in [("[]", host_user.as_str()), (r#"["name=rootless"]"#, "0:0")] {
            let root = platform_fixture(daemon, manifest)?;
            std::fs::write(root.join("security-options"), options)?;
            let compose = Compose::new(&root, Env::Test);
            let sh = compose.shell()?;
            let _path = sh.push_env("PATH", format!("{}:{}", root.display(), sh.var("PATH")?));
            let runner = prepare_test_container(&root, &compose, &sh, Execution::Container)?
                .context("a published daemon platform must select the container")?;
            runner.run(&sh, &[], &["true"])?;
            let calls = std::fs::read_to_string(root.join("calls"))?;
            let call = calls
                .lines()
                .find(|line| line.starts_with("run "))
                .context("missing docker run")?;
            assert!(call.contains(&format!("--platform {platform} ")), "{call}");
            assert!(call.contains(&format!("--user {user} ")), "{call}");
            std::fs::remove_dir_all(root)?;
        }
    }
    Ok(())
}

#[test]
fn unverifiable_manifest_fails_before_preparing_the_runner() -> Result<()> {
    for registry_failure in [false, true] {
        let root = platform_fixture("linux/x86_64", "not JSON")?;
        if registry_failure {
            std::fs::write(root.join("manifest-failure"), "")?;
        }
        let compose = Compose::new(&root, Env::Test);
        let sh = compose.shell()?;
        let _path = sh.push_env("PATH", format!("{}:{}", root.display(), sh.var("PATH")?));
        let error = prepare_test_container(&root, &compose, &sh, Execution::Container)
            .expect_err("registry or JSON failure must not fall back to another runner");
        let message = format!("{error:#}");
        for expected in ["linux/amd64", "CI_BASE_IMAGE_DIGEST", "--host"] {
            assert!(message.contains(expected), "missing {expected}: {message}");
        }
        assert!(!root.join("target/test-container").exists());
        let calls = std::fs::read_to_string(root.join("calls"))?;
        assert!(
            !calls.lines().any(|line| line.starts_with("run ")),
            "{calls}"
        );
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}

#[test]
fn failed_tests_keep_the_original_error_and_still_verify_cleanup() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let root = scratch_root()?;
    let docker = root.join("docker");
    std::fs::write(
        &docker,
        "#!/bin/sh\necho \"$*\" >> calls\ncase \"$1\" in compose) exit 19;; esac\n",
    )?;
    std::fs::set_permissions(&docker, std::fs::Permissions::from_mode(0o755))?;
    let compose = Compose::new(&root, Env::Test);
    let sh = compose.shell()?;
    let path = format!("{}:{}", root.display(), sh.var("PATH")?);
    let _path = sh.push_env("PATH", path);
    let error = finish_run(
        &root,
        &compose,
        &sh,
        false,
        Err(anyhow::anyhow!("original test failure")),
    )
    .expect_err("the failed test must remain failed");
    assert_eq!(error.to_string(), "original test failure");
    let calls = std::fs::read_to_string(root.join("calls"))?;
    for query in ["container ls --all", "network ls", "volume ls"] {
        assert!(
            calls.contains(query),
            "verification missing after cleanup failed: {calls}"
        );
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
#[ignore = "requires a local rootful amd64 Docker daemon and the fixed ci-base image; run explicitly"]
fn default_container_preserves_host_ownership() -> Result<()> {
    let root = scratch_root()?;
    let compose = Compose::new(&root, Env::Test);
    let sh = compose.shell()?;
    anyhow::ensure!(
        test_verify::docker_mode(&sh)? == DockerMode::Rootful,
        "this acceptance test requires rootful Docker"
    );
    let runner = prepare_test_container(&root, &compose, &sh, Execution::Container)?
        .context("default rootful amd64 execution must select a container")?;
    let project = compose.project();
    let network = format!("{project}_default");
    let label = format!("com.docker.compose.project={project}");
    cmd!(sh, "docker network create --label {label} {network}").run()?;
    let result = runner.run(
        &sh,
        &[],
        &[
            "sh",
            "-c",
            "printf owned > target/test-container/home/ownership-proof",
        ],
    );
    let cleanup = cmd!(sh, "docker network rm {network}")
        .run()
        .map_err(anyhow::Error::from);
    let ownership = test_verify::ownership(&root, &sh);
    let resources = test_verify::resources_absent(&sh, project);
    for error in [&result, &cleanup, &ownership, &resources]
        .into_iter()
        .filter_map(|r| r.as_ref().err())
    {
        eprintln!("rootful acceptance: {error:#}");
    }
    result.and(cleanup).and(ownership).and(resources)?;
    anyhow::ensure!(
        root.join("target/test-container/home/ownership-proof")
            .is_file(),
        "runner did not write the bind-mounted proof"
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
}
