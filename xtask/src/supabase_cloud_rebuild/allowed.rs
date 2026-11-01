//! Reviewed identities. Observing a Cloud database can never extend this list.

use super::{catalog, fingerprint, verify};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeSet;

#[derive(Clone)]
pub(super) struct Binding {
    pub target: String,
    pub allowed: BTreeSet<String>,
}

impl Binding {
    pub fn product(target: String) -> Result<Self> {
        let manifest: Value = serde_json::from_str(include_str!("allowed-objects.json"))?;
        ensure!(
            manifest["sql_fingerprint"] == verify::source_fingerprint(),
            "allowlist_source_mismatch"
        );
        Ok(Self {
            target,
            allowed: serde_json::from_value(manifest["objects"].clone())?,
        })
    }

    // This fixed seed is used only by the isolated fixture, never Cloud execution.
    pub fn fixture() -> Self {
        Self {
            target: "isolated-compose-fixture".to_owned(),
            allowed: [
                "schema fvoci",
                "table fvoci.legacy",
                "table fvoci.child",
                "table fvoci.a_child",
                "table fvoci._sqlx_migrations",
                "view fvoci.legacy_view",
                "sequence fvoci.legacy_sequence",
                "index fvoci.legacy_pkey",
                "type fvoci.legacy_state",
                "routine fvoci.legacy_function()",
                "column fvoci.legacy.id",
                "column fvoci.legacy.secret",
                "column fvoci.child.id",
                "column fvoci.a_child.id",
                "column fvoci.legacy_view.id",
                "column fvoci.legacy_sequence.last_value",
                "column fvoci.legacy_sequence.log_cnt",
                "column fvoci.legacy_sequence.is_called",
                "column fvoci._sqlx_migrations.version",
                "column fvoci.legacy_pkey.id",
                "constraint fvoci.legacy.legacy_pkey",
                "constraint fvoci.child.child_id_fkey",
                "constraint fvoci.a_child.child_id_fkey",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }

    pub fn digest(&self) -> String {
        fingerprint(&json!(self.allowed))
    }
}

pub(super) const OBJECTS_SQL: &str = "SELECT coalesce(jsonb_agg(identity ORDER BY identity), '[]') FROM (
        SELECT 'schema fvoci' identity FROM pg_namespace WHERE nspname='fvoci'
        UNION ALL SELECT format('%s %I.%I', CASE relkind WHEN 'r' THEN 'table' WHEN 'p' THEN 'table'
            WHEN 'v' THEN 'view' WHEN 'm' THEN 'materialized_view' WHEN 'S' THEN 'sequence'
            WHEN 'i' THEN 'index' WHEN 'I' THEN 'index' ELSE 'unsupported_relation' END, 'fvoci', relname)
            FROM pg_class WHERE relnamespace=to_regnamespace('fvoci')
        UNION ALL SELECT format('routine %I.%I(%s)', 'fvoci', proname, pg_get_function_identity_arguments(oid))
            FROM pg_proc WHERE pronamespace=to_regnamespace('fvoci')
        UNION ALL SELECT format('type %I.%I', 'fvoci', typname) FROM pg_type t
            WHERE typnamespace=to_regnamespace('fvoci') AND typtype IN ('e','d')
            AND NOT EXISTS (SELECT FROM pg_type parent WHERE parent.typarray=t.oid)
        UNION ALL SELECT format('column %I.%I.%I', 'fvoci', c.relname,a.attname)
            FROM pg_attribute a JOIN pg_class c ON c.oid=a.attrelid
            WHERE c.relnamespace=to_regnamespace('fvoci') AND a.attnum>0 AND NOT a.attisdropped
        UNION ALL SELECT format('constraint %I.%I.%I', 'fvoci', coalesce(c.relname,t.typname), k.conname)
            FROM pg_constraint k LEFT JOIN pg_class c ON c.oid=k.conrelid LEFT JOIN pg_type t ON t.oid=k.contypid
            WHERE k.connamespace=to_regnamespace('fvoci') AND k.contype<>'n'
        UNION ALL SELECT format('trigger %I.%I.%I', 'fvoci',c.relname,t.tgname)
            FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid
            WHERE c.relnamespace=to_regnamespace('fvoci') AND NOT t.tgisinternal
        UNION ALL SELECT format('policy %I.%I.%I', 'fvoci',c.relname,p.polname)
            FROM pg_policy p JOIN pg_class c ON c.oid=p.polrelid WHERE c.relnamespace=to_regnamespace('fvoci')
        UNION ALL SELECT format('rule %I.%I.%I', 'fvoci',c.relname,r.rulename)
            FROM pg_rewrite r JOIN pg_class c ON c.oid=r.ev_class
            WHERE c.relnamespace=to_regnamespace('fvoci') AND r.rulename<>'_RETURN'
        UNION ALL SELECT format('default_privileges fvoci.%s',defaclobjtype) FROM pg_default_acl
            WHERE defaclnamespace=to_regnamespace('fvoci')
        ) named";

pub(super) async fn objects(conn: &mut PgConnection) -> Result<BTreeSet<String>> {
    Ok(serde_json::from_value(
        catalog::scalar(conn, OBJECTS_SQL).await?,
    )?)
}
