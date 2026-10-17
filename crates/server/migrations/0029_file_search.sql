-- 첨부 파일 이름 검색 (D-39 "자료 검색"). 본문(첨부 내용) 검색은 형식별 글자 뽑기가 필요해 다음 조각.
--
-- 파일 이름을 색인에 넣고 검색 결과는 파일 권한(`app_file_access`)으로 거른다. 파일이 붙은 문서를 휴지통에
-- 넣거나 되살리면 그 파일들도 다시 색인한다(항목은 0024의 하위 재색인에 더한다).

ALTER TABLE search_queue DROP CONSTRAINT search_queue_kind_check;
ALTER TABLE search_queue ADD CONSTRAINT search_queue_kind_check
  CHECK (kind IN ('document', 'task', 'item', 'event', 'file'));

CREATE TRIGGER files_search AFTER INSERT OR DELETE OR UPDATE OF name, deleted_at, current_version
  ON files FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('file');

CREATE FUNCTION app_search_enqueue_document_files() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'file', id FROM files WHERE document_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;
CREATE TRIGGER documents_search_files AFTER UPDATE OF deleted_at ON documents
  FOR EACH ROW WHEN (OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
  EXECUTE FUNCTION app_search_enqueue_document_files();
REVOKE ALL ON FUNCTION app_search_enqueue_document_files() FROM PUBLIC;

CREATE OR REPLACE FUNCTION app_system_search_payload(p_kinds text[], p_ids uuid[])
  RETURNS TABLE (kind text, id uuid, live boolean, workspace_id uuid, owner_user_id uuid, item_id uuid,
                 title text, reference text, body jsonb, description text, updated_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH wanted AS (SELECT * FROM unnest(p_kinds, p_ids) AS w(kind, id))
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
      END,
      coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id, fl.workspace_id),
      coalesce(d.owner_user_id, fl.owner_user_id),
      coalesce(d.item_id, t.item_id, e.item_id, fl.item_id, (SELECT item_id FROM documents WHERE id = fl.document_id)),
      coalesce(d.title, t.title, it.title, e.title, fl.name),
      CASE w.kind
        WHEN 'task' THEN app_task_reference(t.id)
        WHEN 'item' THEN app_item_reference(it.id)
      END,
      d.body,
      coalesce(t.description, it.description, e.description),
      coalesce(d.updated_at, t.updated_at, it.updated_at, e.updated_at, fl.updated_at)
    FROM wanted w
    LEFT JOIN documents d ON w.kind = 'document' AND d.id = w.id
    LEFT JOIN tasks t ON w.kind = 'task' AND t.id = w.id
    LEFT JOIN items it ON w.kind = 'item' AND it.id = w.id
    LEFT JOIN events e ON w.kind = 'event' AND e.id = w.id
    LEFT JOIN files fl ON w.kind = 'file' AND fl.id = w.id
    WHERE app_user_id() IS NULL
  $$;

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
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
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
      ELSE app_target_visible(w.kind, w.id)
    END
  $$;

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
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;

