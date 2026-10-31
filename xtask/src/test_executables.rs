//! 같은 Cargo 인자·환경의 빌드 산출물과 실행 파일별 결과 행을 대조한다.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, ensure};
use xshell::{Shell, cmd};

const EXPECTED: &str = "==> expected test executable: ";
const NAMES: &str = "==> expected test names: ";
const COMPLETE: &str = "==> test executable inventory complete: ";

/// Cargo test 인자를 실행과 공유하고 같은 Shell의 프로필·기능·target·환경을 그대로 사용한다.
pub fn emit_inventory(sh: &Shell, args: &[&str], test_args: &[&str]) -> Result<()> {
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
    for (name, executable) in &executables {
        // Cargo가 시험을 실행할 때처럼 패키지 디렉터리와 같은 Shell 환경을 쓴다.
        let _dir = sh.push_dir(&executable.package_dir);
        let path = &executable.path;
        let listing = cmd!(sh, "{path} --list --format terse {test_args...}")
            .quiet()
            .read()
            .with_context(|| format!("list tests in {name}"))?;
        let tests = listed_names(&listing)?;
        println!("{EXPECTED}{name}");
        println!(
            "{NAMES}{}",
            serde_json::json!({"executable": name, "tests": tests})
        );
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

#[derive(Debug)]
struct TestExecutable {
    path: PathBuf,
    package_dir: PathBuf,
}

fn listed_names(listing: &str) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for line in listing.lines().filter(|line| !line.is_empty()) {
        let name = line
            .strip_suffix(": test")
            .or_else(|| line.strip_suffix(": benchmark"))
            .with_context(|| format!("unrecognized libtest --list row: {line}"))?;
        ensure!(
            !name.is_empty() && names.insert(name.to_owned()),
            "invalid or duplicate listed test: {line}"
        );
    }
    Ok(names)
}

fn from_cargo(messages: &str) -> Result<BTreeMap<String, TestExecutable>> {
    let mut executables = BTreeMap::new();
    let mut finished = false;
    for line in messages.lines().filter(|line| line.starts_with('{')) {
        let value: serde_json::Value =
            serde_json::from_str(line).context("parse Cargo build artifact")?;
        if value["reason"] == "compiler-artifact"
            && value["profile"]["test"] == true
            && let Some(path) = value["executable"].as_str()
        {
            let manifest = value["manifest_path"]
                .as_str()
                .context("test artifact has no manifest_path")?;
            let executable = TestExecutable {
                path: path.into(),
                package_dir: Path::new(manifest)
                    .parent()
                    .context("manifest has no parent")?
                    .to_owned(),
            };
            ensure!(
                executables
                    .insert(executable_name(path)?, executable)
                    .is_none(),
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

#[derive(Debug)]
struct StdoutGroup {
    declared: usize,
    complete: bool,
    names: BTreeSet<String>,
}

#[derive(Debug, Default)]
pub struct Coverage {
    expected: BTreeSet<String>,
    expected_names: BTreeMap<String, BTreeSet<String>>,
    declared: Option<usize>,
    groups: Vec<TestGroup>,
    stdout_groups: Vec<StdoutGroup>,
    result_rows: usize,
    unassigned_rows: usize,
}

fn test_group_size(line: &str) -> Option<usize> {
    let mut words = line.split_whitespace();
    if words.next() != Some("running") {
        return None;
    }
    let count = words.next()?.parse().ok()?;
    (matches!(words.next(), Some("test" | "tests")) && words.next().is_none()).then_some(count)
}

impl Coverage {
    pub fn observe_stderr(&mut self, line: &str) -> Result<()> {
        if let Some(running) = line.trim_start().strip_prefix("Running ") {
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
        }
        Ok(())
    }

    pub fn observe_stdout(&mut self, line: &str) -> Result<()> {
        if let Some(name) = line.strip_prefix(EXPECTED) {
            ensure!(
                self.declared.is_none()
                    && !name.is_empty()
                    && self.expected.insert(name.to_owned()),
                "invalid or duplicate test executable inventory: {name}"
            );
        } else if let Some(json) = line.strip_prefix(NAMES) {
            let value: serde_json::Value =
                serde_json::from_str(json).context("parse test name inventory")?;
            let name = value["executable"]
                .as_str()
                .context("test name inventory has no executable")?;
            let tests = value["tests"]
                .as_array()
                .context("test name inventory has no tests")?;
            let names: BTreeSet<String> = tests
                .iter()
                .map(|test| {
                    test.as_str()
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned)
                        .context("invalid listed test name")
                })
                .collect::<Result<_>>()?;
            ensure!(
                self.declared.is_none()
                    && names.len() == tests.len()
                    && self.expected_names.insert(name.to_owned(), names).is_none(),
                "invalid or duplicate test name inventory: {name}"
            );
        } else if let Some(count) = line.strip_prefix(COMPLETE) {
            ensure!(
                self.declared.is_none(),
                "duplicate executable inventory completion"
            );
            self.declared = Some(count.parse().context("parse executable inventory count")?);
        } else if let Some(declared) = test_group_size(line) {
            self.stdout_groups.push(StdoutGroup {
                declared,
                complete: false,
                names: BTreeSet::new(),
            });
        } else if line.starts_with("test result:") {
            match self.stdout_groups.last_mut() {
                Some(group) if !group.complete => group.complete = true,
                _ => self.unassigned_rows += 1,
            }
            self.result_rows = self
                .result_rows
                .checked_add(1)
                .context("test group count overflow")?;
        } else if let Some((name, status)) = line
            .strip_prefix("test ")
            .and_then(|s| s.split_once(" ... "))
        {
            let name = name.strip_suffix(" - should panic").unwrap_or(name);
            ensure!(
                status == "ok"
                    || status == "FAILED"
                    || status == "ignored"
                    || status.starts_with("ignored, "),
                "unknown libtest status: {line}"
            );
            let group = self
                .stdout_groups
                .last_mut()
                .filter(|group| !group.complete)
                .context("test name outside stdout group")?;
            ensure!(
                group.names.insert(name.to_owned()),
                "duplicate test name in stdout group: {name}"
            );
        }
        Ok(())
    }

    // 줄 단위 합치기 회귀 fixture 전용. 실제 판정은 스트림을 합치거나 줄로 분리해서 추측하지 않는다.
    #[cfg(test)]
    pub fn observe(&mut self, line: &str) -> Result<()> {
        if line.trim_start().starts_with("Running ") || line.trim_start().starts_with("Doc-tests ")
        {
            self.observe_stderr(line)
        } else {
            self.observe_stdout(line)
        }
    }

    // Cargo는 순차 실행한다. stderr 헤더와 stdout 그룹의 각 스트림 내 순서는 유지되지만
    // 두 파일을 각각 끝까지 읽은 뒤 N번째끼리 대응하며 --list의 시험 이름도 대조한다.
    fn completed(&self) -> BTreeSet<&str> {
        self.groups
            .iter()
            .zip(&self.stdout_groups)
            .filter_map(|(group, result)| match group {
                TestGroup::Executable(name) if result.complete => Some(name.as_str()),
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
        ensure!(
            self.expected_names.keys().eq(self.expected.iter()),
            "missing or unexpected test name inventory"
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
            self.unassigned_rows == 0 && self.stdout_groups.iter().all(|group| group.complete),
            "stdout test groups must each have exactly one result row"
        );
        for (header, group) in self.groups.iter().zip(&self.stdout_groups) {
            ensure!(
                group.declared == group.names.len(),
                "stdout group declared {} tests but printed {} names",
                group.declared,
                group.names.len()
            );
            if let TestGroup::Executable(name) = header {
                let expected = &self.expected_names[name];
                let unexpected: Vec<_> = group.names.difference(expected).collect();
                let missing: Vec<_> = expected.difference(&group.names).collect();
                ensure!(
                    unexpected.is_empty() && missing.is_empty(),
                    "test names do not match executable {name}: unexpected {unexpected:?}; missing {missing:?}"
                );
            }
        }
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
    use std::fmt::Write as _;

    use super::*;

    fn with_names(log: &str, inventories: &[(&str, &[&str])]) -> String {
        let mut text = String::new();
        for (executable, tests) in inventories {
            writeln!(
                text,
                "{NAMES}{}",
                serde_json::json!({"executable": executable, "tests": tests})
            )
            .expect("write test name fixture");
        }
        text.push_str(log);
        text
    }

    #[test]
    fn ignored_should_panic_and_empty_filtered_groups_match_selected_names() -> Result<()> {
        let log = with_names(
            "==> expected test executable: unit-a\n==> expected test executable: unit-b\n==> test executable inventory complete: 2\nRunning unit tests (/work/unit-a)\nRunning filtered tests (/work/unit-b)\nrunning 2 tests\ntest panics - should panic ... ok\ntest manual ... ignored, needs a service\ntest result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 3 filtered out;\nrunning 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out;",
            &[("unit-a", &["panics", "manual"]), ("unit-b", &[])],
        );
        let mut coverage = Coverage::default();
        for line in log.lines() {
            coverage.observe(line)?;
        }
        coverage.validate()?;
        for incomplete in [
            log.replace("test manual ... ignored, needs a service\n", ""),
            log.replace("test manual", "test wrong_name"),
        ] {
            let mut coverage = Coverage::default();
            for line in incomplete.lines() {
                coverage.observe(line)?;
            }
            assert!(coverage.validate().is_err(), "{incomplete}");
        }
        assert_eq!(
            listed_names("manual: test\npanics: test\n")?,
            BTreeSet::from(["manual".to_owned(), "panics".to_owned()])
        );
        assert!(listed_names("")?.is_empty());
        assert!(listed_names("wrong format").is_err());
        assert!(listed_names("duplicate: test\nduplicate: test").is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn real_libtest_list_and_filtered_results_use_the_same_names() -> Result<()> {
        let sh = Shell::new()?;
        let executable = std::env::current_exe()?;
        let name = executable_name(executable.to_str().context("test executable UTF-8")?)?;
        let filters = [
            "--exact",
            "test_verify::tests::review_regression_setgid_inheritance_is_allowed_but_root_is_not",
            "compose::tests::hold_lock_until_killed",
        ];
        let listing = cmd!(sh, "{executable} --list --format terse {filters...}")
            .quiet()
            .read()?;
        let tests = listed_names(&listing)?;
        ensure!(
            tests.len() == 2,
            "selected fixture must list a normal and an ignored test"
        );
        let stdout = cmd!(sh, "{executable} --color never {filters...}")
            .quiet()
            .read()?;
        let log = format!(
            "{EXPECTED}{name}\n{NAMES}{}\n{COMPLETE}1\nRunning unit tests ({})\n{stdout}",
            serde_json::json!({"executable":name,"tests":tests}),
            executable.display()
        );
        let mut coverage = Coverage::default();
        for line in log.lines() {
            coverage.observe(line)?;
        }
        coverage.validate()?;
        Ok(())
    }

    #[test]
    fn review_regression_two_groups_cannot_cover_a_silent_executable() -> Result<()> {
        let log = "==> expected test executable: unit-a\n\
==> expected test executable: unit-b\n\
==> expected test names: {\"executable\":\"unit-a\",\"tests\":[\"a::one\"]}\n\
==> expected test names: {\"executable\":\"unit-b\",\"tests\":[\"b::one\"]}\n\
==> test executable inventory complete: 2\n\
Running unittests src/lib.rs (/work/unit-a)\n\
running 1 test\n\
test a::one ... ok\n\
test result: ok. 1 passed;\n\
running 1 test\n\
test a::one ... ok\n\
test result: ok. 1 passed;\n\
Running tests/b.rs (/work/unit-b)\n";
        let mut coverage = Coverage::default();
        for line in log.lines() {
            coverage.observe(line)?;
        }
        let error = coverage.validate().expect_err("unit-b produced no stdout");
        assert!(error.to_string().contains("unit-b"), "{error:#}");
        Ok(())
    }

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
test isolation_a ... ok\n\
test isolation_b ... ok\n\
test isolation_c ... ok\n\
test isolation_d ... ok\n\
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
     Running tests/workspace_delete.rs (target/test-container/build/debug/deps/workspace_delete-13e2e563cfc8d2f8)\n\
running 3 tests\n\
test purge_succeeds_with_a_trashed_module ... ok\n\
test only_the_owner_deletes_and_everyone_loses_access_until_restored ... ok\n\
     Running unittests src/lib.rs (target/test-container/build/debug/deps/fvoci_storage-3f640a1697b6fa11)\n\
test purge_removes_everything_but_personal_time_and_expiry_purges_in_the_worker ... ok\n\
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n\
running 4 tests\n\
test storage_a ... ok\n\
test storage_b ... ok\n\
test storage_c ... ok\n\
test storage_d ... ok\n\
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n";
        let log = with_names(
            log,
            &[
                (
                    "worker-d23f2f3e1008d35c",
                    &[
                        "indexing_rechecks_the_claim_and_generation_before_sending",
                        "storage_cleanup_deletes_every_object_queued_together",
                        "the_worker_purges_only_expired_trash_and_fills_repeating_tasks",
                    ],
                ),
                (
                    "worker_isolation-e03adb3e3835618a",
                    &["isolation_a", "isolation_b", "isolation_c", "isolation_d"],
                ),
                (
                    "workspace_delete-13e2e563cfc8d2f8",
                    &[
                        "purge_succeeds_with_a_trashed_module",
                        "only_the_owner_deletes_and_everyone_loses_access_until_restored",
                        "purge_removes_everything_but_personal_time_and_expiry_purges_in_the_worker",
                    ],
                ),
                (
                    "fvoci_storage-3f640a1697b6fa11",
                    &["storage_a", "storage_b", "storage_c", "storage_d"],
                ),
            ],
        );
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
printf '{"reason":"compiler-artifact","profile":{"test":true},"executable":"%s/list-tests","manifest_path":"%s/package/Cargo.toml"}\n' "$PWD" "$PWD"
printf '%s\n' '{"reason":"build-finished","success":true}'
"#,
        )?;
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755))?;
        let executable = root.join("list-tests");
        std::fs::write(
            &executable,
            r#"#!/bin/sh
test "$*" = '--list --format terse --skip excluded_' || exit 41
test "$CARGO_TARGET_DIR" = '/work with spaces/test-target' || exit 42
test "$CARGO_PROFILE_TEST_OPT_LEVEL" = '1' || exit 43
test -f package-working-directory || exit 44
printf '%s\n' 'selected: test' 'ignored_case: test'
"#,
        )?;
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))?;
        std::fs::write(root.join("shell-working-directory"), "")?;
        std::fs::create_dir(root.join("package"))?;
        std::fs::write(root.join("package/package-working-directory"), "")?;
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
        emit_inventory(&sh, &args, &["--skip", "excluded_"])?;
        std::fs::write(root.join("reject"), "")?;
        assert!(
            emit_inventory(&sh, &args, &["--skip", "excluded_"]).is_err(),
            "a failed inventory build must not pass"
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn cargo_inventory_selects_only_test_executables_and_requires_build_success() -> Result<()> {
        let messages = r#"{"reason":"compiler-artifact","profile":{"test":true},"executable":"/work/target/debug/deps/unit-a","manifest_path":"/work/Cargo.toml"}
{"reason":"compiler-artifact","profile":{"test":false},"executable":"/work/target/debug/app"}
{"reason":"compiler-artifact","profile":{"test":true},"executable":null}
{"reason":"build-finished","success":true}"#;
        let inventory = from_cargo(messages)?;
        assert_eq!(
            inventory.keys().map(String::as_str).collect::<Vec<_>>(),
            ["unit-a"]
        );
        assert_eq!(
            inventory["unit-a"].path,
            Path::new("/work/target/debug/deps/unit-a")
        );
        assert_eq!(inventory["unit-a"].package_dir, Path::new("/work"));
        assert!(from_cargo(&messages.replace("\"success\":true", "\"success\":false")).is_err());
        assert!(from_cargo("").is_err());
        Ok(())
    }

    #[test]
    fn missing_duplicate_unknown_and_unassigned_results_fail() -> Result<()> {
        let complete = with_names(
            "==> expected test executable: unit-a\n==> test executable inventory complete: 1\n     Running unittests src/lib.rs (/work with spaces/target/debug/deps/unit-a)\nrunning 1 test\ntest one ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n   Doc-tests example\nrunning 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;",
            &[("unit-a", &["one"])],
        );
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
            "test unit_case ... ok",
            "test result: ok. 1 passed;",
            "running 0 tests",
            "test result: ok. 0 passed;",
            "running 0 tests",
            "test result: ok. 0 passed;",
        ];
        let merges = stream_merges(&stderr, &stdout);
        assert_eq!(merges.len(), 286);
        for log in merges {
            let log = with_names(&log, &[("unit-a", &["unit_case"]), ("unit-b", &[])]);
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
