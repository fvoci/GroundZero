-- 남은 항목 ②(docs/development.md 82절): 색인 작업이 몇 초 안에 끝나지 않으면 대기열에 든 것으로 보고 행을 지웠다. 그 뒤
-- 작업이 실패해도 다시 하지 않았다. 이제 Meilisearch가 받은 작업 번호와 그때 보낸 세대(`queued_at`)를 남기고, 성공을
-- 확인한 뒤에만 지운다. 기다리는 동안은 다시 보내지 않고 실패 횟수(`attempts`, 꺼낼 때 하나씩 늚)도 쓰지 않는다. 그 사이
-- 더 새 변경이 오르면(`queued_at`이 바뀜) 앞 작업이 끝난 뒤 남겨 다시 보낸다. 실패·취소·없는 작업은 다시 보낸다.
-- Meilisearch 데이터를 지워 작업 번호가 처음부터 다시 매겨져도, 색인을 새로 만들 때 모두 다시 올라(`queued_at` 바뀜) 옛
-- 번호의 성공이 새 세대를 지우지 않는다.

ALTER TABLE search_queue
  ADD COLUMN task_uid bigint,
  ADD COLUMN task_queued_at timestamptz,
  ADD CONSTRAINT search_queue_task_pair CHECK ((task_uid IS NULL) = (task_queued_at IS NULL));
CREATE INDEX search_queue_task ON search_queue (task_uid) WHERE task_uid IS NOT NULL;

-- 꺼내기: 보낸 작업을 기다리는 행은 꺼내지 않는다. 함수 본문은 0024와 같고 "바뀐 곳"만 다르다.
CREATE OR REPLACE FUNCTION app_system_search_claim(p_limit integer)
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
        -- 바뀐 곳: 보낸 작업을 기다리는 행은 그 작업이 끝날 때까지 꺼내지 않는다.
        AND s.task_uid IS NULL
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING q.kind, q.target_id, q.queued_at
  $$;

-- 보냈지만 아직 끝나지 않은 작업: 행마다 작업 번호와 보낸 세대를 남기고 표시를 푼다.
CREATE FUNCTION app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamptz[], p_task bigint)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN;
    END IF;
    UPDATE search_queue q SET task_uid = p_task, task_queued_at = d.queued_at, claimed_until = NULL
    FROM unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE q.kind = d.kind AND q.target_id = d.id;
  END
  $$;

-- 기다리는 작업 번호들(확인할 차례인 것). 표시만 하고 실패 횟수는 늘리지 않는다.
CREATE FUNCTION app_system_search_pending(p_limit integer) RETURNS TABLE (task_uid bigint)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH picked AS (
      UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
      WHERE (q.kind, q.target_id) IN (
        SELECT s.kind, s.target_id FROM search_queue s
        WHERE app_user_id() IS NULL AND s.task_uid IS NOT NULL
          AND (s.claimed_until IS NULL OR s.claimed_until < now())
        ORDER BY s.queued_at
        LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
      RETURNING q.task_uid)
    SELECT DISTINCT picked.task_uid FROM picked
  $$;

-- 작업의 결과. 'succeeded': 그 작업으로 보낸 세대 그대로인 행만 지우고, 그 사이 더 새 변경이 오른 행은 다시 보내게 한다.
-- 'failed'(실패·취소·없는 작업): 다시 보내게 한다(실패 횟수는 꺼낼 때 셌다). 'pending': 표시만 푼다. 지운 수.
CREATE FUNCTION app_system_search_settled(p_task bigint, p_outcome text) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    IF p_outcome NOT IN ('succeeded', 'failed', 'pending') THEN
      RAISE EXCEPTION 'unknown search task outcome' USING ERRCODE = '22023';
    END IF;
    IF p_outcome = 'pending' THEN
      UPDATE search_queue SET claimed_until = NULL WHERE task_uid = p_task;
      RETURN 0;
    END IF;
    IF p_outcome = 'succeeded' THEN
      DELETE FROM search_queue WHERE task_uid = p_task AND queued_at = task_queued_at;
      GET DIAGNOSTICS v_count = ROW_COUNT;
    END IF;
    UPDATE search_queue SET task_uid = NULL, task_queued_at = NULL, claimed_until = NULL WHERE task_uid = p_task;
    RETURN v_count;
  END
  $$;

REVOKE ALL ON FUNCTION app_system_search_submitted(text[], uuid[], timestamptz[], bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_pending(integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_search_settled(bigint, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_system_search_submitted(text[], uuid[], timestamptz[], bigint),
  app_system_search_pending(integer),
  app_system_search_settled(bigint, text)
TO fvoci_app;
