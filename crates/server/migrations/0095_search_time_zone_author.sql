-- 검색 날짜의 사용자 시간대 경계와 작성자 색인 필터 (D-101·D-102, docs/development.md 97절).
--
-- 색인 내용(`app_system_search_payload`)이 날짜 하나(`day`, UTC 날짜·일정 자체 시간대의 날짜가 섞임) 대신 두 종류를 준다.
-- 실제 시각이 있는 것은 `at`(문서·항목·파일의 고친 때, 발생일 없는 기록의 고친 때, 시각 일정의 시작), 날짜만 있는 것은
-- `on_day`(업무 기한(없으면 시작일), 종일 일정의 시작일, 기록의 발생일). 한 행에는 둘 중 하나만 있다. 검색 API가 보는 사람
-- 시간대의 그날 시작·끝을 계산해(시간 기록의 날 경계와 같은 식, `app_local_instant`) `at`과 비교하고 `on_day`는 날짜로
-- 비교한다. 작성자(`created_by`)도 색인에 실어 검색 서비스가 거른다. 끝에서 모든 것을 한 번 다시 색인한다(두 결정의 재색인을
-- 하나로).

DROP FUNCTION app_system_search_payload(text[], uuid[]);
CREATE FUNCTION app_system_search_payload(p_kinds text[], p_ids uuid[])
  RETURNS TABLE (kind text, id uuid, live boolean, workspace_id uuid, owner_user_id uuid, item_id uuid,
                 title text, reference text, body jsonb, description text, created_by uuid,
                 module_id uuid, status text, assignee_ids uuid[], at timestamptz, on_day date,
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
        coalesce(d.created_by, t.created_by, it.created_by, e.created_by, fl.created_by, rc.created_by) AS created_by,
        it.module_id AS item_module,
        t.status,
        CASE WHEN w.kind = 'task' THEN ARRAY(SELECT a.user_id FROM task_assignees a WHERE a.task_id = t.id ORDER BY 1) END
          AS assignee_ids,
        -- 검색 API의 대체 경로(`search_api.rs` `fallback`)와 같은 규칙.
        CASE w.kind
          WHEN 'task' THEN NULL
          WHEN 'event' THEN e.starts_at
          WHEN 'record' THEN CASE WHEN rc.occurred_on IS NULL THEN rc.updated_at END
          ELSE coalesce(d.updated_at, it.updated_at, fl.updated_at)
        END AS at,
        CASE w.kind
          WHEN 'task' THEN coalesce(t.due_date, t.start_date)
          WHEN 'event' THEN e.start_date
          WHEN 'record' THEN rc.occurred_on
        END AS on_day,
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
           r.description, r.created_by,
           coalesce(r.item_module, (SELECT module_id FROM items WHERE id = r.item_id)),
           r.status, r.assignee_ids, r.at, r.on_day, r.file_key, r.file_type, r.file_size
    FROM rows r
  $$;
REVOKE ALL ON FUNCTION app_system_search_payload(text[], uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_system_search_payload(text[], uuid[]) TO fvoci_app;

-- 색인의 모든 문서를 새 속성(`at`·`date`·`created_by`)으로 다시 만든다. 검색 설정(거를 수 있는 속성)은 서버가 시작할 때
-- 맞춘다(`search::SearchClient::ensure_index`).
SELECT app_system_search_enqueue_all();
