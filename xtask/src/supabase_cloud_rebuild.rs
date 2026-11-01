//! 승인 목록 지문을 사용하는 Cloud 시험 DB 재설치. 연결값·사용자명·원시 오류는 출력하지 않는다.

mod allowed;
mod catalog;
mod catalog_tests;
pub mod fixture;
mod inventory;
mod query;
mod race_tests;
mod receipt;
mod target;
mod verify;

use std::io::{self, Write};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Result, ensure};
use clap::ValueEnum;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection};

use inventory::Inventory;
use target::Target;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Phase {
    Inspect,
    Rebuild,
    Verify,
}

fn fingerprint(value: &Value) -> String {
    Sha256::digest(value.to_string().as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn approve(actual: &str, approved: &str, blocked: bool) -> Result<()> {
    ensure!(!blocked, "blocked_inventory");
    ensure!(
        approved.len() == 64 && approved == actual,
        "inventory_mismatch"
    );
    Ok(())
}

fn confirm_writes(phase: Phase, confirmation: Option<&str>) -> Result<()> {
    if matches!(phase, Phase::Rebuild | Phase::Verify) {
        ensure!(
            confirmation
                == Some(
                    "TEST_ONLY_NO_OTHER_APPS_OR_WRITERS_NO_OTHER_WORK_MATCHED_SHA_AND_FINGERPRINT"
                ),
            "execution_checklist_required"
        );
    }
    Ok(())
}

async fn install(_root: &Path, target: &Target, report: &mut Value) -> Result<()> {
    // Both binaries are built together, including the container runner's CARGO_TARGET_DIR.
    let binary = std::env::current_exe()?.with_file_name("fvoci-server");
    for operation in ["bootstrap", "migrate"] {
        report["stage"] = json!(operation);
        let mut child = tokio::process::Command::new(&binary);
        child
            .args(["db", operation])
            .env_clear()
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if target.certificate.is_none() {
            // Only the isolated fixture constructs a target without a certificate; its credentials
            // are the explicit deploy/ test values. Cloud children always suppress diagnostics.
            child.stderr(Stdio::inherit()).stdout(Stdio::inherit());
        }
        if operation == "bootstrap" {
            child
                .env("DATABASE_ADMIN_URL", target.url(&target.admin))
                .env("FVOCI_DB_MIGRATOR_PASSWORD", &target.migrator_password)
                .env("FVOCI_DB_APP_PASSWORD", &target.app_password);
        } else {
            child.env("DATABASE_MIGRATION_URL", target.url(&target.migrator));
        }
        ensure!(child.status().await?.success(), "product_db_command_failed");
        report[operation] = json!("PASS");
    }
    Ok(())
}

async fn execute(
    root: &Path,
    phase: Phase,
    target: &Target,
    approved: &str,
    report: &mut Value,
) -> Result<()> {
    match phase {
        Phase::Inspect | Phase::Rebuild => {
            report["stage"] = json!("inventory");
            let mut admin = PgConnection::connect_with(&target.admin).await?;
            if matches!(phase, Phase::Inspect) {
                catalog::inspect_authority(&mut admin).await?;
            }
            let inventory = Inventory::read(&mut admin, &target.binding).await?;
            if matches!(phase, Phase::Inspect) {
                *report = inventory.report;
            } else {
                report["stage"] = json!("approval_and_delete");
                let roles = inventory
                    .delete_schema(&mut admin, &target.binding, approved)
                    .await?;
                report["schema_deletion"] = json!("COMMITTED");
                report["deletion"] = json!("PARTIAL");
                report["roles_deletion"] = json!("NOT DELETED");
                report["stage"] = json!("roles_recheck_and_delete");
                roles.delete(&mut admin).await?;
                report["roles_deletion"] = json!("COMMITTED");
                report["deletion"] = json!("COMMITTED");
                admin.close().await?;
                install(root, target, report).await?;
                report["stage"] = json!("record_rebuild_proof");
                receipt::record(
                    target,
                    &receipt::Proof {
                        source_sha: report["source_sha"].as_str().unwrap_or_default().to_owned(),
                        fingerprint: approved.to_owned(),
                    },
                )
                .await?;
                report["rebuild_proof"] = json!("RECORDED");
                return Ok(());
            }
            admin.close().await?;
        }
        Phase::Verify => {
            report["stage"] = json!("verify");
            let proof = receipt::Proof {
                source_sha: report["source_sha"].as_str().unwrap_or_default().to_owned(),
                fingerprint: approved.to_owned(),
            };
            *report = verify::run(target, &proof).await?;
        }
    }
    Ok(())
}

fn run_with(
    root: &Path,
    phase: Phase,
    connect: bool,
    mut lookup: impl FnMut(&str) -> Option<String>,
) -> (Value, i32) {
    let mut report = json!({"phase": format!("{phase:?}").to_lowercase(), "status": "FAIL",
        "stage": "validation", "deletion": "NOT RUN", "bootstrap": "NOT RUN", "migrate": "NOT RUN"});
    if !connect {
        report["reason"] = json!("explicit_connect_required");
        return (report, 2);
    }
    let result = (|| -> Result<()> {
        let sha = target::actions_guard(&mut lookup)?;
        confirm_writes(
            phase,
            lookup("SUPABASE_REBUILD_SAFETY_CONFIRMATION").as_deref(),
        )?;
        let approved = lookup("SUPABASE_REBUILD_APPROVED_FINGERPRINT").unwrap_or_default();
        if matches!(phase, Phase::Rebuild | Phase::Verify) {
            ensure!(
                approved.len() == 64 && approved.bytes().all(|b| b.is_ascii_hexdigit()),
                "approval_required"
            );
        }
        let mut target = Target::load(phase, &sha, &mut lookup)?;
        report["source_sha"] = json!(sha);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let result =
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                runtime.block_on(async {
                    tokio::time::timeout(
                        Duration::from_secs(180),
                        execute(root, phase, &target, &approved, &mut report),
                    )
                    .await?
                })
            });
        report["source_sha"] = json!(sha);
        runtime.shutdown_background();
        if let Some(cert) = &mut target.certificate {
            cert.remove()?;
        }
        result
    })();
    finish_report(report, result)
}

fn finish_report(mut report: Value, result: Result<()>) -> (Value, i32) {
    report["status"] = json!(if result.is_ok() { "PASS" } else { "FAIL" });
    // Deliberately discard every raw error: driver, OS, process and input errors may carry identities.
    let code = i32::from(result.is_err());
    (report, code)
}

pub fn run(root: &Path, phase: Phase, connect: bool) -> i32 {
    let (report, code) = run_with(root, phase, connect, |key| {
        if target::AMBIENT_SETTINGS.contains(&key) {
            std::env::var_os(key).is_some().then(String::new)
        } else {
            std::env::var(key).ok()
        }
    });
    if writeln!(io::stdout().lock(), "{report}").is_err() {
        return 1;
    }
    code
}

pub fn rejected_arguments() -> i32 {
    if writeln!(io::stdout().lock(), "{}", json!({"status": "FAIL", "stage": "validation", "reason": "invalid_arguments", "changes": "NOT RUN"})).is_err() {
        return 1;
    }
    2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_inspect_cannot_keep_a_successful_inventory_status() {
        for reason in ["close_failed", "certificate_cleanup_failed"] {
            let inventory = json!({"status": "PASS", "fingerprint": "approved-candidate"});
            let (report, code) = finish_report(inventory, Err(anyhow::anyhow!(reason)));
            assert_eq!(code, 1);
            assert_eq!(report["status"], "FAIL");
            assert!(!report.to_string().contains(reason));
        }
    }

    #[test]
    fn timed_out_inspect_cannot_keep_a_successful_inventory_status() -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let mut report = json!({"status": "FAIL"});
        let result: Result<()> = runtime.block_on(async {
            tokio::time::timeout(Duration::from_millis(1), async {
                report = json!({"status": "PASS", "fingerprint": "approved-candidate"});
                std::future::pending::<Result<()>>().await
            })
            .await?
        });
        let (report, code) = finish_report(report, result);
        assert_eq!(code, 1);
        assert_eq!(report["status"], "FAIL");
        Ok(())
    }

    #[test]
    fn successful_inspect_retains_its_inventory() {
        let inventory = json!({"status": "PASS", "fingerprint": "approved-candidate"});
        let (report, code) = finish_report(inventory.clone(), Ok(()));
        assert_eq!(code, 0);
        assert_eq!(report, inventory);
    }

    #[test]
    fn approval_must_match_and_blocked_inventory_never_proceeds() {
        let digest = fingerprint(&json!({"object": 1}));
        assert!(approve(&digest, &digest, false).is_ok());
        assert!(approve(&digest, &digest, true).is_err());
        assert!(approve(&digest, &"0".repeat(64), false).is_err());
        assert!(approve(&digest, "", false).is_err());
    }

    #[test]
    fn writes_require_the_complete_execution_checklist() {
        assert!(confirm_writes(Phase::Inspect, None).is_ok());
        for phase in [Phase::Rebuild, Phase::Verify] {
            assert!(confirm_writes(phase, None).is_err());
            assert!(confirm_writes(phase, Some("yes")).is_err());
            assert!(confirm_writes(phase, Some("TEST_ONLY_NO_OTHER_APPS_OR_WRITERS_NO_OTHER_WORK_MATCHED_SHA_AND_FINGERPRINT")).is_ok());
        }
    }

    #[test]
    fn connection_refusal_happens_without_secret_lookup() {
        assert_eq!(
            run_with(Path::new("."), Phase::Inspect, false, |_| panic!(
                "must not read environment"
            ))
            .1,
            2
        );
    }

    #[test]
    fn explicit_connect_still_rejects_non_actions_environment() {
        assert_eq!(
            run_with(Path::new("."), Phase::Inspect, true, |_| None).1,
            1
        );
    }
}
