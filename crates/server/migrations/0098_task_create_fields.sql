-- 업무 한 건을 모든 칸과 함께 한 번에 만든다(D-105·P-31, docs/development.md 99절).
--
-- 업무 CSV 가져오기는 만들기(제목만) → 고치기(나머지 칸) 두 요청이라, 그 사이 고치기가 거절되거나 응답을 잃으면 제목만 있는
-- 업무가 남았다(#56, 감사 W1~W3). 이제 만들기가 고치기(`app_update_task`)와 같은 칸을 받아 같은 확인을 거친 뒤 한 번에
-- 넣는다. 확인 하나라도 실패하면 이 함수를 부른 트랜잭션이 업무·담당·감사 기록·번호 할당을 모두 되돌린다.
--
-- 1. 같은 요청 id(`tasks_request`)는 전처럼 무엇보다 먼저 찾아 그 업무를 돌려준다. 보낸 칸이 달라도 다시 쓰거나 견주지
--    않는다(기존 멱등 계약 그대로: 응답을 잃은 재시도가 그사이 다른 사람이 고친 값을 되돌리지 않는다).
-- 2. 확인은 `app_update_task`(0082)와 같다: 상태·우선순위 값, 시작일 ≤ 마감일, 이정표는 시작일 없음, 담당자는 그 항목을
--    볼 수 있는 멤버(`app_check_assignee`, 멤버 행 공유 잠금은 사용자 id 순). 글자 수·태그·날짜 범위는 서버(Rust)가 고치기와
--    같은 함수로 먼저 본다.
-- 3. 열 잠금과 위치는 받은 상태의 열이다(전에는 늘 '할 일'). 잠금 순서는 전과 같다: 열 → 상위 업무 → 항목 공유 → 멤버 → 번호.
--    완료로 만들면 완료 시각을 적는다. 담당 지정마다 감사 기록을 남긴다(고치기와 같은 'assign').
-- 4. 옛 서명(제목·상위만)은 지운다(D-92). 제목만 보내는 만들기(빠른 추가 등)는 서버가 기본값을 채워 부른다.

DROP FUNCTION app_create_task(uuid, uuid, text, uuid, uuid[]);

-- id: 업무, 만들기 감사, 서로 다른 담당자마다 감사 1개(2 + 담당자 수 이하).
CREATE FUNCTION app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text,
                                p_status text, p_priority text, p_start date, p_due date, p_estimate integer,
                                p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_item   items%ROWTYPE;
    v_access text := app_item_access(p_item);
    v_id     uuid;
    v_number bigint;
    v_uid    uuid;
    v_people uuid[];
    v_at     integer := 2;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM tasks WHERE item_id = p_item AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_status IS NULL OR p_status NOT IN ('todo', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF p_priority IS NULL OR p_priority NOT IN ('none', 'low', 'medium', 'high', 'urgent') THEN
      RAISE EXCEPTION 'invalid priority' USING ERRCODE = '22023';
    END IF;
    IF p_start IS NOT NULL AND p_due IS NOT NULL AND p_start > p_due THEN
      RAISE EXCEPTION 'the start date must not be after the due date' USING ERRCODE = '22023';
    END IF;
    IF p_milestone AND p_start IS NOT NULL THEN
      RAISE EXCEPTION 'a milestone has only a due date' USING ERRCODE = '22023';
    END IF;
    IF p_estimate IS NOT NULL AND p_estimate < 0 THEN
      RAISE EXCEPTION 'estimate_minutes must not be negative' USING ERRCODE = '22023';
    END IF;
    PERFORM app_lock_lane(p_item, p_status);
    PERFORM app_check_task_parent(NULL, p_parent, p_item);
    PERFORM app_lock_item(p_item);
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    SELECT coalesce(array_agg(u ORDER BY u), '{}') INTO v_people
    FROM (SELECT DISTINCT u FROM unnest(coalesce(p_assignees, '{}')) AS u) s;
    FOREACH v_uid IN ARRAY v_people LOOP
      PERFORM app_check_assignee(v_item.workspace_id, p_item, v_uid);
    END LOOP;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'task', v_id);
    INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, description, status, priority, start_date,
                       due_date, estimate_minutes, tags, milestone, completed_at, position, created_by, updated_by,
                       request_id)
    VALUES (v_id, v_item.workspace_id, p_item, p_parent, v_number, btrim(p_title), coalesce(p_description, ''),
            p_status, p_priority, p_start, p_due, p_estimate, coalesce(p_tags, '{}'), coalesce(p_milestone, false),
            CASE WHEN p_status = 'done' THEN now() END, app_task_lane_end(p_item, p_status), v_user, v_user,
            p_request_id);
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'task', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    FOREACH v_uid IN ARRAY v_people LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_item.workspace_id, 'assign', 'task', v_id,
                        jsonb_build_object('user', v_uid));
    END LOOP;
    RETURN v_id;
  END
  $$;

REVOKE ALL ON FUNCTION app_create_task(uuid, uuid, text, uuid, text, text, text, date, date, integer, text[], uuid[],
                                       boolean, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_create_task(uuid, uuid, text, uuid, text, text, text, date, date, integer, text[], uuid[],
                                          boolean, uuid[]) TO fvoci_app;
