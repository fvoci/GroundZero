-- 기록 검색 (D-39 "모듈 항목·자료 검색"). 항목 기록(41절)을 통합 검색에 더한다. 제목·메모·칸의 글자를 색인하고,
-- 결과는 그 항목을 지금 볼 수 있는 사람에게만 보인다. 날짜 필터는 기록 날짜(없으면 고친 날)로 본다.

ALTER TABLE search_queue DROP CONSTRAINT search_queue_kind_check;
ALTER TABLE search_queue ADD CONSTRAINT search_queue_kind_check
  CHECK (kind IN ('document', 'task', 'item', 'event', 'file', 'record'));

DROP FUNCTION app_system_search_payload(text[], uuid[]);
CREATE FUNCTION app_system_search_payload(p_kinds text[], p_ids uuid[])
  RETURNS TABLE (kind text, id uuid, live boolean, workspace_id uuid, owner_user_id uuid, item_id uuid,
                 title text, reference text, body jsonb, description text, updated_at timestamptz,
                 module_id uuid, status text, assignee_ids uuid[], day date,
                 file_key text, file_type text, file_size bigint)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH wanted AS (SELECT * FROM unnest(p_kinds, p_ids) AS w(kind, id)),
    rows AS (
      SELECT w.kind, w.id,
        CASE w.kind
          WHEN 'document' THEN d.id IS NOT NULL AND d.deleted_at IS NULL
            AND (d.item_id IS NULL OR EXISTS (SELECT 1 FROM items i WHERE i.id = d.item_id AND i.deleted_at IS NULL))
          WHEN 'task' THEN t.id IS NOT NULL AND t.deleted_at IS NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = t.item_id AND i.deleted_at IS NULL)
          WHEN 'item' THEN it.id IS NOT NULL AND it.deleted_at IS NULL
          WHEN 'event' THEN e.id IS NOT NULL AND e.deleted_at IS NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = e.item_id AND i.deleted_at IS NULL)
          WHEN 'file' THEN fl.id IS NOT NULL AND fl.deleted_at IS NULL
            AND (fl.document_id IS NULL OR EXISTS (SELECT 1 FROM documents fd WHERE fd.id = fl.document_id AND fd.deleted_at IS NULL))
            AND NOT EXISTS (SELECT 1 FROM items i WHERE i.id = coalesce(fl.item_id, (SELECT item_id FROM documents WHERE id = fl.document_id)) AND i.deleted_at IS NOT NULL)
          WHEN 'record' THEN rc.id IS NOT NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = rc.item_id AND i.deleted_at IS NULL)
        END AS live,
        coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id, fl.workspace_id, rc.workspace_id)
          AS workspace_id,
        coalesce(d.owner_user_id, fl.owner_user_id) AS owner_user_id,
        coalesce(d.item_id, t.item_id, e.item_id, fl.item_id, rc.item_id,
                 (SELECT item_id FROM documents WHERE id = fl.document_id))
          AS item_id,
        coalesce(d.title, t.title, it.title, e.title, fl.name, rc.title) AS title,
        CASE w.kind
          WHEN 'task' THEN app_task_reference(t.id)
          WHEN 'item' THEN app_item_reference(it.id)
        END AS reference,
        d.body,
        coalesce(t.description, it.description, e.description,
          rc.body || ' ' || coalesce((SELECT string_agg(f.value, ' ') FROM jsonb_each_text(rc.fields) f), ''))
          AS description,
        coalesce(d.updated_at, t.updated_at, it.updated_at, e.updated_at, fl.updated_at, rc.updated_at) AS updated_at,
        it.module_id AS item_module,
        t.status,
        CASE WHEN w.kind = 'task' THEN ARRAY(SELECT a.user_id FROM task_assignees a WHERE a.task_id = t.id ORDER BY 1) END
          AS assignee_ids,
        CASE w.kind
          WHEN 'task' THEN coalesce(t.due_date, t.start_date)
          WHEN 'event' THEN coalesce(e.start_date, (e.starts_at AT TIME ZONE coalesce(e.timezone, 'UTC'))::date)
          WHEN 'record' THEN coalesce(rc.occurred_on, rc.updated_at::date)
          ELSE coalesce(d.updated_at, it.updated_at, fl.updated_at)::date
        END AS day,
        v.storage_key AS file_key, v.content_type AS file_type, v.size AS file_size
      FROM wanted w
      LEFT JOIN documents d ON w.kind = 'document' AND d.id = w.id
      LEFT JOIN tasks t ON w.kind = 'task' AND t.id = w.id
      LEFT JOIN items it ON w.kind = 'item' AND it.id = w.id
      LEFT JOIN events e ON w.kind = 'event' AND e.id = w.id
      LEFT JOIN files fl ON w.kind = 'file' AND fl.id = w.id
      LEFT JOIN item_records rc ON w.kind = 'record' AND rc.id = w.id
      LEFT JOIN file_versions v ON v.file_id = fl.id AND v.version = fl.current_version
      WHERE app_user_id() IS NULL
    )
    SELECT r.kind, r.id, r.live, r.workspace_id, r.owner_user_id, r.item_id, r.title, r.reference, r.body,
           r.description, r.updated_at,
           coalesce(r.item_module, (SELECT module_id FROM items WHERE id = r.item_id)),
           r.status, r.assignee_ids, r.day, r.file_key, r.file_type, r.file_size
    FROM rows r
  $$;
REVOKE ALL ON FUNCTION app_system_search_payload(text[], uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_system_search_payload(text[], uuid[]) TO fvoci_app;

CREATE TRIGGER item_records_search AFTER INSERT OR DELETE OR UPDATE OF title, body, fields, occurred_on
  ON item_records FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('record');

CREATE OR REPLACE FUNCTION app_system_search_enqueue_all() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE deleted_at IS NULL
    UNION ALL SELECT 'task', id FROM tasks WHERE deleted_at IS NULL
    UNION ALL SELECT 'item', id FROM items WHERE deleted_at IS NULL
    UNION ALL SELECT 'event', id FROM events WHERE deleted_at IS NULL
    UNION ALL SELECT 'file', id FROM files WHERE deleted_at IS NULL
    UNION ALL SELECT 'record', id FROM item_records
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;

-- 항목이 휴지통에 들어가거나 되살아나면 그 아래 것을 다시 색인한다(기록 포함).
CREATE OR REPLACE FUNCTION app_search_enqueue_item_children() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE item_id = NEW.id
    UNION ALL SELECT 'task', id FROM tasks WHERE item_id = NEW.id
    UNION ALL SELECT 'event', id FROM events WHERE item_id = NEW.id
    UNION ALL SELECT 'file', id FROM files WHERE item_id = NEW.id
    UNION ALL SELECT 'file', f.id FROM files f JOIN documents d ON d.id = f.document_id WHERE d.item_id = NEW.id
    UNION ALL SELECT 'record', id FROM item_records WHERE item_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;

CREATE OR REPLACE FUNCTION app_search_visible(p_kinds text[], p_ids uuid[])
  RETURNS TABLE (kind text, id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT w.kind, w.id FROM unnest(p_kinds, p_ids) AS w(kind, id)
    WHERE CASE w.kind
      WHEN 'event' THEN app_event_access(w.id) IS NOT NULL
      WHEN 'file' THEN app_file_access(w.id) IS NOT NULL
      WHEN 'record' THEN EXISTS (SELECT 1 FROM item_records r
                                 WHERE r.id = w.id AND app_item_access(r.item_id) IS NOT NULL)
      ELSE app_target_visible(w.kind, w.id)
    END
  $$;

SELECT app_system_search_enqueue_all();
