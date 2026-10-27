//! HWP·HWPX 미리보기용 rhwp WASM(D-76). 빌드는 제품 이미지와 같은 단계(`deploy/docker/app.Dockerfile`의
//! `rhwp-out`)로만 한다. 입력(`deploy/rhwp`의 build.sh·inputs.env·Cargo.lock·deny.toml)의 해시가 같으면
//! `target/rhwp/<해시>`의 결과를 다시 쓰고, 다르면 다시 빌드한다. 결과를 개발 서버 자리(`apps/web/public/rhwp`)에
//! 두고, 브라우저에 들어가는 크레이트의 고지(`deploy/rhwp/THIRD_PARTY_NOTICES.txt`)를 만든다. 이 고지 파일 첫 줄의
//! 입력 해시가 지금 입력과 같은지는 `cargo xtask check`가 본다.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use xshell::{Shell, cmd};

/// 해시에 넣는 입력(순서 고정). build.sh도 같은 순서로 해시를 만든다.
const INPUTS: [&str; 4] = ["build.sh", "inputs.env", "Cargo.lock", "deny.toml"];
pub const NOTICES: &str = "deploy/rhwp/THIRD_PARTY_NOTICES.txt";

/// 입력 해시(앞 16자). build.sh의 `cat … | sha256sum | cut -c1-16`과 같다.
pub fn input_hash(root: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    for name in INPUTS {
        let path = root.join("deploy/rhwp").join(name);
        hasher.update(fs::read(&path).with_context(|| format!("read {}", path.display()))?);
    }
    let digest = hasher.finalize();
    Ok(digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()[..16]
        .to_owned())
}

fn notices_header(hash: &str) -> String {
    format!("rhwp-wasm-inputs: {hash}\n")
}

/// 고지 파일이 지금 입력으로 만든 것인지.
pub fn check_notices(root: &Path) -> Result<()> {
    let hash = input_hash(root)?;
    let text = fs::read_to_string(root.join(NOTICES)).with_context(|| format!("read {NOTICES}"))?;
    if !text.starts_with(&notices_header(&hash)) {
        bail!(
            "{NOTICES} was not generated from the current deploy/rhwp inputs; run `cargo xtask rhwp`"
        );
    }
    Ok(())
}

pub fn run(root: &Path, force: bool) -> Result<()> {
    let hash = input_hash(root)?;
    let out = root.join("target/rhwp").join(&hash);
    if force || !out.join("report.json").is_file() {
        let sh = Shell::new()?;
        sh.change_dir(root);
        let dest = out.to_string_lossy().into_owned();
        println!("==> building rhwp WASM (inputs {hash})");
        cmd!(
            sh,
            "docker build --file deploy/docker/app.Dockerfile --target rhwp-out --output type=local,dest={dest} ."
        )
        .run()?;
    } else {
        println!("==> reusing rhwp WASM built from the same inputs ({hash})");
    }
    install_web(root, &out)?;
    write_notices(root, &out, &hash)?;
    println!("{}", fs::read_to_string(out.join("report.json"))?.trim());
    Ok(())
}

/// 개발 서버가 내주는 자리에 둔다(이전 결과는 지운다). 운영 이미지는 Dockerfile이 같은 자리로 복사한다.
fn install_web(root: &Path, out: &Path) -> Result<()> {
    let dest = root.join("apps/web/public/rhwp");
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    copy_dir(&out.join("web"), &dest)
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from).with_context(|| format!("read {}", from.display()))? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// 브라우저에 들어가는 크레이트마다 라이선스 식과 함께 배포되는 원문을 싣는다(앱 고지와 같은 모양).
fn write_notices(root: &Path, out: &Path, hash: &str) -> Result<()> {
    let notices = out.join("notices");
    let index = fs::read_to_string(notices.join("index.tsv")).context("rhwp notices index")?;
    let mut rows: Vec<(String, String, String, String)> = index
        .lines()
        .filter(|l| !l.is_empty())
        .map(|line| {
            let mut cols = line.split('\t');
            let mut next = || cols.next().unwrap_or_default().to_owned();
            (next(), next(), next(), next())
        })
        .collect();
    rows.sort();
    rows.dedup();
    let mut text = notices_header(hash);
    let _ignored = write!(
        text,
        "Rust crates compiled into the HWP/HWPX preview WebAssembly (rhwp, D-76): {}\n\n",
        rows.len()
    );
    for (name, version, license, repository) in &rows {
        let _ignored = write!(text, "---- {name} {version} ----\nLicense: {license}\n");
        if !repository.is_empty() {
            let _ignored = writeln!(text, "Source: {repository}");
        }
        text.push('\n');
        let dir: PathBuf = notices.join(format!("{name}-{version}"));
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        files.sort();
        if files.is_empty() {
            text.push_str("(No license file shipped with the crate; see the SPDX text of the license above.)\n\n");
        }
        for file in files {
            let body = String::from_utf8_lossy(&fs::read(&file)?).replace("\r\n", "\n");
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            let _ignored = write!(text, "[{name}]\n{}\n\n", body.trim_end());
        }
    }
    let upstream = notices.join("rhwp-THIRD_PARTY_LICENSES.md");
    if upstream.is_file() {
        text.push_str("---- rhwp THIRD_PARTY_LICENSES.md (upstream) ----\n\n");
        text.push_str(
            fs::read_to_string(upstream)?
                .replace("\r\n", "\n")
                .trim_end(),
        );
        text.push('\n');
    }
    fs::write(root.join(NOTICES), text).with_context(|| format!("write {NOTICES}"))?;
    println!("wrote {NOTICES}");
    Ok(())
}

/// 앞서 만든 결과(같은 입력)의 감사 작업 공간에서 보안 권고만 다시 본다. 권고 DB는 cargo-deny가 매번 새로 받는다.
/// 입력이 같아 다시 빌드하지 않는 동안에도 새로 나온 권고를 놓치지 않기 위한 것이다(CI는 매 실행 돈다).
pub fn audit(root: &Path) -> Result<()> {
    let hash = input_hash(root)?;
    let manifest = root
        .join("target/rhwp")
        .join(&hash)
        .join("audit/Cargo.toml");
    if !manifest.is_file() {
        bail!("no rhwp build for inputs {hash}; run `cargo xtask rhwp` first");
    }
    let sh = Shell::new()?;
    sh.change_dir(root);
    let manifest = manifest.to_string_lossy().into_owned();
    println!("==> cargo deny check advisories (rhwp WASM graph, inputs {hash})");
    cmd!(
        sh,
        "cargo deny --locked --manifest-path {manifest} --config deploy/rhwp/deny.toml check advisories"
    )
    .run()?;
    Ok(())
}
