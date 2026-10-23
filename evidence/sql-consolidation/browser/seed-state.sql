BEGIN TRANSACTION READ ONLY;
SELECT jsonb_build_object(
  'migration_versions', (SELECT jsonb_agg(version ORDER BY version) FROM fvoci._sqlx_migrations),
  'seed_markers', (SELECT jsonb_object_agg(key, value) FROM fvoci.instance_state WHERE key LIKE 'dev_%'),
  'users', (SELECT count(*) FROM fvoci.users),
  'workspaces', (SELECT count(*) FROM fvoci.workspaces),
  'documents', (SELECT count(*) FROM fvoci.documents),
  'modules', (SELECT count(*) FROM fvoci.modules),
  'items', (SELECT count(*) FROM fvoci.items),
  'tasks', (SELECT count(*) FROM fvoci.tasks)
);
ROLLBACK;
