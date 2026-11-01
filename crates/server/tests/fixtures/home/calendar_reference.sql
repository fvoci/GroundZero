-- Frozen from main 4ed0f893 for #291; only the function name and test ACL differ.
CREATE FUNCTION fvoci.calendar_reference(p_workspace uuid, p_from date, p_to date, p_tz text, p_module uuid DEFAULT NULL, p_item uuid DEFAULT NULL) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
    SET timezone TO 'UTC'
    AS $$
DECLARE
    v_from_at timestamptz;
    v_to_at timestamptz;
BEGIN
    IF p_from IS NULL OR p_to IS NULL OR p_to < p_from OR p_to - p_from >= 62
       OR p_from < DATE '1900-01-01' OR p_to > DATE '2200-12-31' THEN
        RAISE EXCEPTION 'the range must be 1 to 62 days between 1900 and 2200' USING ERRCODE = '22023';
    END IF;
    IF app_access('workspace', p_workspace) = 'none' THEN
        RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_check_timezone(p_tz);
    IF p_item IS NOT NULL THEN
        IF NOT EXISTS (SELECT 1 FROM items i WHERE i.id = p_item AND i.workspace_id = p_workspace
                       AND (p_module IS NULL OR i.module_id = p_module)
                       AND app_access('item', i.id) <> 'none') THEN
            RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
        END IF;
    ELSIF p_module IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM modules m WHERE m.id = p_module AND m.workspace_id = p_workspace
    ) THEN
        RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    -- 날짜 경계는 처음 오는 0시이며, 0시가 없으면 건너뛴 직후다(D-50·D-81).
    v_from_at := coalesce(app_local_instant(p_from::timestamp, p_tz), p_from::timestamp AT TIME ZONE p_tz);
    v_to_at := coalesce(app_local_instant((p_to + 1)::timestamp, p_tz), (p_to + 1)::timestamp AT TIME ZONE p_tz);
    RETURN (
        WITH scope AS MATERIALIZED (
            SELECT i.id FROM items i
            WHERE i.workspace_id = p_workspace AND app_access('item', i.id) <> 'none'
              AND (p_module IS NULL OR i.module_id = p_module) AND (p_item IS NULL OR i.id = p_item)
        ),
        task_rows AS MATERIALIZED (
            SELECT t.id, t.item_id, app_task_reference(t.id) AS reference, t.title,
                   s.name AS status, s.category AS status_category, t.milestone, t.start_date, t.due_date,
                   coalesce(t.start_date, t.due_date) AS sort_date, t.number
            FROM tasks t JOIN task_statuses s ON s.id = t.status_id JOIN scope ON scope.id = t.item_id
            WHERE t.workspace_id = p_workspace
              AND coalesce(t.start_date, t.due_date) <= p_to AND coalesce(t.due_date, t.start_date) >= p_from
        ),
        projected_rows AS MATERIALIZED (
            SELECT o.* FROM app_projected_occurrences(p_workspace, NULL, p_from, p_to) o
            JOIN scope ON scope.id = o.item_id
        ),
        occurrence_rows AS MATERIALIZED (
            SELECT o.* FROM app_event_occurrences(p_workspace, v_from_at, v_to_at, p_from, p_to) o
            JOIN events e ON e.id = o.event_id AND e.deleted_at IS NULL
            JOIN scope ON scope.id = e.item_id
        ),
        event_rows AS MATERIALIZED (
            SELECT e.id, e.item_id, e.workspace_id, e.kind, e.title, e.description, e.all_day,
                   e.start_date, e.end_date, e.starts_at, e.ends_at, e.timezone, e.recurrence,
                   e.excluded_dates, e.version, app_access('event', e.id) AS access
            FROM events e JOIN scope ON scope.id = e.item_id
            WHERE e.deleted_at IS NULL AND EXISTS (SELECT 1 FROM occurrence_rows o WHERE o.event_id = e.id)
        ),
        date_rows AS MATERIALIZED (
            SELECT 'episode'::text AS kind, e.id, e.item_id, e.number::text || '. ' || e.title AS title,
                   coalesce(p.planned_on, e.planned_on) AS date,
                   p.planned_on IS NOT NULL AS personal, coalesce(p.status = 'done', false) AS done
            FROM course_episodes e JOIN scope ON scope.id = e.item_id
            LEFT JOIN episode_progress p ON p.episode_id = e.id AND p.user_id = app_user_id()
            WHERE e.workspace_id = p_workspace AND coalesce(p.planned_on, e.planned_on) BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'study_unit', u.id, u.item_id, u.number::text || '. ' || u.title,
                   coalesce(p.planned_on, u.planned_on), p.planned_on IS NOT NULL, coalesce(p.status = 'done', false)
            FROM study_units u JOIN scope ON scope.id = u.item_id
            LEFT JOIN unit_progress p ON p.unit_id = u.id AND p.user_id = app_user_id()
            WHERE u.workspace_id = p_workspace AND coalesce(p.planned_on, u.planned_on) BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'submission', r.id, r.item_id, r.title, r.occurred_on,
                   false, coalesce(r.fields->>'status' IN ('published', 'rejected'), false)
            FROM item_records r JOIN scope ON scope.id = r.item_id
            WHERE r.workspace_id = p_workspace AND r.kind = 'submission' AND r.occurred_on BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'revision_due', r.id, r.item_id, r.title, app_iso_date(r.fields->>'revision_due'),
                   false, coalesce(r.fields->>'status' IN ('accepted', 'published', 'rejected'), false)
            FROM item_records r JOIN scope ON scope.id = r.item_id
            WHERE r.workspace_id = p_workspace AND r.kind = 'submission'
              AND app_iso_date(r.fields->>'revision_due') BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'deliverable', r.id, r.item_id, r.title, r.occurred_on,
                   false, coalesce(r.fields->>'status' IN ('done', 'accepted'), false)
            FROM item_records r JOIN scope ON scope.id = r.item_id
            WHERE r.workspace_id = p_workspace AND r.kind = 'deliverable' AND r.occurred_on BETWEEN p_from AND p_to
        ),
        item_ids AS (
            SELECT item_id FROM task_rows UNION SELECT item_id FROM projected_rows
            UNION SELECT item_id FROM event_rows UNION SELECT item_id FROM date_rows
        ),
        item_rows AS (
            SELECT i.id, app_item_reference(i.id) AS reference, i.title FROM items i JOIN item_ids ON item_ids.item_id = i.id
        )
        SELECT jsonb_build_object(
            'tasks', (SELECT coalesce(jsonb_agg(to_jsonb(t) - 'sort_date' - 'number' ORDER BY t.sort_date, t.number), '[]'::jsonb) FROM task_rows t),
            'projected_tasks', (SELECT coalesce(jsonb_agg(to_jsonb(p) ORDER BY p.occurrence, p.series_id), '[]'::jsonb) FROM projected_rows p),
            'events', (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY coalesce(e.starts_at, e.start_date::timestamp AT TIME ZONE e.timezone), e.id), '[]'::jsonb) FROM event_rows e),
            'occurrences', (SELECT coalesce(jsonb_agg(to_jsonb(o) ORDER BY coalesce(o.starts_at, o.start_date::timestamp AT TIME ZONE 'UTC', o.occurrence::timestamp AT TIME ZONE 'UTC'), o.event_id), '[]'::jsonb) FROM occurrence_rows o),
            'dates', (SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY d.date, d.kind, d.title), '[]'::jsonb) FROM date_rows d),
            'items', (SELECT coalesce(jsonb_agg(to_jsonb(i) ORDER BY i.title, i.id), '[]'::jsonb) FROM item_rows i)
        )
    );
END;
$$;

ALTER FUNCTION fvoci.calendar_reference(p_workspace uuid, p_from date, p_to date, p_tz text, p_module uuid, p_item uuid) OWNER TO fvoci_migrator;

REVOKE ALL ON FUNCTION fvoci.calendar_reference(uuid,date,date,text,uuid,uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.calendar_reference(uuid,date,date,text,uuid,uuid) TO fvoci_app;

