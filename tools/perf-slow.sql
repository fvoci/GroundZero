-- #210: replay the measured document-list and scalar hotspots on fresh T6 data.
-- psql variables: actor, item, document. App role and RLS remain active.
\pset pager off
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'actor', true);
\echo document list (100 rows), same SQL and bindings as the browser
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, fvoci.app_document_reference(id) AS reference, title, revision, updated_at,
       CASE WHEN fvoci.app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id
FROM fvoci.documents WHERE item_id=:'item'::uuid
ORDER BY position, number NULLS LAST, created_at, id;
\echo single document reference
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_document_reference(:'document'::uuid);
\echo single access call
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_access('document', :'document'::uuid);
\echo timezone validation
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_check_timezone('Asia/Seoul');
ROLLBACK;
