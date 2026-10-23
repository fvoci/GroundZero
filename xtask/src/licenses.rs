//! 의존성 라이선스 검사. Cargo는 cargo-deny, pnpm은 `pnpm licenses list --json`을 허용 목록과 비교한다.
//! 허용 목록은 docs/decisions.md#d-60. 목록에 없는 라이선스는 실패시키고 보고한다. 조용히 예외 처리하지 않는다.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use xshell::{Shell, cmd};

/// D-60 기본 허용 후보와 사용자가 승인한 것(D-68: Zlib). `deny.toml`의 allow와 같아야 한다.
const ALLOWED: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "PostgreSQL",
    // 사용자 승인 2026-10-17(D-68). Zlib만 더했다.
    "Zlib",
];

/// 허용 목록 밖이지만 사용자가 패키지·판·사용 범위를 정해 승인한 npm 패키지(D-72). (이름, 판, 라이선스 식).
/// 판이나 식이 바뀌면 맞지 않아 다시 실패하고, 그때 다시 보고해 승인받는다. 쓰이지 않는 항목은 경고한다.
const NPM_EXCEPTIONS: &[(&str, &str, &str)] = &[
    // 빌드·테스트 도구(운영 번들에 들지 않음, docs/decisions.md#d-72)
    ("tslib", "2.8.1", "0BSD"),
    ("lru-cache", "11.5.3", "BlueOak-1.0.0"),
    ("minimatch", "10.2.6", "BlueOak-1.0.0"),
    ("mdn-data", "2.27.1", "CC0-1.0"),
    ("@csstools/color-helpers", "6.1.2", "MIT-0"),
    (
        "@csstools/css-syntax-patches-for-csstree",
        "1.1.15",
        "MIT-0",
    ),
    ("lightningcss", "1.32.0", "MPL-2.0"),
    ("lightningcss", "1.33.0", "MPL-2.0"),
    ("lightningcss-darwin-arm64", "1.32.0", "MPL-2.0"),
    ("lightningcss-darwin-arm64", "1.33.0", "MPL-2.0"),
    ("lightningcss-linux-x64-gnu", "1.32.0", "MPL-2.0"),
    ("lightningcss-linux-x64-gnu", "1.33.0", "MPL-2.0"),
    ("lightningcss-linux-arm64-gnu", "1.32.0", "MPL-2.0"),
    ("lightningcss-linux-arm64-gnu", "1.33.0", "MPL-2.0"),
    ("argparse", "2.0.1", "Python-2.0"),
    // PPTX 미리보기(D-74): @aiden0z/pptx-renderer 1.3.0이 끌어오는 것. tslib 2.3.0은 번들에 들어간다. mtx-decompressor
    // 1.8.0은 설치만 되고 번들에는 렌더러 안에 묶인 1.4.2가 들어간다(고지는 렌더러의 THIRD_PARTY_NOTICES.md).
    ("tslib", "2.3.0", "0BSD"),
    ("mtx-decompressor", "1.8.0", "MPL-2.0"),
    // 화면 문구 번역(D-81, Paraglide JS 2): 메시지를 빌드 때 컴파일하는 inlang SDK가 끌어오는 것. 빌드 때만 쓰고
    // 브라우저 번들·제품 이미지에는 들지 않는다(사용자 승인, 이 판만). 플랫폼별 jco-node-fs는 같은 판의 개발·CI·이미지용.
    (
        "@bytecodealliance/jco-node-fs",
        "0.3.2",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/jco-node-fs-darwin-arm64",
        "0.3.2",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/jco-node-fs-linux-x64-gnu",
        "0.3.2",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/jco-node-fs-linux-arm64-gnu",
        "0.3.2",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/jco-transpile",
        "0.13.0",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/preview2-shim",
        "0.24.1",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/preview2-shim",
        "0.25.0",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
    (
        "@bytecodealliance/preview3-shim",
        "0.6.1",
        "(Apache-2.0 WITH LLVM-exception)",
    ),
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
    let mut used = vec![false; NPM_EXCEPTIONS.len()];
    for (license, packages) in &parsed {
        if is_allowed(license) {
            continue;
        }
        for pkg in packages {
            let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let versions: Vec<&str> = pkg
                .get("versions")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let outside: Vec<&str> = versions
                .iter()
                .copied()
                .filter(|version| match exception(name, version, license) {
                    Some(i) => {
                        used[i] = true;
                        false
                    }
                    None => true,
                })
                .collect();
            if !outside.is_empty() || versions.is_empty() {
                violations.push(format!("{name}@{}: {license}", outside.join(",")));
            }
        }
    }
    for (i, (name, version, license)) in NPM_EXCEPTIONS.iter().enumerate() {
        if !used[i] {
            println!("  note: exception not encountered: {name}@{version} ({license})");
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

/// 승인된 예외의 위치. 이름·판·라이선스 식이 모두 같아야 한다.
fn exception(name: &str, version: &str, license: &str) -> Option<usize> {
    NPM_EXCEPTIONS
        .iter()
        .position(|&(n, v, l)| n == name && v == version && l == license)
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
    use super::{exception, is_allowed};

    /// 승인은 패키지·판·라이선스 식 단위다(D-72). 다른 판이나 다른 식은 통과하지 않는다.
    #[test]
    fn npm_exceptions_match_name_version_and_license() {
        assert!(exception("tslib", "2.8.1", "0BSD").is_some());
        assert!(exception("tslib", "2.8.2", "0BSD").is_none());
        assert!(exception("tslib", "2.8.1", "MIT").is_none());
        assert!(exception("other", "2.8.1", "0BSD").is_none());
        assert!(!is_allowed("0BSD"));
        assert!(!is_allowed("MPL-2.0"));
    }

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
        // (MIT OR GPL) AND MPL-2.0: MPL-2.0(미승인)이 반드시 붙으므로 허용 목록 밖이다.
        assert!(!is_allowed("(MIT OR GPL-3.0-or-later) AND MPL-2.0"));
        assert!(!is_allowed("(MIT AND Unicode-3.0)"));
        assert!(!is_allowed("MIT AND (MPL-2.0 OR GPL-2.0-only)"));
        // AND가 OR보다 먼저 묶인다: MIT OR (Apache-2.0 AND MPL-2.0) → MIT를 고를 수 있다.
        assert!(is_allowed("MIT OR Apache-2.0 AND MPL-2.0"));
        assert!(!is_allowed("(MIT OR Apache-2.0) AND MPL-2.0"));
        // D-68로 Zlib은 허용. 다른 미승인 라이선스와 AND로 묶이면 여전히 밖이다.
        assert!(is_allowed("Zlib"));
        assert!(is_allowed("(MIT AND Zlib)"));
        assert!(!is_allowed("Zlib AND Unicode-3.0"));
        assert!(is_allowed("(MIT OR GPL-3.0-or-later)"));
        assert!(is_allowed("((MIT))"));
        assert!(is_allowed("(Apache-2.0 OR MIT) AND BSD-3-Clause"));
        // npm에 흔한 소문자 연산자.
        assert!(is_allowed("MIT or Apache-2.0"));
        // 예외 조항·이후 판·읽을 수 없는 식은 보고 대상.
        assert!(!is_allowed("(Apache-2.0 WITH LLVM-exception)"));
        assert!(!is_allowed("SEE LICENSE IN LICENSE.md"));
        assert!(!is_allowed("(MIT OR Apache-2.0"));
        assert!(!is_allowed(""));
    }
}
