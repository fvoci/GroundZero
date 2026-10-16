-- 업무·하위 업무·담당자·업무 체크리스트 (D-14, D-18, D-30, D-31, D-33, D-48, D-50, D-57).
--
-- 업무는 모듈 항목 아래에 있고 모듈 번호열을 항목·문서와 함께 쓴다(DEV-1 항목, DEV-2 업무…).
-- 권한은 항목 권한을 따르고 보관된 항목·모듈 아래에서는 읽기 전용이다(app_task_access 하나로 판정).
-- 담당자는 그 항목을 볼 수 있는 사람만 될 수 있고, 멤버에서 빠지거나 항목이 비공개가 되어 볼 수 없게 되면
-- 담당에서도 빠진다. 독자는 담당자여도 바꿀 수 없다(새 권한 규칙을 만들지 않음).
-- 체크리스트 저장(O-04): 업무 체크리스트는 이 표의 행, 문서 체크리스트는 문서 본문 안. 각 체크리스트의 원본은
-- 하나다. Markdown으로 붙여넣은 체크리스트는 화면에서 항목으로 펴서 사용자가 확인한 뒤 뒤에 덧붙인다.
-- 상태는 todo·in_progress·done 고정이며(프로젝트별 상태 설정은 이후), 완료 시각은 서버가 기록한다.

CREATE TABLE tasks (
  id               uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id     uuid NOT NULL REFERENCES workspaces (id),
  item_id          uuid NOT NULL,
  parent_id        uuid REFERENCES tasks (id),
  number           bigint NOT NULL,
  title            text NOT NULL CHECK (length(btrim(title)) > 0),
  description      text NOT NULL DEFAULT '',
  status           text NOT NULL DEFAULT 'todo' CHECK (status IN ('todo', 'in_progress', 'done')),
  priority         text NOT NULL DEFAULT 'none' CHECK (priority IN ('none', 'low', 'medium', 'high', 'urgent')),
  start_date       date,
  due_date         date,
  estimate_minutes integer CHECK (estimate_minutes >= 0),
  tags             text[] NOT NULL DEFAULT '{}',
  position         numeric NOT NULL DEFAULT 0,
  version          integer NOT NULL DEFAULT 1 CHECK (version > 0),
  completed_at     timestamptz,
  created_by       uuid NOT NULL REFERENCES users (id),
  created_at       timestamptz NOT NULL DEFAULT now(),
  updated_by       uuid NOT NULL REFERENCES users (id),
  updated_at       timestamptz NOT NULL DEFAULT now(),
  request_id       uuid NOT NULL,
  CONSTRAINT tasks_dates CHECK (start_date IS NULL OR due_date IS NULL OR start_date <= due_date),
  CONSTRAINT tasks_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES items (id, workspace_id),
  CONSTRAINT tasks_request UNIQUE (item_id, created_by, request_id)
);
CREATE INDEX tasks_lane ON tasks (item_id, status, position);
CREATE INDEX tasks_parent ON tasks (parent_id);

CREATE TABLE task_assignees (
  task_id     uuid NOT NULL REFERENCES tasks (id),
  user_id     uuid NOT NULL REFERENCES users (id),
  assigned_by uuid NOT NULL REFERENCES users (id),
  created_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (task_id, user_id)
);
CREATE INDEX task_assignees_user ON task_assignees (user_id);

CREATE TABLE checklist_items (
  id         uuid PRIMARY KEY DEFAULT uuidv7(),
  task_id    uuid NOT NULL REFERENCES tasks (id),
  text       text NOT NULL CHECK (length(btrim(text)) > 0),
  checked    boolean NOT NULL DEFAULT false,
  position   numeric NOT NULL,
  request_id uuid NOT NULL,
  created_by uuid NOT NULL REFERENCES users (id),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT checklist_items_request UNIQUE (task_id, request_id)
);
CREATE INDEX checklist_items_task ON checklist_items (task_id, position);

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

-- 업무 권한 = 항목 권한, 보관된 항목·모듈 아래에서는 read.
CREATE FUNCTION app_task_access(p_task uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   uuid;
    v_access text;
  BEGIN
    SELECT item_id INTO v_item FROM tasks WHERE id = p_task;
    IF v_item IS NULL THEN
      RETURN NULL;
    END IF;
    v_access := app_item_access(v_item);
    IF v_access IS NOT NULL AND v_access <> 'read' AND app_item_archived(v_item) THEN
      RETURN 'read';
    END IF;
    RETURN v_access;
  END
  $$;

-- 볼 수 없으면 P0002, 쓰기 권한이 아니면 42501.
CREATE FUNCTION app_require_task_write(p_task uuid) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_task_access(p_task);
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
  END
  $$;

-- 다른 사용자의 항목 권한(담당자 지정 판정용). 앱 역할에는 주지 않는다.
CREATE FUNCTION app_item_access_as(p_item uuid, p_user uuid) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_me     text := current_setting('app.user_id', true);
    v_access text;
  BEGIN
    PERFORM set_config('app.user_id', p_user::text, true);
    v_access := app_item_access(p_item);
    PERFORM set_config('app.user_id', coalesce(v_me, ''), true);
    RETURN v_access;
  END
  $$;

CREATE FUNCTION app_task_reference(p_task uuid) RETURNS text
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT k.key || '-' || n.number
    FROM numbered_targets n
    JOIN module_keys k ON k.module_id = n.scope_id AND k.retired_at IS NULL
    WHERE n.target_kind = 'task' AND n.target_id = p_task
  $$;

------------------------------------------------------------------------------
-- 업무 만들기·고치기·옮기기
------------------------------------------------------------------------------

CREATE FUNCTION app_task_lane_end(p_item uuid, p_status text) RETURNS numeric
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce(max(position), 0) + 1 FROM tasks WHERE item_id = p_item AND status = p_status
  $$;

-- 같은 항목 안의 한 단계 하위 업무만 허용한다. 두 업무를 id 순서로 잠근 뒤 확인한다(서로를 부모로 삼는 동시 변경 방지).
CREATE FUNCTION app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_parent tasks%ROWTYPE;
  BEGIN
    IF p_parent IS NULL THEN
      RETURN;
    END IF;
    IF p_task IS NOT NULL AND p_parent = p_task THEN
      RAISE EXCEPTION 'a task cannot be its own parent' USING ERRCODE = '22023';
    END IF;
    IF p_task IS NOT NULL THEN
      PERFORM 1 FROM tasks WHERE id IN (p_task, p_parent) ORDER BY id FOR UPDATE;
    ELSE
      PERFORM 1 FROM tasks WHERE id = p_parent FOR UPDATE;
    END IF;
    SELECT * INTO v_parent FROM tasks WHERE id = p_parent;
    IF v_parent.id IS NULL OR app_task_access(p_parent) IS NULL THEN
      RAISE EXCEPTION 'parent task not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_parent.item_id <> p_item THEN
      RAISE EXCEPTION 'the parent task must be in the same item' USING ERRCODE = '22023';
    END IF;
    IF v_parent.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'subtasks can only be one level deep' USING ERRCODE = '22023';
    END IF;
    IF p_task IS NOT NULL AND EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task) THEN
      RAISE EXCEPTION 'a task with subtasks cannot become a subtask' USING ERRCODE = '22023';
    END IF;
  END
  $$;

CREATE FUNCTION app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid) RETURNS uuid
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

-- 필드 고치기. 기준 버전이 지났으면 'conflict'. 담당자는 항목을 볼 수 있는 사람만.
CREATE FUNCTION app_update_task(
  p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text,
  p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[]
) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_task tasks%ROWTYPE;
    v_uid  uuid;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task FOR UPDATE;
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
    FOREACH v_uid IN ARRAY coalesce(p_assignees, '{}') LOOP
      IF app_item_access_as(v_task.item_id, v_uid) IS NULL THEN
        RAISE EXCEPTION 'an assignee cannot see this item' USING ERRCODE = '22023';
      END IF;
    END LOOP;

    UPDATE tasks SET
      title = btrim(p_title), description = coalesce(p_description, ''), status = p_status,
      priority = p_priority, start_date = p_start, due_date = p_due, estimate_minutes = p_estimate,
      tags = coalesce(p_tags, '{}'), version = version + 1, updated_by = v_user, updated_at = now(),
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
      PERFORM app_audit(v_task.workspace_id, 'unassign', 'task', p_task, jsonb_build_object('user', v_uid));
    END LOOP;
    FOR v_uid IN
      SELECT DISTINCT u FROM unnest(coalesce(p_assignees, '{}')) AS u
      WHERE NOT EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = p_task AND a.user_id = u)
    LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (p_task, v_uid, v_user);
      PERFORM app_audit(v_task.workspace_id, 'assign', 'task', p_task, jsonb_build_object('user', v_uid));
    END LOOP;
    RETURN 'updated';
  END
  $$;

-- 칸반 옮기기: 상태(열)와 그 안의 위치. 위치만 바꾸면 버전을 올리지 않고, 상태가 바뀌면 올린다
-- (오래된 필드 편집이 바뀐 상태를 조용히 되돌리지 않게).
CREATE FUNCTION app_move_task(p_task uuid, p_status text, p_before uuid) RETURNS void
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

CREATE FUNCTION app_set_task_parent(p_task uuid, p_parent uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task tasks%ROWTYPE;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_check_task_parent(p_task, p_parent, v_task.item_id);
    IF p_parent IS NULL THEN
      PERFORM 1 FROM tasks WHERE id = p_task FOR UPDATE;
    END IF;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    IF v_task.parent_id IS NOT DISTINCT FROM p_parent THEN
      RETURN;
    END IF;
    UPDATE tasks SET parent_id = p_parent, updated_by = app_user_id(), updated_at = now() WHERE id = p_task;
    PERFORM app_audit(v_task.workspace_id, 'reparent', 'task', p_task, jsonb_build_object('parent', p_parent));
  END
  $$;

------------------------------------------------------------------------------
-- 업무 체크리스트
------------------------------------------------------------------------------

-- 여러 항목을 한 번에 덧붙인다. 항목마다 request_id가 있어 재시도해도 중복되지 않는다.
-- p_items: [{"request_id": uuid, "text": text, "checked": bool}, ...]
CREATE FUNCTION app_add_checklist_items(p_task uuid, p_items jsonb) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task tasks%ROWTYPE;
    v_item jsonb;
    v_next numeric;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task FOR UPDATE;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    SELECT coalesce(max(position), 0) + 1 INTO v_next FROM checklist_items WHERE task_id = p_task;
    FOR v_item IN SELECT * FROM jsonb_array_elements(coalesce(p_items, '[]'::jsonb)) LOOP
      INSERT INTO checklist_items (task_id, text, checked, position, request_id, created_by)
      VALUES (p_task, btrim(v_item ->> 'text'), coalesce((v_item ->> 'checked')::boolean, false), v_next,
              (v_item ->> 'request_id')::uuid, app_user_id())
      ON CONFLICT ON CONSTRAINT checklist_items_request DO NOTHING;
      IF FOUND THEN
        v_next := v_next + 1;
      END IF;
    END LOOP;
  END
  $$;

CREATE FUNCTION app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_task_access(v_task) IS NULL THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    UPDATE checklist_items SET text = btrim(p_text), checked = p_checked, updated_at = now() WHERE id = p_id;
  END
  $$;

CREATE FUNCTION app_delete_checklist_item(p_id uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_task_access(v_task) IS NULL THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    DELETE FROM checklist_items WHERE id = p_id;
  END
  $$;

------------------------------------------------------------------------------
-- 담당이 접근보다 오래 남지 않게: 멤버 제거·탈퇴, 항목 비공개 전환
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'the owner cannot leave or be removed; transfer ownership first' USING ERRCODE = '55000';
    END IF;
    DELETE FROM group_members gm USING groups g
    WHERE gm.group_id = g.id AND g.workspace_id = p_workspace AND gm.user_id = p_member;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace AND to_user = p_member;
    DELETE FROM task_assignees a USING tasks t
    WHERE a.task_id = t.id AND t.workspace_id = p_workspace AND a.user_id = p_member;
    DELETE FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_audit(p_workspace, p_action, 'member', p_member, '{}'::jsonb);
  END
  $$;

CREATE OR REPLACE FUNCTION app_update_item(p_item uuid, p_title text, p_description text, p_visibility text)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
    v_uid    uuid;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(v_item.id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF p_visibility <> v_item.visibility AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can change visibility' USING ERRCODE = '42501';
    END IF;
    UPDATE items SET title = btrim(p_title), description = coalesce(p_description, ''),
                     visibility = p_visibility, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.title <> btrim(p_title) THEN
      PERFORM app_audit(v_item.workspace_id, 'rename', 'item', p_item,
                        jsonb_build_object('from', v_item.title, 'to', btrim(p_title)));
    END IF;
    IF v_item.visibility <> p_visibility THEN
      PERFORM app_audit(v_item.workspace_id, 'change_visibility', 'item', p_item,
                        jsonb_build_object('from', v_item.visibility, 'to', p_visibility));
      -- 비공개가 되어 볼 수 없게 된 사람은 그 항목 업무의 담당에서 뺀다.
      FOR v_uid IN
        SELECT DISTINCT a.user_id FROM task_assignees a JOIN tasks t ON t.id = a.task_id WHERE t.item_id = p_item
      LOOP
        IF app_item_access_as(p_item, v_uid) IS NULL THEN
          DELETE FROM task_assignees a USING tasks t
          WHERE a.task_id = t.id AND t.item_id = p_item AND a.user_id = v_uid;
        END IF;
      END LOOP;
    END IF;
  END
  $$;

------------------------------------------------------------------------------
-- @ 참조 대상에 업무 추가 (D-18)
------------------------------------------------------------------------------

DROP POLICY numbered_targets_select ON numbered_targets;
CREATE POLICY numbered_targets_select ON numbered_targets FOR SELECT
  USING (CASE target_kind
           WHEN 'item' THEN app_item_access(target_id) IS NOT NULL
           WHEN 'document' THEN app_document_access(target_id) IS NOT NULL
           WHEN 'task' THEN app_task_access(target_id) IS NOT NULL
           ELSE false
         END);

ALTER TABLE document_links DROP CONSTRAINT document_links_target_kind_check;
ALTER TABLE document_links ADD CONSTRAINT document_links_target_kind_check
  CHECK (target_kind IN ('item', 'document', 'task'));

CREATE OR REPLACE FUNCTION app_reference_nodes(p_body jsonb) RETURNS TABLE (kind text, id uuid)
  LANGUAGE sql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT DISTINCT r.kind, r.raw_id::uuid
    FROM (
      SELECT n -> 'attrs' ->> 'kind' AS kind, n -> 'attrs' ->> 'id' AS raw_id
      FROM jsonb_path_query(p_body, 'lax $.**') AS n
      WHERE jsonb_typeof(n) = 'object' AND n ->> 'type' = 'reference'
    ) r
    WHERE r.kind IN ('item', 'document', 'task', 'user')
      AND r.raw_id ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  $$;

-- 링크 기록에 업무를 포함한다(본문은 0008과 같고 종류 목록만 넓힌다).
CREATE OR REPLACE FUNCTION app_record_document_references(p_doc uuid, p_notify boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_body     jsonb;
    v_revision integer;
    v_previous jsonb;
    v_uid      uuid;
    v_new      integer;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT body, revision INTO v_body, v_revision FROM documents WHERE id = p_doc;

    DELETE FROM document_links WHERE document_id = p_doc;
    INSERT INTO document_links (document_id, target_kind, target_id)
    SELECT p_doc, r.kind, r.id FROM app_reference_nodes(v_body) r
    WHERE r.kind IN ('item', 'document', 'task') AND NOT (r.kind = 'document' AND r.id = p_doc);

    IF NOT p_notify THEN
      RETURN;
    END IF;
    SELECT body INTO v_previous FROM document_revisions
    WHERE document_id = p_doc AND revision = v_revision - 1;
    FOR v_uid IN
      SELECT r.id FROM app_reference_nodes(v_body) r JOIN users u ON u.id = r.id
      WHERE r.kind = 'user' AND r.id <> v_user
        AND NOT EXISTS (SELECT 1 FROM app_reference_nodes(coalesce(v_previous, '{}'::jsonb)) p
                        WHERE p.kind = 'user' AND p.id = r.id)
    LOOP
      IF app_document_access_as(p_doc, v_uid) IS NOT NULL THEN
        INSERT INTO document_mentions (document_id, user_id, mentioned_by)
        VALUES (p_doc, v_uid, v_user)
        ON CONFLICT DO NOTHING;
        GET DIAGNOSTICS v_new = ROW_COUNT;
        IF v_new > 0 THEN
          INSERT INTO notifications (user_id, kind, document_id, actor)
          VALUES (v_uid, 'mention', p_doc, v_user);
        END IF;
      END IF;
    END LOOP;
  END
  $$;

------------------------------------------------------------------------------
-- 담당자 후보: 그 항목을 볼 수 있는 워크스페이스 멤버(app_item_access와 같은 규칙). 부르는 사람이 항목을 볼 수 있어야 한다.
------------------------------------------------------------------------------

CREATE FUNCTION app_item_assignable(p_item uuid) RETURNS TABLE (user_id uuid, display_name text)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item items%ROWTYPE;
  BEGIN
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    RETURN QUERY
    SELECT u.id, u.display_name
    FROM workspace_members m JOIN users u ON u.id = m.user_id
    WHERE m.workspace_id = v_item.workspace_id
      AND (m.role = 'admin' OR v_item.visibility = 'workspace' OR v_item.created_by = m.user_id)
    ORDER BY u.display_name, u.id;
  END
  $$;

------------------------------------------------------------------------------
-- RLS·권한
------------------------------------------------------------------------------

ALTER TABLE tasks ENABLE ROW LEVEL SECURITY;
CREATE POLICY tasks_select ON tasks FOR SELECT USING (app_task_access(id) IS NOT NULL);
ALTER TABLE task_assignees ENABLE ROW LEVEL SECURITY;
CREATE POLICY task_assignees_select ON task_assignees FOR SELECT USING (app_task_access(task_id) IS NOT NULL);
ALTER TABLE checklist_items ENABLE ROW LEVEL SECURITY;
CREATE POLICY checklist_items_select ON checklist_items FOR SELECT USING (app_task_access(task_id) IS NOT NULL);
GRANT SELECT ON tasks, task_assignees, checklist_items TO fvoci_app;

REVOKE ALL ON FUNCTION app_task_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_require_task_write(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_item_access_as(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_task_reference(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_task_lane_end(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_check_task_parent(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_task(uuid, uuid, text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_move_task(uuid, text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_task_parent(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_checklist_items(uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_checklist_item(uuid, text, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_checklist_item(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_item_assignable(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_task_access(uuid),
  app_task_reference(uuid),
  app_create_task(uuid, uuid, text, uuid),
  app_update_task(uuid, integer, text, text, text, text, date, date, integer, text[], uuid[]),
  app_move_task(uuid, text, uuid),
  app_set_task_parent(uuid, uuid),
  app_add_checklist_items(uuid, jsonb),
  app_update_checklist_item(uuid, text, boolean),
  app_delete_checklist_item(uuid),
  app_item_assignable(uuid)
TO fvoci_app;
