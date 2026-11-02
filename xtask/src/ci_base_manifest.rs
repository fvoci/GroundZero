//! 두 ci-base digest의 플랫폼을 태그 게시 전에 검증한다.

use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;
use xshell::{Shell, cmd};

use crate::test::platform;

pub fn run(image: &str, tag: &str, digests_dir: &Path) -> Result<()> {
    let digest = publish(&Shell::new()?, image, tag, digests_dir)?;
    println!("{digest}");
    Ok(())
}

fn publish(sh: &Shell, image: &str, tag: &str, digests_dir: &Path) -> Result<String> {
    let names = digest_files(digests_dir)?;
    validate_digests(&names)?;
    let sources: Vec<String> = names
        .iter()
        .map(|name| format!("{image}@sha256:{name}"))
        .collect();
    let sources = sources.as_slice();
    // dry-run은 최종 index를 만들되 레지스트리에 쓰거나 태그를 붙이지 않는다.
    let raw = cmd!(sh, "docker buildx imagetools create --dry-run {sources...}")
        .quiet()
        .read()
        .context("prepare ci-base manifest without publishing")?;
    let planned: Value = serde_json::from_str(&raw).context("parse planned ci-base manifest")?;
    validate_manifest(&planned)?;

    let tagged = format!("{image}:{tag}");
    cmd!(
        sh,
        "docker buildx imagetools create --tag {tagged} {sources...}"
    )
    .quiet()
    .read()
    .context("publish validated ci-base manifest")?;
    let format = "{{json .Manifest}}";
    let raw = cmd!(
        sh,
        "docker buildx imagetools inspect {tagged} --format {format}"
    )
    .quiet()
    .read()
    .context("inspect published ci-base manifest")?;
    let published: Value =
        serde_json::from_str(&raw).context("parse published ci-base manifest")?;
    validate_manifest(&published)?;
    let digest = published
        .get("digest")
        .and_then(Value::as_str)
        .context("published manifest has no digest")?;
    anyhow::ensure!(
        digest.strip_prefix("sha256:").is_some_and(is_sha256_hash),
        "published manifest digest must be canonical sha256"
    );
    Ok(digest.to_owned())
}

fn digest_files(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).context("read platform digest directory")? {
        let entry = entry?;
        anyhow::ensure!(
            entry.file_type()?.is_file(),
            "digest entries must be regular files"
        );
        names.push(
            entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow::anyhow!("digest filename must be UTF-8"))?,
        );
    }
    names.sort_unstable();
    Ok(names)
}

fn is_sha256_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn validate_digests(names: &[String]) -> Result<()> {
    anyhow::ensure!(names.len() == 2, "expected two platform digests");
    anyhow::ensure!(names[0] != names[1], "platform digests must be distinct");
    anyhow::ensure!(
        names.iter().all(|name| is_sha256_hash(name)),
        "digest filenames must be canonical sha256 hashes"
    );
    Ok(())
}

fn validate_manifest(manifest: &Value) -> Result<()> {
    let entries = manifest
        .get("manifests")
        .and_then(Value::as_array)
        .context("missing manifest list")?;
    let mut amd64 = 0;
    let mut arm64 = 0;
    for entry in entries {
        let candidate = entry
            .get("platform")
            .context("manifest descriptor has no platform")?;
        let os = candidate.get("os").and_then(Value::as_str);
        let arch = candidate.get("architecture").and_then(Value::as_str);
        let variant = candidate.get("variant");
        // BuildKit의 증명 manifest는 실행 플랫폼이 아니다.
        if os == Some("unknown")
            && arch == Some("unknown")
            && entry
                .pointer("/annotations/vnd.docker.reference.type")
                .and_then(Value::as_str)
                == Some("attestation-manifest")
        {
            continue;
        }
        if platform::matches_platform(os, arch, variant, "linux/amd64") {
            amd64 += 1;
        } else if platform::matches_platform(os, arch, variant, "linux/arm64") {
            arm64 += 1;
        } else {
            anyhow::bail!("unexpected or incompatible manifest platform: {candidate}");
        }
    }
    anyhow::ensure!(
        amd64 == 1 && arm64 == 1,
        "expected linux/amd64 and linux/arm64 exactly once (got {amd64} and {arm64})"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn index(platforms: &[Value]) -> Value {
        json!({"manifests": platforms.iter().map(|platform| json!({"platform": platform})).collect::<Vec<_>>()})
    }

    fn platform_pair() -> Value {
        index(&[
            json!({"os":"linux","architecture":"amd64"}),
            json!({"os":"linux","architecture":"arm64","variant":"v8"}),
        ])
    }

    #[test]
    fn only_marked_attestations_are_outside_the_runtime_platform_set() -> Result<()> {
        let mut manifest = platform_pair();
        let attestation = json!({"platform":{"os":"unknown","architecture":"unknown"},"annotations":{"vnd.docker.reference.type":"attestation-manifest"}});
        manifest["manifests"]
            .as_array_mut()
            .context("test manifest list")?
            .push(attestation);
        validate_manifest(&manifest)?;
        manifest["manifests"][2]["annotations"] = json!({});
        assert!(validate_manifest(&manifest).is_err());
        manifest["manifests"][2] = json!({});
        assert!(validate_manifest(&manifest).is_err());
        Ok(())
    }

    #[cfg(unix)]
    fn publication_fixture(planned: &Value) -> Result<std::path::PathBuf> {
        let root = crate::compose::tests::scratch_root()?;
        std::fs::create_dir(root.join("digests"))?;
        for name in ["a".repeat(64), "b".repeat(64)] {
            std::fs::write(root.join("digests").join(name), "")?;
        }
        std::fs::write(root.join("planned.json"), planned.to_string())?;
        let mut published = platform_pair();
        published["digest"] = json!(format!("sha256:{}", "c".repeat(64)));
        std::fs::write(root.join("published.json"), published.to_string())?;
        let docker = root.join("docker");
        crate::test_fixture::write_executable(
            &docker,
            r#"#!/bin/sh
echo "$*" >> calls
case "$1 $2 $3 $4" in
  'buildx imagetools create --dry-run') cat planned.json ;;
  'buildx imagetools create --tag') echo published ;;
  'buildx imagetools inspect '*) cat published.json ;;
  *) exit 97 ;;
esac
"#,
        )?;
        Ok(root)
    }

    #[cfg(unix)]
    #[test]
    fn publication_rejects_an_invalid_plan_before_attaching_a_tag() -> Result<()> {
        for valid in [false, true] {
            let mut planned = platform_pair();
            if !valid {
                planned["manifests"][0]["platform"]["variant"] = json!("v3");
            }
            let root = publication_fixture(&planned)?;
            let sh = Shell::new()?;
            sh.change_dir(&root);
            let _path = sh.push_env("PATH", format!("{}:{}", root.display(), sh.var("PATH")?));
            let result = publish(
                &sh,
                "ghcr.io/fvoci/groundzero-ci-base",
                "source-sha",
                &root.join("digests"),
            );
            let calls = std::fs::read_to_string(root.join("calls"))?;
            let calls: Vec<_> = calls.lines().collect();
            assert!(calls[0].starts_with("buildx imagetools create --dry-run "));
            if valid {
                assert_eq!(result?, format!("sha256:{}", "c".repeat(64)));
                assert_eq!(calls.len(), 3);
                assert!(calls[1].starts_with("buildx imagetools create --tag "));
                assert!(calls[2].starts_with("buildx imagetools inspect "));
            } else {
                assert!(result.is_err());
                assert_eq!(calls.len(), 1, "invalid plan must never attach a tag");
            }
            std::fs::remove_dir_all(root)?;
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn invalid_source_files_stop_before_any_docker_command() -> Result<()> {
        for directory_entry in [false, true] {
            let root = publication_fixture(&platform_pair())?;
            if directory_entry {
                std::fs::create_dir(root.join("digests/unexpected"))?;
            } else {
                std::fs::remove_file(root.join("digests").join("b".repeat(64)))?;
            }
            let sh = Shell::new()?;
            sh.change_dir(&root);
            let _path = sh.push_env("PATH", format!("{}:{}", root.display(), sh.var("PATH")?));
            assert!(
                publish(
                    &sh,
                    "ghcr.io/fvoci/groundzero-ci-base",
                    "source-sha",
                    &root.join("digests")
                )
                .is_err()
            );
            assert!(!root.join("calls").exists());
            std::fs::remove_dir_all(root)?;
        }
        Ok(())
    }

    #[test]
    fn exactly_two_distinct_canonical_digest_files_are_required() -> Result<()> {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        validate_digests(&[a.clone(), b.clone()])?;
        for invalid in [
            vec![],
            vec![a.clone()],
            vec![a.clone(), b, "c".repeat(64)],
            vec![a.clone(), a.clone()],
            vec![a, "not-a-digest".to_owned()],
        ] {
            assert!(validate_digests(&invalid).is_err(), "accepted {invalid:?}");
        }
        Ok(())
    }

    #[test]
    fn each_runtime_platform_must_appear_exactly_once() -> Result<()> {
        let amd64 = json!({"os":"linux","architecture":"amd64"});
        let arm64 = json!({"os":"linux","architecture":"arm64","variant":"v8"});
        validate_manifest(&index(&[amd64.clone(), arm64.clone()]))?;
        for platforms in [
            vec![],
            vec![amd64.clone()],
            vec![amd64.clone(), amd64.clone()],
            vec![amd64.clone(), arm64.clone(), arm64.clone()],
            vec![amd64, arm64, json!({"os":"windows","architecture":"amd64"})],
        ] {
            assert!(
                validate_manifest(&index(&platforms)).is_err(),
                "accepted {platforms}",
                platforms = json!(platforms)
            );
        }
        Ok(())
    }

    #[test]
    fn runtime_variants_follow_the_test_runner_rules() -> Result<()> {
        let amd64 = json!({"os":"linux","architecture":"amd64"});
        let arm64 = json!({"os":"linux","architecture":"arm64"});
        validate_manifest(&index(&[amd64.clone(), arm64.clone()]))?;
        for platforms in [
            vec![
                json!({"os":"linux","architecture":"amd64","variant":"v3"}),
                arm64,
            ],
            vec![
                amd64,
                json!({"os":"linux","architecture":"arm64","variant":"v9"}),
            ],
        ] {
            assert!(
                validate_manifest(&index(&platforms)).is_err(),
                "accepted {}",
                json!(platforms)
            );
        }
        Ok(())
    }
}
