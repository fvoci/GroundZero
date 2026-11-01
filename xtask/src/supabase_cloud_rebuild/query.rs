//! SQL fragments composed into one statement; every inventory field shares its command snapshot.

use super::{allowed, catalog, consumers, deletion_guard};

pub(super) const AUTHORITY: &str = "SELECT jsonb_build_object(
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
          AND NOT pg_has_role(current_user, oid, 'SET')))";

pub(super) const ROLE_STATE: &str = "SELECT jsonb_build_object(
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
            AND EXISTS (SELECT FROM scope s WHERE (s.classid,s.objid)=(d.classid,d.objid)))))";

pub(super) const ROLES: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object(
        'oid', oid, 'name', rolname, 'super', rolsuper, 'bypass', rolbypassrls,
        'create_role', rolcreaterole, 'create_db', rolcreatedb, 'login', rolcanlogin,
        'inherit', rolinherit, 'config', rolconfig, 'connections',rolconnlimit,
        'valid_until',rolvaliduntil, 'replication',rolreplication) ORDER BY oid), '[]') FROM pg_roles
        WHERE rolname IN ('fvoci_app','fvoci_migrator') OR oid IN
          (SELECT member FROM pg_auth_members WHERE roleid IN
            (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator')))
          OR oid IN (SELECT d.refobjid FROM pg_shdepend d JOIN scope s ON (s.classid,s.objid)=(d.classid,d.objid)
            WHERE d.refclassid='pg_authid'::regclass AND d.dbid=(SELECT oid FROM pg_database WHERE datname=current_database()))";

pub(super) const MEMBERSHIPS: &str = "SELECT coalesce(jsonb_agg(to_jsonb(m) ORDER BY roleid, member, grantor), '[]')
        FROM pg_auth_members m WHERE roleid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))
        OR member IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))";

pub(super) const SHARED: &str =
    "SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY dbid, classid, objid, objsubid, deptype), '[]')
        FROM pg_shdepend d WHERE refclassid = 'pg_authid'::regclass AND refobjid IN
        (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))";

pub(super) const DEPENDENCIES: &str = "SELECT coalesce(jsonb_agg(to_jsonb(d)
        ORDER BY d.classid, d.objid, d.objsubid, d.refclassid, d.refobjid, d.refobjsubid, d.deptype), '[]')
        FROM pg_depend d WHERE EXISTS (SELECT FROM scope s WHERE
        (s.classid, s.objid) IN ((d.classid,d.objid),(d.refclassid,d.refobjid)))";

pub(super) const LEDGERS: &str = "SELECT coalesce(jsonb_agg(jsonb_build_object('oid', c.oid,
        'schema', n.nspname, 'owner', c.relowner) ORDER BY c.oid), '[]')
        FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace WHERE c.relname = '_sqlx_migrations'";

pub(super) const DATABASE: &str = "SELECT to_jsonb(d) - 'datfrozenxid' - 'datminmxid' FROM pg_database d WHERE datname = current_database()";

pub(super) const PRIVILEGED: &str = "SELECT to_jsonb(EXISTS (SELECT FROM pg_roles WHERE
        rolname IN ('fvoci_app','fvoci_migrator') AND (rolsuper OR rolbypassrls OR rolcreaterole
        OR (rolname = 'fvoci_app' AND rolcreatedb))))";

pub(super) const DROPS: &str = "SELECT owner, statement FROM (
        SELECT pg_get_userbyid(c.relowner)::text owner,
          format('ALTER TABLE %I.%I DROP CONSTRAINT %I RESTRICT', n.nspname,c.relname,k.conname) statement
          FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace
          WHERE n.nspname='fvoci' AND k.contype='f' AND k.conparentid=0
        UNION ALL SELECT pg_get_userbyid(c.relowner)::text,
          format('DROP %s IF EXISTS %I.%I RESTRICT', CASE c.relkind WHEN 'v' THEN 'VIEW' WHEN 'm' THEN 'MATERIALIZED VIEW' WHEN 'S' THEN 'SEQUENCE' ELSE 'TABLE' END,n.nspname,c.relname)
          FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='fvoci' AND c.relkind IN ('r','p','v','m','S')
        UNION ALL SELECT pg_get_userbyid(p.proowner)::text,
          format('DROP ROUTINE %I.%I(%s) RESTRICT',n.nspname,p.proname,pg_get_function_identity_arguments(p.oid))
          FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname='fvoci' AND p.prokind IN ('f','p')
        UNION ALL SELECT pg_get_userbyid(t.typowner)::text,
          format('DROP %s %I.%I RESTRICT',CASE WHEN t.typtype='d' THEN 'DOMAIN' ELSE 'TYPE' END,n.nspname,t.typname)
          FROM pg_type t JOIN pg_namespace n ON n.oid=t.typnamespace WHERE n.nspname='fvoci'
          AND t.typtype IN ('e','d') AND NOT EXISTS (SELECT FROM pg_type parent WHERE parent.typarray=t.oid)
        UNION ALL SELECT pg_get_userbyid(nspowner)::text, 'DROP SCHEMA fvoci RESTRICT' FROM pg_namespace WHERE nspname='fvoci'
        ) operations ORDER BY statement";

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

fn object(fields: impl IntoIterator<Item = (String, String)>) -> String {
    let fields = fields
        .into_iter()
        .map(|(key, sql)| format!("'{key}',({sql})"))
        .collect::<Vec<_>>()
        .join(",");
    format!("SELECT jsonb_build_object({fields})")
}

fn objects() -> String {
    let mut fields = Vec::new();
    for name in [
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
        let row = if name == "pg_class" {
            catalog::RELATION_ROW
        } else {
            "to_jsonb(c)"
        };
        let (class, key) = if name == "pg_attribute" {
            ("pg_class", "c.attrelid")
        } else {
            (name, "c.oid")
        };
        fields.push((name.to_owned(),format!("SELECT coalesce(jsonb_agg(({row}) ORDER BY ({row})::text),'[]') FROM {name} c WHERE EXISTS (SELECT FROM scope s WHERE s.classid='{class}'::regclass AND s.objid={key})")));
    }
    for (name, predicate) in [
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
        fields.push((name.to_owned(),format!("SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY to_jsonb(c)::text),'[]') FROM {name} c WHERE EXISTS (SELECT FROM scope s WHERE {predicate})")));
    }
    object(fields)
}

pub(super) fn snapshot() -> String {
    let mut fields = [
        ("authority", AUTHORITY),
        ("roles", ROLES),
        ("memberships", MEMBERSHIPS),
        ("shared", SHARED),
        ("dependencies", DEPENDENCIES),
        ("ledgers", LEDGERS),
        ("database", DATABASE),
        ("role_state", ROLE_STATE),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect::<Vec<_>>();
    fields.push(("objects".to_owned(), objects()));
    fields.push((
        "drop_targets".to_owned(),
        deletion_guard::TARGETS_SQL.to_owned(),
    ));
    fields.push(("consumers".to_owned(), consumers::SQL.to_owned()));
    object(fields)
}

fn counts() -> String {
    let mut fields = COUNTS
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect::<Vec<_>>();
    fields.push(("privileged_product_role".to_owned(), PRIVILEGED.to_owned()));
    object(fields)
}

pub(super) fn inventory() -> String {
    let raw=object([
        ("catalog".to_owned(),snapshot()),("counts".to_owned(),counts()),
        ("identities".to_owned(),allowed::OBJECTS_SQL.to_owned()),
        ("drops".to_owned(),format!("SELECT coalesce(jsonb_agg(jsonb_build_array(owner,statement) ORDER BY statement),'[]') FROM ({DROPS}) operations")),
        ("roles".to_owned(),"SELECT coalesce(jsonb_agg(rolname::text ORDER BY rolname),'[]') FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator')".to_owned()),
    ]);
    format!(
        "{} , scan_gate AS MATERIALIZED (SELECT CASE WHEN $1::bigint IS NULL THEN NULL ELSE pg_advisory_xact_lock($1) END) {raw} FROM scan_gate",
        catalog::SCOPE
    )
}

pub(super) fn role_recheck() -> String {
    format!("{} {}",catalog::SCOPE,object([
        ("state".to_owned(),ROLE_STATE.to_owned()),
        ("owned".to_owned(),"SELECT count(*) FROM pg_shdepend WHERE deptype='o' AND refclassid='pg_authid'::regclass AND refobjid IN (SELECT oid FROM pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator'))".to_owned()),
    ]))
}
