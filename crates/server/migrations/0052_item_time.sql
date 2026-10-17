-- 시간 기록 종류에 취미(hobby)를 더한다(사용자 결정, D-78). 기존 기록은 그대로다.
ALTER TABLE time_entries DROP CONSTRAINT time_entries_kind_check;
ALTER TABLE time_entries ADD CONSTRAINT time_entries_kind_check
  CHECK (kind IN ('work', 'study', 'lecture', 'hobby', 'rest', 'other'));

-- 항목별 시간 합계(D-78·D-32·D-37). 같은 시간 기록(time_entries)에서 항목 자체와 그 아래 업무·문서·강의
-- 회차·스터디 단위를 대상으로 한 기록을 종류별로 모은다. 기록 하나하나는 돌려주지 않는다.
-- 내 합계는 본인 기록만, 함께 보는 합계는 업무 시간(app_task_time)과 같이 그 워크스페이스에서 공유를 켠
-- 멤버의 끝난 기록만이다(D-32 개인 시간 기록 기본 비공개). 항목을 볼 수 없으면 404.

CREATE FUNCTION app_item_time(p_item uuid)
  RETURNS TABLE (entry_kind text, mine_seconds bigint, shared_seconds bigint, shared_people integer)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item;
    RETURN QUERY
      WITH targets (target_kind, target_id) AS (
        SELECT 'item'::text, p_item
        UNION ALL SELECT 'task', t.id FROM tasks t WHERE t.item_id = p_item
        UNION ALL SELECT 'document', d.id FROM documents d WHERE d.item_id = p_item
        UNION ALL SELECT 'episode', c.id FROM course_episodes c WHERE c.item_id = p_item
        UNION ALL SELECT 'study_unit', u.id FROM study_units u WHERE u.item_id = p_item
      )
      SELECT
        e.kind,
        coalesce(sum(app_entry_seconds(e)) FILTER (WHERE e.user_id = app_user_id()), 0)::bigint,
        coalesce(sum(app_entry_seconds(e)) FILTER (WHERE s.user_id IS NOT NULL), 0)::bigint,
        count(DISTINCT e.user_id) FILTER (WHERE s.user_id IS NOT NULL)::integer
      FROM time_entries e
      JOIN targets g ON g.target_kind = e.target_kind AND g.target_id = e.target_id
      LEFT JOIN time_sharing s ON s.user_id = e.user_id AND s.workspace_id = v_ws
        AND EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_ws AND m.user_id = e.user_id)
      WHERE e.deleted_at IS NULL AND e.ended_at IS NOT NULL
      GROUP BY e.kind
      ORDER BY e.kind;
  END
  $$;
REVOKE ALL ON FUNCTION app_item_time(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_item_time(uuid) TO fvoci_app;
