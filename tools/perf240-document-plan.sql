-- #240 / #245: unchanged product document list, application role and RLS enabled.
SELECT pg_stat_statements_reset();
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'owner', true);
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, fvoci.app_document_reference(id) AS reference, title, revision, updated_at,
       CASE WHEN fvoci.app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id
FROM fvoci.documents WHERE item_id=:'item'::uuid
ORDER BY position, number NULLS LAST, created_at, id;
ROLLBACK;
SELECT calls,total_exec_time,mean_exec_time,shared_blks_hit,query
FROM pg_stat_statements WHERE dbid=(SELECT oid FROM pg_database WHERE datname=current_database())
ORDER BY total_exec_time DESC LIMIT 25;
