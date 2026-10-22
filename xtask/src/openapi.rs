//! `contracts/openapi.json` 생성과 최신 여부 확인. 확인할 때는 그 계약에서 만든 TS client(`schema.ts`)도 같은지 본다.

use std::path::Path;

use anyhow::{Context, Result, bail};
use xshell::{Shell, cmd};

/// 계약에서 생성해 커밋하는 TS client.
const CLIENT_SCHEMA: &str = "packages/api-client/src/generated/schema.ts";
/// `packages/api-client/package.json`의 `generate` 스크립트(`pnpm run generate:api`가 부름). 아래 비교는 같은 생성기·
/// 같은 입력으로 출력 위치만 바꿔 돌리므로, 스크립트가 바뀌면 이 값과 비교 명령을 함께 고친다.
const GENERATE_SCRIPT: &str =
    "openapi-typescript ../../contracts/openapi.json --output ./src/generated/schema.ts";

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
        return check_client(root);
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&target, generated)?;
    println!("wrote {}", target.display());
    println!("regenerate the TS client with `pnpm run generate:api`");
    Ok(())
}

/// 커밋된 TS client가 지금 `contracts/openapi.json`에서 만든 결과와 바이트 단위로 같은지 본다. 패키지에 고정된
/// openapi-typescript를 `generate` 스크립트와 같은 인자로 돌리되 결과는 `target/` 아래에 써서 커밋된 파일을 건드리지 않는다.
fn check_client(root: &Path) -> Result<()> {
    let package = root.join("packages/api-client");
    let manifest = package.join("package.json");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest)
            .with_context(|| format!("read {}", manifest.display()))?,
    )?;
    if manifest["scripts"]["generate"].as_str() != Some(GENERATE_SCRIPT) {
        bail!(
            "packages/api-client/package.json `generate` script changed; update GENERATE_SCRIPT and the comparison in xtask/src/openapi.rs"
        );
    }
    let out = root.join("target/api-client-check/schema.ts");
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let sh = Shell::new()?;
    sh.change_dir(&package);
    let out_arg = out.to_string_lossy().into_owned();
    cmd!(
        sh,
        "pnpm exec openapi-typescript ../../contracts/openapi.json --output {out_arg}"
    )
    .quiet()
    .run()?;
    let fresh = std::fs::read(&out).with_context(|| format!("read {}", out.display()))?;
    let committed = std::fs::read(root.join(CLIENT_SCHEMA)).unwrap_or_default();
    if committed != fresh {
        bail!("{CLIENT_SCHEMA} does not match contracts/openapi.json; run `pnpm run generate:api`");
    }
    println!("{CLIENT_SCHEMA} matches contracts/openapi.json");
    Ok(())
}
