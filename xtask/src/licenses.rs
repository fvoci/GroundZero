//! 의존성 라이선스 검사. Cargo는 cargo-deny, pnpm은 `pnpm licenses list --json`을 허용 목록과 비교한다.
//! 허용 목록은 docs/decisions.md D-60. 목록에 없는 라이선스는 실패시키고 보고한다. 조용히 예외 처리하지 않는다.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use xshell::{Shell, cmd};

/// D-60 기본 허용 후보. `deny.toml`의 allow와 같아야 한다.
const ALLOWED: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "PostgreSQL",
];

pub fn run(root: &Path) -> Result<()> {
    let sh = Shell::new()?;
    sh.change_dir(root);
    println!("==> cargo deny check licenses");
    let cargo = cmd!(sh, "cargo deny --locked check licenses").run();
    println!("==> pnpm licenses");
    let pnpm = check_pnpm(root);
    match (cargo, pnpm) {
        (Ok(()), Ok(())) => Ok(()),
        (c, p) => {
            if let Err(e) = c {
                eprintln!("cargo deny: {e:#}");
            }
            if let Err(e) = p {
                eprintln!("pnpm licenses: {e:#}");
            }
            bail!("license check failed")
        }
    }
}

/// pnpm 워크스페이스 전체(의존·개발 의존)의 라이선스를 허용 목록과 비교한다.
pub fn check_pnpm(root: &Path) -> Result<()> {
    let sh = Shell::new()?;
    sh.change_dir(root);
    let out = cmd!(sh, "pnpm licenses list --json --recursive")
        .read()
        .context("pnpm licenses list failed (run `pnpm install` first)")?;
    // 형식: { "<license>": [ { "name": ..., "versions": [...], ... }, ... ] }
    let parsed: BTreeMap<String, Vec<serde_json::Value>> =
        serde_json::from_str(&out).context("unexpected `pnpm licenses list --json` output")?;

    let mut violations: Vec<String> = Vec::new();
    for (license, packages) in &parsed {
        if is_allowed(license) {
            continue;
        }
        for pkg in packages {
            let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let versions = pkg
                .get("versions")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            violations.push(format!("{name}@{versions}: {license}"));
        }
    }

    if violations.is_empty() {
        println!("pnpm licenses: all within allow list");
        Ok(())
    } else {
        for v in &violations {
            eprintln!("  not allowed: {v}");
        }
        bail!(
            "{} package(s) outside the allow list; report per D-60, do not add exceptions silently",
            violations.len()
        )
    }
}

/// SPDX 식 중 OR 결합은 한 쪽이 허용되면 통과, AND 결합은 모두 허용되어야 통과.
fn is_allowed(expr: &str) -> bool {
    let expr = expr.trim().trim_start_matches('(').trim_end_matches(')');
    if expr.contains(" OR ") {
        return expr.split(" OR ").any(is_allowed);
    }
    if expr.contains(" AND ") {
        return expr.split(" AND ").all(is_allowed);
    }
    ALLOWED.contains(&expr.trim())
}

#[cfg(test)]
mod tests {
    use super::is_allowed;

    #[test]
    fn spdx_or_and() {
        assert!(is_allowed("MIT"));
        assert!(is_allowed("(MIT OR Apache-2.0)"));
        assert!(is_allowed("MIT AND ISC"));
        assert!(!is_allowed("MIT AND GPL-3.0"));
        assert!(!is_allowed("Python-2.0"));
        assert!(!is_allowed("Unknown"));
    }
}
