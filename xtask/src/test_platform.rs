//! ci-base의 게시 플랫폼을 daemon과 대조한다. 에뮬레이션에 기대어 다른 아키텍처를 실행하지 않는다.

use anyhow::{Context, Result};
use serde_json::Value;
use xshell::{Shell, cmd};

pub(super) fn daemon(sh: &Shell) -> Result<String> {
    let format = "{{.OSType}}/{{.Architecture}}";
    let raw = cmd!(sh, "docker info --format {format}")
        .quiet()
        .read()
        .context("read Docker daemon platform before selecting the test runner")?;
    let (os, architecture) = raw
        .trim()
        .split_once('/')
        .context("invalid Docker daemon platform; use --host")?;
    anyhow::ensure!(
        os == "linux",
        "Docker daemon platform {raw} is not Linux; use --host"
    );
    let architecture = match architecture {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        other => other,
    };
    Ok(format!("{os}/{architecture}"))
}

pub(super) fn require_image(sh: &Shell, image: &str, platform: &str) -> Result<()> {
    let guidance = || {
        format!(
            "ci-base {image} must provide Docker daemon platform {platform}; update CI_BASE_IMAGE_DIGEST (or FVOCI_TEST_CI_BASE_IMAGE_DIGEST) to the published multi-platform manifest digest, or use --host"
        )
    };
    // --verbose는 단일 manifest에도 config에서 얻은 OS·아키텍처를 붙인다.
    let raw = cmd!(sh, "docker manifest inspect --verbose {image}")
        .quiet()
        .read()
        .with_context(guidance)?;
    let manifest: Value = serde_json::from_str(&raw).with_context(guidance)?;
    anyhow::ensure!(
        provides(&manifest, platform),
        "published image does not include {platform}; {}",
        guidance()
    );
    Ok(())
}

fn provides(manifest: &Value, platform: &str) -> bool {
    let entries = match manifest {
        Value::Array(entries) => entries.as_slice(),
        Value::Object(_) => std::slice::from_ref(manifest),
        _ => return false,
    };
    entries.iter().any(|entry| {
        let os = entry
            .pointer("/Descriptor/platform/os")
            .and_then(Value::as_str);
        let arch = entry
            .pointer("/Descriptor/platform/architecture")
            .and_then(Value::as_str);
        matches!((os, arch), (Some(os), Some(arch)) if format!("{os}/{arch}") == platform)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_manifest_and_indexes_require_the_exact_os_and_architecture() {
        let amd64 = serde_json::json!({"Descriptor": {"platform": {"os": "linux", "architecture": "amd64"}}});
        let arm64 = serde_json::json!({"Descriptor": {"platform": {"os": "linux", "architecture": "arm64", "variant": "v8"}}});
        let attestation = serde_json::json!({"Descriptor": {"platform": {"os": "unknown", "architecture": "unknown"}}});
        assert!(provides(&amd64, "linux/amd64"));
        assert!(!provides(&amd64, "linux/arm64"));
        let old_index = serde_json::json!([amd64, attestation]);
        assert!(provides(&old_index, "linux/amd64"));
        assert!(!provides(&old_index, "linux/arm64"));
        let multi_index = serde_json::json!([amd64, arm64, attestation]);
        assert!(provides(&multi_index, "linux/amd64"));
        assert!(provides(&multi_index, "linux/arm64"));
        assert!(!provides(&multi_index, "windows/amd64"));
        for invalid in [serde_json::json!([]), serde_json::json!({}), Value::Null] {
            assert!(!provides(&invalid, "linux/amd64"), "{invalid}");
        }
    }
}
