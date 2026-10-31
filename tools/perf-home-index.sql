-- #245 A-stage only: an ordered recent-task index, rolled back in this test DB.
-- actor is the T6 owner (not the share recipient). No baseline DDL is changed.
\pset pager off
\pset format unaligned
\pset tuples_only on
\timing on
BEGIN;
SET LOCAL search_path = fvoci, pg_temp;
SET LOCAL ROLE fvoci_app;
SELECT set_config('app.user_id', :'actor', true);
SELECT fvoci.app_home(:'home_org'::uuid, 'Asia/Seoul') AS home_before \gset
WITH paths(label, statement) AS (VALUES
 ('home', format('SELECT fvoci.app_home(%L::uuid,''Asia/Seoul'')', :'home_org')),
 ('recent_tasks', format($q$SELECT t.id,t.item_id,t.workspace_id,app_task_reference(t.id) AS reference,
    t.title,s.name AS status,s.category AS status_category,t.due_date,NULL::text AS bucket
    FROM tasks t JOIN task_statuses s ON s.id=t.status_id WHERE t.updated_by=app_user_id()
    AND t.workspace_id IN (SELECT app_my_workspace_ids(%L::uuid))
    ORDER BY t.updated_at DESC,t.id LIMIT 8$q$, :'home_org')),
 ('my_tasks', format('SELECT * FROM app_my_tasks(%L::uuid,(now() AT TIME ZONE ''Asia/Seoul'')::date)', :'home_org')),
 ('activity', format('SELECT * FROM app_recent_activity(%L::uuid,20)', :'home_org'))
)
SELECT format('SELECT %L AS component; EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) %s;', 'before_'||label||'_'||n,statement)
FROM paths CROSS JOIN generate_series(1,3) n \gexec
SET LOCAL ROLE fvoci_migrator;
CREATE INDEX home_recent_tasks_probe ON tasks(updated_by,updated_at DESC,id);
SELECT pg_relation_size('home_recent_tasks_probe') AS index_bytes;
SET LOCAL ROLE fvoci_app;
WITH paths(label, statement) AS (VALUES
 ('home', format('SELECT fvoci.app_home(%L::uuid,''Asia/Seoul'')', :'home_org')),
 ('recent_tasks', format($q$SELECT t.id,t.item_id,t.workspace_id,app_task_reference(t.id) AS reference,
    t.title,s.name AS status,s.category AS status_category,t.due_date,NULL::text AS bucket
    FROM tasks t JOIN task_statuses s ON s.id=t.status_id WHERE t.updated_by=app_user_id()
    AND t.workspace_id IN (SELECT app_my_workspace_ids(%L::uuid))
    ORDER BY t.updated_at DESC,t.id LIMIT 8$q$, :'home_org')),
 ('my_tasks', format('SELECT * FROM app_my_tasks(%L::uuid,(now() AT TIME ZONE ''Asia/Seoul'')::date)', :'home_org')),
 ('activity', format('SELECT * FROM app_recent_activity(%L::uuid,20)', :'home_org'))
)
SELECT format('SELECT %L AS component; EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) %s;', 'after_'||label||'_'||n,statement)
FROM paths CROSS JOIN generate_series(1,3) n \gexec
SELECT :'home_before'::jsonb = fvoci.app_home(:'home_org'::uuid,'Asia/Seoul') AS same_home_json;
-- Finish every read comparison before any rolled-back heap update.
SET LOCAL ROLE fvoci_migrator;
DROP INDEX home_recent_tasks_probe;
\echo index component: before_update_rollback
SAVEPOINT write_probe;
EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON)
UPDATE tasks SET updated_at=updated_at+interval '1 microsecond' WHERE updated_by=:'actor'::uuid;
ROLLBACK TO SAVEPOINT write_probe;
RELEASE SAVEPOINT write_probe;
CREATE INDEX home_recent_tasks_probe ON tasks(updated_by,updated_at DESC,id);
\echo index component: after_update_rollback
SAVEPOINT write_probe;
EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON)
UPDATE tasks SET updated_at=updated_at+interval '1 microsecond' WHERE updated_by=:'actor'::uuid;
ROLLBACK TO SAVEPOINT write_probe;
RELEASE SAVEPOINT write_probe;
ROLLBACK;
