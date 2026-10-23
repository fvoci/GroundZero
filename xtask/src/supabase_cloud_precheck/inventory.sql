-- Read-only metadata for a specific recreation approval, never application row contents or role passwords.
WITH schemas AS (
  SELECT n.oid, n.nspname, n.nspowner::regrole::text AS owner, coalesce(n.nspacl::text,'') AS acl
  FROM pg_catalog.pg_namespace n WHERE n.nspname !~ '^pg_' AND n.nspname <> 'information_schema'
), relations AS (
  SELECT n.nspname AS schema, c.relname AS name, c.relkind AS kind,
    c.relowner::regrole::text AS owner, c.relrowsecurity AS rls, c.relforcerowsecurity AS force_rls,
    coalesce(c.relacl::text, '') AS acl,
    (SELECT count(*) FROM pg_catalog.pg_policy p WHERE p.polrelid=c.oid) AS policy_count,
    (SELECT count(*) FROM pg_catalog.pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal) AS trigger_count
  FROM pg_catalog.pg_class c JOIN schemas n ON n.oid=c.relnamespace
  WHERE n.nspname IN ('fvoci','public') OR c.relowner::regrole::text IN ('fvoci_app','fvoci_migrator')
), functions AS (
  SELECT n.nspname AS schema, p.oid::regprocedure::text AS signature,
    p.proowner::regrole::text AS owner, p.prosecdef AS security_definer, coalesce(p.proacl::text,'') AS acl
  FROM pg_catalog.pg_proc p JOIN schemas n ON n.oid=p.pronamespace
  WHERE n.nspname='fvoci' OR p.proowner::regrole::text IN ('fvoci_app','fvoci_migrator')
), types AS (
  SELECT n.nspname AS schema, t.typname AS name, t.typtype AS kind,
    t.typowner::regrole::text AS owner, coalesce(t.typacl::text,'') AS acl
  FROM pg_catalog.pg_type t JOIN schemas n ON n.oid=t.typnamespace
  WHERE n.nspname='fvoci' OR t.typowner::regrole::text IN ('fvoci_app','fvoci_migrator')
), default_privileges AS (
  SELECT d.defaclrole::regrole::text AS role, n.nspname AS schema,
    d.defaclobjtype AS object_type, d.defaclacl::text AS acl
  FROM pg_catalog.pg_default_acl d LEFT JOIN pg_catalog.pg_namespace n ON n.oid=d.defaclnamespace
  WHERE n.nspname='fvoci' OR d.defaclrole::regrole::text IN ('fvoci_app','fvoci_migrator')
), cross_schema_dependencies AS (
  SELECT dependent.type AS dependent_type, dependent.schema AS dependent_schema,
    dependent.identity AS dependent_identity, referenced.type AS referenced_type,
    referenced.schema AS referenced_schema, referenced.identity AS referenced_identity, d.deptype AS kind
  FROM pg_catalog.pg_depend d
  CROSS JOIN LATERAL pg_catalog.pg_identify_object(d.classid,d.objid,d.objsubid) dependent
  CROSS JOIN LATERAL pg_catalog.pg_identify_object(d.refclassid,d.refobjid,d.refobjsubid) referenced
  WHERE dependent.schema IS DISTINCT FROM referenced.schema
    AND ((referenced.schema='fvoci' AND dependent.schema IS DISTINCT FROM 'pg_catalog')
      OR (dependent.schema='fvoci' AND referenced.schema IS NOT NULL
          AND referenced.schema NOT IN ('fvoci','pg_catalog','information_schema')))
)
SELECT jsonb_build_object(
 'identity', jsonb_build_object('database',current_database(),'current_user',current_user,'session_user',session_user,
    'server_version_num',current_setting('server_version_num')::int,'server_version',current_setting('server_version'),
    'server_address',inet_server_addr()::text,'server_port',inet_server_port(),
    'transaction_read_only',current_setting('transaction_read_only'),'transaction_isolation',current_setting('transaction_isolation')),
 'roles', (SELECT coalesce(jsonb_agg(jsonb_build_object('name',rolname,'superuser',rolsuper,'createrole',rolcreaterole,
    'createdb',rolcreatedb,'login',rolcanlogin,'bypassrls',rolbypassrls,'inherit',rolinherit) ORDER BY rolname),'[]'::jsonb)
    FROM pg_catalog.pg_roles WHERE rolname IN ('fvoci_app','fvoci_migrator',current_user)),
 'memberships', (SELECT coalesce(jsonb_agg(jsonb_build_object('role',roleid::regrole::text,'member',member::regrole::text,
    'grantor',grantor::regrole::text,'admin',admin_option,'inherit',inherit_option,'set',set_option)
    ORDER BY roleid::regrole::text,member::regrole::text),'[]'::jsonb)
    FROM pg_catalog.pg_auth_members WHERE roleid::regrole::text IN ('fvoci_app','fvoci_migrator')
      OR member::regrole::text IN ('fvoci_app','fvoci_migrator')),
 'schemas', (SELECT coalesce(jsonb_agg(jsonb_build_object('name',nspname,'owner',owner,'acl',acl,
    'relation_count',(SELECT count(*) FROM pg_catalog.pg_class c WHERE c.relnamespace=s.oid),
    'function_count',(SELECT count(*) FROM pg_catalog.pg_proc p WHERE p.pronamespace=s.oid)) ORDER BY nspname),'[]'::jsonb) FROM schemas s),
 'relations', (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY schema,name),'[]'::jsonb) FROM relations r),
 'functions', (SELECT coalesce(jsonb_agg(to_jsonb(f) ORDER BY schema,signature),'[]'::jsonb) FROM functions f),
 'types', (SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY schema,name),'[]'::jsonb) FROM types t),
 'default_privileges', (SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY role,schema,object_type),'[]'::jsonb) FROM default_privileges d),
 'cross_schema_dependencies', (SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY dependent_identity,referenced_identity),'[]'::jsonb) FROM cross_schema_dependencies d),
 'connections_by_role', (SELECT coalesce(jsonb_agg(to_jsonb(a) ORDER BY usename),'[]'::jsonb)
    FROM (SELECT usename,count(*) FROM pg_catalog.pg_stat_activity WHERE datname=current_database() GROUP BY usename) a)
);
