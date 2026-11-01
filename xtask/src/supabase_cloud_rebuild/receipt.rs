//! Schema-owner marker binding verification to the successful rebuild's source and approval.

use anyhow::{Result, ensure};
use serde_json::Value;
use sqlx::{Connection, PgConnection};

pub(super) struct Proof {
    pub source_sha: String,
    pub fingerprint: String,
}

pub(super) fn validate(stored: Option<&Value>, proof: &Proof) -> Result<()> {
    ensure!(
        proof.source_sha.len() == 40
            && proof.source_sha.bytes().all(|b| b.is_ascii_hexdigit())
            && proof.fingerprint.len() == 64
            && proof.fingerprint.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid_rebuild_proof"
    );
    let stored = stored.ok_or_else(|| anyhow::anyhow!("missing_rebuild_proof"))?;
    ensure!(
        stored["source_sha"] == proof.source_sha && stored["fingerprint"] == proof.fingerprint,
        "rebuild_proof_mismatch"
    );
    Ok(())
}

const PREFIX: &str = "fvoci-cloud-rebuild-v1:";

pub(super) async fn check(conn: &mut PgConnection, proof: &Proof) -> Result<()> {
    let comment: Option<String> =
        sqlx::query_scalar("SELECT obj_description(to_regnamespace('fvoci'),'pg_namespace')")
            .fetch_one(conn)
            .await?;
    let stored = comment
        .as_deref()
        .and_then(|c| c.strip_prefix(PREFIX))
        .map(serde_json::from_str::<Value>)
        .transpose()?;
    validate(stored.as_ref(), proof)
}

pub(super) async fn record(target: &super::Target, proof: &Proof) -> Result<()> {
    let value = serde_json::json!({"source_sha":proof.source_sha,"fingerprint":proof.fingerprint});
    validate(Some(&value), proof)?;
    let mut conn = PgConnection::connect_with(&target.migrator).await?;
    let mut tx = conn.begin().await?;
    super::verify::checksums(&mut tx).await?;
    let statement: String =
        sqlx::query_scalar("SELECT format('COMMENT ON SCHEMA fvoci IS %L',$1::text)")
            .bind(format!("{PREFIX}{value}"))
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(&statement).execute(&mut *tx).await?;
    tx.commit().await?;
    conn.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn missing_or_mismatched_rebuild_proof_is_rejected() {
        let proof = Proof {
            source_sha: "a".repeat(40),
            fingerprint: "b".repeat(64),
        };
        let stored = json!({"source_sha": "a".repeat(40), "fingerprint": "b".repeat(64)});
        assert!(validate(Some(&stored), &proof).is_ok());
        assert!(validate(None, &proof).is_err());
        let advanced = Proof {
            source_sha: "c".repeat(40),
            fingerprint: proof.fingerprint.clone(),
        };
        assert!(validate(Some(&stored), &advanced).is_err());
        let wrong = Proof {
            source_sha: proof.source_sha,
            fingerprint: "d".repeat(64),
        };
        assert!(validate(Some(&stored), &wrong).is_err());
    }
}
