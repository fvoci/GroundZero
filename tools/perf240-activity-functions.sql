-- #240: function-call plans can differ from the verbatim body plans. Run in
-- the held measurement database; restore the experimental global index at end.
\if :{?candidate}
\else
\set candidate perf240_recent_reference
\endif
SET ROLE fvoci_migrator;
SELECT set_config('app.user_id', :'actor', false);
SELECT format('EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.%I(%L::uuid,20)',
              :'candidate', :'organization') AS plan \gset
DROP INDEX IF EXISTS fvoci.activity_recent;
\echo function existing indexes
:plan;
:plan;
:plan;
CREATE INDEX activity_recent ON fvoci.activity(created_at DESC,id DESC);
\echo function global index
:plan;
:plan;
:plan;
DROP INDEX fvoci.activity_recent;
CREATE INDEX perf240_activity_workspace ON fvoci.activity(workspace_id,created_at DESC,id DESC);
\echo function workspace index
:plan;
:plan;
:plan;
DROP INDEX fvoci.perf240_activity_workspace;
CREATE INDEX activity_recent ON fvoci.activity(created_at DESC,id DESC);
RESET ROLE;
