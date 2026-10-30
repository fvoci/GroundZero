-- #210 measurement only. psql variables: actor, item. No product DDL or policy changes.
\pset pager off
\echo schema and data shape
SELECT version();
SELECT version, success, encode(checksum, 'hex') AS checksum FROM fvoci._sqlx_migrations ORDER BY version;
SELECT 'documents' AS kind, count(*) FROM fvoci.documents
UNION ALL SELECT 'tasks', count(*) FROM fvoci.tasks
UNION ALL SELECT 'files', count(*) FROM fvoci.files
UNION ALL SELECT 'collab_states', count(*) FROM fvoci.document_collab_states;
SELECT tablename, indexname, indexdef FROM pg_indexes WHERE schemaname='fvoci' ORDER BY tablename, indexname;
SELECT organization_id AS org FROM fvoci.workspaces WHERE id=:'workspace'::uuid \gset
\echo cumulative function statistics (includes generation; per-row cost uses EXPLAIN below)
SELECT schemaname, funcname, calls, total_time, self_time FROM pg_stat_user_functions WHERE schemaname='fvoci' ORDER BY total_time DESC LIMIT 25;
\echo owner baseline and explicit per-row access cost, repeat this file three times
BEGIN;
SET LOCAL ROLE fvoci_migrator;
SELECT set_config('app.user_id', :'actor', true);
EXPLAIN (ANALYZE, BUFFERS) SELECT count(*) FROM fvoci.documents;
EXPLAIN (ANALYZE, BUFFERS) SELECT sum(length(fvoci.app_access('document', id))) FROM fvoci.documents;
EXPLAIN (ANALYZE, BUFFERS) SELECT count(*) FROM fvoci.documents WHERE item_id=:'item'::uuid;
\echo recent activity function body, same effective role as its SECURITY DEFINER
EXPLAIN (ANALYZE, BUFFERS)
SELECT a.id, a.kind, a.actor, CASE WHEN fvoci.app_shares_organization(a.actor) THEN u.display_name END,
       a.workspace_id, a.target_kind, a.target_id, a.item_id, a.detail, a.created_at
FROM fvoci.activity a LEFT JOIN fvoci.users u ON u.id=a.actor
WHERE a.workspace_id IN (SELECT fvoci.app_my_workspace_ids(:'org'::uuid))
  AND fvoci.app_access(a.target_kind, a.target_id) <> 'none'
ORDER BY a.created_at DESC, a.id DESC LIMIT 20;
ROLLBACK;
\echo aggregate statements under the app role (same arguments as the browser)
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'actor', true);
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_home(:'org'::uuid, 'Asia/Seoul');
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.app_my_tasks(:'org'::uuid, '2026-10-30'::date);
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_calendar(:'workspace'::uuid, '2026-09-28'::date, '2026-11-08'::date, 'Asia/Seoul');
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.app_recent_activity(:'org'::uuid, 20);
ROLLBACK;
\echo identical item read with the product RLS policy, app role
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'actor', true);
EXPLAIN (ANALYZE, BUFFERS) SELECT count(*) FROM fvoci.documents WHERE item_id=:'item'::uuid;
ROLLBACK;
