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
fn check_pnpm(root: &Path) -> Result<()> {
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

/// SPDX 식을 `spdx` 크레이트(cargo-deny와 같은 해석기)로 읽어 판정한다. 괄호·우선순위(AND가 OR보다 먼저)를
/// 지키고, 한 쪽을 고를 수 있는 OR는 허용된 쪽이 있으면 통과, AND는 모두 허용되어야 통과한다. 예외 조항(WITH)이
/// 붙은 항목, `+`(이후 판), 읽을 수 없는 식(예: `SEE LICENSE IN ...`, `Unknown`)은 허용하지 않고 보고한다.
fn is_allowed(expr: &str) -> bool {
    let Ok(parsed) = spdx::Expression::parse_mode(expr.trim(), spdx::ParseMode::LAX) else {
        return false;
    };
    parsed.evaluate(|req| {
        req.addition.is_none()
            && matches!(&req.license, spdx::LicenseItem::Spdx { id, or_later: false } if ALLOWED.contains(&id.name))
    })
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

    /// 예전 판정은 바깥 괄호 한 쌍만 벗기고 " OR "로 먼저 나눠, 괄호 묶음 뒤의 AND 조건을 놓쳤다.
    #[test]
    fn compound_expressions_keep_grouping_and_precedence() {
        // (MIT OR GPL) AND Zlib: Zlib이 반드시 붙으므로 허용 목록 밖이다.
        assert!(!is_allowed("(MIT OR GPL-3.0-or-later) AND Zlib"));
        assert!(!is_allowed("(MIT AND Zlib)"));
        assert!(!is_allowed("MIT AND (Zlib OR GPL-2.0-only)"));
        // AND가 OR보다 먼저 묶인다: MIT OR (Apache-2.0 AND Zlib) → MIT를 고를 수 있다.
        assert!(is_allowed("MIT OR Apache-2.0 AND Zlib"));
        assert!(!is_allowed("(MIT OR Apache-2.0) AND Zlib"));
        assert!(is_allowed("(MIT OR GPL-3.0-or-later)"));
        assert!(is_allowed("((MIT))"));
        assert!(is_allowed("(Apache-2.0 OR MIT) AND BSD-3-Clause"));
        // npm에 흔한 소문자 연산자.
        assert!(is_allowed("MIT or Apache-2.0"));
        // 예외 조항·이후 판·읽을 수 없는 식은 보고 대상.
        assert!(!is_allowed("Apache-2.0 WITH LLVM-exception"));
        assert!(!is_allowed("SEE LICENSE IN LICENSE.md"));
        assert!(!is_allowed("(MIT OR Apache-2.0"));
        assert!(!is_allowed(""));
    }
}
