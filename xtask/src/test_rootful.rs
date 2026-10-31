//! 실제 rootful Docker에서 기본 경로 선택·마운트 쓰기·호스트 소유권을 확인하는 대상 시험.

use super::{Execution, finish_run, prepare_test_container};
use crate::compose::tests::scratch_root;
use crate::compose::{Compose, Env};
use crate::test_verify::{self, DockerMode};
use anyhow::{Context, Result};
use xshell::cmd;

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
