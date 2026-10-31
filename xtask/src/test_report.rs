//! 기본 Compose 시험 로그의 수와 D-10 검증 결과를 CI·로컬에서 같은 규칙으로 집계한다.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use clap::ValueEnum;
use xshell::{Shell, cmd};

use crate::test_executables::Coverage;
use crate::test_verify::{OWNERSHIP_PASSED, RESOURCES_PASSED};

const STORAGE_TESTS: [&str; 3] = [
    "s3_files_round_trip_and_purge_with_the_document",
    "files::storage::s3_storage_round_trip",
    "files::storage::s3_rejects_a_wrong_secret",
];
const SEED_PASSED: &str = "==> dev seed check passed (second apply changed nothing)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Outcome {
    Success,
    Failure,
    Cancelled,
    Skipped,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    passed: u64,
    failed: u64,
    ignored: u64,
    filtered: u64,
}

impl Counts {
    fn parse(row: &str) -> Result<Self> {
        let words: Vec<&str> = row.split_whitespace().collect();
        let mut counters = [None; 4];
        for pair in words.windows(2) {
            if let Some(index) = ["passed;", "failed;", "ignored;", "filtered"]
                .iter()
                .position(|label| *label == pair[1])
            {
                ensure!(counters[index].is_none(), "duplicate test counter: {row}");
                counters[index] = Some(pair[0].parse::<u64>().context("invalid test count")?);
            }
        }
        let [passed, failed, ignored, filtered] = counters;
        Ok(Self {
            passed: passed.context("missing passed test count")?,
            failed: failed.context("missing failed test count")?,
            ignored: ignored.context("missing ignored test count")?,
            filtered: filtered.context("missing filtered test count")?,
        })
    }

    fn add(&mut self, row: Self) -> Result<()> {
        self.passed = self
            .passed
            .checked_add(row.passed)
            .context("passed count overflow")?;
        self.failed = self
            .failed
            .checked_add(row.failed)
            .context("failed count overflow")?;
        self.ignored = self
            .ignored
            .checked_add(row.ignored)
            .context("ignored count overflow")?;
        self.filtered = self
            .filtered
            .checked_add(row.filtered)
            .context("filtered count overflow")?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct Report {
    coverage: Coverage,
    counts: Counts,
    rows: Vec<String>,
    storage: BTreeMap<String, String>,
    seed: bool,
    ownership: bool,
    resources: bool,
}

impl Report {
    fn parse(log: &str) -> Result<Self> {
        let mut report = Self::default();
        for line in log.lines() {
            report.coverage.observe(line)?;
            if line.starts_with("test result:") {
                report.counts.add(Counts::parse(line)?)?;
                report.rows.push(line.to_owned());
            }
            if let Some((name, status)) = line
                .strip_prefix("test ")
                .and_then(|s| s.split_once(" ... "))
                && STORAGE_TESTS.contains(&name)
            {
                ensure!(
                    report
                        .storage
                        .insert(name.to_owned(), status.to_owned())
                        .is_none(),
                    "duplicate storage result: {name}"
                );
            }
            report.seed |= line == SEED_PASSED;
            report.ownership |= line.starts_with(OWNERSHIP_PASSED);
            report.resources |= line.starts_with(RESOURCES_PASSED);
        }
        Ok(report)
    }

    fn validate(&self, outcome: Outcome) -> Result<()> {
        ensure!(
            outcome == Outcome::Success,
            "default Compose test outcome: {outcome:?}; counts may be partial"
        );
        ensure!(
            !self.rows.is_empty() && self.counts.passed > 0,
            "no completed Rust test results"
        );
        ensure!(self.counts.failed == 0, "Rust tests failed");
        self.coverage.validate()?;
        for name in STORAGE_TESTS {
            ensure!(
                self.storage.get(name).is_some_and(|status| status == "ok"),
                "required Compose storage test did not pass: {name}"
            );
        }
        ensure!(self.seed, "dev seed check did not pass");
        ensure!(self.ownership, "host ownership check did not pass");
        ensure!(self.resources, "Compose resource check did not pass");
        Ok(())
    }

    fn markdown(&self, sha: &str, outcome: Outcome) -> Result<String> {
        let mut text = format!("Commit: {sha}\n\nDefault Compose test outcome: {outcome:?}\n\n");
        writeln!(text, "{}\n", self.coverage.summary())?;
        if self.rows.is_empty() {
            text.push_str("Rust PASS/FAIL/NOT RUN counts: unknown (no completed result rows). This is not a passing suite.\n\n");
        } else {
            let not_run = self
                .counts
                .ignored
                .checked_add(self.counts.filtered)
                .context("NOT RUN count overflow")?;
            writeln!(
                text,
                "Rust rows: {}; PASS: {}; FAIL: {}; NOT RUN: {} (ignored: {}, filtered: {}). Counts are partial unless the command succeeded.\n",
                self.rows.len(),
                self.counts.passed,
                self.counts.failed,
                not_run,
                self.counts.ignored,
                self.counts.filtered
            )?;
        }
        text.push_str(
            "| Additional default Compose check | Result | ci/integration |\n| --- | --- | --- |\n",
        );
        for name in STORAGE_TESTS {
            let status = self.storage.get(name).map_or("NOT RUN", String::as_str);
            writeln!(text, "| {name} | {status} | NOT RUN |")?;
        }
        for (name, passed, integration) in [
            ("dev seed (1 check)", self.seed, "NOT RUN"),
            (
                "host ownership",
                self.ownership,
                "separate infrastructure check",
            ),
            (
                "Compose resources absent",
                self.resources,
                "separate infrastructure check",
            ),
        ] {
            writeln!(
                text,
                "| {name} | {} | {integration} |",
                if passed { "PASS" } else { "NOT RUN or failed" }
            )?;
        }
        text.push_str("\nCompare the Rust result rows below with the same-commit ci/integration log: default Compose adds the three storage tests above (PASS +3, filtered NOT RUN -3 when all pass). Dev seed is one additional check outside the Rust totals. OIDC (oidc_*), Supabase (supabase_*), and ignored tests are NOT RUN in both paths. No expected count is reduced.\n\n~~~text\n");
        for row in &self.rows {
            writeln!(text, "{row}")?;
        }
        text.push_str("~~~\n");
        Ok(text)
    }
}

/// CI는 GITHUB_STEP_SUMMARY에 추가하며 로컬 호출은 같은 요약과 수락 판정을 표준 출력으로 받는다.
pub fn run(root: &Path, log: &Path, outcome: Outcome) -> Result<()> {
    let log = match std::fs::read_to_string(log) {
        Ok(log) => log,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && outcome != Outcome::Success =>
        {
            String::new()
        }
        Err(error) => return Err(error).context("read default Compose test log"),
    };
    let report = Report::parse(&log)?;
    let sh = Shell::new()?;
    sh.change_dir(root);
    let sha = cmd!(sh, "git rev-parse HEAD").quiet().read()?;
    let markdown = report.markdown(&sha, outcome)?;
    print!("{markdown}");
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .context("open GitHub test summary")?;
        file.write_all(markdown.as_bytes())
            .context("write GitHub test summary")?;
    }
    report.validate(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete_log() -> String {
        let mut log = "==> expected test executable: unit-a\n==> expected test executable: storage-b\n==> test executable inventory complete: 2\n     Running unittests src/lib.rs (/work/target/debug/deps/unit-a)\nrunning 14 tests\ntest result: ok. 12 passed; 0 failed; 2 ignored; 0 measured; 5 filtered out; finished in 0.01s\n     Running tests/files.rs (/work/target/debug/deps/storage-b)\nrunning 3 tests\ntest result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n".to_owned();
        for name in STORAGE_TESTS {
            writeln!(log, "test {name} ... ok").expect("write test fixture");
        }
        writeln!(
            log,
            "{SEED_PASSED}\n{OWNERSHIP_PASSED} (UID 1000, 0.1s)\n{RESOURCES_PASSED} (project)"
        )
        .expect("write test fixture");
        log
    }

    #[test]
    fn review_regression_missing_executable_is_rejected() -> Result<()> {
        let log = format!(
            "==> expected test executable: missing-suite-c\n{}",
            complete_log()
        )
        .replace("inventory complete: 2", "inventory complete: 3");
        let report = Report::parse(&log)?;
        let error = report
            .validate(Outcome::Success)
            .expect_err("a whole executable missing from the log must fail");
        assert!(format!("{error:#}").contains("missing-suite-c"));
        Ok(())
    }

    #[test]
    fn complete_results_count_ignored_filtered_and_extra_checks_separately() -> Result<()> {
        let report = Report::parse(&complete_log())?;
        report.validate(Outcome::Success)?;
        let markdown = report.markdown("target-sha", Outcome::Success)?;
        assert!(markdown.contains("Commit: target-sha"));
        assert!(markdown.contains("PASS: 15; FAIL: 0; NOT RUN: 7"));
        assert!(markdown.contains("dev seed (1 check) | PASS"));
        Ok(())
    }

    #[test]
    fn missing_checks_failed_results_or_an_interrupted_command_cannot_pass() -> Result<()> {
        let log = complete_log();
        for marker in [
            SEED_PASSED,
            OWNERSHIP_PASSED,
            RESOURCES_PASSED,
            STORAGE_TESTS[0],
        ] {
            let incomplete = log
                .lines()
                .filter(|line| !line.contains(marker))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                Report::parse(&incomplete)?
                    .validate(Outcome::Success)
                    .is_err(),
                "{marker}"
            );
        }
        let failed = log.replace("0 failed;", "1 failed;");
        assert!(Report::parse(&failed)?.validate(Outcome::Success).is_err());
        for outcome in [Outcome::Failure, Outcome::Cancelled, Outcome::Skipped] {
            assert!(
                Report::parse(&log)?.validate(outcome).is_err(),
                "{outcome:?}"
            );
        }
        let empty = Report::parse("Error: setup failed")?;
        assert!(empty.validate(Outcome::Success).is_err());
        assert!(
            empty
                .markdown("sha", Outcome::Failure)?
                .contains("counts: unknown")
        );
        Ok(())
    }

    #[test]
    fn malformed_duplicate_and_overflowing_counts_are_rejected() {
        for log in [
            "test result: ok. 2 passed;",
            "test result: ok. wrong passed; 0 failed; 0 ignored; 0 filtered out;",
            "test result: ok. 1 passed; 1 passed; 0 failed; 0 ignored; 0 filtered out;",
            "test result: ok. 18446744073709551615 passed; 0 failed; 0 ignored; 0 filtered out;\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 filtered out;",
        ] {
            let framed = log.replace("test result:", "running 1 test\ntest result:");
            assert!(Report::parse(&framed).is_err(), "{log}");
        }
    }
}
