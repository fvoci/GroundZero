-- #240 / #245: index-only alternatives, unchanged activity result predicate.
-- Run BEFORE installing candidate (a). Trial indexes exist only in this database.
\pset pager off
SET ROLE fvoci_migrator;
SELECT set_config('app.user_id', :'actor', false);
SELECT workspace_id,count(*) FROM fvoci.activity GROUP BY workspace_id ORDER BY workspace_id;
SELECT count(*) AS caller_workspaces FROM fvoci.app_my_workspace_ids(:'organization'::uuid);
SELECT format($query$
EXPLAIN (ANALYZE, BUFFERS)
SELECT a.id,a.kind,a.actor,CASE WHEN fvoci.app_shares_organization(a.actor) THEN u.display_name END,
       a.workspace_id,a.target_kind,a.target_id,a.item_id,a.detail,a.created_at
FROM fvoci.activity a LEFT JOIN fvoci.users u ON u.id=a.actor
WHERE a.workspace_id IN (SELECT fvoci.app_my_workspace_ids(%L::uuid))
  AND fvoci.app_access(a.target_kind,a.target_id) <> 'none'
ORDER BY a.created_at DESC,a.id DESC LIMIT 20
$query$, :'organization') AS plan \gset
\echo existing indexes
:plan;
:plan;
:plan;
CREATE INDEX perf240_activity_global ON fvoci.activity(created_at DESC,id DESC);
\echo global created_at id index
:plan;
:plan;
:plan;
DROP INDEX fvoci.perf240_activity_global;
CREATE INDEX perf240_activity_workspace ON fvoci.activity(workspace_id,created_at DESC,id DESC);
\echo workspace created_at id index
:plan;
:plan;
:plan;
DROP INDEX fvoci.perf240_activity_workspace;
RESET ROLE;
