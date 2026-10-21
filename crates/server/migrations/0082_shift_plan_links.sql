-- 재배치 계획의 오래됨 판정과 관계 편집의 기준 조건 (D-31·D-91·D-92, 코드 전반 감사 S2·S4·W4).
--
-- 1. 오래된 승인 목록(S2): 미리본 뒤 후속 업무의 날짜 모양이 바뀌면(시작일을 지우거나 마일스톤으로 바꿈, 판이 오름)
--    판 비교에서 오래됐다고 표시한 뒤에도 날짜 모양 검사가 먼저 22023(400)을 냈다. 화면은 400만 보고 다시 미리 보지
--    못했다. 오래된 항목(판·관계 기준)은 날짜 모양 검사 전에 걸러 409 `shift_conflict`로 돌린다(아무것도 바꾸지 않음).
-- 2. 관계가 뒤집힌 계획(S4): A→B로 미리본 뒤 A→B를 지우고 B→A를 만들면 B는 지금 A의 선행 쪽이라 'invalid'(400)였다.
--    0078은 "순환이 없으니 선행 쪽은 후속이었던 적이 없다"고 보았지만 그것은 한 순간에만 맞다. 같은 항목인데 지금
--    후속이 아닌 업무는 모두 오래된 계획('stale', 409)으로 본다. 고치는 업무 자신, 관계 경로의 선행 업무, 없는 업무·다른
--    항목 업무는 400 그대로다.
-- 3. 관계 편집의 기준 조건(W4): 관계 설정이 아무 기준 없이 저장해, 화면이 연 뒤 다른 사람이 바꾼 종류·지연일을 말없이
--    덮었다(감사 기록만 남음). 화면이 편집을 시작한 관계의 종류·지연일(`p_base_kind`·`p_base_lag`, 새 관계면 둘 다 NULL)을
--    보내고, 잠근 뒤의 관계와 다르면 아무것도 바꾸지 않고 'link_conflict'(409, 최신본 포함)를 돌려준다. 새로 만들려는데
--    이미 있거나, 고치려는데 그사이 지워졌어도 같다(지운 관계를 말없이 다시 만들지 않음). 지금 관계가 이미 보낸 값과
--    같으면 충돌이 아니다(같은 요청의 재시도는 그대로 성공).
-- 4. D-92: 본문 없는 관계 PUT(옛 요청: 없으면 FS·0으로 만들고 있으면 그대로)과 그 전용 함수 `app_add_task_dependency`를
--    없앤다. 개발 예제(`deploy/postgres/dev-module-seed.sql`)는 `app_set_task_dependency`로 바꿨다. 기존 관계 행은 그대로다.
-- 함수 본문은 0078의 마지막 정의와 같고 위 부분만 다르다.

CREATE OR REPLACE FUNCTION app_shift_target(p_from uuid, p_task uuid, p_include_from boolean) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE
      WHEN t.id IS NULL OR t.item_id <> f.item_id THEN 'invalid'
      WHEN t.id = f.id THEN CASE WHEN p_include_from THEN 'successor' ELSE 'invalid' END
      WHEN EXISTS (
        WITH RECURSIVE down(id) AS (
          SELECT d.successor_id FROM task_dependencies d WHERE d.predecessor_id = f.id
          UNION
          SELECT d.successor_id FROM task_dependencies d JOIN down r ON d.predecessor_id = r.id
        )
        SELECT 1 FROM down WHERE id = t.id
      ) THEN 'successor'
      ELSE 'stale'
    END
    FROM tasks f LEFT JOIN tasks t ON t.id = p_task
    WHERE f.id = p_from
  $$;

CREATE OR REPLACE FUNCTION app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text,
                                p_status text, p_priority text, p_start date, p_due date, p_estimate integer,
                                p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb,
                                p_ids uuid[]) RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_task   tasks%ROWTYPE;
    v_other  tasks%ROWTYPE;
    v_uid    uuid;
    v_shift  jsonb;
    v_ids    uuid[];
    v_start  date;
    v_due    date;
    v_new    uuid[];
    v_lock   uuid[];
    v_at     integer := 1;
    v_state  text;
    v_stale  boolean := false;
  BEGIN
    IF p_shifts IS NULL OR jsonb_typeof(p_shifts) <> 'array' THEN
      RAISE EXCEPTION 'shifts must be an array' USING ERRCODE = '22023';
    END IF;
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(DISTINCT (s ->> 'task_id')::uuid) INTO v_ids FROM jsonb_array_elements(p_shifts) s;
    IF coalesce(cardinality(v_ids), 0) <> jsonb_array_length(p_shifts) THEN
      RAISE EXCEPTION 'each shifted task must appear once' USING ERRCODE = '22023';
    END IF;
    IF p_task = ANY (coalesce(v_ids, '{}')) THEN
      RAISE EXCEPTION 'the edited task cannot also be a shifted successor' USING ERRCODE = '22023';
    END IF;
    -- 이 업무와 옮길 후속 업무(같은 항목만, 다른 항목 id는 잠그지 않고 아래에서 거부)를 열 → 행 순으로 잠근다.
    SELECT array_agg(id) INTO v_lock FROM tasks
    WHERE id = ANY (coalesce(v_ids, '{}')) AND item_id = (SELECT item_id FROM tasks WHERE id = p_task);
    PERFORM app_lock_task_rows(array_prepend(p_task, coalesce(v_lock, '{}')), p_status);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    IF jsonb_array_length(p_shifts) > 0 THEN
      -- 후속 여부 확인과 기록 사이에 선행 관계가 바뀌지 않게(항목 공유 → 관계 잠금 순서).
      PERFORM app_lock_dependencies(v_task.item_id);
    END IF;
    IF v_task.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    IF p_status NOT IN ('todo', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF p_priority NOT IN ('none', 'low', 'medium', 'high', 'urgent') THEN
      RAISE EXCEPTION 'invalid priority' USING ERRCODE = '22023';
    END IF;
    IF p_start IS NOT NULL AND p_due IS NOT NULL AND p_start > p_due THEN
      RAISE EXCEPTION 'the start date must not be after the due date' USING ERRCODE = '22023';
    END IF;
    IF p_milestone AND p_start IS NOT NULL THEN
      RAISE EXCEPTION 'a milestone has only a due date' USING ERRCODE = '22023';
    END IF;

    -- 승인한 후속 업무: 이 업무에서 이어지는 후속이어야 하고, 기준 버전이 모두 맞아야 한다. 0078: 같은 항목인데 지금은
    -- 이어지지 않는 업무(미리본 뒤 관계가 지워짐)는 오래된 계획이라 `shift_conflict`(아무것도 바꾸지 않음), 처음부터 옮길
    -- 수 없는 업무(없음·다른 항목)는 400 그대로다(0082: 지금 선행 쪽인 업무도 오래된 계획).
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_state := app_shift_target(p_task, v_other.id, false);
      IF v_state = 'invalid' THEN
        RAISE EXCEPTION 'a shifted task must be a successor of the edited task' USING ERRCODE = '22023';
      END IF;
      -- 0082: 오래된 항목은 날짜 모양을 보기 전에 거른다(미리본 뒤 시작일을 지운 후속도 400이 아니라 409).
      IF v_state = 'stale' OR v_other.version IS DISTINCT FROM (v_shift ->> 'base_version')::integer THEN
        v_stale := true;
        CONTINUE;
      END IF;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      IF (v_start IS NULL) <> (v_other.start_date IS NULL) OR (v_due IS NULL) <> (v_other.due_date IS NULL)
         OR (v_start IS NOT NULL AND v_due IS NOT NULL AND v_start > v_due) THEN
        RAISE EXCEPTION 'a shift moves existing dates only' USING ERRCODE = '22023';
      END IF;
    END LOOP;
    IF v_stale THEN
      RETURN 'shift_conflict';
    END IF;

    -- 새로 넣는 담당자만 확인한다(이미 있는 담당 때문에 다른 필드 고치기가 막히지 않게). 새 담당자 목록은 여기서
    -- 한 번 정하고 그 목록만 넣는다(그사이 다른 트랜잭션이 지운 기존 담당을 확인 없이 되살리지 않게).
    -- 멤버 행 공유 잠금은 사용자 id 순으로 잡는다.
    SELECT coalesce(array_agg(u ORDER BY u), '{}') INTO v_new
    FROM (SELECT DISTINCT u FROM unnest(coalesce(p_assignees, '{}')) AS u
          WHERE NOT EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = p_task AND a.user_id = u)) s;
    FOREACH v_uid IN ARRAY v_new LOOP
      PERFORM app_check_assignee(v_task.workspace_id, v_task.item_id, v_uid);
    END LOOP;

    UPDATE tasks SET
      title = btrim(p_title), description = coalesce(p_description, ''), status = p_status,
      priority = p_priority, start_date = p_start, due_date = p_due, estimate_minutes = p_estimate,
      tags = coalesce(p_tags, '{}'), milestone = p_milestone, version = version + 1, updated_by = v_user,
      updated_at = now(),
      completed_at = CASE WHEN p_status = 'done' AND v_task.status <> 'done' THEN now()
                          WHEN p_status <> 'done' THEN NULL ELSE completed_at END,
      position = CASE WHEN p_status <> v_task.status THEN app_task_lane_end(v_task.item_id, p_status)
                      ELSE position END
    WHERE id = p_task;

    IF v_task.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_task.workspace_id, 'rename', 'task', p_task,
                        jsonb_build_object('from', v_task.title, 'to', btrim(p_title)));
    END IF;
    FOR v_uid IN
      SELECT a.user_id FROM task_assignees a
      WHERE a.task_id = p_task AND NOT (a.user_id = ANY (coalesce(p_assignees, '{}')))
    LOOP
      DELETE FROM task_assignees WHERE task_id = p_task AND user_id = v_uid;
      IF FOUND THEN
        v_at := v_at + 1;
        PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'unassign', 'task', p_task,
                          jsonb_build_object('user', v_uid));
      END IF;
    END LOOP;
    FOREACH v_uid IN ARRAY v_new LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (p_task, v_uid, v_user);
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'assign', 'task', p_task,
                        jsonb_build_object('user', v_uid));
    END LOOP;

    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      UPDATE tasks SET start_date = v_start, due_date = v_due, version = version + 1,
                       updated_by = v_user, updated_at = now()
      WHERE id = v_other.id;
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'reschedule', 'task', v_other.id,
                        jsonb_build_object('reason', 'predecessor_moved', 'predecessor', p_task,
                                           'from', jsonb_build_array(v_other.start_date, v_other.due_date),
                                           'to', jsonb_build_array(v_start, v_due)));
    END LOOP;
    RETURN 'updated';
  END
  $$;

DROP FUNCTION app_set_task_dependency(uuid, uuid, text, integer, jsonb, uuid[]);

CREATE FUNCTION app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer,
                                        p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[])
  RETURNS TABLE (outcome text, old_kind text, old_lag integer)
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_succ  tasks%ROWTYPE;
    v_pred  tasks%ROWTYPE;
    v_other tasks%ROWTYPE;
    v_old   task_dependencies%ROWTYPE;
    v_shift jsonb;
    v_ids   uuid[];
    v_state text;
    v_stale boolean := false;
    v_lock  uuid[];
    v_start date;
    v_due   date;
    v_at    integer := 1;
  BEGIN
    IF p_kind IS NULL OR p_kind NOT IN ('FS', 'SS', 'FF', 'SF') THEN
      RAISE EXCEPTION 'kind must be FS, SS, FF or SF' USING ERRCODE = '22023';
    END IF;
    IF p_lag IS NULL OR p_lag < 0 OR p_lag > 3650 THEN
      RAISE EXCEPTION 'lag_days must be between 0 and 3650' USING ERRCODE = '22023';
    END IF;
    IF p_shifts IS NULL OR jsonb_typeof(p_shifts) <> 'array' THEN
      RAISE EXCEPTION 'shifts must be an array' USING ERRCODE = '22023';
    END IF;
    PERFORM app_require_task_write(p_successor);
    IF app_task_access(p_predecessor) IS NULL THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_succ FROM tasks WHERE id = p_successor;
    SELECT * INTO v_pred FROM tasks WHERE id = p_predecessor;
    IF v_pred.item_id <> v_succ.item_id THEN
      RAISE EXCEPTION 'dependencies must stay within one item' USING ERRCODE = '22023';
    END IF;
    IF p_predecessor = p_successor THEN
      RAISE EXCEPTION 'a task cannot depend on itself' USING ERRCODE = '22023';
    END IF;
    SELECT array_agg(DISTINCT (s ->> 'task_id')::uuid) INTO v_ids FROM jsonb_array_elements(p_shifts) s;
    IF coalesce(cardinality(v_ids), 0) <> jsonb_array_length(p_shifts) THEN
      RAISE EXCEPTION 'each shifted task must appear once' USING ERRCODE = '22023';
    END IF;
    -- 후속 업무와 옮길 업무(같은 항목만)를 열 → 행 순으로 잠그고, 항목 공유 → 관계 잠금(app_update_task와 같은 순서).
    SELECT array_agg(id) INTO v_lock FROM tasks
    WHERE id = ANY (coalesce(v_ids, '{}')) AND id <> p_successor AND item_id = v_succ.item_id;
    PERFORM app_lock_task_rows(array_prepend(p_successor, coalesce(v_lock, '{}')), NULL);
    PERFORM app_lock_item(v_succ.item_id);
    PERFORM app_lock_dependencies(v_succ.item_id);
    PERFORM app_require_task_write(p_successor);
    SELECT * INTO v_old FROM task_dependencies WHERE predecessor_id = p_predecessor AND successor_id = p_successor;
    -- 0082: 화면이 편집을 시작한 관계(새 관계면 없음)와 잠근 뒤의 관계가 다르면 아무것도 바꾸지 않는다. 지금 관계가
    -- 이미 보낸 종류·지연일과 같으면(응답을 잃은 같은 요청의 재시도 등) 충돌로 보지 않는다.
    IF NOT (v_old.predecessor_id IS NOT NULL AND v_old.kind = p_kind AND v_old.lag_days = p_lag)
       AND ((v_old.predecessor_id IS NULL AND p_base_kind IS NOT NULL)
            OR (v_old.predecessor_id IS NOT NULL
                AND (p_base_kind IS NULL OR v_old.kind IS DISTINCT FROM p_base_kind
                     OR v_old.lag_days IS DISTINCT FROM p_base_lag))) THEN
      RETURN QUERY SELECT 'link_conflict', v_old.kind, v_old.lag_days;
      RETURN;
    END IF;
    -- 새 관계: 후속에서 출발해 선행에 닿으면 순환이다.
    IF v_old.predecessor_id IS NULL AND EXISTS (
      WITH RECURSIVE reach(id) AS (
        SELECT p_successor
        UNION
        SELECT d.successor_id FROM task_dependencies d JOIN reach r ON d.predecessor_id = r.id
      )
      SELECT 1 FROM reach WHERE id = p_predecessor
    ) THEN
      RAISE EXCEPTION 'the dependency would create a cycle' USING ERRCODE = '22023';
    END IF;

    -- 승인한 재배치: 관계를 고친 후속 업무 자신이거나 그 뒤 업무여야 하고, 기준 버전이 모두 맞아야 한다. 0078: 같은
    -- 항목인데 지금은 이어지지 않는 업무(미리본 뒤 관계가 지워짐)는 오래된 계획이라 `shift_conflict`(아무것도 바꾸지
    -- 않음), 처음부터 옮길 수 없는 업무(없음·다른 항목·관계의 선행 업무)는 400 그대로다(0082: 지금 선행 쪽인 업무도 오래된 계획).
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_state := CASE WHEN v_other.id = p_predecessor THEN 'invalid'
                      ELSE app_shift_target(p_successor, v_other.id, true) END;
      IF v_state = 'invalid' THEN
        RAISE EXCEPTION 'a shifted task must be the successor or come after it' USING ERRCODE = '22023';
      END IF;
      -- 0082: 오래된 항목은 날짜 모양을 보기 전에 거른다.
      IF v_state = 'stale' OR v_other.version IS DISTINCT FROM (v_shift ->> 'base_version')::integer THEN
        v_stale := true;
        CONTINUE;
      END IF;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      IF (v_start IS NULL) <> (v_other.start_date IS NULL) OR (v_due IS NULL) <> (v_other.due_date IS NULL)
         OR (v_start IS NOT NULL AND v_due IS NOT NULL AND v_start > v_due) THEN
        RAISE EXCEPTION 'a shift moves existing dates only' USING ERRCODE = '22023';
      END IF;
    END LOOP;
    IF v_stale THEN
      RETURN QUERY SELECT 'shift_conflict', v_old.kind, v_old.lag_days;
      RETURN;
    END IF;

    IF v_old.predecessor_id IS NULL THEN
      INSERT INTO task_dependencies (predecessor_id, successor_id, item_id, created_by, kind, lag_days)
      VALUES (p_predecessor, p_successor, v_succ.item_id, v_user, p_kind, p_lag);
      PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'link_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor, 'kind', p_kind, 'lag_days', p_lag));
    ELSIF v_old.kind <> p_kind OR v_old.lag_days <> p_lag THEN
      UPDATE task_dependencies SET kind = p_kind, lag_days = p_lag
      WHERE predecessor_id = p_predecessor AND successor_id = p_successor;
      PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'change_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor,
                                           'from', jsonb_build_object('kind', v_old.kind, 'lag_days', v_old.lag_days),
                                           'to', jsonb_build_object('kind', p_kind, 'lag_days', p_lag)));
    ELSIF jsonb_array_length(p_shifts) = 0 THEN
      RETURN QUERY SELECT 'unchanged', v_old.kind, v_old.lag_days;
      RETURN;
    END IF;

    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      UPDATE tasks SET start_date = v_start, due_date = v_due, version = version + 1,
                       updated_by = v_user, updated_at = now()
      WHERE id = v_other.id;
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_succ.workspace_id, 'reschedule', 'task', v_other.id,
                        jsonb_build_object('reason', 'dependency_changed', 'predecessor', p_predecessor,
                                           'successor', p_successor,
                                           'from', jsonb_build_array(v_other.start_date, v_other.due_date),
                                           'to', jsonb_build_array(v_start, v_due)));
    END LOOP;
    RETURN QUERY SELECT CASE WHEN v_old.predecessor_id IS NULL THEN 'created' ELSE 'updated' END,
                        v_old.kind, v_old.lag_days;
  END
  $$;

REVOKE ALL ON FUNCTION app_shift_target(uuid, uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_task_dependency(uuid, uuid, text, integer, text, integer, jsonb, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_task_dependency(uuid, uuid, text, integer, text, integer, jsonb, uuid[]) TO fvoci_app;

-- D-92: 본문 없는 옛 관계 PUT 전용 함수.
DROP FUNCTION app_add_task_dependency(uuid, uuid, uuid[]);
