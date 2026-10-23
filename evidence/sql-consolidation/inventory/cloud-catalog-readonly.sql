\set ON_ERROR_STOP on
BEGIN TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY;
SET LOCAL statement_timeout = '10s';
SET LOCAL lock_timeout = '2s';

-- Connection identity only; never print connection URLs or credentials.
SELECT current_database() AS database_name,
       session_user AS session_role, current_user AS effective_role,
       current_setting('server_version_num')::integer AS server_version_num,
       current_setting('transaction_read_only') AS transaction_read_only;

SELECT nspname AS schema_name, pg_get_userbyid(nspowner) AS owner, nspacl::text AS acl
FROM pg_namespace
WHERE nspname NOT LIKE 'pg_%' AND nspname <> 'information_schema'
ORDER BY nspname;

SELECT n.nspname AS schema_name, c.relname AS object_name, c.relkind,
       pg_get_userbyid(c.relowner) AS owner, c.relrowsecurity, c.relforcerowsecurity,
       c.relacl::text AS acl
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'fvoci' OR pg_get_userbyid(c.relowner) IN ('fvoci_app', 'fvoci_migrator')
ORDER BY n.nspname, c.relkind, c.relname;

SELECT n.nspname AS schema_name, p.proname,
       pg_get_function_identity_arguments(p.oid) AS identity_arguments,
       pg_get_userbyid(p.proowner) AS owner, p.prosecdef, p.proconfig, p.proacl::text AS acl
FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
WHERE n.nspname = 'fvoci' OR pg_get_userbyid(p.proowner) IN ('fvoci_app', 'fvoci_migrator')
ORDER BY n.nspname, p.proname, identity_arguments;

SELECT rolname, rolsuper, rolinherit, rolcreaterole, rolcreatedb, rolcanlogin, rolbypassrls
FROM pg_roles WHERE rolname IN ('fvoci_app', 'fvoci_migrator', session_user, current_user)
ORDER BY rolname;

SELECT parent.rolname AS granted_role, member.rolname AS member_role,
       m.admin_option, m.inherit_option, m.set_option
FROM pg_auth_members m
JOIN pg_roles parent ON parent.oid = m.roleid
JOIN pg_roles member ON member.oid = m.member
WHERE parent.rolname IN ('fvoci_app', 'fvoci_migrator')
   OR member.rolname IN ('fvoci_app', 'fvoci_migrator')
ORDER BY parent.rolname, member.rolname;

-- Exact aggregate counts only; no user names, document text, or private data.
SELECT format('SELECT %L AS table_name, count(*) AS row_count FROM %I.%I;',
              c.relname, n.nspname, c.relname)
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = 'fvoci' AND c.relkind IN ('r', 'p')
ORDER BY c.relname
\gexec

SELECT format('SELECT %L AS ledger_schema, version, description, installed_on, success, encode(checksum, ''hex'') AS checksum FROM %I.%I ORDER BY version;',
              n.nspname, n.nspname, c.relname)
FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relname = '_sqlx_migrations' AND c.relkind IN ('r', 'p')
ORDER BY n.nspname
\gexec

-- Other active roles/connections are a sharing signal, not proof of exclusivity.
SELECT datname, usename, state, count(*) AS connection_count
FROM pg_stat_activity WHERE datname = current_database()
GROUP BY datname, usename, state ORDER BY datname, usename, state;

-- Dependencies must be reviewed before any approved object deletion.
SELECT pg_describe_object(d.classid, d.objid, d.objsubid) AS dependent_object,
       pg_describe_object(d.refclassid, d.refobjid, d.refobjsubid) AS referenced_object,
       d.deptype
FROM pg_depend d
WHERE d.refclassid = 'pg_namespace'::regclass
  AND d.refobjid = (SELECT oid FROM pg_namespace WHERE nspname = 'fvoci')
ORDER BY dependent_object, referenced_object;
ROLLBACK;
