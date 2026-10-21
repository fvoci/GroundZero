-- 검색 색인 보내기 직전의 점유 재확인 (코드 전반 감사 S12).
--
-- 색인 대기열의 꺼냄(`app_system_search_claim`)은 2분짜리 시간 점유일 뿐이다. 작업 프로세스가 첨부 내려받기 등으로 2분을
-- 넘기는 사이 사용자가 문서를 고치면(세대 `queued_at`이 바뀜) 다른 작업 프로세스가 그 행을 다시 꺼내 새 내용을 먼저
-- 보내고, 늦은 쪽이 옛 내용을 덮어 색인이 지운 문단을 계속 찾게 했다(다음 변경 전까지). 보내기 직전에 이 함수로 "세대가
-- 그대로이고 그 뒤 아무도 다시 꺼내지 않은 행"만 골라 점유를 늘린다. 고르지 못한 행은 보내지 않는다(대기열에 남아 다음
-- 차례에 새 내용으로 처리된다).
--
-- 리뷰 반영:
-- - 점유는 꺼낼 때마다 오르는 `attempts`로 구별한다. 같은 세대를 다른 작업 프로세스가 다시 꺼냈으면 `attempts`가 달라 앞
--   작업 프로세스는 보내지 않는다(같은 문서를 두 번 보내지 않음). 점유 시각이 지났어도 아무도 다시 꺼내지 않았으면 그대로
--   보낸다. 첨부가 많은 묶음은 준비가 2분을 넘길 수 있는데, 시각만 보면 매번 버리고 `attempts`만 올라 10회 뒤 영영 꺼내지
--   않게 됐다.
-- - 보내지 않기로 한 행은 `app_system_search_release_stale`로 이 꺼냄의 점유를 바로 푼다(그 행이 아직 이 점유이거나, 그 뒤
--   다시 대기열에 올라 `attempts`가 0이고 아무도 꺼내지 않았을 때). 그대로 두면 새 세대를 남은 점유 시간(최대 2분) 동안
--   아무도 꺼내지 못해, 그 사이 고친 문서가 검색에 늦게 나왔다. 재확인과 같은 문장에서 풀면, 문장이 시작된 뒤 커밋된 다시
--   올리기를 옛 값으로 판단해 놓치므로 따로 부른다.

CREATE FUNCTION app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamptz[], p_attempts integer[])
  RETURNS TABLE (kind text, target_id uuid)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
    FROM unnest(p_kinds, p_ids, p_queued, p_attempts) AS d(kind, id, queued_at, attempts)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at AND q.attempts = d.attempts
      AND q.task_uid IS NULL
    RETURNING q.kind, q.target_id
  $$;

CREATE FUNCTION app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamptz[], p_attempts integer[])
  RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE search_queue q SET claimed_until = NULL
    FROM unnest(p_kinds, p_ids, p_queued, p_attempts) AS d(kind, id, queued_at, attempts)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.task_uid IS NULL
      AND ((q.queued_at = d.queued_at AND q.attempts = d.attempts) OR q.attempts = 0)
  $$;

REVOKE ALL ON FUNCTION app_system_search_recheck(text[], uuid[], timestamptz[], integer[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_release_stale(text[], uuid[], timestamptz[], integer[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_system_search_recheck(text[], uuid[], timestamptz[], integer[]) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_system_search_release_stale(text[], uuid[], timestamptz[], integer[]) TO fvoci_app;
