//! Only catalog data is read here. The raw snapshot never leaves the process.

use anyhow::{Result, ensure};
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
    scalar(conn, "SELECT jsonb_build_object(
        'version', current_setting('server_version_num')::int,
        'user', current_user, 'database', current_database(),
        'create', has_database_privilege(current_user, current_database(), 'CREATE'),
        'connect', has_database_privilege(current_user, current_database(), 'CONNECT'),
        'grant_connect', has_database_privilege(current_user, current_database(), 'CONNECT WITH GRANT OPTION'),
        'createrole', (SELECT rolcreaterole OR rolsuper FROM pg_roles WHERE rolname = current_user),
        'manageable', NOT EXISTS (SELECT FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator')
          AND NOT ((SELECT rolsuper FROM pg_roles WHERE rolname = current_user)
            OR pg_has_role(current_user, oid, 'MEMBER WITH ADMIN OPTION'))),
        'set_role', NOT EXISTS (SELECT FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator')
          AND NOT pg_has_role(current_user, oid, 'SET')))").await
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
    let minimal: bool = sqlx::query_scalar("SELECT NOT (rolsuper OR rolbypassrls OR rolcreaterole OR rolcreatedb OR rolreplication)
        AND rolcanlogin AND NOT has_database_privilege(current_user,current_database(),'CREATE')
        AND NOT EXISTS (SELECT FROM pg_auth_members m JOIN pg_roles me ON me.oid=m.member WHERE me.rolname=current_user)
        AND NOT EXISTS (SELECT FROM pg_roles p WHERE p.rolname IN ('fvoci_app','fvoci_migrator')
          AND (pg_has_role(current_user,p.oid,'MEMBER') OR pg_has_role(current_user,p.oid,'SET')))
        AND NOT EXISTS (SELECT FROM pg_namespace n WHERE n.nspname='fvoci'
          AND (has_schema_privilege(current_user,n.oid,'USAGE') OR has_schema_privilege(current_user,n.oid,'CREATE')))
        FROM pg_roles WHERE rolname=current_user").fetch_one(conn).await?;
    ensure!(minimal, "inspection_requires_minimal_catalog_reader");
    Ok(())
}

pub(super) async fn role_state(conn: &mut PgConnection) -> Result<Value> {
    scalar(conn, &format!("{SCOPE} SELECT jsonb_build_object(
        'attributes',(SELECT coalesce(jsonb_agg(to_jsonb(r)-'rolpassword' ORDER BY oid),'[]') FROM pg_roles r WHERE rolname IN ('fvoci_app','fvoci_migrator')),
        'memberships',(SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY roleid,member,grantor),'[]') FROM pg_auth_members m
          WHERE roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
          OR member IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
          OR grantor IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))),
        'comments',(SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY objoid),'[]') FROM pg_shdescription c WHERE classoid='pg_authid'::regclass
          AND objoid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))),
        'labels',(SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY to_jsonb(c)::text),'[]') FROM pg_shseclabel c WHERE classoid='pg_authid'::regclass
          AND objoid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))),
        'settings',(SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY setdatabase,setrole),'[]') FROM pg_db_role_setting c
          WHERE setrole IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))),
        'database_acl',(SELECT jsonb_build_object('owner',datdba,'acl',datacl) FROM pg_database WHERE datname=current_database()),
        'external_dependencies',(SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY to_jsonb(d)::text),'[]') FROM pg_shdepend d
          WHERE refclassid='pg_authid'::regclass AND refobjid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
          AND NOT (d.dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
            AND EXISTS (SELECT FROM scope s WHERE (s.classid,s.objid)=(d.classid,d.objid)))))")).await
}

// Omit only planner statistics and freeze horizons. Derive lazy existence flags from
// authoritative rows so VACUUM cannot change them; retain the actual structural facts.
// https://www.postgresql.org/docs/17/catalog-pg-class.html
const RELATION_ROW: &str = "(to_jsonb(c) - 'relpages' - 'reltuples' - 'relallvisible'
    - 'relallfrozen' - 'relfrozenxid' - 'relminmxid') || jsonb_build_object(
    'relhasindex', EXISTS (SELECT FROM pg_index WHERE indrelid=c.oid),
    'relhasrules', EXISTS (SELECT FROM pg_rewrite WHERE ev_class=c.oid),
    'relhastriggers', EXISTS (SELECT FROM pg_trigger WHERE tgrelid=c.oid),
    'relhassubclass', EXISTS (SELECT FROM pg_inherits WHERE inhparent=c.oid))";

async fn objects(conn: &mut PgConnection) -> Result<Value> {
    let mut objects = serde_json::Map::new();
    // Include definitions, owners, ACLs, columns, policies, constraints and defaults.
    for catalog in [
        "pg_namespace",
        "pg_class",
        "pg_proc",
        "pg_type",
        "pg_constraint",
        "pg_attribute",
        "pg_attrdef",
        "pg_trigger",
        "pg_rewrite",
        "pg_policy",
        "pg_default_acl",
    ] {
        let row = if catalog == "pg_class" {
            RELATION_ROW
        } else {
            "to_jsonb(c)"
        };
        let (class, key) = if catalog == "pg_attribute" {
            ("pg_class", "c.attrelid")
        } else {
            (catalog, "c.oid")
        };
        let rows = scalar(
            conn,
            &format!(
                "{SCOPE} SELECT coalesce(jsonb_agg(({row}) ORDER BY ({row})::text), '[]')
            FROM {catalog} c WHERE EXISTS (SELECT FROM scope s WHERE s.classid = '{class}'::regclass
            AND s.objid = {key})"
            ),
        )
        .await?;
        objects.insert(catalog.to_owned(), rows);
    }
    // These definitions are attached to scoped objects rather than namespace members.
    for (catalog, predicate) in [
        (
            "pg_enum",
            "s.classid='pg_type'::regclass AND s.objid=c.enumtypid",
        ),
        (
            "pg_range",
            "s.classid='pg_type'::regclass AND s.objid=c.rngtypid",
        ),
        (
            "pg_index",
            "s.classid='pg_class'::regclass AND s.objid=c.indexrelid",
        ),
        (
            "pg_sequence",
            "s.classid='pg_class'::regclass AND s.objid=c.seqrelid",
        ),
        (
            "pg_inherits",
            "s.classid='pg_class'::regclass AND s.objid IN (c.inhrelid,c.inhparent)",
        ),
        (
            "pg_description",
            "s.classid=c.classoid AND s.objid=c.objoid",
        ),
        (
            "pg_partitioned_table",
            "s.classid='pg_class'::regclass AND s.objid=c.partrelid",
        ),
        ("pg_init_privs", "s.classid=c.classoid AND s.objid=c.objoid"),
        ("pg_seclabel", "s.classid=c.classoid AND s.objid=c.objoid"),
    ] {
        objects.insert(
            catalog.to_owned(),
            scalar(
                conn,
                &format!(
                    "{SCOPE}
            SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY to_jsonb(c)::text),'[]')
            FROM {catalog} c WHERE EXISTS (SELECT FROM scope s WHERE {predicate})"
                ),
            )
            .await?,
        );
    }
    Ok(Value::Object(objects))
}

pub(super) async fn snapshot(conn: &mut PgConnection) -> Result<Value> {
    let authority = authority(conn).await?;
    let roles = scalar(
        conn,
        &format!("{SCOPE} SELECT coalesce(jsonb_agg(jsonb_build_object(
        'oid', oid, 'name', rolname, 'super', rolsuper, 'bypass', rolbypassrls,
        'create_role', rolcreaterole, 'create_db', rolcreatedb, 'login', rolcanlogin,
        'inherit', rolinherit, 'config', rolconfig, 'connections',rolconnlimit,
        'valid_until',rolvaliduntil, 'replication',rolreplication) ORDER BY oid), '[]') FROM pg_roles
        WHERE rolname IN ('fvoci_app','fvoci_migrator') OR oid IN
          (SELECT member FROM pg_auth_members WHERE roleid IN
            (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator')))
          OR oid IN (SELECT d.refobjid FROM pg_shdepend d JOIN scope s ON (s.classid,s.objid)=(d.classid,d.objid)
            WHERE d.refclassid='pg_authid'::regclass AND d.dbid=(SELECT oid FROM pg_database WHERE datname=current_database()))"),
    )
    .await?;
    let memberships = scalar(conn, "SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY roleid, member, grantor), '[]')
        FROM pg_auth_members m WHERE roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
        OR member IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))").await?;
    let shared = scalar(conn, "SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY dbid, classid, objid, objsubid, deptype), '[]')
        FROM pg_shdepend d WHERE refclassid = 'pg_authid'::regclass AND refobjid IN
        (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))").await?;
    let dependencies = scalar(conn, &format!("{SCOPE} SELECT coalesce(jsonb_agg(to_jsonb(d)
        ORDER BY d.classid, d.objid, d.objsubid, d.refclassid, d.refobjid, d.refobjsubid, d.deptype), '[]')
        FROM pg_depend d WHERE EXISTS (SELECT FROM scope s WHERE
        (s.classid, s.objid) IN ((d.classid,d.objid),(d.refclassid,d.refobjid)))")).await?;
    let ledgers = scalar(conn, "SELECT coalesce(jsonb_agg(jsonb_build_object('oid', c.oid,
        'schema', n.nspname, 'owner', c.relowner) ORDER BY c.oid), '[]')
        FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relname = '_sqlx_migrations'").await?;
    let database = scalar(conn, "SELECT to_jsonb(d) - 'datfrozenxid' - 'datminmxid' FROM pg_database d WHERE datname = current_database()").await?;
    let role_state = role_state(conn).await?;
    Ok(
        json!({"authority": authority, "roles": roles, "memberships": memberships, "shared": shared,
        "dependencies": dependencies, "objects": objects(conn).await?, "ledgers": ledgers, "database": database,"role_state":role_state}),
    )
}

const COUNTS: &[(&str, &str)] = &[
    ("unsupported_catalog_objects", "SELECT count(*) FROM scope WHERE classid NOT IN
        ('pg_namespace'::regclass,'pg_class'::regclass,'pg_proc'::regclass,'pg_type'::regclass,
         'pg_constraint'::regclass,'pg_attrdef'::regclass,'pg_trigger'::regclass,'pg_rewrite'::regclass,
         'pg_policy'::regclass,'pg_default_acl'::regclass)"),
    ("external_dependents", "SELECT count(*) FROM pg_depend d
        JOIN scope s ON (s.classid,s.objid) = (d.refclassid,d.refobjid)
        WHERE NOT EXISTS (SELECT FROM scope x WHERE (x.classid,x.objid) = (d.classid,d.objid))"),
    ("outside_role_dependencies", "SELECT count(*) FROM pg_shdepend d
        WHERE refclassid = 'pg_authid'::regclass AND refobjid IN
          (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
        AND NOT (d.dbid = (SELECT oid FROM pg_database WHERE datname = current_database())
          AND EXISTS (SELECT FROM scope s WHERE (s.classid,s.objid) = (d.classid,d.objid)))
        AND NOT (d.dbid = 0 AND d.classid = 'pg_database'::regclass
          AND d.objid = (SELECT oid FROM pg_database WHERE datname = current_database()) AND d.deptype = 'a')"),
    ("shared_memberships", "SELECT count(*) FROM pg_auth_members m
        WHERE (roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
            AND NOT (m.admin_option AND EXISTS (SELECT FROM pg_roles a WHERE a.oid=m.member AND (a.rolsuper OR a.rolcreaterole))))
        OR member IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
        OR grantor IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))"),
    ("foreign_owners", "SELECT count(*) FROM pg_shdepend d JOIN scope s
        ON (s.classid,s.objid) = (d.classid,d.objid)
        WHERE d.dbid = (SELECT oid FROM pg_database WHERE datname=current_database())
        AND d.deptype = 'o' AND d.refobjid NOT IN
          (SELECT oid FROM principals)"),
    ("extension_members", "SELECT count(*) FROM pg_depend d
        JOIN scope s ON (s.classid,s.objid) = (d.classid,d.objid) WHERE d.deptype = 'e'"),
    ("outside_automatic_objects", "SELECT count(*) FROM scope s WHERE
        (s.classid = 'pg_class'::regclass AND EXISTS (SELECT FROM pg_class c JOIN pg_namespace n
            ON n.oid=c.relnamespace WHERE c.oid=s.objid AND n.nspname NOT IN ('fvoci','pg_toast')))
        OR (s.classid = 'pg_proc'::regclass AND EXISTS (SELECT FROM pg_proc p JOIN pg_namespace n
            ON n.oid=p.pronamespace WHERE p.oid=s.objid AND n.nspname <> 'fvoci'))
        OR (s.classid = 'pg_type'::regclass AND EXISTS (SELECT FROM pg_type t JOIN pg_namespace n
            ON n.oid=t.typnamespace WHERE t.oid=s.objid AND n.nspname NOT IN ('fvoci','pg_toast')))"),
    ("foreign_grants", "SELECT count(*) FROM pg_shdepend d JOIN scope s
        ON (s.classid,s.objid) = (d.classid,d.objid)
        WHERE d.dbid = (SELECT oid FROM pg_database WHERE datname=current_database()) AND d.deptype IN ('a','r')
        AND d.refclassid = 'pg_authid'::regclass AND d.refobjid NOT IN
          (SELECT oid FROM principals)"),
    ("unsupported_objects", "SELECT
        (SELECT count(*) FROM pg_class WHERE relnamespace = to_regnamespace('fvoci') AND relkind NOT IN ('r','p','v','m','S','i','I','t')) +
        (SELECT count(*) FROM pg_proc WHERE pronamespace = to_regnamespace('fvoci') AND prokind NOT IN ('f','p')) +
        (SELECT count(*) FROM pg_type t WHERE typnamespace = to_regnamespace('fvoci')
            AND (typtype NOT IN ('b','c','d','e') OR (typtype='c' AND EXISTS (SELECT FROM pg_class WHERE oid=t.typrelid AND relkind='c'))
            OR (typtype='b' AND NOT EXISTS (SELECT FROM pg_type parent WHERE parent.typarray=t.oid))))"),
];

pub(super) async fn blockers(conn: &mut PgConnection, raw: &Value) -> Result<Value> {
    let mut blockers = serde_json::Map::new();
    for (key, query) in COUNTS {
        let count: i64 = sqlx::query_scalar(&format!("{SCOPE}{query}"))
            .fetch_one(&mut *conn)
            .await?;
        blockers.insert((*key).to_owned(), json!(count));
    }
    blockers.insert(
        "privileged_product_role".into(),
        scalar(
            conn,
            "SELECT to_jsonb(EXISTS (SELECT FROM pg_roles WHERE
        rolname IN ('fvoci_app','fvoci_migrator') AND (rolsuper OR rolbypassrls OR rolcreaterole
        OR (rolname = 'fvoci_app' AND rolcreatedb))))",
        )
        .await?,
    );
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
