//! 로컬과 CI가 같은 진입점으로 쓰는 검사. 외부 서비스가 필요한 통합 테스트는 `test`에서 돈다.

use std::path::Path;

use anyhow::{Result, bail};
use xshell::{Shell, cmd};

type Step<'a> = Box<dyn Fn(&Shell) -> Result<()> + 'a>;

/// 검사 단계. 실패해도 나머지를 계속 돌리고 마지막에 모아서 실패시킨다.
/// `skip_licenses`면 라이선스 단계를 뺀다(CI는 `cargo xtask licenses`를 따로 job으로 돌린다).
pub fn run(root: &Path, skip_licenses: bool) -> Result<()> {
    let sh = Shell::new()?;
    sh.change_dir(root);
    let mut failed: Vec<&str> = Vec::new();

    let mut steps: Vec<(&str, Step<'_>)> = vec![
        (
            "cargo fmt --check",
            Box::new(|sh| Ok(cmd!(sh, "cargo fmt --all --check").run()?)),
        ),
        (
            "cargo clippy",
            Box::new(|sh| {
                Ok(cmd!(
                    sh,
                    "cargo clippy --workspace --all-targets --locked -- -D warnings"
                )
                .run()?)
            }),
        ),
        (
            "cargo test (unit)",
            Box::new(|sh| Ok(cmd!(sh, "cargo test --workspace --lib --bins --locked").run()?)),
        ),
        // 라이선스는 아래 `licenses` 단계(cargo deny check licenses + pnpm)에서 본다.
        (
            "cargo deny (advisories, bans, sources)",
            Box::new(|sh| Ok(cmd!(sh, "cargo deny --locked check advisories bans sources").run()?)),
        ),
        (
            "openapi.json and api-client schema.ts up to date",
            Box::new(|_| crate::openapi::run(root, true)),
        ),
        (
            "pnpm format:check",
            Box::new(|sh| Ok(cmd!(sh, "pnpm run format:check").run()?)),
        ),
        (
            "pnpm lint",
            Box::new(|sh| Ok(cmd!(sh, "pnpm run lint").run()?)),
        ),
        (
            "pnpm check (svelte-check/tsc)",
            Box::new(|sh| Ok(cmd!(sh, "pnpm run check").run()?)),
        ),
        (
            "i18n messages (both languages, same keys, compiled)",
            Box::new(|_| crate::i18n::check(root)),
        ),
        (
            "rhwp WASM notices match deploy/rhwp inputs",
            Box::new(|_| crate::rhwp::check_notices(root)),
        ),
        (
            "third-party notices up to date",
            Box::new(|_| crate::notices::run(root, true)),
        ),
        (
            "pnpm test",
            Box::new(|sh| Ok(cmd!(sh, "pnpm run test").run()?)),
        ),
        // story가 실제 Vite 빌드로 묶이는지(설정·import 오류). 출력은 target/storybook-static.
        (
            "storybook build",
            Box::new(|sh| Ok(cmd!(sh, "pnpm --filter @fvoci/web run build-storybook").run()?)),
        ),
    ];
    if skip_licenses {
        println!("SKIP licenses (--skip-licenses; run `cargo xtask licenses` separately)");
    } else {
        steps.push((
            "licenses (cargo deny, pnpm)",
            Box::new(|_| crate::licenses::run(root)),
        ));
    }

    for (name, step) in &steps {
        println!("==> {name}");
        if let Err(err) = step(&sh) {
            eprintln!("FAIL {name}: {err:#}");
            failed.push(name);
        } else {
            println!("PASS {name}");
        }
    }

    if failed.is_empty() {
        println!("all checks passed");
        Ok(())
    } else {
        bail!("failed checks: {}", failed.join(", "))
    }
}
