-- Each invocation runs on its own fresh physical clone. The identical initial
-- fingerprint also warms the task heap before the optional index is built.
\pset pager off
\pset format unaligned
\pset tuples_only on
BEGIN;
SET LOCAL search_path = fvoci, pg_temp;
SET LOCAL ROLE fvoci_migrator;
SELECT set_config('app.user_id', :'actor', true);
SELECT jsonb_build_object('rows',count(*), 'heap_bytes',pg_relation_size('tasks'),
  'rows_md5',md5(string_agg(to_jsonb(t)::text,'' ORDER BY id))) AS initial_fixture
FROM tasks t;
\if :indexed
CREATE INDEX home_recent_tasks_probe ON tasks(updated_by,updated_at DESC,id);
\endif
EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON)
UPDATE tasks SET updated_at=updated_at+interval '1 microsecond' WHERE updated_by=:'actor'::uuid;
ROLLBACK;
