//! Only catalog data is read here. The raw snapshot never leaves the process.

use super::query;
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sqlx::PgConnection;

pub(super) const SCOPE: &str = "WITH RECURSIVE scope(classid, objid) AS (
    SELECT 'pg_namespace'::regclass::oid, oid FROM pg_namespace WHERE nspname = 'fvoci'
    UNION
    SELECT d.classid, d.objid FROM pg_depend d JOIN scope s
      ON (d.refclassid, d.refobjid) = (s.classid, s.objid)
    WHERE d.deptype IN ('a', 'i') OR s.classid = 'pg_namespace'::regclass
), principals AS (
    SELECT r.oid FROM pg_roles r WHERE r.rolname IN ('fvoci_app','fvoci_migrator') OR (
      (r.rolsuper OR r.rolcreaterole) AND NOT EXISTS (SELECT FROM pg_roles p
        WHERE p.rolname IN ('fvoci_app','fvoci_migrator') AND NOT (r.rolsuper OR (
          pg_has_role(r.oid,p.oid,'MEMBER WITH ADMIN OPTION') AND pg_has_role(r.oid,p.oid,'SET')))))
) ";

pub(super) async fn scalar(conn: &mut PgConnection, query: &str) -> Result<Value> {
    Ok(sqlx::query_scalar(query).fetch_one(conn).await?)
}

async fn authority(conn: &mut PgConnection) -> Result<Value> {
    scalar(conn, query::AUTHORITY).await
}

pub(super) async fn write_authority(conn: &mut PgConnection) -> Result<()> {
    let a = authority(conn).await?;
    ensure!(
        [
            "create",
            "connect",
            "grant_connect",
            "createrole",
            "manageable",
            "set_role"
        ]
        .iter()
        .all(|k| a[k] == true),
        "authority_missing"
    );
    Ok(())
}

pub(super) async fn inspect_authority(conn: &mut PgConnection) -> Result<()> {
    let minimal: bool = sqlx::query_scalar(&format!("{SCOPE} SELECT NOT (rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb OR rolreplication)
        AND rolcanlogin AND NOT has_database_privilege(current_user,current_database(),'CREATE')
        AND NOT EXISTS (SELECT FROM pg_auth_members m JOIN pg_roles me ON me.oid=m.member WHERE me.rolname=current_user)
        AND NOT EXISTS (SELECT FROM pg_roles p WHERE p.rolname IN ('fvoci_app','fvoci_migrator')
          AND (pg_has_role(current_user,p.oid,'MEMBER') OR pg_has_role(current_user,p.oid,'SET')))
        AND NOT EXISTS (SELECT FROM pg_namespace n WHERE n.nspname='fvoci'
          AND (has_schema_privilege(current_user,n.oid,'USAGE') OR has_schema_privilege(current_user,n.oid,'CREATE')))
        AND NOT EXISTS (SELECT FROM pg_shdepend d JOIN scope s ON (s.classid,s.objid)=(d.classid,d.objid)
          WHERE d.dbid=(SELECT oid FROM pg_database WHERE datname=current_database()) AND d.deptype='o'
          AND d.refclassid='pg_authid'::regclass AND d.refobjid=me.oid)
        AND NOT EXISTS (SELECT FROM pg_class c WHERE c.relnamespace=to_regnamespace('fvoci') AND (
          (c.relkind IN ('r','p','v','m','f') AND (has_table_privilege(me.oid,c.oid,'SELECT,INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER,MAINTAIN')
            OR has_any_column_privilege(me.oid,c.oid,'SELECT,INSERT,UPDATE,REFERENCES')))
          OR (c.relkind='S' AND has_sequence_privilege(me.oid,c.oid,'USAGE,SELECT,UPDATE'))))
        AND NOT EXISTS (SELECT FROM pg_proc p WHERE p.pronamespace=to_regnamespace('fvoci') AND has_function_privilege(me.oid,p.oid,'EXECUTE')
          AND NOT (p.prokind='f' AND NOT p.prosecdef AND NOT p.proretset AND (
            (p.proname='app_normalize_key' AND p.pronargs=1 AND p.proargtypes[0]='text'::regtype AND p.prorettype='text'::regtype)
            OR (p.proname='app_local_instant' AND p.pronargs=2 AND p.proargtypes[0]='timestamp'::regtype AND p.proargtypes[1]='text'::regtype AND p.prorettype='timestamptz'::regtype))
            AND NOT EXISTS (SELECT FROM aclexplode(coalesce(p.proacl,acldefault('f',p.proowner))) a WHERE a.grantee=me.oid AND a.privilege_type='EXECUTE')))
        FROM pg_roles me WHERE rolname=current_user")).fetch_one(conn).await?;
    ensure!(minimal, "inspection_requires_minimal_catalog_reader");
    Ok(())
}

// Omit only planner statistics and freeze horizons. Derive lazy existence flags from
// authoritative rows so VACUUM cannot change them; retain the actual structural facts.
// https://www.postgresql.org/docs/17/catalog-pg-class.html
pub(super) const RELATION_ROW: &str = "(to_jsonb(c) - 'relpages' - 'reltuples' - 'relallvisible'
    - 'relallfrozen' - 'relfrozenxid' - 'relminmxid') || jsonb_build_object(
    'relhasindex', EXISTS (SELECT FROM pg_index WHERE indrelid=c.oid),
    'relhasrules', EXISTS (SELECT FROM pg_rewrite WHERE ev_class=c.oid),
    'relhastriggers', EXISTS (SELECT FROM pg_trigger WHERE tgrelid=c.oid),
    'relhassubclass', EXISTS (SELECT FROM pg_inherits WHERE inhparent=c.oid))";

pub(super) async fn snapshot(conn: &mut PgConnection) -> Result<Value> {
    scalar(conn, &format!("{} {}", SCOPE, query::snapshot())).await
}

pub(super) fn blockers(raw: &Value, counts: &Value) -> Result<Value> {
    let mut blockers = counts
        .as_object()
        .context("invalid_blocker_counts")?
        .clone();
    blockers.insert(
        "history_outside_product_schema".into(),
        json!(
            raw["ledgers"]
                .as_array()
                .is_none_or(|rows| rows.iter().any(|r| r["schema"] != "fvoci"))
        ),
    );
    blockers.insert(
        "postgres_unsupported".into(),
        json!(
            raw["authority"]["version"]
                .as_i64()
                .is_none_or(|v| v < 170_000)
        ),
    );
    Ok(Value::Object(blockers))
}
