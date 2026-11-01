//! Exact inventory approval and transactional RESTRICT deletion; raw catalog data stays in memory.

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};

use super::{allowed::Binding, approve, catalog, fingerprint, query};

// Same cooperative lock as crates/db/src/admin.rs::MIGRATOR_LOCK. Cloud targets only postgres.
const MIGRATOR_LOCK: i64 = 0x6676_6f63_696d_6967;

pub(super) struct Inventory {
    pub report: Value,
    pub digest: String,
    pub blocked: bool,
    // All identifiers are quoted by PostgreSQL format(%I). Never print SQL or errors.
    drops: Vec<(String, String)>,
    roles: Vec<String>,
    role_digest: String,
}

pub(super) struct RolePlan {
    roles: Vec<String>,
    digest: String,
}

impl Inventory {
    pub async fn read(conn: &mut PgConnection, binding: &Binding) -> Result<Self> {
        let mut tx = conn
            .begin_with("BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .await?;
        let result = Self::snapshot(&mut tx, binding).await;
        tx.rollback().await?;
        result
    }

    async fn snapshot(conn: &mut PgConnection, binding: &Binding) -> Result<Self> {
        Self::snapshot_with_probe(conn, binding, None, std::future::ready(Ok(()))).await
    }

    async fn snapshot_with_probe(
        conn: &mut PgConnection,
        binding: &Binding,
        gate: Option<i64>,
        during: impl Future<Output = Result<()>>,
    ) -> Result<Self> {
        sqlx::query("SET LOCAL search_path=pg_catalog")
            .execute(&mut *conn)
            .await?;
        let sql = query::inventory();
        let query = sqlx::query_scalar::<_, Value>(&sql).bind(gate);
        let (captured, injected) = tokio::join!(query.fetch_one(&mut *conn), during);
        let captured = captured?;
        injected?;
        let mut raw = captured["catalog"].clone();
        let mut blockers = catalog::blockers(&raw, &captured["counts"])?;
        let objects: std::collections::BTreeSet<String> =
            serde_json::from_value(captured["identities"].clone())?;
        let unexpected: Vec<_> = objects.difference(&binding.allowed).collect();
        blockers["unlisted_objects"] = json!(unexpected.len());
        let blocked = blockers
            .as_object()
            .context("invalid_blockers")?
            .values()
            .any(|v| v == &json!(true) || v.as_i64().is_some_and(|n| n > 0));
        let drops: Vec<(String, String)> = serde_json::from_value(captured["drops"].clone())?;
        let roles: Vec<String> = serde_json::from_value(captured["roles"].clone())?;
        raw["format"] = json!(1);
        let authority = raw
            .as_object_mut()
            .context("invalid_catalog")?
            .remove("authority")
            .context("missing_authority")?;
        raw["binding"] = json!(binding.target);
        raw["server_version"] = authority["version"].clone();
        raw["allowlist"] = json!(binding.digest());
        raw["blockers"] = blockers;
        raw["drops"] = json!(drops);
        let digest = fingerprint(&raw);
        let role_digest = fingerprint(&raw["role_state"]);
        let candidates: Vec<Value> = drops
            .iter()
            .map(|(owner, sql)| {
                json!({
            "object": fingerprint(&json!(sql)), "owner": fingerprint(&json!(owner)),
            "kind": operation_kind(sql), "included": !blocked})
            })
            .collect();
        let history: Vec<Value> = raw["ledgers"]
            .as_array()
            .context("invalid_ledger_inventory")?
            .iter()
            .map(|r| {
                json!({"location": fingerprint(r), "owner": fingerprint(&r["owner"]),
                "inside_product_schema": r["schema"] == "fvoci", "included": !blocked})
            })
            .collect();
        let named: Vec<_> = objects.iter().map(|o| {
            let (kind, name) = o.split_once(' ').unwrap_or(("unsupported", o));
            json!({"kind":kind,"name":name,"allowed":binding.allowed.contains(o),"included":!blocked})
        }).collect();
        let report = json!({"format": 1, "phase": "inspect", "status": "PASS", "fingerprint": digest,
            "postgres_version": authority["version"], "authority": {
                "create": authority["create"], "connect": authority["connect"], "grant_connect": authority["grant_connect"], "set_role": authority["set_role"],
                "createrole": authority["createrole"], "manageable": authority["manageable"]},
            "blockers": raw["blockers"], "can_rebuild": !blocked, "write_authority":"CHECKED_AT_REBUILD", "candidates": candidates, "schema_objects": named,
            "role_candidates": roles.iter().map(|r| json!({"object": fingerprint(&json!(r)), "included": !blocked})).collect::<Vec<_>>(),
            "roles": redacted_roles(&raw), "memberships": redacted_memberships(&raw),
            "current_user": fingerprint(&authority["user"]),
            "history_locations": history, "read_only_transaction": "ROLLBACK", "changes": "NOT RUN"});
        Ok(Self {
            report,
            digest,
            blocked,
            drops,
            roles,
            role_digest,
        })
    }

    pub async fn delete(
        &self,
        conn: &mut PgConnection,
        binding: &Binding,
        approved: &str,
    ) -> Result<()> {
        let plan = self.delete_schema(conn, binding, approved).await?;
        plan.delete(conn).await
    }

    pub async fn delete_schema(
        &self,
        conn: &mut PgConnection,
        binding: &Binding,
        approved: &str,
    ) -> Result<RolePlan> {
        self.delete_schema_after_locks(conn, binding, approved, std::future::ready(Ok(())))
            .await
    }

    pub async fn delete_schema_after_locks(
        &self,
        conn: &mut PgConnection,
        binding: &Binding,
        approved: &str,
        after_locks: impl Future<Output = Result<()>>,
    ) -> Result<RolePlan> {
        self.delete_schema_hooks(
            conn,
            binding,
            approved,
            after_locks,
            None,
            std::future::ready(Ok(())),
        )
        .await
    }

    pub async fn delete_schema_with_probe(
        &self,
        conn: &mut PgConnection,
        binding: &Binding,
        approved: &str,
        gate: i64,
        during: impl Future<Output = Result<()>>,
    ) -> Result<RolePlan> {
        self.delete_schema_hooks(
            conn,
            binding,
            approved,
            std::future::ready(Ok(())),
            Some(gate),
            during,
        )
        .await
    }

    async fn delete_schema_hooks(
        &self,
        conn: &mut PgConnection,
        binding: &Binding,
        approved: &str,
        after_locks: impl Future<Output = Result<()>>,
        gate: Option<i64>,
        during: impl Future<Output = Result<()>>,
    ) -> Result<RolePlan> {
        approve(&self.digest, approved, self.blocked)?;
        catalog::write_authority(conn).await?;
        let mut tx = conn.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(MIGRATOR_LOCK)
            .execute(&mut *tx)
            .await?;
        lock_relations(&mut tx).await?;
        after_locks.await?;
        let current = Self::snapshot_with_probe(&mut tx, binding, gate, during).await?;
        if let Err(error) = approve(&current.digest, approved, current.blocked) {
            tx.rollback().await?;
            return Err(error);
        }
        delete_objects(&mut tx, current.drops).await?;
        tx.commit().await?;
        Ok(RolePlan {
            roles: self.roles.clone(),
            digest: self.role_digest.clone(),
        })
    }
}

impl RolePlan {
    pub async fn delete(&self, conn: &mut PgConnection) -> Result<()> {
        let mut tx = conn.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(MIGRATOR_LOCK)
            .execute(&mut *tx)
            .await?;
        let recheck = catalog::scalar(&mut tx, &query::role_recheck()).await?;
        let before = &recheck["state"];
        if fingerprint(before) != self.digest {
            tx.rollback().await?;
            anyhow::bail!("role_fingerprint_mismatch");
        }
        let owned = recheck["owned"]
            .as_i64()
            .context("invalid_ownership_count")?;
        if owned != 0 {
            tx.rollback().await?;
            anyhow::bail!("role_still_owns_objects");
        }
        for role in &self.roles {
            let revoke: String = sqlx::query_scalar("SELECT format('REVOKE ALL ON DATABASE %I FROM %I RESTRICT', current_database(), $1::text)")
                .bind(role).fetch_one(&mut *tx).await?;
            sqlx::query(&revoke).execute(&mut *tx).await?;
            let drop_role: String = sqlx::query_scalar("SELECT format('DROP ROLE %I', $1::text)")
                .bind(role)
                .fetch_one(&mut *tx)
                .await?;
            sqlx::query(&drop_role).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

fn operation_kind(sql: &str) -> &'static str {
    for (prefix, kind) in [
        ("ALTER TABLE", "foreign_key"),
        ("DROP TABLE", "table"),
        ("DROP VIEW", "view"),
        ("DROP MATERIALIZED VIEW", "materialized_view"),
        ("DROP SEQUENCE", "sequence"),
        ("DROP ROUTINE", "routine"),
        ("DROP DOMAIN", "domain"),
        ("DROP TYPE", "type"),
        ("DROP SCHEMA", "schema"),
    ] {
        if sql.starts_with(prefix) {
            return kind;
        }
    }
    "unsupported"
}

fn redacted_roles(raw: &Value) -> Vec<Value> {
    raw["roles"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            json!({
        "role": fingerprint(&r["name"]), "superuser": r["super"], "bypassrls": r["bypass"],
        "createrole": r["create_role"], "createdb": r["create_db"], "login": r["login"],
        "inherit": r["inherit"]})
        })
        .collect()
}

fn redacted_memberships(raw: &Value) -> Vec<Value> {
    raw["memberships"].as_array().into_iter().flatten().map(|r| json!({
        "role": fingerprint(&r["roleid"]), "member": fingerprint(&r["member"]), "grantor": fingerprint(&r["grantor"]),
        "admin": r["admin_option"], "set": r["set_option"], "inherit": r["inherit_option"]})).collect()
}

async fn lock_relations(conn: &mut PgConnection) -> Result<()> {
    let statements: Vec<(String, String)> = sqlx::query_as(
        "SELECT
        format('SET LOCAL ROLE %I', pg_get_userbyid(c.relowner)),
        format('LOCK TABLE ONLY %I.%I IN ACCESS EXCLUSIVE MODE', n.nspname,c.relname)
        FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
        WHERE n.nspname='fvoci' AND c.relkind IN ('r','p') ORDER BY c.oid",
    )
    .fetch_all(&mut *conn)
    .await?;
    for (role, statement) in statements {
        sqlx::query(&role).execute(&mut *conn).await?;
        sqlx::query(&statement).execute(&mut *conn).await?;
        sqlx::query("RESET ROLE").execute(&mut *conn).await?;
    }
    Ok(())
}

async fn delete_objects(conn: &mut PgConnection, mut pending: Vec<(String, String)>) -> Result<()> {
    while !pending.is_empty() {
        let before = pending.len();
        let mut remaining = Vec::new();
        for (owner, statement) in pending {
            let mut attempt = conn.begin().await?;
            let set_role: String =
                sqlx::query_scalar("SELECT format('SET LOCAL ROLE %I', $1::text)")
                    .bind(&owner)
                    .fetch_one(&mut *attempt)
                    .await?;
            sqlx::query(&set_role).execute(&mut *attempt).await?;
            match sqlx::query(&statement).execute(&mut *attempt).await {
                Ok(_) => {
                    sqlx::query("RESET ROLE").execute(&mut *attempt).await?;
                    attempt.commit().await?;
                }
                Err(error)
                    if error.as_database_error().and_then(|e| e.code()).as_deref()
                        == Some("2BP01") =>
                {
                    attempt.rollback().await?;
                    remaining.push((owner, statement));
                }
                Err(error) => return Err(error.into()),
            }
        }
        ensure!(
            remaining.len() < before,
            "dependency_cycle_or_new_external_reference"
        );
        pending = remaining;
    }
    Ok(())
}
