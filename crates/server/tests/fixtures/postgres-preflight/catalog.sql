-- Read-only before/after evidence for a fresh, isolated rejection fixture. No password hashes.
SELECT jsonb_build_object(
  'server_version_num', current_setting('server_version_num')::integer,
  'roles', (SELECT coalesce(jsonb_agg(to_jsonb(r) ORDER BY rolname), '[]')
            FROM pg_roles r WHERE rolname IN ('fvoci_app', 'fvoci_migrator')),
  'memberships', (SELECT coalesce(jsonb_agg(jsonb_build_object(
      'role', pg_get_userbyid(roleid), 'member', pg_get_userbyid(member),
      'grantor', pg_get_userbyid(grantor), 'admin', admin_option,
      'inherit', inherit_option, 'set', set_option)
      ORDER BY roleid, member, grantor), '[]') FROM pg_auth_members),
  'schema', (SELECT to_jsonb(s) FROM
      (SELECT nspname, pg_get_userbyid(nspowner) AS owner, nspacl
       FROM pg_namespace WHERE nspname = 'fvoci') s),
  'relations', (SELECT coalesce(jsonb_agg(jsonb_build_object(
      'name', c.relname, 'kind', c.relkind, 'owner', pg_get_userbyid(c.relowner),
      'acl', c.relacl) ORDER BY c.relname), '[]')
      FROM pg_class c JOIN pg_namespace n ON c.relnamespace=n.oid WHERE n.nspname='fvoci'),
  'ledger', to_regclass('fvoci._sqlx_migrations')::text,
  'database_acl', (SELECT datacl FROM pg_database WHERE datname=current_database()),
  'languages', (SELECT jsonb_agg(jsonb_build_object('name', lanname, 'trusted', lanpltrusted,
      'acl', lanacl) ORDER BY lanname) FROM pg_language WHERE lanname IN ('sql', 'plpgsql')),
  'uuid_function', (SELECT jsonb_build_object('name', p.oid::regprocedure::text, 'acl', proacl)
      FROM pg_proc p WHERE p.oid=to_regprocedure('pg_catalog.uuid_extract_version(uuid)'))
);
