-- 검색 색인 보내기 직전의 점유 재확인 (코드 전반 감사 S12).
--
-- 색인 대기열의 꺼냄(`app_system_search_claim`)은 2분짜리 시간 점유일 뿐이다. 작업 프로세스가 첨부 내려받기 등으로 2분을
-- 넘기는 사이 사용자가 문서를 고치면(세대 `queued_at`이 바뀜) 다른 작업 프로세스가 그 행을 다시 꺼내 새 내용을 먼저
-- 보내고, 늦은 쪽이 옛 내용을 덮어 색인이 지운 문단을 계속 찾게 했다(다음 변경 전까지). 보내기 직전에 이 함수로 "아직
-- 쥐고 있고(점유가 남음) 세대가 그대로인 행"만 골라 점유를 늘린다. 고르지 못한 행은 보내지 않는다(대기열에 남아 다음
-- 차례에 새 내용으로 처리된다).

CREATE FUNCTION app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamptz[])
  RETURNS TABLE (kind text, target_id uuid)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
    FROM unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at
      AND q.task_uid IS NULL AND q.claimed_until > now()
    RETURNING q.kind, q.target_id
  $$;

REVOKE ALL ON FUNCTION app_system_search_recheck(text[], uuid[], timestamptz[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_system_search_recheck(text[], uuid[], timestamptz[]) TO fvoci_app;
