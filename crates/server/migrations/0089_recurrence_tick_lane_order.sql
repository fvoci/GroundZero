-- 반복 회차 맞추기의 열 잠금 순서 (코드 전반 감사 S3).
--
-- 작업 프로세스의 반복 회차 맞추기(`app_system_recurrence_tick`)는 모든 반복을 한 트랜잭션에서 id 순으로 돌며, 반복마다
-- 그 뿌리 업무의 열(예: 할 일)을 잠그고 끝까지 쥐었다. 같은 항목에 뿌리가 '할 일'인 반복과 '완료'인 반복이 있으면
-- 할 일 → 완료 순으로 잡게 되는데, 칸반 이동·항목 동기화는 (항목, 상태 이름순: 완료 → 진행 중 → 할 일)으로 잡는다. 그래서
-- 완료 열을 쥔 채 할 일 열을 기다리는 사용자 요청과 맞물려 교착(40P01)이 났고, 사용자 쪽은 약 1초 뒤 409, 작업 쪽은 그
-- 반복의 회차를 다음 주기까지 건너뛰었다. 이제 반복을 항목 순으로 돌고, 한 항목의 첫 반복 앞에서 그 항목의 세 열을 항목
-- 동기화(`app_sync_item_series`)와 같은 순서로 먼저 잠근다. id 몫을 나누는 순서도 같은 순서를 따른다(개수 함수는 합이라
-- 그대로다). 열 잠금이 실패하면(교착 상대로 골라짐, 잠금 시간 초과 등) 전처럼 그 항목의 반복만 경고하고 건너뛴다
-- (전에는 잠금이 반복별 예외 블록 안에서 잡혔다). 그 항목의 id 몫도 건너뛰어 다음 항목의 몫은 그대로다.
-- 함수 본문은 0073의 마지막 정의와 같고 순서와 열 잠금만 다르다.

-- 작업 프로세스: 모든 반복. id는 반복마다 `app_task_series_bound` 몫(개수는 `_ids`).
CREATE OR REPLACE FUNCTION app_system_recurrence_tick(p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row  record;
    v_item uuid;
    v_made integer := 0;
    v_at   integer := 0;
    v_need integer;
    v_skip boolean := false;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_row IN
      SELECT s.id, s.item_id FROM task_series s
      JOIN tasks r ON r.id = s.root_task_id
      JOIN items i ON i.id = s.item_id
      JOIN workspaces w ON w.id = s.workspace_id
      WHERE s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL AND w.deleted_at IS NULL
      ORDER BY s.item_id, s.id
    LOOP
      -- 항목이 바뀌면 그 항목의 세 열을 사용자 요청과 같은 순서로 먼저 잠근다(트랜잭션 끝까지 쥔다).
      IF v_item IS DISTINCT FROM v_row.item_id THEN
        v_item := v_row.item_id;
        v_skip := false;
        BEGIN
          PERFORM app_lock_lane(v_item, 'done');
          PERFORM app_lock_lane(v_item, 'in_progress');
          PERFORM app_lock_lane(v_item, 'todo');
        EXCEPTION WHEN OTHERS THEN
          RAISE WARNING 'recurrence lane locks of item % failed with SQLSTATE %', v_item, SQLSTATE;
          v_skip := true;
        END;
      END IF;
      v_need := app_task_series_bound(v_row.id);
      IF NOT v_skip THEN
        BEGIN
          v_made := v_made + app_sync_task_series_as(v_row.id, true,
                                                     coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
        EXCEPTION WHEN OTHERS THEN
          RAISE WARNING 'recurrence sync of series % failed with SQLSTATE %', v_row.id, SQLSTATE;
        END;
      END IF;
      v_at := v_at + v_need;
    END LOOP;
    RETURN v_made;
  END
  $$;
