-- 감사 5·6·7번(docs/development.md 81절). 함수 본문은 앞선 정의와 같고 "바뀐 곳"만 다르다.
--  5. 워크스페이스 영구 삭제: 휴지통에 든 모듈이 휴지통 묶음을 가리킨 채 묶음을 먼저 지워 외래 키 위반으로 실패했다
--     (보존 기간이 지나도 작업이 경고만 남기고 지우지 못했다). 모듈을 지운 뒤 묶음을 지운다.
--  6. 업무 지우기: 하위 업무를 지우는 동안 상위 업무를 지우면 잠금을 기다린 뒤 이미 다른 묶음에 든 하위 업무를 자기
--     묶음으로 옮겼다. 잠근 뒤 아직 지우지 않은 것만 다시 고른다.
--  7. 항목 만들기: 휴지통에 든 모듈에도 만들었다(복제는 만든 항목을 다시 읽지 않아 그대로 남았다).

CREATE OR REPLACE FUNCTION app_purge_workspace_as(p_ws uuid, p_system boolean) RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws    workspaces%ROWTYPE;
    v_items uuid[];
  BEGIN
    -- 새 내용이 이 워크스페이스를 가리키며 들어오지 못하게 행을 잠근다(외래 키 공유 잠금과 충돌).
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NULL
       OR (NOT p_system AND v_ws.owner_id IS DISTINCT FROM app_user_id()) THEN
      RAISE EXCEPTION 'deleted workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_system AND v_ws.deleted_at + make_interval(days => v_ws.trash_retention_days) > now() THEN
      RETURN false;
    END IF;
    v_items := ARRAY(SELECT id FROM items WHERE workspace_id = p_ws);
    PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE workspace_id = p_ws));
    DELETE FROM task_dependencies WHERE item_id = ANY (v_items);
    DELETE FROM task_series WHERE workspace_id = p_ws;
    PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE workspace_id = p_ws));
    DELETE FROM events WHERE workspace_id = p_ws;
    DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = ANY (v_items);
    DELETE FROM items WHERE workspace_id = p_ws;
    DELETE FROM module_keys WHERE workspace_id = p_ws;
    DELETE FROM modules WHERE workspace_id = p_ws;
    -- 바뀐 곳: 묶음을 가리키는 모듈을 지운 뒤에 묶음을 지운다.
    DELETE FROM trash_batches WHERE workspace_id = p_ws;
    DELETE FROM numbered_targets WHERE workspace_id = p_ws;
    DELETE FROM reference_counters WHERE workspace_id = p_ws;
    DELETE FROM groups WHERE workspace_id = p_ws;
    DELETE FROM invitations WHERE workspace_id = p_ws;
    DELETE FROM ownership_offers WHERE workspace_id = p_ws;
    DELETE FROM time_sharing WHERE workspace_id = p_ws;
    -- 개인 시간 기록은 남는다(대상 이름만 사라진다).
    UPDATE time_entries SET workspace_id = NULL WHERE workspace_id = p_ws;
    -- 감사 기록은 소유자 영역으로 옮겨 남긴다.
    UPDATE audit_log SET workspace_id = NULL, owner_user_id = coalesce(owner_user_id, v_ws.owner_id)
    WHERE workspace_id = p_ws;
    DELETE FROM workspace_members WHERE workspace_id = p_ws;
    DELETE FROM workspaces WHERE id = p_ws;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (NULL, v_ws.owner_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge', 'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
    RETURN true;
  END
  $$;

CREATE OR REPLACE FUNCTION app_trash_task(p_task uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task  tasks%ROWTYPE;
    v_batch uuid;
    v_ids   uuid[];
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    PERFORM app_lock_task_rows(v_ids, NULL);
    -- 잠그는 사이 새로 생긴 하위 업무도 함께(드문 경우, 다시 잠근다).
    IF EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task AND deleted_at IS NULL AND NOT (id = ANY (v_ids))) THEN
      SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
      PERFORM app_lock_task_rows(v_ids, NULL);
    END IF;
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- 바뀐 곳: 잠근 뒤 다시 고른다. 기다리는 사이 다른 지우기가 끝낸 하위 업무는 이미 자기 묶음에 있다.
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE id = ANY (v_ids) AND deleted_at IS NULL;
    INSERT INTO trash_batches (workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_task.workspace_id, 'task', p_task, v_task.title, cardinality(v_ids), app_user_id())
    RETURNING id INTO v_batch;
    UPDATE tasks SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_ids);
    PERFORM app_audit(v_task.workspace_id, 'delete', 'task', p_task,
                      jsonb_build_object('batch', v_batch, 'count', cardinality(v_ids)));
    RETURN v_batch;
  END
  $$;

CREATE OR REPLACE FUNCTION app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_mod  modules%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR SHARE;
    -- 바뀐 곳: 휴지통에 든 모듈은 없는 것으로 본다(모듈 지우기는 행을 갱신해 잠그므로 공유 잠금 뒤에 보면 된다).
    IF v_user IS NULL OR v_mod.id IS NULL OR v_mod.deleted_at IS NOT NULL
       OR app_member_role(v_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(v_mod.workspace_id) NOT IN ('admin', 'writer') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF v_mod.archived_at IS NOT NULL THEN
      RAISE EXCEPTION 'the module is archived' USING ERRCODE = '55000';
    END IF;
    SELECT id INTO v_id FROM items WHERE module_id = p_module AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    v_id := uuidv7();
    PERFORM app_allocate_number(v_mod.workspace_id, p_module, 'item', v_id);
    INSERT INTO items (id, workspace_id, module_id, title, description, created_by, updated_by, request_id)
    VALUES (v_id, v_mod.workspace_id, p_module, btrim(p_title), coalesce(p_description, ''),
            v_user, v_user, p_request_id);
    PERFORM app_audit(v_mod.workspace_id, 'create', 'item', v_id, jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;
