//! 같은 Cargo 인자·환경의 빌드 산출물과 실행 파일별 결과 행을 대조한다.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use xshell::{Shell, cmd};

const EXPECTED: &str = "==> expected test executable: ";
const COMPLETE: &str = "==> test executable inventory complete: ";

/// Cargo test 인자를 실행과 공유하고 같은 Shell의 프로필·기능·target·환경을 그대로 사용한다.
pub fn emit_inventory(sh: &Shell, args: &[&str]) -> Result<()> {
    println!(
        "==> compile test executable inventory (cargo {} --no-run)",
        args.join(" ")
    );
    let mut command: Command = cmd!(sh, "cargo {args...} --no-run --message-format=json").into();
    let output = command
        .stderr(Stdio::inherit())
        .output()
        .context("compile test executable inventory")?;
    let messages = String::from_utf8(output.stdout).context("Cargo inventory must be UTF-8")?;
    for line in messages.lines().filter(|line| line.starts_with('{')) {
        let value: serde_json::Value =
            serde_json::from_str(line).context("parse Cargo diagnostic")?;
        if value["reason"] == "compiler-message"
            && let Some(rendered) = value["message"]["rendered"].as_str()
        {
            eprint!("{rendered}");
        }
    }
    ensure!(
        output.status.success(),
        "test inventory build failed: {}",
        output.status
    );
    let executables = from_cargo(&messages)?;
    for name in &executables {
        println!("{EXPECTED}{name}");
    }
    println!("{COMPLETE}{}", executables.len());
    Ok(())
}

fn executable_name(path: &str) -> Result<String> {
    Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .context("test executable path has no UTF-8 filename")
}

fn from_cargo(messages: &str) -> Result<BTreeSet<String>> {
    let mut executables = BTreeSet::new();
    let mut finished = false;
    for line in messages.lines().filter(|line| line.starts_with('{')) {
        let value: serde_json::Value =
            serde_json::from_str(line).context("parse Cargo build artifact")?;
        if value["reason"] == "compiler-artifact"
            && value["profile"]["test"] == true
            && let Some(path) = value["executable"].as_str()
        {
            ensure!(
                executables.insert(executable_name(path)?),
                "duplicate test executable filename: {path}"
            );
        }
        if value["reason"] == "build-finished" {
            finished = value["success"] == true;
        }
    }
    ensure!(
        finished && !executables.is_empty(),
        "Cargo did not complete a nonempty test executable inventory"
    );
    Ok(executables)
}

#[derive(Debug)]
enum TestGroup {
    Executable(String),
    DocTests,
}

#[derive(Debug, PartialEq, Eq)]
enum StdoutGroup {
    Started,
    Complete,
}

#[derive(Debug, Default)]
pub struct Coverage {
    expected: BTreeSet<String>,
    declared: Option<usize>,
    groups: Vec<TestGroup>,
    stdout_groups: Vec<StdoutGroup>,
    result_rows: usize,
    unassigned_rows: usize,
}

fn starts_test_group(line: &str) -> bool {
    let mut words = line.split_whitespace();
    words.next() == Some("running")
        && words
            .next()
            .and_then(|word| word.parse::<usize>().ok())
            .is_some()
        && matches!(words.next(), Some("test" | "tests"))
        && words.next().is_none()
}

impl Coverage {
    pub fn observe(&mut self, line: &str) -> Result<()> {
        if let Some(name) = line.strip_prefix(EXPECTED) {
            ensure!(
                self.declared.is_none()
                    && !name.is_empty()
                    && self.expected.insert(name.to_owned()),
                "invalid or duplicate test executable inventory: {name}"
            );
        } else if let Some(count) = line.strip_prefix(COMPLETE) {
            ensure!(
                self.declared.is_none(),
                "duplicate executable inventory completion"
            );
            self.declared = Some(count.parse().context("parse executable inventory count")?);
        } else if let Some(running) = line.trim_start().strip_prefix("Running ") {
            let (_, path) = running
                .split_once(" (")
                .context("Cargo Running line has no executable path")?;
            let path = path
                .strip_suffix(')')
                .context("unterminated Cargo executable path")?;
            self.groups
                .push(TestGroup::Executable(executable_name(path)?));
        } else if line.trim_start().starts_with("Doc-tests ") {
            self.groups.push(TestGroup::DocTests);
        } else if starts_test_group(line) {
            self.stdout_groups.push(StdoutGroup::Started);
        } else if line.starts_with("test result:") {
            match self.stdout_groups.last_mut() {
                Some(group @ StdoutGroup::Started) => *group = StdoutGroup::Complete,
                _ => self.unassigned_rows += 1,
            }
            self.result_rows = self
                .result_rows
                .checked_add(1)
                .context("test group count overflow")?;
        }
        Ok(())
    }

    // Cargo는 순차 실행한다. stderr 헤더와 stdout 그룹의 각 스트림 내 순서는 유지되지만
    // Docker/tee가 두 스트림을 합친 도착 순서는 달라질 수 있어, 전부 읽은 뒤 N번째끼리 대응한다.
    fn completed(&self) -> BTreeSet<&str> {
        self.groups
            .iter()
            .zip(&self.stdout_groups)
            .filter_map(|(group, result)| match (group, result) {
                (TestGroup::Executable(name), StdoutGroup::Complete) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.expected.is_empty() && self.declared == Some(self.expected.len()),
            "missing or incomplete Cargo test executable inventory"
        );
        let completed = self.completed();
        let missing: Vec<&str> = self
            .expected
            .iter()
            .map(String::as_str)
            .filter(|name| !completed.contains(name))
            .collect();
        ensure!(
            missing.is_empty(),
            "test executables missing results: {}",
            missing.join(", ")
        );
        let mut seen = BTreeSet::new();
        for group in &self.groups {
            if let TestGroup::Executable(name) = group {
                ensure!(
                    seen.insert(name),
                    "duplicate test executable header: {name}"
                );
                ensure!(
                    self.expected.contains(name),
                    "test result without build artifact: {name}"
                );
            }
        }
        ensure!(
            self.result_rows == self.groups.len() && self.stdout_groups.len() == self.groups.len(),
            "Cargo executable/doc-test groups and stdout groups/results differ: {} headers, {} stdout groups, {} results",
            self.groups.len(),
            self.stdout_groups.len(),
            self.result_rows
        );
        ensure!(
            self.unassigned_rows == 0
                && self
                    .stdout_groups
                    .iter()
                    .all(|group| *group == StdoutGroup::Complete),
            "stdout test groups must each have exactly one result row"
        );
        Ok(())
    }

    pub fn summary(&self) -> String {
        let completed = self.completed();
        let missing: Vec<&str> = self
            .expected
            .iter()
            .map(String::as_str)
            .filter(|name| !completed.contains(name))
            .collect();
        format!(
            "Test executable results: {}/{}; inventory declared: {:?}; Cargo groups/results: {}/{}; missing: {}",
            completed.len(),
            self.expected.len(),
            self.declared,
            self.groups.len(),
            self.result_rows,
            if missing.is_empty() {
                "none".to_owned()
            } else {
                missing.join(", ")
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ci_interleaving_keeps_worker_and_workspace_results() -> Result<()> {
        // 41206300523의 순서: 다음 stderr Running이 이전 stdout 결과보다 먼저 도착한다.
        let log = "==> expected test executable: worker-d23f2f3e1008d35c\n\
==> expected test executable: worker_isolation-e03adb3e3835618a\n\
==> expected test executable: workspace_delete-13e2e563cfc8d2f8\n\
==> expected test executable: fvoci_storage-3f640a1697b6fa11\n\
==> test executable inventory complete: 4\n\
     Running tests/worker.rs (target/test-container/build/debug/deps/worker-d23f2f3e1008d35c)\n\
running 3 tests\n\
test indexing_rechecks_the_claim_and_generation_before_sending ... ok\n\
test storage_cleanup_deletes_every_object_queued_together ... ok\n\
     Running tests/worker_isolation.rs (target/test-container/build/debug/deps/worker_isolation-e03adb3e3835618a)\n\
test the_worker_purges_only_expired_trash_and_fills_repeating_tasks ... ok\n\
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.83s\n\
running 4 tests\n\
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
     Running tests/workspace_delete.rs (target/test-container/build/debug/deps/workspace_delete-13e2e563cfc8d2f8)\n\
running 3 tests\n\
test purge_succeeds_with_a_trashed_module ... ok\n\
test only_the_owner_deletes_and_everyone_loses_access_until_restored ... ok\n\
     Running unittests src/lib.rs (target/test-container/build/debug/deps/fvoci_storage-3f640a1697b6fa11)\n\
test purge_removes_everything_but_personal_time_and_expiry_purges_in_the_worker ... ok\n\
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
running 4 tests\n\
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
        let mut coverage = Coverage::default();
        for line in log.lines() {
            coverage.observe(line)?;
        }
        coverage.validate()?;
        assert!(coverage.summary().contains("4/4"));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn inventory_build_preserves_shell_context_and_all_test_build_arguments() -> Result<()> {
        use std::os::unix::fs::PermissionsExt;

        let root = crate::compose::tests::scratch_root()?;
        let cargo = root.join("cargo");
        std::fs::write(
            &cargo,
            r#"#!/bin/sh
test "$*" = 'test --workspace --locked --profile test --features feature-a --no-run --message-format=json' || exit 31
test "$CARGO_TARGET_DIR" = '/work with spaces/test-target' || exit 32
test "$CARGO_PROFILE_TEST_OPT_LEVEL" = '1' || exit 33
test -f shell-working-directory || exit 34
test ! -f reject || exit 35
printf '%s\n' '{"reason":"compiler-artifact","profile":{"test":true},"executable":"/work with spaces/test-target/debug/deps/unit-a"}' '{"reason":"build-finished","success":true}'
"#,
        )?;
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755))?;
        std::fs::write(root.join("shell-working-directory"), "")?;
        let sh = Shell::new()?;
        sh.change_dir(&root);
        let _path = sh.push_env("PATH", &root);
        let _target = sh.push_env("CARGO_TARGET_DIR", "/work with spaces/test-target");
        let _profile = sh.push_env("CARGO_PROFILE_TEST_OPT_LEVEL", "1");
        let args = [
            "test",
            "--workspace",
            "--locked",
            "--profile",
            "test",
            "--features",
            "feature-a",
        ];
        emit_inventory(&sh, &args)?;
        std::fs::write(root.join("reject"), "")?;
        assert!(
            emit_inventory(&sh, &args).is_err(),
            "a failed inventory build must not pass"
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn cargo_inventory_selects_only_test_executables_and_requires_build_success() -> Result<()> {
        let messages = r#"{"reason":"compiler-artifact","profile":{"test":true},"executable":"/work/target/debug/deps/unit-a"}
{"reason":"compiler-artifact","profile":{"test":false},"executable":"/work/target/debug/app"}
{"reason":"compiler-artifact","profile":{"test":true},"executable":null}
{"reason":"build-finished","success":true}"#;
        assert_eq!(from_cargo(messages)?, BTreeSet::from(["unit-a".to_owned()]));
        assert!(from_cargo(&messages.replace("\"success\":true", "\"success\":false")).is_err());
        assert!(from_cargo("").is_err());
        Ok(())
    }

    #[test]
    fn missing_duplicate_unknown_and_unassigned_results_fail() -> Result<()> {
        let complete = "==> expected test executable: unit-a\n==> test executable inventory complete: 1\n     Running unittests src/lib.rs (/work with spaces/target/debug/deps/unit-a)\nrunning 1 test\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n   Doc-tests example\nrunning 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;";
        let mut coverage = Coverage::default();
        for line in complete.lines() {
            coverage.observe(line)?;
        }
        coverage.validate()?;
        for log in [
            complete.replace(
                "Running unittests src/lib.rs (/work with spaces/target/debug/deps/unit-a)\n",
                "",
            ),
            complete.replace(
                "==> test executable inventory complete: 1",
                "==> test executable inventory complete: 2",
            ),
            complete.replace("/deps/unit-a)", "/deps/unknown-b)"),
            format!(
                "{complete}\nRunning unittests src/lib.rs (/work/target/debug/deps/unit-a)\nrunning 1 test\ntest result: ok. 1 passed;"
            ),
        ] {
            let mut coverage = Coverage::default();
            let parsed = log.lines().try_for_each(|line| coverage.observe(line));
            assert!(parsed.is_err() || coverage.validate().is_err(), "{log}");
        }
        Ok(())
    }

    fn stream_merges(stderr: &[&str], stdout: &[&str]) -> Vec<String> {
        if stderr.is_empty() {
            return vec![stdout.join("\n")];
        }
        if stdout.is_empty() {
            return vec![stderr.join("\n")];
        }
        let mut merged = Vec::new();
        for tail in stream_merges(&stderr[1..], stdout) {
            merged.push(format!("{}\n{tail}", stderr[0]));
        }
        for tail in stream_merges(stderr, &stdout[1..]) {
            merged.push(format!("{}\n{tail}", stdout[0]));
        }
        merged
    }

    #[test]
    fn every_order_preserving_stream_merge_has_the_same_coverage() -> Result<()> {
        let stderr = [
            "Running unittests src/lib.rs (/work/unit-a)",
            "Running tests/b.rs (/work/unit-b)",
            "Doc-tests example",
        ];
        let stdout = [
            "==> expected test executable: unit-a",
            "==> expected test executable: unit-b",
            "==> test executable inventory complete: 2",
            "running 1 test",
            "test result: ok. 1 passed;",
            "running 0 tests",
            "test result: ok. 0 passed;",
            "running 0 tests",
            "test result: ok. 0 passed;",
        ];
        let merges = stream_merges(&stderr, &stdout);
        assert_eq!(merges.len(), 220);
        for log in merges {
            let mut coverage = Coverage::default();
            for line in log.lines() {
                coverage.observe(line)?;
            }
            coverage
                .validate()
                .with_context(|| format!("stream merge failed:\n{log}"))?;
        }
        Ok(())
    }

    #[test]
    fn missing_or_duplicate_stdout_results_cannot_shift_pairing_silently() {
        let inventory = "==> expected test executable: unit-a\n==> expected test executable: unit-b\n==> test executable inventory complete: 2\nRunning unittests src/lib.rs (/work/unit-a)\nRunning tests/b.rs (/work/unit-b)\n";
        for stdout in [
            "running 1 test\nrunning 1 test\ntest result: ok. 1 passed;",
            "running 1 test\ntest result: ok. 1 passed;\ntest result: ok. 1 passed;",
            "test result: ok. 1 passed;",
            "running 1 test\nrunning 1 test\ntest result: ok. 1 passed;\ntest result: ok. 1 passed;",
        ] {
            let mut coverage = Coverage::default();
            let log = format!("{inventory}{stdout}");
            assert!(
                log.lines()
                    .try_for_each(|line| coverage.observe(line))
                    .and_then(|()| coverage.validate())
                    .is_err(),
                "{stdout}"
            );
        }
    }

    #[test]
    fn a_duplicate_result_cannot_cover_another_executables_missing_row() -> Result<()> {
        let log = "==> expected test executable: unit-a\n==> expected test executable: unit-b\n==> test executable inventory complete: 2\nRunning unittests src/lib.rs (/work/unit-a)\nRunning tests/b.rs (/work/unit-b)\nrunning 1 test\nrunning 1 test\ntest result: ok. 1 passed;\ntest result: ok. 1 passed;";
        let mut coverage = Coverage::default();
        for line in log.lines() {
            coverage.observe(line)?;
        }
        let error = coverage
            .validate()
            .expect_err("unit-a did not produce a result row");
        assert!(
            error.to_string().contains("missing results: unit-a"),
            "{error:#}"
        );
        Ok(())
    }
}
