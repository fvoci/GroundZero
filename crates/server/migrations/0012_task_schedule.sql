-- 마일스톤·선행 관계·후속 재배치 (D-31, D-33, D-50, D-57).
--
-- 마일스톤은 날짜 하나(마감일)를 가진 업무다. 선행 관계는 같은 항목 안의 "끝나야 시작"(finish-to-start) 하나다.
-- 선행 업무의 날짜를 바꾸면 서버가 영향받는 후속 업무와 제안 날짜를 미리 보여 주고(읽기), 사용자가 승인한
-- 목록만 같은 트랜잭션에서 옮긴다. 승인 목록의 업무는 모두 기준 버전을 맞춰야 하며 하나라도 바뀌었으면 아무것도
-- 바꾸지 않는다. 승인 없이 날짜만 고치면 후속 업무는 그대로 둔다.
-- 잠금 순서(업무): 열 → 업무 행(여럿이면 id 순) → 항목 공유 → 번호. 선행 관계 추가: 항목 공유 → 관계 잠금.
-- 볼 수 없게 된 담당 정리는 마지막에 워크스페이스 잠금으로 직렬화한다.

ALTER TABLE tasks ADD COLUMN milestone boolean NOT NULL DEFAULT false;
ALTER TABLE tasks ADD CONSTRAINT tasks_milestone_single_date CHECK (NOT milestone OR start_date IS NULL);

CREATE TABLE task_dependencies (
  predecessor_id uuid NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
  successor_id   uuid NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
  item_id        uuid NOT NULL REFERENCES items (id),
  created_by     uuid NOT NULL REFERENCES users (id),
  created_at     timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (predecessor_id, successor_id),
  CONSTRAINT task_dependencies_distinct CHECK (predecessor_id <> successor_id)
);
CREATE INDEX task_dependencies_successor ON task_dependencies (successor_id);
CREATE INDEX task_dependencies_item ON task_dependencies (item_id);

-- 같은 항목의 선행 관계 변경을 직렬화한다(순환 검사와 기록 사이에 다른 관계가 끼어들지 않게).
CREATE FUNCTION app_lock_dependencies(p_item uuid) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT pg_advisory_xact_lock(hashtextextended('fvoci-deps:' || p_item::text, 0))
  $$;

-- 선행 관계 추가(후속 업무 쓰기 권한). 같은 항목의 두 업무만, 순환 금지, 이미 있으면 그대로.
CREATE FUNCTION app_add_task_dependency(p_predecessor uuid, p_successor uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(v_succ.workspace_id, 'link_predecessor', 'task', p_successor,
                      jsonb_build_object('predecessor', p_predecessor));
  END
  $$;

-- 선행 관계 지우기(후속 업무 쓰기 권한). 없으면 그대로.
CREATE FUNCTION app_remove_task_dependency(p_predecessor uuid, p_successor uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      PERFORM app_audit(v_succ.workspace_id, 'unlink_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor));
    END IF;
  END
  $$;

-- 업무 고치기: 마일스톤과 승인한 후속 재배치(`p_shifts`: [{task_id, base_version, start_date, due_date}]).
DROP FUNCTION app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[]);
CREATE FUNCTION app_update_task(
  p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text,
  p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean,
  p_shifts jsonb
) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
  BEGIN
    IF p_shifts IS NULL OR jsonb_typeof(p_shifts) <> 'array' THEN
      RAISE EXCEPTION 'shifts must be an array' USING ERRCODE = '22023';
    END IF;
    PERFORM app_require_task_write(p_task);
    -- 열 잠금은 업무 행보다 먼저(다시 매기기와의 교착 방지). 항목은 바뀌지 않으므로 잠그기 전에 읽는다.
    PERFORM app_lock_lane((SELECT item_id FROM tasks WHERE id = p_task), p_status);
    -- 이 업무와 옮길 후속 업무의 행을 id 순으로 한 번에 잠근다(서로 다른 순서로 잠그는 교착 방지).
    SELECT array_agg(DISTINCT (s ->> 'task_id')::uuid) INTO v_ids FROM jsonb_array_elements(p_shifts) s;
    IF p_task = ANY (coalesce(v_ids, '{}')) THEN
      RAISE EXCEPTION 'the edited task cannot also be a shifted successor' USING ERRCODE = '22023';
    END IF;
    -- 다른 항목의 업무 id가 섞여 와도 그 행은 잠그지 않는다(아래 검사에서 거부).
    PERFORM 1 FROM tasks
    WHERE id = ANY (array_append(coalesce(v_ids, '{}'), p_task))
      AND item_id = (SELECT item_id FROM tasks WHERE id = p_task)
    ORDER BY id FOR UPDATE;
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    IF v_task.version <> p_base_version THEN
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
      IF v_other.version <> (v_shift ->> 'base_version')::integer THEN
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
      PERFORM app_audit(v_task.workspace_id, 'rename', 'task', p_task,
                        jsonb_build_object('from', v_task.title, 'to', btrim(p_title)));
    END IF;
    FOR v_uid IN
      SELECT a.user_id FROM task_assignees a
      WHERE a.task_id = p_task AND NOT (a.user_id = ANY (coalesce(p_assignees, '{}')))
    LOOP
      DELETE FROM task_assignees WHERE task_id = p_task AND user_id = v_uid;
      IF FOUND THEN
        PERFORM app_audit(v_task.workspace_id, 'unassign', 'task', p_task, jsonb_build_object('user', v_uid));
      END IF;
    END LOOP;
    FOREACH v_uid IN ARRAY v_new LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (p_task, v_uid, v_user);
      PERFORM app_audit(v_task.workspace_id, 'assign', 'task', p_task, jsonb_build_object('user', v_uid));
    END LOOP;

    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      UPDATE tasks SET start_date = v_start, due_date = v_due, version = version + 1,
                       updated_by = v_user, updated_at = now()
      WHERE id = v_other.id;
      PERFORM app_audit(v_task.workspace_id, 'reschedule', 'task', v_other.id,
                        jsonb_build_object('reason', 'predecessor_moved', 'predecessor', p_task,
                                           'from', jsonb_build_array(v_other.start_date, v_other.due_date),
                                           'to', jsonb_build_array(v_start, v_due)));
    END LOOP;
    RETURN 'updated';
  END
  $$;

-- 리뷰 반영(0011 업무 함수의 동시성): 열 잠금을 업무 행보다 먼저, 다시 매기기는 그 열에 남은 행만,
-- 볼 수 없게 된 담당 정리는 워크스페이스 단위로 직렬화, 새 담당자는 한 번 정한 목록만 넣는다.

CREATE OR REPLACE FUNCTION app_renumber_lane(p_item uuid, p_status text) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE tasks t SET position = r.pos
    FROM (SELECT id, row_number() OVER (ORDER BY position, number) AS pos
          FROM tasks WHERE item_id = p_item AND status = p_status) r
    WHERE t.id = r.id AND t.item_id = p_item AND t.status = p_status
  $$;

CREATE OR REPLACE FUNCTION app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row record;
    v_ws  uuid := coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item));
  BEGIN
    -- 같은 워크스페이스의 정리를 직렬화한다. 비공개 전환과 역할 강등이 동시에 일어나도 나중 쪽이 앞의 변경을
    -- 보고 정리한다(이 잠금 뒤의 조회는 앞 트랜잭션이 끝난 뒤의 상태를 본다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-assignees:' || v_ws::text, 0));
    FOR v_row IN
      SELECT a.task_id, a.user_id, t.workspace_id, t.item_id
      FROM task_assignees a JOIN tasks t ON t.id = a.task_id
      WHERE (p_item IS NULL OR t.item_id = p_item)
        AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
        AND (p_user IS NULL OR a.user_id = p_user)
    LOOP
      IF app_item_access_as(v_row.item_id, v_row.user_id) IS NULL THEN
        DELETE FROM task_assignees WHERE task_id = v_row.task_id AND user_id = v_row.user_id;
        IF FOUND THEN
          PERFORM app_audit(v_row.workspace_id, 'unassign', 'task', v_row.task_id,
                          jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
        END IF;
      END IF;
    END LOOP;
  END
  $$;

CREATE OR REPLACE FUNCTION app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    v_id := uuidv7();
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'task', v_id);
    INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, position, created_by, updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, p_parent, v_number, btrim(p_title),
            app_task_lane_end(p_item, 'todo'), v_user, v_user, p_request_id);
    PERFORM app_audit(v_item.workspace_id, 'create', 'task', v_id, jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

CREATE OR REPLACE FUNCTION app_move_task(p_task uuid, p_status text, p_before uuid) RETURNS void
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
    -- 항목은 바뀌지 않으므로 잠그기 전에 읽어 열부터 잠근다(다시 매기기와의 교착 방지).
    PERFORM app_lock_lane((SELECT item_id FROM tasks WHERE id = p_task), p_status);
    SELECT * INTO v_task FROM tasks WHERE id = p_task FOR UPDATE;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
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

ALTER TABLE task_dependencies ENABLE ROW LEVEL SECURITY;
CREATE POLICY task_dependencies_select ON task_dependencies FOR SELECT
  USING (app_task_access(predecessor_id) IS NOT NULL AND app_task_access(successor_id) IS NOT NULL);
GRANT SELECT ON task_dependencies TO fvoci_app;

REVOKE ALL ON FUNCTION app_lock_dependencies(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_task_dependency(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_task_dependency(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[],
                                       boolean, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_add_task_dependency(uuid, uuid),
  app_remove_task_dependency(uuid, uuid),
  app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[], boolean, jsonb)
TO fvoci_app;
