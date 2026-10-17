-- 통합 검색 색인 대기열 (D-39).
--
-- 문서·업무·항목·일정이 생기거나 바뀌거나 지워지면 대기열에 이름이 오른다(트리거). 백그라운드 작업이 대기열을
-- 꺼내 지금 내용(또는 없어졌음)을 읽어 Meilisearch에 반영한다. 색인에는 거친 범위(워크스페이스·개인 노트
-- 소유자)만 넣고, 검색 결과는 PostgreSQL의 접근 함수로 한 번 더 걸러 볼 수 있는 것만 돌려준다. 공유 대상 이메일,
-- 담당자, 본문의 참조 표시 이름은 색인에 넣지 않는다.

CREATE TABLE search_queue (
  kind      text NOT NULL CHECK (kind IN ('document', 'task', 'item', 'event')),
  target_id uuid NOT NULL,
  queued_at timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (kind, target_id)
);
CREATE INDEX search_queue_order ON search_queue (queued_at);

CREATE FUNCTION app_search_enqueue() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_kind text := TG_ARGV[0];
    v_id   uuid := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
  BEGIN
    INSERT INTO search_queue (kind, target_id) VALUES (v_kind, v_id)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now();
    RETURN NULL;
  END
  $$;

CREATE TRIGGER documents_search AFTER INSERT OR DELETE OR UPDATE OF title, body, deleted_at, visibility, item_id,
  workspace_id, owner_user_id ON documents FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('document');
CREATE TRIGGER tasks_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, status, item_id
  ON tasks FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('task');
CREATE TRIGGER items_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, visibility,
  archived_at ON items FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('item');
CREATE TRIGGER events_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, item_id
  ON events FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('event');

-- 대기열에서 꺼낸다(여러 작업 프로세스가 같은 것을 꺼내지 않게). 반영에 실패하면 트랜잭션이 되돌려 다시 남는다.
CREATE FUNCTION app_system_search_claim(p_limit integer)
  RETURNS TABLE (kind text, target_id uuid)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    DELETE FROM search_queue q
    WHERE (q.kind, q.target_id) IN (
      SELECT s.kind, s.target_id FROM search_queue s
      WHERE app_user_id() IS NULL
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING q.kind, q.target_id
  $$;

-- 색인할 내용. live=false면 색인에서 지운다(지웠거나, 휴지통에 있는 항목 안이거나, 없어짐).
CREATE FUNCTION app_system_search_payload(p_kinds text[], p_ids uuid[])
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
      END,
      coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id),
      d.owner_user_id,
      coalesce(d.item_id, t.item_id, e.item_id),
      coalesce(d.title, t.title, it.title, e.title),
      CASE w.kind
        WHEN 'document' THEN app_document_reference(d.id)
        WHEN 'task' THEN app_task_reference(t.id)
        WHEN 'item' THEN app_item_reference(it.id)
      END,
      d.body,
      coalesce(t.description, it.description, e.description),
      coalesce(d.updated_at, t.updated_at, it.updated_at, e.updated_at)
    FROM wanted w
    LEFT JOIN documents d ON w.kind = 'document' AND d.id = w.id
    LEFT JOIN tasks t ON w.kind = 'task' AND t.id = w.id
    LEFT JOIN items it ON w.kind = 'item' AND it.id = w.id
    LEFT JOIN events e ON w.kind = 'event' AND e.id = w.id
    -- 요청 처리(사용자 문맥이 있는 트랜잭션)에서는 아무것도 돌려주지 않는다. 작업 프로세스만 쓴다.
    WHERE app_user_id() IS NULL
  $$;

-- 처음 한 번(또는 다시 색인할 때) 살아 있는 것을 모두 대기열에 올린다.
CREATE FUNCTION app_system_search_enqueue_all() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE deleted_at IS NULL
    UNION ALL SELECT 'task', id FROM tasks WHERE deleted_at IS NULL
    UNION ALL SELECT 'item', id FROM items WHERE deleted_at IS NULL
    UNION ALL SELECT 'event', id FROM events WHERE deleted_at IS NULL
    ON CONFLICT (kind, target_id) DO NOTHING;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;

-- 검색 결과 거르기: 지금 볼 수 있는 것만(종류별 접근 함수).
CREATE FUNCTION app_search_visible(p_kinds text[], p_ids uuid[])
  RETURNS TABLE (kind text, id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT w.kind, w.id FROM unnest(p_kinds, p_ids) AS w(kind, id)
    WHERE CASE w.kind
      WHEN 'event' THEN app_event_access(w.id) IS NOT NULL
      ELSE app_target_visible(w.kind, w.id)
    END
  $$;

ALTER TABLE search_queue ENABLE ROW LEVEL SECURITY;
CREATE POLICY search_queue_none ON search_queue FOR SELECT USING (false);

REVOKE ALL ON FUNCTION app_search_enqueue() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_claim(integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_payload(text[], uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_enqueue_all() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_search_visible(text[], uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_system_search_claim(integer),
  app_system_search_payload(text[], uuid[]),
  app_system_search_enqueue_all(),
  app_search_visible(text[], uuid[])
TO fvoci_app;
