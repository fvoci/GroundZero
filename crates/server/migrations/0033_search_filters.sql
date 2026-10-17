-- 검색 필터와 첨부 본문 (D-39 "모듈·담당자·상태·날짜 필터", "지원 형식의 첨부 본문 검색").
--
-- 색인 내용에 모듈·상태(업무)·담당자(업무)·날짜(업무는 기한 또는 시작일, 일정은 시작일, 그 밖은 고친 날)를 더하고,
-- 파일은 현재 버전의 저장소 키·형식·크기를 넘겨 작업 프로세스가 지원하는 형식의 글자를 뽑아 색인한다. 담당자·
-- 날짜가 바뀌면 다시 색인한다. 바뀐 색인 형식을 채우도록 살아 있는 것을 모두 다시 올린다.

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
        END AS live,
        coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id, fl.workspace_id) AS workspace_id,
        coalesce(d.owner_user_id, fl.owner_user_id) AS owner_user_id,
        coalesce(d.item_id, t.item_id, e.item_id, fl.item_id, (SELECT item_id FROM documents WHERE id = fl.document_id))
          AS item_id,
        coalesce(d.title, t.title, it.title, e.title, fl.name) AS title,
        CASE w.kind
          WHEN 'task' THEN app_task_reference(t.id)
          WHEN 'item' THEN app_item_reference(it.id)
        END AS reference,
        d.body,
        coalesce(t.description, it.description, e.description) AS description,
        coalesce(d.updated_at, t.updated_at, it.updated_at, e.updated_at, fl.updated_at) AS updated_at,
        it.module_id AS item_module,
        t.status,
        CASE WHEN w.kind = 'task' THEN ARRAY(SELECT a.user_id FROM task_assignees a WHERE a.task_id = t.id ORDER BY 1) END
          AS assignee_ids,
        CASE w.kind
          WHEN 'task' THEN coalesce(t.due_date, t.start_date)
          WHEN 'event' THEN coalesce(e.start_date, (e.starts_at AT TIME ZONE coalesce(e.timezone, 'UTC'))::date)
          ELSE coalesce(d.updated_at, it.updated_at, fl.updated_at)::date
        END AS day,
        v.storage_key AS file_key, v.content_type AS file_type, v.size AS file_size
      FROM wanted w
      LEFT JOIN documents d ON w.kind = 'document' AND d.id = w.id
      LEFT JOIN tasks t ON w.kind = 'task' AND t.id = w.id
      LEFT JOIN items it ON w.kind = 'item' AND it.id = w.id
      LEFT JOIN events e ON w.kind = 'event' AND e.id = w.id
      LEFT JOIN files fl ON w.kind = 'file' AND fl.id = w.id
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

DROP TRIGGER tasks_search ON tasks;
CREATE TRIGGER tasks_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, status, item_id,
  due_date, start_date ON tasks FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('task');
DROP TRIGGER events_search ON events;
CREATE TRIGGER events_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, item_id,
  start_date, starts_at ON events FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('event');

-- 담당자가 바뀌면 그 업무를 다시 색인한다.
CREATE FUNCTION app_search_enqueue_assignee() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    VALUES ('task', CASE WHEN TG_OP = 'DELETE' THEN OLD.task_id ELSE NEW.task_id END)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;
REVOKE ALL ON FUNCTION app_search_enqueue_assignee() FROM PUBLIC;
CREATE TRIGGER task_assignees_search AFTER INSERT OR DELETE ON task_assignees
  FOR EACH ROW EXECUTE FUNCTION app_search_enqueue_assignee();

SELECT app_system_search_enqueue_all();
