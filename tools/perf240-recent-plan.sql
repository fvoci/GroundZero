-- #240: original SQL function versus scope-first cursor, same caller and limit.
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'actor', true);
\echo original function with candidate a index
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.perf240_recent_reference(:'organization'::uuid,20);
\echo cursor function with candidate a index
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.app_recent_activity(:'organization'::uuid,20);
ROLLBACK;
