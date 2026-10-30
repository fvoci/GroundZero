-- #240 / #245: run three times with each index configuration. Owner role is the
-- effective role of app_recent_activity; app_access still checks the same caller.
BEGIN;
SET LOCAL ROLE fvoci_migrator;
SELECT set_config('app.user_id', :'actor', true);
EXPLAIN (ANALYZE, BUFFERS)
SELECT a.id, a.kind, a.actor, CASE WHEN fvoci.app_shares_organization(a.actor) THEN u.display_name END,
       a.workspace_id, a.target_kind, a.target_id, a.item_id, a.detail, a.created_at
FROM fvoci.activity a LEFT JOIN fvoci.users u ON u.id=a.actor
WHERE a.workspace_id IN (SELECT fvoci.app_my_workspace_ids(:'organization'::uuid))
  AND fvoci.app_access(a.target_kind, a.target_id) <> 'none'
ORDER BY a.created_at DESC, a.id DESC LIMIT 20;
ROLLBACK;
