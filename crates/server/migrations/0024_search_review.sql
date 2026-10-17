-- 검색 리뷰 반영 (D-39, 0023 보완).
--
-- 1. 항목을 휴지통에 넣거나 되살리면 그 안의 문서·업무·일정도 대기열에 올린다(색인에서 빼거나 다시 넣기).
-- 2. 모듈 키가 바뀌면 그 모듈의 항목·업무를 다시 색인한다(번호 검색).
-- 3. 대기열 꺼내기는 지우지 않고 "처리 중" 표시만 하고 바로 끝낸다. 반영이 끝난 뒤 그 사이 다시 오르지 않은 것만
--    지운다. Meilisearch를 기다리는 동안 사용자의 저장이 대기열 행 잠금에 걸리지 않는다. 실패한 것은 표시가
--    풀릴 때(2분)까지 미뤄지고, 10번 실패하면 다시 바뀔 때까지 쉰다(한 건이 대기열 전체를 막지 않게).
-- 4. 문서 번호는 색인하지 않는다. 멤버가 아닌 공유 대상이 번호로 찾아 키를 알아내지 않게(번호는 응답에서
--    호출자 권한으로 다시 읽는다).
-- 5. 전체 다시 올리기도 사용자 문맥에서는 아무것도 하지 않는다.

ALTER TABLE search_queue ADD COLUMN claimed_until timestamptz;
ALTER TABLE search_queue ADD COLUMN attempts integer NOT NULL DEFAULT 0;

CREATE OR REPLACE FUNCTION app_search_enqueue() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_kind text := TG_ARGV[0];
    v_id   uuid := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
  BEGIN
    INSERT INTO search_queue (kind, target_id) VALUES (v_kind, v_id)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;

CREATE FUNCTION app_search_enqueue_item_children() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE item_id = NEW.id
    UNION ALL SELECT 'task', id FROM tasks WHERE item_id = NEW.id
    UNION ALL SELECT 'event', id FROM events WHERE item_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;

CREATE TRIGGER items_search_children AFTER UPDATE OF deleted_at ON items
  FOR EACH ROW WHEN (OLD.deleted_at IS DISTINCT FROM NEW.deleted_at)
  EXECUTE FUNCTION app_search_enqueue_item_children();

CREATE FUNCTION app_search_enqueue_module() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'item', i.id FROM items i WHERE i.module_id = NEW.module_id
    UNION ALL SELECT 'task', t.id FROM tasks t JOIN items i ON i.id = t.item_id WHERE i.module_id = NEW.module_id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;

CREATE TRIGGER module_keys_search AFTER INSERT OR UPDATE ON module_keys
  FOR EACH ROW EXECUTE FUNCTION app_search_enqueue_module();

DROP FUNCTION app_system_search_claim(integer);
CREATE FUNCTION app_system_search_claim(p_limit integer)
  RETURNS TABLE (kind text, target_id uuid, queued_at timestamptz)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes', attempts = q.attempts + 1
    WHERE (q.kind, q.target_id) IN (
      SELECT s.kind, s.target_id FROM search_queue s
      WHERE app_user_id() IS NULL
        AND (s.claimed_until IS NULL OR s.claimed_until < now())
        AND s.attempts < 10
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING q.kind, q.target_id, q.queued_at
  $$;

-- 반영을 마친 것. 처리하는 동안 다시 오른 것(queued_at이 바뀜)은 남기고 표시만 풀어 다음에 다시 반영한다.
CREATE FUNCTION app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamptz[]) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    DELETE FROM search_queue q
    USING unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE search_queue q SET claimed_until = NULL
    FROM unnest(p_kinds, p_ids) AS d(kind, id)
    WHERE q.kind = d.kind AND q.target_id = d.id;
    RETURN v_count;
  END
  $$;

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
      END,
      coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id),
      d.owner_user_id,
      coalesce(d.item_id, t.item_id, e.item_id),
      coalesce(d.title, t.title, it.title, e.title),
      CASE w.kind
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
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;

REVOKE ALL ON FUNCTION app_search_enqueue_item_children() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_search_enqueue_module() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_claim(integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_done(text[], uuid[], timestamptz[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_system_search_claim(integer),
  app_system_search_done(text[], uuid[], timestamptz[])
TO fvoci_app;
