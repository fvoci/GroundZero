-- 앱이 만든 id로 행을 만든다(D-88, 0070 머리말). 이 파일: 업무(만들기·고치기·부모·선행 관계·체크리스트·반복과
-- 회차 만들기), 일정, 시간 기록. 함수 본문은 마지막 정의와 같고 id 부분만 다르다.

------------------------------------------------------------------------------------------------------------------
-- 업무
------------------------------------------------------------------------------------------------------------------

-- id 2개: 업무, 감사 기록.
CREATE FUNCTION app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_ids uuid[])
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
    PERFORM app_lock_lane(p_item, 'todo');
    PERFORM app_check_task_parent(NULL, p_parent, p_item);
    PERFORM app_lock_item(p_item);
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'task', v_id);
    INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, position, created_by, updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, p_parent, v_number, btrim(p_title),
            app_task_lane_end(p_item, 'todo'), v_user, v_user, p_request_id);
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'task', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

-- id: 이름 변경 감사, 빠진 담당자마다, 새 담당자마다, 옮긴 후속 업무마다 감사 1개(개수는 `_ids`).
CREATE FUNCTION app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text,
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

    -- 승인한 후속 업무: 이 업무에서 이어지는 후속이어야 하고, 기준 버전이 모두 맞아야 한다.
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      IF v_other.id IS NULL OR v_other.item_id <> v_task.item_id OR NOT EXISTS (
        WITH RECURSIVE reach(id) AS (
          SELECT d.successor_id FROM task_dependencies d WHERE d.predecessor_id = p_task
          UNION
          SELECT d.successor_id FROM task_dependencies d JOIN reach r ON d.predecessor_id = r.id
        )
        SELECT 1 FROM reach WHERE id = v_other.id
      ) THEN
        RAISE EXCEPTION 'a shifted task must be a successor of the edited task' USING ERRCODE = '22023';
      END IF;
      IF v_other.version IS DISTINCT FROM (v_shift ->> 'base_version')::integer THEN
        RETURN 'shift_conflict';
      END IF;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      IF (v_start IS NULL) <> (v_other.start_date IS NULL) OR (v_due IS NULL) <> (v_other.due_date IS NULL)
         OR (v_start IS NOT NULL AND v_due IS NOT NULL AND v_start > v_due) THEN
        RAISE EXCEPTION 'a shift moves existing dates only' USING ERRCODE = '22023';
      END IF;
    END LOOP;

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

-- 이름 변경 1 + 지금 담당자 수(빠질 수 있는 사람) + 보낸 담당자 수 + 옮길 후속 업무 수.
CREATE FUNCTION app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_task_access(p_task) IS NULL THEN 0 ELSE
      1 + (SELECT count(*) FROM task_assignees WHERE task_id = p_task)::integer
        + coalesce(cardinality(p_assignees), 0)
        + CASE WHEN jsonb_typeof(p_shifts) = 'array' THEN jsonb_array_length(p_shifts) ELSE 0 END
    END
  $$;

-- id 1개.
CREATE FUNCTION app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task tasks%ROWTYPE;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_check_task_parent(p_task, p_parent, v_task.item_id);
    IF p_parent IS NULL THEN
      PERFORM app_lock_task_rows(ARRAY[p_task], NULL);
    END IF;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    IF v_task.parent_id IS NOT DISTINCT FROM p_parent THEN
      RETURN;
    END IF;
    UPDATE tasks SET parent_id = p_parent, updated_by = app_user_id(), updated_at = now() WHERE id = p_task;
    PERFORM app_audit(app_id(p_ids, 1), v_task.workspace_id, 'reparent', 'task', p_task,
                      jsonb_build_object('parent', p_parent));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_add_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_succ tasks%ROWTYPE;
    v_pred tasks%ROWTYPE;
  BEGIN
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
    PERFORM app_lock_item(v_succ.item_id);
    PERFORM app_lock_dependencies(v_succ.item_id);
    PERFORM app_require_task_write(p_successor);
    IF EXISTS (SELECT 1 FROM task_dependencies
               WHERE predecessor_id = p_predecessor AND successor_id = p_successor) THEN
      RETURN;
    END IF;
    -- 후속에서 출발해 선행에 닿으면 순환이다.
    IF EXISTS (
      WITH RECURSIVE reach(id) AS (
        SELECT p_successor
        UNION
        SELECT d.successor_id FROM task_dependencies d JOIN reach r ON d.predecessor_id = r.id
      )
      SELECT 1 FROM reach WHERE id = p_predecessor
    ) THEN
      RAISE EXCEPTION 'the dependency would create a cycle' USING ERRCODE = '22023';
    END IF;
    INSERT INTO task_dependencies (predecessor_id, successor_id, item_id, created_by)
    VALUES (p_predecessor, p_successor, v_succ.item_id, app_user_id());
    PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'link_predecessor', 'task', p_successor,
                      jsonb_build_object('predecessor', p_predecessor));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_succ tasks%ROWTYPE;
  BEGIN
    PERFORM app_require_task_write(p_successor);
    SELECT * INTO v_succ FROM tasks WHERE id = p_successor;
    PERFORM app_lock_item(v_succ.item_id);
    PERFORM app_lock_dependencies(v_succ.item_id);
    PERFORM app_require_task_write(p_successor);
    DELETE FROM task_dependencies WHERE predecessor_id = p_predecessor AND successor_id = p_successor;
    IF FOUND THEN
      PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'unlink_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor));
    END IF;
  END
  $$;

-- id: 보낸 줄마다 1개(같은 요청으로 이미 있는 줄은 쓰지 않음).
CREATE FUNCTION app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task tasks%ROWTYPE;
    v_item jsonb;
    v_next numeric;
    v_at   integer := 0;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task FOR UPDATE;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    SELECT coalesce(max(position), 0) + 1 INTO v_next FROM checklist_items WHERE task_id = p_task;
    FOR v_item IN SELECT * FROM jsonb_array_elements(coalesce(p_items, '[]'::jsonb)) LOOP
      v_at := v_at + 1;
      INSERT INTO checklist_items (id, task_id, text, checked, position, request_id, created_by)
      VALUES (app_id(p_ids, v_at), p_task, btrim(v_item ->> 'text'), coalesce((v_item ->> 'checked')::boolean, false),
              v_next, (v_item ->> 'request_id')::uuid, app_user_id())
      ON CONFLICT ON CONSTRAINT checklist_items_request DO NOTHING;
      IF FOUND THEN
        v_next := v_next + 1;
      END IF;
    END LOOP;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 반복 업무
------------------------------------------------------------------------------------------------------------------

-- id 2개: 반복(새로 정할 때), 감사 기록.
CREATE FUNCTION app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task   tasks%ROWTYPE;
    v_series task_series%ROWTYPE;
    v_anchor date;
    v_today  date;
  BEGIN
    PERFORM app_require_task_write(p_task);
    IF p_rule IS NOT NULL THEN
      PERFORM app_check_recurrence(p_rule);
    END IF;
    PERFORM app_check_timezone(p_timezone);
    PERFORM app_lock_task_rows(ARRAY[p_task], NULL);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    v_today := (now() AT TIME ZONE p_timezone)::date;
    IF v_task.series_id IS NOT NULL THEN
      SELECT * INTO v_series FROM task_series WHERE id = v_task.series_id FOR UPDATE;
      IF v_series.root_task_id <> p_task THEN
        RAISE EXCEPTION 'change the recurrence on the first occurrence' USING ERRCODE = '22023';
      END IF;
      IF (p_rule ->> 'until')::date < v_series.anchor THEN
        RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
      END IF;
      IF p_rule IS NOT NULL AND v_task.due_date IS NULL AND v_task.start_date IS NULL THEN
        RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
      END IF;
      IF p_rule IS NOT NULL AND v_task.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'only top-level tasks can repeat' USING ERRCODE = '22023';
      END IF;
      UPDATE task_series SET
        rule = p_rule, timezone = p_timezone, updated_at = now(),
        effective_from = CASE WHEN p_rule IS DISTINCT FROM v_series.rule
                              THEN greatest(v_series.anchor, v_today) ELSE effective_from END
      WHERE id = v_series.id;
      PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'set_recurrence', 'task', p_task,
                        jsonb_build_object('from', v_series.rule, 'to', p_rule, 'timezone', p_timezone));
      RETURN v_series.id;
    END IF;
    IF p_rule IS NULL THEN
      RETURN NULL;
    END IF;
    IF v_task.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'only top-level tasks can repeat' USING ERRCODE = '22023';
    END IF;
    v_anchor := coalesce(v_task.due_date, v_task.start_date);
    IF v_anchor IS NULL THEN
      RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
    END IF;
    IF (p_rule ->> 'until')::date < v_anchor THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    INSERT INTO task_series (id, workspace_id, item_id, root_task_id, rule, timezone, anchor, effective_from,
                             created_by)
    VALUES (app_id(p_ids, 1), v_task.workspace_id, v_task.item_id, p_task, p_rule, p_timezone, v_anchor, v_anchor,
            app_user_id())
    RETURNING * INTO v_series;
    UPDATE tasks SET series_id = v_series.id, occurrence_date = v_anchor WHERE id = p_task;
    PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'set_recurrence', 'task', p_task,
                      jsonb_build_object('from', NULL, 'to', p_rule, 'timezone', p_timezone));
    RETURN v_series.id;
  END
  $$;

-- 한 반복이 지금 만들 회차 수(아래 함수와 같은 고르기, 최대 50) × 회차마다 id 수(업무·요청 id·감사 3 + 체크리스트
-- 줄마다 id·요청 id 2). 권한 확인 전.
CREATE FUNCTION app_task_series_bound(p_series uuid) RETURNS integer
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_series  task_series%ROWTYPE;
    v_today   date;
    v_missing integer;
    v_lines   integer;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL OR v_series.rule IS NULL THEN
      RETURN 0;
    END IF;
    v_today := (now() AT TIME ZONE v_series.timezone)::date;
    SELECT count(*) INTO v_missing FROM (
      WITH wanted AS (
        SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_series.effective_from, v_today)
        UNION
        (SELECT occurrence
         FROM app_recurrence_dates(v_series.rule, v_series.anchor, greatest(v_today + 1, v_series.effective_from),
                                   greatest(v_today + 1, v_series.effective_from) + 3660)
         ORDER BY occurrence LIMIT 1)
      )
      SELECT 1 FROM wanted w
      WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = p_series AND t.occurrence_date = w.occurrence)
        AND NOT (w.occurrence = ANY (v_series.skipped_dates))
      LIMIT 50) x;
    SELECT count(*) INTO v_lines FROM checklist_items WHERE task_id = v_series.root_task_id;
    RETURN v_missing * (3 + 2 * v_lines);
  END
  $$;

-- id: 회차마다 업무·요청 id, 체크리스트 줄마다 id·요청 id, 감사 기록(개수는 `app_task_series_bound`).
CREATE FUNCTION app_sync_task_series_as(p_series uuid, p_system boolean, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_series task_series%ROWTYPE;
    v_root   tasks%ROWTYPE;
    v_today  date;
    v_date   date;
    v_id     uuid;
    v_made   integer := 0;
    v_user   uuid := CASE WHEN p_system THEN NULL ELSE app_user_id() END;
    v_module uuid;
    v_uid    uuid;
    v_at     integer := 0;
    v_count  integer;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    PERFORM app_lock_task_rows(ARRAY[v_series.root_task_id], 'todo');
    SELECT * INTO v_root FROM tasks WHERE id = v_series.root_task_id;
    PERFORM app_lock_item(v_root.item_id);
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    -- 작업 프로세스는 만든 사람 이름으로 회차를 만들고 감사 기록에는 system으로 남긴다.
    v_user := coalesce(v_user, v_series.created_by);
    IF v_series.rule IS NULL OR (v_root.due_date IS NULL AND v_root.start_date IS NULL)
       OR v_root.deleted_at IS NOT NULL OR app_item_archived(v_root.item_id) THEN
      RETURN 0;
    END IF;
    v_today := (now() AT TIME ZONE v_series.timezone)::date;
    SELECT module_id INTO v_module FROM items WHERE id = v_root.item_id;
    FOR v_date IN
      WITH wanted AS (
        SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_series.effective_from, v_today)
        UNION
        (SELECT occurrence
         FROM app_recurrence_dates(v_series.rule, v_series.anchor, greatest(v_today + 1, v_series.effective_from),
                                   greatest(v_today + 1, v_series.effective_from) + 3660)
         ORDER BY occurrence LIMIT 1)
      )
      SELECT w.occurrence FROM wanted w
      WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = p_series AND t.occurrence_date = w.occurrence)
        AND NOT (w.occurrence = ANY (v_series.skipped_dates))
      ORDER BY w.occurrence
      LIMIT 50
    LOOP
      v_id := app_id(p_ids, v_at + 1);
      INSERT INTO tasks (id, workspace_id, item_id, number, title, description, priority, start_date, due_date,
                         estimate_minutes, tags, milestone, position, created_by, updated_by, request_id,
                         series_id, occurrence_date)
      VALUES (v_id, v_root.workspace_id, v_root.item_id,
              app_allocate_number(v_root.workspace_id, v_module, 'task', v_id),
              v_root.title, v_root.description, v_root.priority,
              v_root.start_date + (v_date - v_series.anchor), v_root.due_date + (v_date - v_series.anchor),
              v_root.estimate_minutes, v_root.tags, v_root.milestone, app_task_lane_end(v_root.item_id, 'todo'),
              v_user, v_user, app_id(p_ids, v_at + 2), p_series, v_date)
      ON CONFLICT (series_id, occurrence_date) DO NOTHING;
      v_at := v_at + 2;
      IF NOT FOUND THEN
        CONTINUE;
      END IF;
      INSERT INTO checklist_items (id, task_id, text, checked, position, request_id, created_by)
      SELECT app_id(p_ids, v_at + 2 * c.n - 1), v_id, c.text, false, c.position, app_id(p_ids, v_at + 2 * c.n), v_user
      FROM (SELECT c.*, (row_number() OVER (ORDER BY c.position, c.id))::integer AS n
            FROM checklist_items c WHERE c.task_id = v_root.id) c;
      GET DIAGNOSTICS v_count = ROW_COUNT;
      v_at := v_at + 2 * v_count;
      FOR v_uid IN SELECT a.user_id FROM task_assignees a WHERE a.task_id = v_root.id ORDER BY a.user_id LOOP
        IF EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_root.workspace_id AND m.user_id = v_uid)
           AND app_item_access_as(v_root.item_id, v_uid) IS NOT NULL THEN
          INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
        END IF;
      END LOOP;
      v_at := v_at + 1;
      INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, v_at), v_root.workspace_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END,
              'create', 'task', v_id,
              jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'series', p_series,
                                 'occurrence', v_date)
                || CASE WHEN p_system THEN jsonb_build_object('system', true) ELSE '{}'::jsonb END);
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE FUNCTION app_sync_task_series(p_series uuid, p_ids uuid[]) RETURNS integer
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT app_sync_task_series_as(p_series, false, p_ids) $$;

CREATE FUNCTION app_sync_task_series_ids(p_series uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_task_access(s.root_task_id) IS NULL THEN 0 ELSE app_task_series_bound(s.id) END
    FROM task_series s WHERE s.id = p_series
  $$;

-- id: 반복마다 `app_task_series_bound` 몫을 차례로 넘긴다(개수는 `_ids`).
CREATE FUNCTION app_sync_item_series(p_item uuid, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_item_access(p_item);
    v_id     uuid;
    v_made   integer := 0;
    v_at     integer := 0;
    v_need   integer;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_lane(p_item, 'done');
    PERFORM app_lock_lane(p_item, 'in_progress');
    PERFORM app_lock_lane(p_item, 'todo');
    FOR v_id IN
      SELECT s.id FROM task_series s JOIN tasks r ON r.id = s.root_task_id
      WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      v_need := app_task_series_bound(v_id);
      v_made := v_made + app_sync_task_series(v_id, coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
      v_at := v_at + v_need;
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE FUNCTION app_sync_item_series_ids(p_item uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_item_access(p_item) IS NULL THEN 0 ELSE
      coalesce((SELECT sum(app_task_series_bound(s.id)) FROM task_series s JOIN tasks r ON r.id = s.root_task_id
                WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL), 0)::integer
    END
  $$;

-- 작업 프로세스: 모든 반복. id는 반복마다 `app_task_series_bound` 몫(개수는 `_ids`).
CREATE FUNCTION app_system_recurrence_tick(p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
    v_at   integer := 0;
    v_need integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_id IN
      SELECT s.id FROM task_series s
      JOIN tasks r ON r.id = s.root_task_id
      JOIN items i ON i.id = s.item_id
      JOIN workspaces w ON w.id = s.workspace_id
      WHERE s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL AND w.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      v_need := app_task_series_bound(v_id);
      BEGIN
        v_made := v_made + app_sync_task_series_as(v_id, true, coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'recurrence sync of series % failed with SQLSTATE %', v_id, SQLSTATE;
      END;
      v_at := v_at + v_need;
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE FUNCTION app_system_recurrence_tick_ids() RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      coalesce((SELECT sum(app_task_series_bound(s.id)) FROM task_series s
                JOIN tasks r ON r.id = s.root_task_id
                JOIN items i ON i.id = s.item_id
                JOIN workspaces w ON w.id = s.workspace_id
                WHERE s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL AND w.deleted_at IS NULL),
               0)::integer
    END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 일정
------------------------------------------------------------------------------------------------------------------

-- id 2개: 일정, 감사 기록.
CREATE FUNCTION app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text,
                                 p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamptz,
                                 p_ends_at timestamptz, p_timezone text, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_access text := app_item_access(p_item);
    v_item   items%ROWTYPE;
    v_id     uuid;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM events WHERE item_id = p_item AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    PERFORM app_check_timezone(p_timezone);
    PERFORM app_lock_item(p_item);
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    INSERT INTO events (id, workspace_id, item_id, kind, title, description, all_day, start_date, end_date,
                        starts_at, ends_at, timezone, created_by, updated_by, request_id)
    VALUES (app_id(p_ids, 1), v_item.workspace_id, p_item, p_kind, btrim(p_title), coalesce(p_description, ''),
            p_all_day, p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone, v_user, v_user, p_request_id)
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'event', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

-- id 2개: 이름 변경 감사, 일시 변경 감사.
CREATE FUNCTION app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text,
                                 p_description text, p_all_day boolean, p_start_date date, p_end_date date,
                                 p_starts_at timestamptz, p_ends_at timestamptz, p_timezone text, p_ids uuid[])
  RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF v_event.version <> p_base_version THEN
      RETURN 'conflict';
    END IF;
    PERFORM app_check_timezone(p_timezone);
    UPDATE events SET
      kind = p_kind, title = btrim(p_title), description = coalesce(p_description, ''), all_day = p_all_day,
      start_date = p_start_date, end_date = p_end_date, starts_at = p_starts_at, ends_at = p_ends_at,
      timezone = p_timezone, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    IF v_event.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'rename', 'event', p_event,
                        jsonb_build_object('from', v_event.title, 'to', btrim(p_title)));
    END IF;
    IF (v_event.start_date, v_event.end_date, v_event.starts_at, v_event.ends_at, v_event.timezone)
       IS DISTINCT FROM (p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone) THEN
      PERFORM app_audit(app_id(p_ids, 2), v_event.workspace_id, 'reschedule', 'event', p_event, jsonb_build_object(
        'from', jsonb_build_array(coalesce(v_event.start_date::text, app_utc_text(v_event.starts_at)),
                                  coalesce(v_event.end_date::text, app_utc_text(v_event.ends_at)), v_event.timezone),
        'to', jsonb_build_array(coalesce(p_start_date::text, app_utc_text(p_starts_at)),
                                coalesce(p_end_date::text, app_utc_text(p_ends_at)), p_timezone)));
    END IF;
    RETURN 'updated';
  END
  $$;

-- id 2개: 휴지통 묶음, 감사 기록.
CREATE FUNCTION app_delete_event(p_event uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
    v_batch uuid;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_event.workspace_id, 'event', p_event, v_event.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE events SET deleted_at = now(), deleted_by = app_user_id(), trash_batch_id = v_batch WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 2), v_event.workspace_id, 'delete', 'event', p_event,
                      jsonb_build_object('title', v_event.title, 'batch', v_batch));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF v_event.recurrence IS NULL THEN
      RAISE EXCEPTION 'the event does not repeat' USING ERRCODE = '22023';
    END IF;
    IF p_date = ANY (v_event.excluded_dates) THEN
      RETURN;
    END IF;
    UPDATE events SET excluded_dates = array_append(excluded_dates, p_date), version = version + 1,
                      updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'exclude_occurrence', 'event', p_event,
                      jsonb_build_object('date', p_date));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_set_event_recurrence(p_event uuid, p_rule jsonb, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    IF p_rule IS NOT NULL THEN
      PERFORM app_check_recurrence(p_rule);
    END IF;
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF (p_rule ->> 'until')::date < coalesce(v_event.start_date, (v_event.starts_at AT TIME ZONE v_event.timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    UPDATE events SET recurrence = p_rule, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'set_recurrence', 'event', p_event,
                      jsonb_build_object('from', v_event.recurrence, 'to', p_rule));
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 시간 기록
------------------------------------------------------------------------------------------------------------------

-- id 1개.
CREATE FUNCTION app_add_time_entry(p_request_id uuid, p_started timestamptz, p_ended timestamptz, p_paused integer,
                                   p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
    v_ws   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM time_entries WHERE user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    INSERT INTO time_entries (id, user_id, started_at, ended_at, paused_seconds, kind, memo, target_kind, target_id,
                              workspace_id, request_id)
    VALUES (app_id(p_ids, 1), v_user, p_started, p_ended, p_paused, p_kind, coalesce(p_memo, ''), p_target_kind,
            p_target, v_ws, p_request_id)
    RETURNING id INTO v_id;
    RETURN v_id;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid,
                                p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
    v_ws   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM time_entries WHERE user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    -- 같은 사용자의 시작 요청을 줄 세운다(하나만 진행 중이라는 규칙을 고유 색인과 함께 지킨다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || v_user::text, 0));
    SELECT id INTO v_id FROM time_entries WHERE user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF EXISTS (SELECT 1 FROM time_entries WHERE user_id = v_user AND ended_at IS NULL AND deleted_at IS NULL) THEN
      RAISE EXCEPTION 'a timer is already running' USING ERRCODE = '55000';
    END IF;
    INSERT INTO time_entries (id, user_id, started_at, kind, memo, target_kind, target_id, workspace_id, request_id)
    VALUES (app_id(p_ids, 1), v_user, now(), p_kind, coalesce(p_memo, ''), p_target_kind, p_target, v_ws,
            p_request_id)
    RETURNING id INTO v_id;
    RETURN v_id;
  END
  $$;

-- id 1개(고치기 전 값을 남기는 이력).
CREATE FUNCTION app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamptz,
                                      p_ended timestamptz, p_paused integer, p_kind text, p_memo text,
                                      p_target_kind text, p_target uuid, p_ids uuid[]) RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
    v_ws    uuid;
  BEGIN
    IF v_entry.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    IF v_entry.ended_at IS NULL THEN
      IF p_ended IS NOT NULL OR p_paused IS DISTINCT FROM v_entry.paused_seconds
         OR p_started > coalesce(v_entry.paused_at, now()) THEN
        RAISE EXCEPTION 'a running timer only changes its start, kind, memo and target' USING ERRCODE = '22023';
      END IF;
    ELSIF p_ended IS NULL OR p_ended < p_started OR p_paused < 0
          OR p_paused > extract(epoch FROM p_ended - p_started) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    -- 대상을 바꿀 때만 지금 볼 수 있는지 확인한다(이미 연결된 대상은 볼 수 없게 되어도 그대로 둘 수 있다).
    IF (p_target_kind, p_target) IS DISTINCT FROM (v_entry.target_kind, v_entry.target_id) THEN
      v_ws := app_time_target_workspace(p_target_kind, p_target);
    ELSE
      v_ws := v_entry.workspace_id;
    END IF;
    INSERT INTO time_entry_revisions (id, entry_id, user_id, started_at, ended_at, paused_seconds, kind, memo,
                                      target_kind, target_id)
    VALUES (app_id(p_ids, 1), p_entry, v_entry.user_id, v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds,
            v_entry.kind, v_entry.memo, v_entry.target_kind, v_entry.target_id);
    UPDATE time_entries SET
      started_at = p_started, ended_at = p_ended, paused_seconds = p_paused, kind = p_kind,
      memo = coalesce(p_memo, ''), target_kind = p_target_kind, target_id = p_target, workspace_id = v_ws,
      version = version + 1, updated_at = now()
    WHERE id = p_entry;
    RETURN 'updated';
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_create_task(uuid, uuid, text, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[],
                                       boolean, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_task_ids(uuid, uuid[], jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_task_parent(uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_task_dependency(uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_task_dependency(uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_checklist_items(uuid, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_task_recurrence(uuid, jsonb, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_task_series_bound(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_task_series_as(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_task_series(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_task_series_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_item_series(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_item_series_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_recurrence_tick(uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_recurrence_tick_ids() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_event(uuid, uuid, text, text, text, boolean, date, date, timestamptz, timestamptz,
                                        text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_event(uuid, integer, text, text, text, boolean, date, date, timestamptz,
                                        timestamptz, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_event(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_exclude_event_occurrence(uuid, date, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_event_recurrence(uuid, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_time_entry(uuid, timestamptz, timestamptz, integer, text, text, text, uuid, uuid[])
  FROM PUBLIC;
REVOKE ALL ON FUNCTION app_start_timer(uuid, text, text, text, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_time_entry(uuid, integer, timestamptz, timestamptz, integer, text, text, text, uuid,
                                             uuid[]) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
  app_create_task(uuid, uuid, text, uuid, uuid[]),
  app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[], boolean, jsonb, uuid[]),
  app_update_task_ids(uuid, uuid[], jsonb),
  app_set_task_parent(uuid, uuid, uuid[]),
  app_add_task_dependency(uuid, uuid, uuid[]),
  app_remove_task_dependency(uuid, uuid, uuid[]),
  app_add_checklist_items(uuid, jsonb, uuid[]),
  app_set_task_recurrence(uuid, jsonb, text, uuid[]),
  app_sync_task_series(uuid, uuid[]),
  app_sync_task_series_ids(uuid),
  app_sync_item_series(uuid, uuid[]),
  app_sync_item_series_ids(uuid),
  app_system_recurrence_tick(uuid[]),
  app_system_recurrence_tick_ids(),
  app_create_event(uuid, uuid, text, text, text, boolean, date, date, timestamptz, timestamptz, text, uuid[]),
  app_update_event(uuid, integer, text, text, text, boolean, date, date, timestamptz, timestamptz, text, uuid[]),
  app_delete_event(uuid, uuid[]),
  app_exclude_event_occurrence(uuid, date, uuid[]),
  app_set_event_recurrence(uuid, jsonb, uuid[]),
  app_add_time_entry(uuid, timestamptz, timestamptz, integer, text, text, text, uuid, uuid[]),
  app_start_timer(uuid, text, text, text, uuid, uuid[]),
  app_update_time_entry(uuid, integer, timestamptz, timestamptz, integer, text, text, text, uuid, uuid[])
TO fvoci_app;
