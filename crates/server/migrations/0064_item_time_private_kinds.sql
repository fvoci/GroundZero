-- 감사 13번(docs/development.md 81절): 항목 시간 합계가 공유하지 않은 멤버의 기록도 종류별 줄(값 0)로 돌려주어,
-- 다른 사람에게 그 항목에 어떤 종류의 시간을 기록했는지가 드러났다(D-32 개인 시간 기록 기본 비공개).
-- 내 기록과 공유한 멤버의 기록만 모은다. 함수 본문은 0052와 같고 "바뀐 곳"만 다르다.

CREATE OR REPLACE FUNCTION app_item_time(p_item uuid)
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
        -- 바뀐 곳: 남의 기록은 공유한 사람 것만 센다(종류 줄도 만들지 않는다).
        AND (e.user_id = app_user_id() OR s.user_id IS NOT NULL)
      GROUP BY e.kind
      ORDER BY e.kind;
  END
  $$;
