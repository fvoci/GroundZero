-- #210 measurement only. actor/item/workspace are the fixture; replay arguments come
-- from db-arguments.json (first observed request in repeat 1 of each named screen).
-- No product DDL or policy changes.
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
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_home(:'home_org'::uuid, :'home_tz');
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.app_my_tasks(:'tasks_org'::uuid, :'today'::date);
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_calendar(:'calendar_workspace'::uuid, :'calendar_from'::date, :'calendar_to'::date, :'calendar_tz', nullif(:'calendar_module', '')::uuid, nullif(:'calendar_item', '')::uuid);
EXPLAIN (ANALYZE, BUFFERS) SELECT * FROM fvoci.app_recent_activity(:'org'::uuid, 20);
ROLLBACK;
\echo identical item read with the product RLS policy, app role
BEGIN;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'actor', true);
EXPLAIN (ANALYZE, BUFFERS) SELECT count(*) FROM fvoci.documents WHERE item_id=:'item'::uuid;
ROLLBACK;
\echo document list components: RLS, reference, parent visibility, tree order
BEGIN;
SET LOCAL ROLE fvoci_app;
SET LOCAL search_path = fvoci, pg_temp;
SELECT set_config('app.user_id', :'actor', true);
\echo document list: RLS only
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, title, revision, updated_at FROM documents WHERE item_id=:'item'::uuid;
\echo document list: RLS + reference
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, app_document_reference(id) AS reference, title, revision, updated_at
FROM documents WHERE item_id=:'item'::uuid;
\echo document list: RLS + reference + parent visibility
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, app_document_reference(id) AS reference, title, revision, updated_at,
       CASE WHEN app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id
FROM documents WHERE item_id=:'item'::uuid;
\echo document list: original full query (a81cab4d)
EXPLAIN (ANALYZE, BUFFERS)
SELECT id, app_document_reference(id) AS reference, title, revision, updated_at,
       CASE WHEN app_access('document', parent_id) <> 'none' THEN parent_id END AS parent_id
FROM documents WHERE item_id=:'item'::uuid
ORDER BY position, number NULLS LAST, created_at, id;
\echo document list candidate C1: one item key lookup
EXPLAIN (ANALYZE, BUFFERS)
SELECT d.id,
       (SELECT k.key FROM fvoci.items i
        JOIN fvoci.module_keys k ON k.module_id=i.module_id AND k.retired_at IS NULL
        WHERE i.id=:'item'::uuid) || '-' || d.number AS reference,
       d.title, d.revision, d.updated_at,
       CASE WHEN app_access('document', d.parent_id) <> 'none' THEN d.parent_id END AS parent_id
FROM documents d WHERE d.item_id=:'item'::uuid
ORDER BY d.position, d.number NULLS LAST, d.created_at, d.id;
\echo current product document list: C2, reuse RLS-visible parents (items.rs ITEM_DOCUMENTS_SQL)
EXPLAIN (ANALYZE, BUFFERS)
WITH visible AS MATERIALIZED (
    SELECT id, title, revision, updated_at, parent_id, number, position, created_at
    FROM documents WHERE item_id=:'item'::uuid
)
SELECT d.id,
       (SELECT k.key FROM fvoci.items i
        JOIN fvoci.module_keys k ON k.module_id=i.module_id AND k.retired_at IS NULL
        WHERE i.id=:'item'::uuid) || '-' || d.number AS reference,
       d.title, d.revision, d.updated_at, p.id AS parent_id
FROM visible d LEFT JOIN visible p ON p.id=d.parent_id
ORDER BY d.position, d.number NULLS LAST, d.created_at, d.id;
ROLLBACK;
\echo timezone validation and current task resource reads (tasks/resource.rs TASK_SELECT)
BEGIN;
SET LOCAL ROLE fvoci_app;
SET LOCAL search_path = fvoci, pg_temp;
SELECT set_config('app.user_id', :'actor', true);
EXPLAIN (ANALYZE, BUFFERS) SELECT fvoci.app_check_timezone(:'home_tz');
\echo task list item visibility gate
EXPLAIN (ANALYZE, BUFFERS) SELECT app_access('item', :'item'::uuid) <> 'none';
\echo task list, same columns and ordering as load_tasks
EXPLAIN (ANALYZE, BUFFERS)
SELECT t.id, t.item_id, i.module_id, t.workspace_id, t.parent_id,
       app_task_reference(t.id) AS reference, t.title, t.description, s.name AS status,
       s.category AS status_category, t.priority, t.start_date, t.due_date, t.estimate_minutes,
       t.tags, t.milestone, t.series_id, t.occurrence_date, t.version, t.move_version,
       t.completed_at, t.updated_at
FROM tasks t JOIN items i ON i.id = t.item_id JOIN task_statuses s ON s.id = t.status_id
WHERE t.item_id = :'item'::uuid ORDER BY s.position, t.position, t.number;
\echo task detail, same columns as load_task
EXPLAIN (ANALYZE, BUFFERS)
SELECT t.id, t.item_id, i.module_id, t.workspace_id, t.parent_id,
       app_task_reference(t.id) AS reference, t.title, t.description, s.name AS status,
       s.category AS status_category, t.priority, t.start_date, t.due_date, t.estimate_minutes,
       t.tags, t.milestone, t.series_id, t.occurrence_date, t.version, t.move_version,
       t.completed_at, t.updated_at
FROM tasks t JOIN items i ON i.id = t.item_id JOIN task_statuses s ON s.id = t.status_id
WHERE t.id = :'task'::uuid;
ROLLBACK;
