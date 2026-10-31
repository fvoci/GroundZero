-- Frozen app_home from a81cab4d5824d30092bb334683bf3beb8bc4ac18; only the name differs.
CREATE FUNCTION fvoci.home_reference(p_organization uuid, p_tz text) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
DECLARE
  v_tz text;
  v_today date;
BEGIN
  IF app_access('organization', p_organization) = 'none' THEN
    RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
  END IF;
  SELECT coalesce(nullif(btrim(p_tz), ''), nullif(s.time_zone, ''), 'UTC') INTO v_tz
    FROM (SELECT 1) seed LEFT JOIN user_settings s ON s.user_id = app_user_id();
  PERFORM app_check_timezone(v_tz);
  v_today := (now() AT TIME ZONE v_tz)::date;
  RETURN (
    WITH mine AS MATERIALIZED (
      SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id) AS reference,
             t.title, s.name AS status, s.category AS status_category, t.due_date,
             CASE WHEN t.due_date < v_today THEN 'overdue'
                  WHEN t.due_date = v_today THEN 'today' ELSE 'upcoming' END AS bucket,
             t.number
      FROM tasks t JOIN task_statuses s ON s.id = t.status_id
      JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
      WHERE s.category <> 'done' AND t.due_date IS NOT NULL AND t.due_date <= v_today + 7
        AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND t.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
        AND EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id AND a.user_id = app_user_id())
    ), ranked AS (
      SELECT mine.*, row_number() OVER (PARTITION BY bucket ORDER BY due_date, number, id) AS rn FROM mine
    ), upcoming AS (
      SELECT id, item_id, workspace_id, reference, title, status, status_category, due_date, bucket
      FROM ranked WHERE rn <= 50 ORDER BY due_date, number, id
    ), recent_tasks AS (
      SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id) AS reference,
             t.title, s.name AS status, s.category AS status_category, t.due_date, NULL::text AS bucket
      FROM tasks t JOIN task_statuses s ON s.id = t.status_id
      WHERE t.updated_by = app_user_id()
        AND t.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
      ORDER BY t.updated_at DESC, t.id LIMIT 8
    ), recent_documents AS (
      SELECT d.id, d.title, app_document_reference(d.id) AS reference, d.workspace_id, d.item_id, d.updated_at
      FROM documents d WHERE d.updated_by = app_user_id()
        AND (d.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
             OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id()))
      ORDER BY d.updated_at DESC, d.id LIMIT 8
    ), active AS (
      SELECT i.id, i.workspace_id, m.kind AS module_kind, app_item_reference(i.id) AS reference,
             i.title, app_access('item', i.id) IN ('write', 'owner') AS can_write,
             CASE m.kind
               WHEN 'course' THEN (
                 SELECT jsonb_build_object('kind', 'course', 'done', count(*) FILTER (WHERE p.status = 'done'), 'total', count(*))
                 FROM course_episodes e LEFT JOIN episode_progress p ON p.episode_id = e.id AND p.user_id = app_user_id()
                 WHERE e.item_id = i.id
               )
               WHEN 'study' THEN jsonb_build_object('kind', 'study', 'amounts', coalesce((
                 SELECT jsonb_agg(to_jsonb(amounts) ORDER BY amounts.unit) FROM (
                   SELECT u.unit, sum(u.amount) AS target,
                          sum(CASE WHEN p.status = 'done' THEN u.amount ELSE least(coalesce(p.done_amount, 0), u.amount) END) AS done
                   FROM study_units u LEFT JOIN unit_progress p ON p.unit_id = u.id AND p.user_id = app_user_id()
                   WHERE u.item_id = i.id GROUP BY u.unit
                 ) amounts
               ), '[]'::jsonb))
               ELSE jsonb_build_object('kind', 'tasks', 'done', c.done, 'total', c.total)
             END AS progress
      FROM items i JOIN modules m ON m.id = i.module_id
      CROSS JOIN LATERAL (
        SELECT count(*) FILTER (WHERE s.category = 'done') AS done, count(*) AS total, max(t.updated_at) AS active
        FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.item_id = i.id
      ) c
      WHERE i.status = 'active' AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND i.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
      ORDER BY greatest(i.updated_at, c.active) DESC, i.id LIMIT 8
    ), my_tasks AS MATERIALIZED (
      SELECT * FROM app_my_tasks(p_organization, v_today) WITH ORDINALITY
    ), workspace_tasks AS (
      SELECT workspace_id, workspace_name,
             jsonb_agg((to_jsonb(t) - 'workspace_id' - 'workspace_name' - 'task_id' - 'ordinality')
                       || jsonb_build_object('id', task_id) ORDER BY ordinality) AS tasks,
             min(ordinality) AS first_row
      FROM my_tasks t GROUP BY workspace_id, workspace_name
    ), marks AS MATERIALIZED (
      SELECT m.mark, m.target_kind AS kind, m.target_id AS id,
             CASE WHEN m.target_kind = 'document' AND d.item_id IS NOT NULL
                       AND app_access('item', d.item_id) = 'none'
                  THEN d.title
                  ELSE app_time_target_label(m.target_kind, m.target_id) END AS label,
             coalesce(t.workspace_id, d.workspace_id, i.workspace_id) AS workspace_id,
             CASE WHEN app_access('item', coalesce(t.item_id, d.item_id)) <> 'none'
                  THEN coalesce(t.item_id, d.item_id) END AS item_id, m.created_at
      FROM user_marks m
      LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
      LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
      LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
      WHERE m.mark IN ('pin', 'favorite')
        AND (coalesce(t.workspace_id, d.workspace_id, i.workspace_id) IN (SELECT app_my_workspace_ids(p_organization))
             OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id())
             OR d.id IN (SELECT id FROM app_shared_with_me(p_organization)))
    )
    SELECT jsonb_build_object(
      'today', to_char(v_today, 'YYYY-MM-DD'),
      'my_tasks', jsonb_build_object(
        'counts', (SELECT jsonb_build_object(
          'overdue', count(*) FILTER (WHERE bucket = 'overdue')::integer,
          'today', count(*) FILTER (WHERE bucket = 'today')::integer,
          'upcoming', count(*) FILTER (WHERE bucket = 'upcoming')::integer,
          'undated', count(*) FILTER (WHERE bucket = 'none')::integer,
          'done', count(*) FILTER (WHERE bucket = 'done')::integer) FROM my_tasks),
        'workspaces', coalesce((SELECT jsonb_agg(to_jsonb(w) - 'first_row' ORDER BY first_row) FROM workspace_tasks w), '[]'::jsonb)
      ),
      'up_next', coalesce((SELECT jsonb_agg(to_jsonb(u)) FROM upcoming u), '[]'::jsonb),
      'up_next_counts', (SELECT jsonb_build_object(
        'overdue', count(*) FILTER (WHERE bucket = 'overdue'),
        'today', count(*) FILTER (WHERE bucket = 'today'),
        'upcoming', count(*) FILTER (WHERE bucket = 'upcoming')) FROM mine),
      'recent_tasks', coalesce((SELECT jsonb_agg(to_jsonb(t)) FROM recent_tasks t), '[]'::jsonb),
      'recent_documents', coalesce((SELECT jsonb_agg(to_jsonb(d)) FROM recent_documents d), '[]'::jsonb),
      'active_items', coalesce((SELECT jsonb_agg(to_jsonb(i)) FROM active i), '[]'::jsonb),
      'pins', coalesce((SELECT jsonb_agg(to_jsonb(m) - 'mark' - 'created_at' ORDER BY m.created_at, m.id)
                       FROM marks m WHERE m.mark = 'pin' AND m.label IS NOT NULL), '[]'::jsonb),
      'favorites', coalesce((SELECT jsonb_agg(to_jsonb(m) - 'mark' - 'created_at' ORDER BY m.created_at, m.id)
                            FROM marks m WHERE m.mark = 'favorite' AND m.label IS NOT NULL), '[]'::jsonb)
    )
  );
END
$$;

ALTER FUNCTION fvoci.home_reference(p_organization uuid, p_tz text) OWNER TO fvoci_migrator;

REVOKE ALL ON FUNCTION fvoci.home_reference(uuid,text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.home_reference(uuid,text) TO fvoci_app;
