//! 로컬과 CI가 같은 진입점으로 쓰는 검사. 외부 서비스가 필요한 통합 테스트는 `test`에서 돈다.

use std::path::Path;

use anyhow::{Result, bail};
use xshell::{Shell, cmd};

type Step<'a> = Box<dyn Fn(&Shell) -> Result<()> + 'a>;

/// 검사 단계. 실패해도 나머지를 계속 돌리고 마지막에 모아서 실패시킨다.
pub fn run(root: &Path) -> Result<()> {
    let sh = Shell::new()?;
    sh.change_dir(root);
    let mut failed: Vec<&str> = Vec::new();

    let steps: Vec<(&str, Step<'_>)> = vec![
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
        (
            "cargo deny",
            Box::new(|sh| Ok(cmd!(sh, "cargo deny --locked check").run()?)),
        ),
        (
            "openapi up to date",
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
            "pnpm test",
            Box::new(|sh| Ok(cmd!(sh, "pnpm run test").run()?)),
        ),
        (
            "pnpm licenses",
            Box::new(|_| crate::licenses::check_pnpm(root)),
        ),
    ];

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
