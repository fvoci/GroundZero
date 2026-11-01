//! Compare catalog object addresses before/after DROP without reading object contents.

use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use sqlx::PgConnection;

// The third component addresses live user columns; whole objects have subid=0.
pub(super) type Addresses = BTreeSet<(i64, i64, i32)>;

// Capture this in the same statement/snapshot as the approved catalog, never from a later scope.
pub(super) const TARGETS_SQL: &str = "SELECT coalesce(jsonb_agg(jsonb_build_array(classid,objid,subid) ORDER BY classid,objid,subid),'[]') FROM (
 SELECT classid::bigint,objid::bigint,0::int subid FROM scope
 UNION SELECT 'pg_enum'::regclass::bigint,e.oid::bigint,0 FROM pg_enum e JOIN scope s ON s.classid='pg_type'::regclass AND s.objid=e.enumtypid
 UNION SELECT 'pg_class'::regclass::bigint,a.attrelid::bigint,a.attnum::int FROM pg_attribute a JOIN scope s
  ON s.classid='pg_class'::regclass AND s.objid=a.attrelid WHERE a.attnum>0 AND NOT a.attisdropped
) addresses";

pub(super) struct Guard {
    query: String,
    targets: Addresses,
    outside: Addresses,
}

impl Guard {
    pub async fn capture(conn: &mut PgConnection, targets: Addresses) -> Result<Self> {
        // Discover core OID catalogs rather than silently omitting classes added by a PG version.
        // Public views expose authid/user_mapping IDs without password hashes or mapping options.
        let catalogs: Vec<(String, bool)> = sqlx::query_as(
            "SELECT format('SELECT %s::bigint,%I::bigint,0::int FROM pg_catalog.%I',c.oid,
                CASE c.relname WHEN 'pg_user_mapping' THEN 'umid' ELSE 'oid' END,
                CASE c.relname WHEN 'pg_authid' THEN 'pg_roles' WHEN 'pg_user_mapping' THEN 'pg_user_mappings' ELSE c.relname END),
             has_column_privilege(current_user,CASE c.relname WHEN 'pg_authid' THEN 'pg_catalog.pg_roles'::regclass
                WHEN 'pg_user_mapping' THEN 'pg_catalog.pg_user_mappings'::regclass ELSE c.oid END,
                CASE c.relname WHEN 'pg_user_mapping' THEN 'umid' ELSE 'oid' END,'SELECT')
             FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace JOIN pg_attribute a ON a.attrelid=c.oid
             WHERE n.nspname='pg_catalog' AND c.relkind='r' AND a.attname='oid' AND a.atttypid='oid'::regtype AND NOT a.attisdropped ORDER BY c.oid",
        ).fetch_all(&mut *conn).await?;
        ensure!(!catalogs.is_empty(), "missing_oid_catalogs");
        let mut selects = Vec::new();
        for (select, readable) in catalogs {
            ensure!(readable, "object_oid_catalog_unreadable");
            // Identifiers are quoted by PostgreSQL format(%I), never supplied by workflow input.
            selects.push(select);
        }
        selects.push("SELECT 'pg_class'::regclass::bigint,attrelid::bigint,attnum::int FROM pg_attribute WHERE attnum>0 AND NOT attisdropped".to_owned());
        let query = selects.join(" UNION ALL ");
        let before = read(conn, &query).await?;
        ensure!(targets.is_subset(&before), "drop_target_missing_before");
        let outside = before.difference(&targets).copied().collect();
        Ok(Self {
            query,
            targets,
            outside,
        })
    }

    pub async fn verify(self, conn: &mut PgConnection) -> Result<()> {
        let after = read(conn, &self.query).await?;
        ensure!(self.targets.is_disjoint(&after), "drop_targets_remain");
        ensure!(self.outside.is_subset(&after), "unapproved_object_deleted");
        Ok(())
    }
}

async fn read(conn: &mut PgConnection, query: &str) -> Result<Addresses> {
    let rows: Vec<(i64, i64, i32)> = sqlx::query_as(query).fetch_all(conn).await?;
    Ok(rows.into_iter().collect())
}
