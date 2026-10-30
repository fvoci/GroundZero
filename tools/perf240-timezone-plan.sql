-- #240: scalar validation only, same application role and canonical input.
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'owner', true);
\echo original pg_timezone_names check
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_check_timezone('Asia/Seoul');
\echo parser candidate (decision required)
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.perf240_timezone_candidate('Asia/Seoul');
ROLLBACK;
