//! fork 자식이 상속한 쓰기 FD까지 닫힌 뒤 가짜 실행 파일을 게시한다.

use std::fs::File;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{Context, Result};

pub(crate) fn write_executable(path: &Path, script: &str) -> Result<()> {
    let pending = path.with_extension("pending");
    let mut writer = File::options()
        .write(true)
        .create_new(true)
        .open(&pending)?;
    writer.write_all(script.as_bytes())?;
    writer.set_permissions(std::fs::Permissions::from_mode(0o755))?;
    // flock은 fork/dup으로 공유한 open file description의 마지막 FD까지 유지된다.
    writer
        .lock()
        .context("lock executable writer and its inherited descriptors")?;
    drop(writer);
    publish_executable(&pending, path)
}

fn publish_executable(pending: &Path, path: &Path) -> Result<()> {
    let reader = File::open(pending)?;
    reader
        .lock_shared()
        .context("wait for all executable writer descriptors to close")?;
    std::fs::rename(pending, path).context("publish completed executable")
}

#[test]
fn publication_waits_for_a_childs_inherited_writer_before_replacing_the_target() -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    let root = crate::compose::tests::scratch_root()?;
    let target = root.join("fake-tool");
    let pending = target.with_extension("pending");
    std::fs::write(&target, "old target")?;
    let mut writer = File::create_new(&pending)?;
    writer.write_all(b"#!/bin/sh\nexit 0\n")?;
    writer.set_permissions(std::fs::Permissions::from_mode(0o755))?;
    writer.lock()?;
    // stdout에 같은 쓰기 FD를 상속시켜 fork→exec 사이의 보유 상태를 확정적으로 유지한다.
    let mut child = Command::new("sh")
        .args(["-c", "printf 'ready\\n' >&2; read -r release || :"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(writer.try_clone()?))
        .stderr(Stdio::piped())
        .spawn()?;
    drop(writer);
    let mut ready = String::new();
    BufReader::new(child.stderr.take().context("child readiness pipe")?).read_line(&mut ready)?;
    let (completed, completion) = mpsc::channel();
    let publish_target = target.clone();
    let publisher = std::thread::spawn(move || -> Result<()> {
        let result = publish_executable(&pending, &publish_target);
        completed
            .send(())
            .context("report executable publication")?;
        result
    });
    let premature = completion.recv_timeout(Duration::from_millis(100)).is_ok();
    let before_release = std::fs::read_to_string(&target);
    // 단언 전에 자식을 끝내고 게시 스레드를 회수해 실패 경로도 잠금을 남기지 않는다.
    drop(child.stdin.take());
    let status = child.wait()?;
    let published = publisher
        .join()
        .map_err(|_| anyhow::anyhow!("publisher thread panicked"))?;
    assert_eq!(ready, "ready\n");
    assert!(status.success(), "writer holder failed: {status}");
    assert!(
        !premature,
        "executable was published while an inherited writer remained open"
    );
    assert_eq!(before_release?, "old target");
    published?;
    assert!(Command::new(&target).status()?.success());
    std::fs::remove_dir_all(root)?;
    Ok(())
}
