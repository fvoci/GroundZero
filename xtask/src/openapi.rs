//! `contracts/openapi.json` 생성과 최신 여부 확인.

use std::path::Path;

use anyhow::{Result, bail};
use xshell::{Shell, cmd};

pub fn run(root: &Path, check: bool) -> Result<()> {
    let sh = Shell::new()?;
    sh.change_dir(root);
    let generated = cmd!(
        sh,
        "cargo run --quiet --package fvoci-server --locked -- --print-openapi"
    )
    .read()?;
    let generated = format!("{}\n", generated.trim_end());
    let target = root.join("contracts/openapi.json");

    if check {
        let current = std::fs::read_to_string(&target).unwrap_or_default();
        if current != generated {
            bail!("contracts/openapi.json is stale; run `cargo xtask openapi`");
        }
        println!("contracts/openapi.json is up to date");
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, generated)?;
    println!("wrote {}", target.display());
    println!("regenerate the TS client with `pnpm run generate:api`");
    Ok(())
}
