-- 칸반 이동의 조건부 저장(D-89). 다시 시도·되돌리기가 그사이 다른 사람이 바꾼 자리를 덮지 않게 한다.
--
-- 1. 업무마다 이동 판(`move_version`)을 둔다. 열(상태)이나 열 안 위치가 바뀌면 1 오른다. 어느 함수가 바꿨든 같게
--    표의 트리거가 올린다(이동·필드 고치기의 상태 바꾸기·열 다시 매기기 등). 필드 고치기의 판(`version`)과는 따로다.
--    직접 다른 값을 넣어도 트리거가 이전 값 또는 이전 값 + 1로 맞춘다.
-- 2. `app_move_task`에 선택 인자 `p_base_move_version`을 더한다. 주면 행을 잠근 같은 트랜잭션에서 비교하고, 다르면
--    아무것도 바꾸지 않고 `FV003`을 올린다(서버가 409 `move_conflict`로 돌려준다). 주지 않으면(NULL) 이전과 같다.
--    함수 본문은 0012와 같고 표시한 곳만 다르다.

ALTER TABLE tasks ADD COLUMN move_version integer NOT NULL DEFAULT 0;

CREATE FUNCTION app_tasks_move_version() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF NEW.status IS DISTINCT FROM OLD.status OR NEW.position IS DISTINCT FROM OLD.position THEN
      NEW.move_version := OLD.move_version + 1;
    ELSE
      NEW.move_version := OLD.move_version;
    END IF;
    RETURN NEW;
  END
  $$;
REVOKE ALL ON FUNCTION app_tasks_move_version() FROM PUBLIC;

CREATE TRIGGER tasks_move_version BEFORE UPDATE ON tasks
  FOR EACH ROW EXECUTE FUNCTION app_tasks_move_version();

DROP FUNCTION app_move_task(uuid, text, uuid);

CREATE FUNCTION app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task     tasks%ROWTYPE;
    v_before   tasks%ROWTYPE;
    v_lower    numeric;
    v_position numeric;
  BEGIN
    PERFORM app_require_task_write(p_task);
    -- 지금 열과 옮겨 갈 열을 먼저, 그다음 행(다시 매기기와의 교착 방지).
    PERFORM app_lock_task_rows(ARRAY[p_task], p_status);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- D-89: 읽은 뒤 그사이 이 업무의 열·위치가 바뀌었으면(이동 판이 다르면) 아무것도 바꾸지 않는다. 행을 잠근 뒤라
    -- 비교와 옮기기 사이에 다른 이동이 끼지 못한다.
    IF p_base_move_version IS NOT NULL AND v_task.move_version IS DISTINCT FROM p_base_move_version THEN
      RAISE EXCEPTION 'the task moved since it was read' USING ERRCODE = 'FV003';
    END IF;
    IF p_status NOT IN ('todo', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF p_before IS NOT NULL THEN
      SELECT * INTO v_before FROM tasks WHERE id = p_before;
      IF v_before.id IS NULL OR v_before.id = p_task OR v_before.item_id <> v_task.item_id
         OR v_before.status <> p_status OR app_task_access(p_before) IS NULL THEN
        RAISE EXCEPTION 'before must be a task in the same column' USING ERRCODE = '22023';
      END IF;
      SELECT max(position) INTO v_lower FROM tasks
      WHERE item_id = v_task.item_id AND status = p_status AND id <> p_task AND position < v_before.position;
      v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
      IF scale(v_position) > 40 THEN
        -- 같은 틈에 너무 많이 끼워 자릿수가 길어졌다. 열을 다시 매긴 뒤 계산한다.
        PERFORM app_renumber_lane(v_task.item_id, p_status);
        SELECT position INTO v_before.position FROM tasks WHERE id = p_before;
        SELECT max(position) INTO v_lower FROM tasks
        WHERE item_id = v_task.item_id AND status = p_status AND id <> p_task AND position < v_before.position;
        v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
      END IF;
    ELSE
      SELECT coalesce(max(position), 0) + 1 INTO v_position FROM tasks
      WHERE item_id = v_task.item_id AND status = p_status AND id <> p_task;
    END IF;
    UPDATE tasks SET
      status = p_status, position = v_position,
      version = CASE WHEN p_status <> v_task.status THEN version + 1 ELSE version END,
      completed_at = CASE WHEN p_status = 'done' AND v_task.status <> 'done' THEN now()
                          WHEN p_status <> 'done' THEN NULL ELSE completed_at END,
      updated_by = CASE WHEN p_status <> v_task.status THEN app_user_id() ELSE updated_by END,
      updated_at = CASE WHEN p_status <> v_task.status THEN now() ELSE updated_at END
    WHERE id = p_task;
  END
  $$;

REVOKE ALL ON FUNCTION app_move_task(uuid, text, uuid, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_move_task(uuid, text, uuid, integer) TO fvoci_app;
