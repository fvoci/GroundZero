-- #210 / #245: diagnostic decomposition of the installed app_home body.
-- Run after screen sampling, as fvoci_app with that request's actor/org/tz/date.
-- Each list consumes the original CTE's complete JSON, including reference calls.
-- Independent plans overlap (mine/my_tasks/marks); their times are not additive.
\pset pager off
\pset format unaligned
\pset tuples_only on
BEGIN;
SET LOCAL ROLE fvoci_app;
SET LOCAL search_path = fvoci, pg_temp;
SELECT set_config('app.user_id', :'actor', true);
\echo home component: organization
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT fvoci.app_access('organization', :'home_org'::uuid);
\echo home component: timezone_choice
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT coalesce(nullif(btrim(:'home_tz'), ''), nullif(s.time_zone, ''), 'UTC')
FROM (SELECT 1) seed LEFT JOIN fvoci.user_settings s ON s.user_id = fvoci.app_user_id();
SELECT coalesce(nullif(btrim(:'home_tz'), ''), nullif(s.time_zone, ''), 'UTC') AS chosen_tz
FROM (SELECT 1) seed LEFT JOIN fvoci.user_settings s ON s.user_id = fvoci.app_user_id() \gset
\echo home component: timezone_validation_unchanged
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT fvoci.app_check_timezone(:'chosen_tz');
\echo home component: today
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT (now() AT TIME ZONE :'chosen_tz')::date;
\echo home component: workspace_scope
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT * FROM fvoci.app_my_workspace_ids(:'home_org'::uuid);
-- Fail visibly if the implementation's framing changes, rather than measuring
-- a stale copy or silently falling back to a different statement.
WITH body AS (
  SELECT substring(prosrc FROM E'  RETURN \\(\n(.*)\n  \\);') AS statement
  FROM pg_proc WHERE oid = 'fvoci.app_home(uuid,text)'::regprocedure
)
SELECT statement IS NOT NULL AND strpos(statement, E'    SELECT jsonb_build_object(\n      ''today''') > 0 AS valid
FROM body \gset
\if :valid
\else
  DO $$ BEGIN RAISE EXCEPTION 'unexpected app_home body framing'; END $$;
\endif
WITH body AS (
  SELECT replace(replace(substring(prosrc FROM E'  RETURN \\(\n(.*)\n  \\);'),
         'p_organization', format('%L::uuid', :'home_org')),
         'v_today', format('%L::date', :'today')) AS statement
  FROM pg_proc WHERE oid = 'fvoci.app_home(uuid,text)'::regprocedure
), parts AS (
  SELECT statement, left(statement, strpos(statement, E'    SELECT jsonb_build_object(\n      ''today''') - 1) AS ctes
  FROM body
), components(label, expression) AS (VALUES
  ('up_next_and_counts', $q$jsonb_build_object('rows', (SELECT jsonb_agg(to_jsonb(u)) FROM upcoming u), 'counts',
    (SELECT jsonb_build_object('overdue',count(*) FILTER (WHERE bucket='overdue'), 'today',count(*) FILTER (WHERE bucket='today'),
      'upcoming',count(*) FILTER (WHERE bucket='upcoming')) FROM mine))$q$),
  ('recent_tasks', '(SELECT jsonb_agg(to_jsonb(t)) FROM recent_tasks t)'),
  ('recent_documents', '(SELECT jsonb_agg(to_jsonb(d)) FROM recent_documents d)'),
  ('active_items', '(SELECT jsonb_agg(to_jsonb(i)) FROM active i)'),
  ('my_tasks', $q$jsonb_build_object('counts', (SELECT jsonb_build_object(
    'overdue',count(*) FILTER (WHERE bucket='overdue')::integer, 'today',count(*) FILTER (WHERE bucket='today')::integer,
    'upcoming',count(*) FILTER (WHERE bucket='upcoming')::integer, 'undated',count(*) FILTER (WHERE bucket='none')::integer,
    'done',count(*) FILTER (WHERE bucket='done')::integer) FROM my_tasks),
    'workspaces', (SELECT jsonb_agg(to_jsonb(w)-'first_row' ORDER BY first_row) FROM workspace_tasks w))$q$),
  ('pins_and_favorites_unchanged', $q$jsonb_build_object(
    'pins',(SELECT jsonb_agg(to_jsonb(m)-'mark'-'created_at' ORDER BY m.created_at,m.id) FROM marks m WHERE m.mark='pin' AND m.label IS NOT NULL),
    'favorites',(SELECT jsonb_agg(to_jsonb(m)-'mark'-'created_at' ORDER BY m.created_at,m.id) FROM marks m WHERE m.mark='favorite' AND m.label IS NOT NULL))$q$)
)
SELECT format('SELECT %L AS component; EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) %s SELECT %s;', label, ctes, expression)
FROM parts CROSS JOIN components
UNION ALL
SELECT format('SELECT %L AS component; EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) %s;', 'all_lists_and_json', statement) FROM parts
\gexec
\echo home component: whole_function
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) SELECT fvoci.app_home(:'home_org'::uuid, :'home_tz');
ROLLBACK;
