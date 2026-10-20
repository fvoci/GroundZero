-- 앱이 만든 id로 행을 만든다(D-88, 0070 머리말). 이 파일: 모듈·항목(만들기·고치기·보관·복제·멤버·프로필·템플릿),
-- 강좌 정보·회차, 스터디 단위, 기록, 저장한 보기. 함수 본문은 마지막 정의와 같고 id 부분만 다르다.

------------------------------------------------------------------------------------------------------------------
-- 모듈
------------------------------------------------------------------------------------------------------------------

-- id 2개: 모듈, 감사 기록.
CREATE FUNCTION app_create_module(p_workspace uuid, p_request_id uuid, p_kind text, p_name text, p_key text,
                                  p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
    v_key  text := app_normalize_key(p_key);
    v_id   uuid;
  BEGIN
    SELECT id INTO v_id FROM modules
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_kind NOT IN ('project', 'research', 'study', 'course', 'hobby') THEN
      RAISE EXCEPTION 'unknown module kind' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM module_keys WHERE workspace_id = p_workspace AND key = v_key) THEN
      RAISE EXCEPTION 'this key is in use or was used before' USING ERRCODE = '55000';
    END IF;
    INSERT INTO modules (id, workspace_id, kind, name, created_by, request_id)
    VALUES (app_id(p_ids, 1), p_workspace, p_kind, btrim(p_name), v_user, p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO module_keys (workspace_id, key, module_id) VALUES (p_workspace, v_key, v_id);
    PERFORM app_audit(app_id(p_ids, 2), p_workspace, 'create', 'module', v_id,
                      jsonb_build_object('kind', p_kind, 'name', btrim(p_name), 'key', v_key));
    RETURN v_id;
  END
  $$;

-- id 2개: 이름 변경 감사, 키 변경 감사.
CREATE FUNCTION app_update_module(p_module uuid, p_name text, p_key text, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_mod     modules%ROWTYPE;
    v_key     text := app_normalize_key(p_key);
    v_current text;
    v_owner   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR UPDATE;
    IF v_mod.id IS NULL OR app_member_role(v_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_mod.workspace_id);

    IF v_mod.name <> btrim(p_name) THEN
      UPDATE modules SET name = btrim(p_name) WHERE id = p_module;
      PERFORM app_audit(app_id(p_ids, 1), v_mod.workspace_id, 'rename', 'module', p_module,
                        jsonb_build_object('from', v_mod.name, 'to', btrim(p_name)));
    END IF;

    SELECT key INTO v_current FROM module_keys WHERE module_id = p_module AND retired_at IS NULL;
    IF v_current <> v_key THEN
      SELECT module_id INTO v_owner FROM module_keys WHERE workspace_id = v_mod.workspace_id AND key = v_key;
      -- 주인 없는 예약 키(영구 삭제한 모듈의 것)도 거절한다.
      IF FOUND AND v_owner IS DISTINCT FROM p_module THEN
        RAISE EXCEPTION 'this key is in use or was used before' USING ERRCODE = '55000';
      END IF;
      UPDATE module_keys SET retired_at = now() WHERE module_id = p_module AND retired_at IS NULL;
      IF v_owner = p_module THEN
        -- 같은 모듈의 옛 키를 되찾는다.
        UPDATE module_keys SET retired_at = NULL WHERE workspace_id = v_mod.workspace_id AND key = v_key;
      ELSE
        INSERT INTO module_keys (workspace_id, key, module_id) VALUES (v_mod.workspace_id, v_key, p_module);
      END IF;
      PERFORM app_audit(app_id(p_ids, 2), v_mod.workspace_id, 'change_key', 'module', p_module,
                        jsonb_build_object('from', v_current, 'to', v_key));
    END IF;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_archive_module(p_module uuid, p_archived boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_mod modules%ROWTYPE;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR UPDATE;
    IF v_mod.id IS NULL OR app_member_role(v_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_mod.workspace_id);
    IF (v_mod.archived_at IS NOT NULL) <> p_archived THEN
      UPDATE modules SET archived_at = CASE WHEN p_archived THEN now() END WHERE id = p_module;
      PERFORM app_audit(app_id(p_ids, 1), v_mod.workspace_id, CASE WHEN p_archived THEN 'archive' ELSE 'unarchive' END,
                        'module', p_module, '{}'::jsonb);
    END IF;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 항목
------------------------------------------------------------------------------------------------------------------

-- id 2개: 항목, 감사 기록.
CREATE FUNCTION app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_mod  modules%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR SHARE;
    -- 휴지통에 든 모듈은 없는 것으로 본다(모듈 지우기는 행을 갱신해 잠그므로 공유 잠금 뒤에 보면 된다).
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
    v_id := app_id(p_ids, 1);
    PERFORM app_allocate_number(v_mod.workspace_id, p_module, 'item', v_id);
    INSERT INTO items (id, workspace_id, module_id, title, description, created_by, updated_by, request_id)
    VALUES (v_id, v_mod.workspace_id, p_module, btrim(p_title), coalesce(p_description, ''),
            v_user, v_user, p_request_id);
    PERFORM app_audit(app_id(p_ids, 2), v_mod.workspace_id, 'create', 'item', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

-- id: 이름 변경 감사, 공개 범위 감사, 담당자 정리 몫(개수는 `_ids`).
CREATE FUNCTION app_update_item(p_item uuid, p_title text, p_description text, p_visibility text, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
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
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'rename', 'item', p_item,
                        jsonb_build_object('from', v_item.title, 'to', btrim(p_title)));
    END IF;
    IF v_item.visibility <> p_visibility THEN
      PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'change_visibility', 'item', p_item,
                        jsonb_build_object('from', v_item.visibility, 'to', p_visibility));
      -- 비공개가 되어 볼 수 없게 된 사람은 그 항목 업무의 담당에서 뺀다.
      PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_private', app_ids_from(p_ids, 3));
    END IF;
  END
  $$;

CREATE FUNCTION app_update_item_ids(p_item uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_item_access(p_item) IS NULL THEN 0
                ELSE 2 + app_unseen_assignees_bound(p_item, NULL, NULL) END
  $$;

-- id 1개.
CREATE FUNCTION app_archive_item(p_item uuid, p_archived boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item items%ROWTYPE;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_access(p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can archive' USING ERRCODE = '42501';
    END IF;
    IF (v_item.archived_at IS NOT NULL) <> p_archived THEN
      UPDATE items SET archived_at = CASE WHEN p_archived THEN now() END WHERE id = p_item;
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, CASE WHEN p_archived THEN 'archive' ELSE 'unarchive' END,
                        'item', p_item, '{}'::jsonb);
    END IF;
  END
  $$;

-- id: 새 항목과 그 감사(2), 업무마다 id·요청 id(2), 체크리스트 줄마다 id·요청 id(2), 문서마다 id·요청 id(2, 첫
-- 리비전도 같은 요청 id), 회차·스터디 단위마다 id·요청 id(2), 복제 감사(1). 개수는 `_ids`.
CREATE FUNCTION app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text, p_reset boolean,
                              p_ids uuid[])
  RETURNS TABLE (copied_item uuid, skipped_count integer)
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_src     items%ROWTYPE;
    v_src_mod modules%ROWTYPE;
    v_dst_mod modules%ROWTYPE;
    v_new     uuid;
    v_existed boolean;
    v_row     record;
    v_id      uuid;
    v_skipped integer := 0;
    v_access  text;
    v_map     jsonb := '{}'::jsonb;
    v_at      integer := 2;
    v_count   integer;
  BEGIN
    SELECT * INTO v_src FROM items WHERE id = p_item;
    IF v_src.id IS NULL OR app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_src_mod FROM modules WHERE id = v_src.module_id;
    SELECT * INTO v_dst_mod FROM modules WHERE id = p_module;
    IF v_dst_mod.id IS NULL OR app_member_role(v_dst_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_dst_mod.workspace_id <> v_src.workspace_id OR v_dst_mod.kind <> v_src_mod.kind THEN
      RAISE EXCEPTION 'copy into a module of the same kind in the same workspace' USING ERRCODE = '22023';
    END IF;
    v_existed := EXISTS (SELECT 1 FROM items WHERE module_id = p_module AND created_by = v_user AND request_id = p_request);
    -- 권한·보관·번호·감사는 항목 만들기와 같은 길로.
    v_new := app_create_item(p_module, p_request, coalesce(nullif(btrim(p_title), ''), v_src.title), v_src.description,
                             p_ids[1:2]);
    IF v_existed THEN
      RETURN QUERY SELECT v_new, 0;
      RETURN;
    END IF;
    UPDATE items SET goal = v_src.goal, scope = v_src.scope, details = v_src.details,
                     status = CASE WHEN p_reset THEN 'planned' ELSE v_src.status END,
                     starts_on = CASE WHEN p_reset THEN NULL ELSE v_src.starts_on END,
                     ends_on = CASE WHEN p_reset THEN NULL ELSE v_src.ends_on END
    WHERE id = v_new;

    -- 업무: 부모 먼저(하위 업무는 한 단계). v_map: 옛 id → 새 id.
    FOR v_row IN
      SELECT * FROM tasks WHERE item_id = p_item AND deleted_at IS NULL
      ORDER BY (parent_id IS NOT NULL), number
    LOOP
      v_id := app_id(p_ids, v_at + 1);
      INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, description, status, priority,
                         start_date, due_date, estimate_minutes, tags, position, milestone, completed_at,
                         created_by, updated_by, request_id)
      VALUES (v_id, v_src.workspace_id, v_new, (v_map->>v_row.parent_id::text)::uuid,
              app_allocate_number(v_src.workspace_id, p_module, 'task', v_id), v_row.title, v_row.description,
              CASE WHEN p_reset THEN 'todo' ELSE v_row.status END, v_row.priority,
              CASE WHEN p_reset THEN NULL ELSE v_row.start_date END,
              CASE WHEN p_reset THEN NULL ELSE v_row.due_date END,
              v_row.estimate_minutes, v_row.tags, v_row.position, v_row.milestone,
              CASE WHEN p_reset OR v_row.status <> 'done' THEN NULL ELSE now() END,
              v_user, v_user, app_id(p_ids, v_at + 2));
      v_at := v_at + 2;
      v_map := v_map || jsonb_build_object(v_row.id::text, v_id);
      INSERT INTO checklist_items (id, task_id, text, checked, position, request_id, created_by)
        SELECT app_id(p_ids, v_at + 2 * c.n - 1), v_id, c.text, CASE WHEN p_reset THEN false ELSE c.checked END,
               c.position, app_id(p_ids, v_at + 2 * c.n), v_user
        FROM (SELECT c.*, (row_number() OVER (ORDER BY c.position, c.id))::integer AS n
              FROM checklist_items c WHERE c.task_id = v_row.id) c;
      GET DIAGNOSTICS v_count = ROW_COUNT;
      v_at := v_at + 2 * v_count;
    END LOOP;
    INSERT INTO task_dependencies (predecessor_id, successor_id, item_id, created_by)
      SELECT (v_map->>d.predecessor_id::text)::uuid, (v_map->>d.successor_id::text)::uuid, v_new, v_user
      FROM task_dependencies d
      WHERE v_map ? d.predecessor_id::text AND v_map ? d.successor_id::text;

    -- 항목 문서: 볼 수 있고 복제가 허용된 것(또는 소유 권한), 부모 먼저.
    v_map := '{}'::jsonb;
    FOR v_row IN
      WITH RECURSIVE tree AS (
        SELECT d.*, 0 AS depth FROM documents d
        WHERE d.item_id = p_item AND d.deleted_at IS NULL AND d.parent_id IS NULL
        UNION ALL
        SELECT d.*, t.depth + 1 FROM documents d JOIN tree t ON d.parent_id = t.id
        WHERE d.deleted_at IS NULL
      )
      SELECT * FROM tree ORDER BY depth, position
    LOOP
      v_access := app_document_access(v_row.id);
      IF v_access IS NULL OR (v_access <> 'owner' AND NOT v_row.allow_copy)
         OR (v_row.parent_id IS NOT NULL AND NOT v_map ? v_row.parent_id::text) THEN
        v_skipped := v_skipped + 1;
        CONTINUE;
      END IF;
      v_id := app_id(p_ids, v_at + 1);
      INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility, parent_id, position)
      VALUES (v_id, v_src.workspace_id, v_new, 'module', p_module::text,
              app_allocate_number(v_src.workspace_id, p_module, 'document', v_id), v_row.title,
              v_row.schema_version, v_row.body, 1, v_user, v_user, app_id(p_ids, v_at + 2),
              v_row.visibility,
              (v_map->>v_row.parent_id::text)::uuid, v_row.position);
      INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
      VALUES (v_id, 1, v_row.title, v_row.schema_version, v_row.body, v_user, app_id(p_ids, v_at + 2));
      v_at := v_at + 2;
      v_map := v_map || jsonb_build_object(v_row.id::text, v_id);
    END LOOP;

    -- 강좌 정보·회차(진도 제외), 스터디 단위(진행 제외).
    INSERT INTO course_details (item_id, workspace_id, subject, instructor, platform, url, textbook, starts_on,
                                expires_on, target_on, updated_by)
      SELECT v_new, workspace_id, subject, instructor, platform, url, textbook,
             CASE WHEN p_reset THEN NULL ELSE starts_on END, CASE WHEN p_reset THEN NULL ELSE expires_on END,
             CASE WHEN p_reset THEN NULL ELSE target_on END, v_user
      FROM course_details WHERE item_id = p_item;
    INSERT INTO course_episodes (id, item_id, workspace_id, number, title, chapter, duration_seconds, url, materials,
                                 planned_on, created_by, request_id)
      SELECT app_id(p_ids, v_at + 2 * e.n - 1), v_new, e.workspace_id, e.number, e.title, e.chapter,
             e.duration_seconds, e.url, e.materials,
             CASE WHEN p_reset THEN NULL ELSE e.planned_on END, v_user, app_id(p_ids, v_at + 2 * e.n)
      FROM (SELECT e.*, (row_number() OVER (ORDER BY e.number, e.id))::integer AS n
            FROM course_episodes e WHERE e.item_id = p_item) e;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_at := v_at + 2 * v_count;
    INSERT INTO study_units (id, item_id, workspace_id, number, title, chapter, unit, amount, planned_on, created_by,
                             request_id)
      SELECT app_id(p_ids, v_at + 2 * u.n - 1), v_new, u.workspace_id, u.number, u.title, u.chapter, u.unit,
             u.amount, CASE WHEN p_reset THEN NULL ELSE u.planned_on END, v_user, app_id(p_ids, v_at + 2 * u.n)
      FROM (SELECT u.*, (row_number() OVER (ORDER BY u.number, u.id))::integer AS n
            FROM study_units u WHERE u.item_id = p_item) u;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_at := v_at + 2 * v_count;

    PERFORM app_audit(app_id(p_ids, v_at + 1), v_src.workspace_id, 'copy', 'item', v_new,
                      jsonb_build_object('from', p_item, 'reset', p_reset, 'skipped_documents', v_skipped));
    RETURN QUERY SELECT v_new, v_skipped;
  END
  $$;

CREATE FUNCTION app_copy_item_ids(p_item uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_item_access(p_item) IS NULL THEN 0 ELSE
      3 + 2 * (SELECT count(*) FROM tasks WHERE item_id = p_item AND deleted_at IS NULL)::integer
        + 2 * (SELECT count(*) FROM checklist_items c JOIN tasks t ON t.id = c.task_id
               WHERE t.item_id = p_item AND t.deleted_at IS NULL)::integer
        + 2 * (SELECT count(*) FROM documents WHERE item_id = p_item AND deleted_at IS NULL)::integer
        + 2 * (SELECT count(*) FROM course_episodes WHERE item_id = p_item)::integer
        + 2 * (SELECT count(*) FROM study_units WHERE item_id = p_item)::integer
    END
  $$;

-- id: 멤버 변경 감사(1), 담당자 정리 몫(개수는 `_ids`).
CREATE FUNCTION app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[], p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
    v_users  uuid[] := ARRAY(SELECT DISTINCT unnest(coalesce(p_users, '{}')));
    v_groups uuid[] := ARRAY(SELECT DISTINCT unnest(coalesce(p_groups, '{}')));
    v_before jsonb;
    v_after  jsonb;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can choose members' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF cardinality(v_users) + cardinality(v_groups) > 500 THEN
      RAISE EXCEPTION 'too many members' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM unnest(v_users) u
               WHERE NOT EXISTS (SELECT 1 FROM workspace_members m
                                 WHERE m.workspace_id = v_item.workspace_id AND m.user_id = u)) THEN
      RAISE EXCEPTION 'members must belong to the workspace' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM unnest(v_groups) g
               WHERE NOT EXISTS (SELECT 1 FROM groups gr WHERE gr.id = g AND gr.workspace_id = v_item.workspace_id)) THEN
      RAISE EXCEPTION 'groups must belong to the workspace' USING ERRCODE = '22023';
    END IF;
    SELECT jsonb_build_object('users', coalesce(jsonb_agg(user_id) FILTER (WHERE user_id IS NOT NULL), '[]'),
                              'groups', coalesce(jsonb_agg(group_id) FILTER (WHERE group_id IS NOT NULL), '[]'))
      INTO v_before FROM item_members WHERE item_id = p_item;
    DELETE FROM item_members WHERE item_id = p_item
      AND ((user_id IS NOT NULL AND user_id <> ALL (v_users)) OR (group_id IS NOT NULL AND group_id <> ALL (v_groups)));
    INSERT INTO item_members (item_id, user_id, added_by)
      SELECT p_item, u, app_user_id() FROM unnest(v_users) u
      ON CONFLICT (item_id, user_id) WHERE user_id IS NOT NULL DO NOTHING;
    INSERT INTO item_members (item_id, group_id, added_by)
      SELECT p_item, g, app_user_id() FROM unnest(v_groups) g
      ON CONFLICT (item_id, group_id) WHERE group_id IS NOT NULL DO NOTHING;
    v_after := jsonb_build_object('users', to_jsonb(v_users), 'groups', to_jsonb(v_groups));
    IF v_before IS DISTINCT FROM v_after THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'set_members', 'item', p_item,
                        jsonb_build_object('users', cardinality(v_users), 'groups', cardinality(v_groups)));
    END IF;
    PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_member_removed', app_ids_from(p_ids, 2));
    IF v_item.leader_id IS NOT NULL AND app_item_access_as(p_item, v_item.leader_id) IS NULL THEN
      UPDATE items SET leader_id = NULL WHERE id = p_item;
    END IF;
  END
  $$;

CREATE FUNCTION app_set_item_members_ids(p_item uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_item_access(p_item) IS DISTINCT FROM 'owner' THEN 0
                ELSE 1 + app_unseen_assignees_bound(p_item, NULL, NULL) END
  $$;

-- id 2개: 상태 변경 감사, 리더 변경 감사.
CREATE FUNCTION app_set_item_profile(p_item uuid, p_leader uuid, p_goal text, p_scope text, p_starts date,
                                     p_ends date, p_status text, p_details jsonb, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_status NOT IN ('planned', 'active', 'on_hold', 'done', 'cancelled') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF p_starts IS NOT NULL AND p_ends IS NOT NULL AND p_starts > p_ends THEN
      RAISE EXCEPTION 'the period ends before it starts' USING ERRCODE = '22023';
    END IF;
    IF length(coalesce(p_goal, '')) > 4000 OR length(coalesce(p_scope, '')) > 4000
       OR length(coalesce(p_details, '{}'::jsonb)::text) > 20000 THEN
      RAISE EXCEPTION 'item details are too long' USING ERRCODE = '22023';
    END IF;
    IF p_leader IS NOT NULL AND p_leader IS DISTINCT FROM v_item.leader_id
       AND app_item_access_as(p_item, p_leader) IS NULL THEN
      RAISE EXCEPTION 'the leader must be able to see the item' USING ERRCODE = '22023';
    END IF;
    UPDATE items SET leader_id = p_leader, goal = coalesce(p_goal, ''), scope = coalesce(p_scope, ''),
                     starts_on = p_starts, ends_on = p_ends, status = p_status,
                     details = coalesce(p_details, '{}'::jsonb), updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.status <> p_status THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'change_status', 'item', p_item,
                        jsonb_build_object('from', v_item.status, 'to', p_status));
    END IF;
    IF v_item.leader_id IS DISTINCT FROM p_leader THEN
      PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'change_leader', 'item', p_item,
                        jsonb_build_object('from', v_item.leader_id, 'to', p_leader));
    END IF;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_set_item_template(p_item uuid, p_template boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR UPDATE;
    PERFORM app_record_write_check(p_item);
    UPDATE items SET is_template = p_template, updated_at = now() WHERE id = p_item;
    PERFORM app_audit(app_id(p_ids, 1), v_ws, CASE WHEN p_template THEN 'mark_template' ELSE 'unmark_template' END,
                      'item', p_item, '{}'::jsonb);
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 강좌·스터디
------------------------------------------------------------------------------------------------------------------

-- id 1개.
CREATE FUNCTION app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text,
                                       p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_require_course_write(p_item);
    IF length(coalesce(p_url, '')) > 2000 OR length(coalesce(p_subject, '') || coalesce(p_instructor, '')
       || coalesce(p_platform, '') || coalesce(p_textbook, '')) > 2000 THEN
      RAISE EXCEPTION 'course details are too long' USING ERRCODE = '22023';
    END IF;
    IF NOT app_web_url_ok(btrim(coalesce(p_url, ''))) THEN
      RAISE EXCEPTION 'the link must start with http:// or https://' USING ERRCODE = '22023';
    END IF;
    INSERT INTO course_details (item_id, workspace_id, subject, instructor, platform, url, textbook,
                                starts_on, expires_on, target_on, updated_by, updated_at)
    SELECT p_item, i.workspace_id, btrim(coalesce(p_subject, '')), btrim(coalesce(p_instructor, '')),
           btrim(coalesce(p_platform, '')), btrim(coalesce(p_url, '')), btrim(coalesce(p_textbook, '')),
           p_starts, p_expires, p_target, app_user_id(), now()
    FROM items i WHERE i.id = p_item
    ON CONFLICT (item_id) DO UPDATE SET
      subject = EXCLUDED.subject, instructor = EXCLUDED.instructor, platform = EXCLUDED.platform,
      url = EXCLUDED.url, textbook = EXCLUDED.textbook, starts_on = EXCLUDED.starts_on,
      expires_on = EXCLUDED.expires_on, target_on = EXCLUDED.target_on,
      updated_by = EXCLUDED.updated_by, updated_at = now();
    PERFORM app_audit(app_id(p_ids, 1), (SELECT workspace_id FROM items WHERE id = p_item), 'update_course', 'item',
                      p_item, '{}'::jsonb);
  END
  $$;

-- id: 보낸 줄마다 1개(같은 요청으로 이미 있는 줄은 쓰지 않음), 그 뒤 감사 기록 1개.
CREATE FUNCTION app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
    v_at    integer := 0;
  BEGIN
    PERFORM app_require_course_write(p_item);
    IF jsonb_typeof(p_rows) <> 'array' OR jsonb_array_length(p_rows) NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'give 1 to 1000 episodes' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR NO KEY UPDATE;
    SELECT coalesce(max(number), 0) + 1 INTO v_next FROM course_episodes WHERE item_id = p_item;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_rows) LOOP
      IF length(btrim(coalesce(v_row->>'title', ''))) NOT BETWEEN 1 AND 300 THEN
        RAISE EXCEPTION 'every episode needs a title (1-300 characters)' USING ERRCODE = '22023';
      END IF;
      v_at := v_at + 1;
      INSERT INTO course_episodes (id, item_id, workspace_id, number, title, chapter, duration_seconds, url, materials,
                                   planned_on, created_by, request_id)
      VALUES (app_id(p_ids, v_at), p_item, v_ws, coalesce((v_row->>'number')::integer, v_next), btrim(v_row->>'title'),
              btrim(coalesce(v_row->>'chapter', '')), coalesce((v_row->>'duration_seconds')::integer, 0),
              btrim(coalesce(v_row->>'url', '')), coalesce(v_row->>'materials', ''),
              (v_row->>'planned_on')::date, app_user_id(), (v_row->>'request_id')::uuid)
      ON CONFLICT (item_id, created_by, request_id) DO NOTHING;
      IF FOUND THEN
        v_added := v_added + 1;
        v_next := greatest(v_next, coalesce((v_row->>'number')::integer, v_next)) + 1;
      END IF;
    END LOOP;
    IF v_added > 0 THEN
      PERFORM app_audit(app_id(p_ids, v_at + 1), v_ws, 'add_episodes', 'item', p_item,
                        jsonb_build_object('count', v_added));
    END IF;
    RETURN v_added;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow
         OR numeric_value_out_of_range OR check_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- id 1개.
CREATE FUNCTION app_delete_episode(p_episode uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ep course_episodes%ROWTYPE;
  BEGIN
    SELECT * INTO v_ep FROM course_episodes WHERE id = p_episode;
    IF v_ep.id IS NULL OR app_item_access(v_ep.item_id) IS NULL THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_ep.item_id);
    DELETE FROM course_episodes WHERE id = p_episode;
    PERFORM app_audit(app_id(p_ids, 1), v_ep.workspace_id, 'delete_episode', 'item', v_ep.item_id,
                      jsonb_build_object('number', v_ep.number, 'title', v_ep.title));
  END
  $$;

-- id: 보낸 줄마다 1개, 그 뒤 감사 기록 1개.
CREATE FUNCTION app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
    v_at    integer := 0;
  BEGIN
    PERFORM app_require_study_write(p_item);
    IF jsonb_typeof(p_rows) <> 'array' OR jsonb_array_length(p_rows) NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'give 1 to 1000 units' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR NO KEY UPDATE;
    SELECT coalesce(max(number), 0) + 1 INTO v_next FROM study_units WHERE item_id = p_item;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_rows) LOOP
      v_at := v_at + 1;
      INSERT INTO study_units (id, item_id, workspace_id, number, title, chapter, unit, amount, planned_on, created_by,
                               request_id)
      VALUES (app_id(p_ids, v_at), p_item, v_ws, coalesce((v_row->>'number')::integer, v_next), btrim(v_row->>'title'),
              btrim(coalesce(v_row->>'chapter', '')), coalesce(v_row->>'unit', 'pages'),
              coalesce((v_row->>'amount')::integer, 0), (v_row->>'planned_on')::date, app_user_id(),
              (v_row->>'request_id')::uuid)
      ON CONFLICT (item_id, created_by, request_id) DO NOTHING;
      IF FOUND THEN
        v_added := v_added + 1;
        v_next := greatest(v_next, coalesce((v_row->>'number')::integer, v_next)) + 1;
      END IF;
    END LOOP;
    IF v_added > 0 THEN
      PERFORM app_audit(app_id(p_ids, v_at + 1), v_ws, 'add_study_units', 'item', p_item,
                        jsonb_build_object('count', v_added));
    END IF;
    RETURN v_added;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow
         OR numeric_value_out_of_range OR check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- id 1개.
CREATE FUNCTION app_delete_study_unit(p_unit uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_unit study_units%ROWTYPE;
  BEGIN
    SELECT * INTO v_unit FROM study_units WHERE id = p_unit;
    IF v_unit.id IS NULL OR app_item_access(v_unit.item_id) IS NULL THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_unit.item_id);
    DELETE FROM study_units WHERE id = p_unit;
    PERFORM app_audit(app_id(p_ids, 1), v_unit.workspace_id, 'delete_study_unit', 'item', v_unit.item_id,
                      jsonb_build_object('number', v_unit.number, 'title', v_unit.title));
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 기록·저장한 보기
------------------------------------------------------------------------------------------------------------------

-- id 2개: 기록, 감사 기록.
CREATE FUNCTION app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date,
                                  p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
    v_ws uuid;
  BEGIN
    SELECT id INTO v_id FROM item_records
    WHERE item_id = p_item AND created_by = app_user_id() AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR KEY SHARE;
    PERFORM app_record_write_check(p_item);
    INSERT INTO item_records (id, item_id, workspace_id, kind, title, occurred_on, body, fields, created_by, request_id)
    VALUES (app_id(p_ids, 1), p_item, v_ws, p_kind, btrim(p_title), p_occurred, coalesce(p_body, ''),
            coalesce(p_fields, '{}'), app_user_id(), p_request)
    RETURNING id INTO v_id;
    PERFORM app_set_record_files(v_id, p_item, p_files);
    PERFORM app_audit(app_id(p_ids, 2), v_ws, 'create_record', 'item', p_item,
                      jsonb_build_object('record', v_id, 'kind', p_kind));
    RETURN v_id;
  EXCEPTION
    WHEN check_violation THEN
      RAISE EXCEPTION 'invalid record: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- id 1개.
CREATE FUNCTION app_delete_record(p_record uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_rec    item_records%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    v_access := app_item_access(v_rec.item_id);
    IF v_rec.id IS NULL OR v_access IS NULL THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.created_by <> app_user_id() AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the author or the item owner can delete a record' USING ERRCODE = '42501';
    END IF;
    DELETE FROM item_records WHERE id = p_record;
    PERFORM app_audit(app_id(p_ids, 1), v_rec.workspace_id, 'delete_record', 'item', v_rec.item_id,
                      jsonb_build_object('record', p_record, 'kind', v_rec.kind, 'title', v_rec.title));
  END
  $$;

-- id 1개(새 보기일 때).
CREATE FUNCTION app_save_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
  BEGIN
    SELECT id INTO v_id FROM saved_views WHERE user_id = app_user_id() AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO saved_views (id, user_id, item_id, name, config, request_id)
    VALUES (app_id(p_ids, 1), app_user_id(), p_item, btrim(p_name), p_config, p_request)
    ON CONFLICT (user_id, item_id, name) DO UPDATE SET config = EXCLUDED.config
    RETURNING id INTO v_id;
    RETURN v_id;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_create_module(uuid, uuid, text, text, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_module(uuid, text, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_archive_module(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_item(uuid, uuid, text, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_item(uuid, text, text, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_item_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_archive_item(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_copy_item(uuid, uuid, uuid, text, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_copy_item_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_members(uuid, uuid[], uuid[], uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_members_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_template(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_course_details(uuid, text, text, text, text, text, date, date, date, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_episodes(uuid, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_episode(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_study_units(uuid, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_study_unit(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_record(uuid, uuid, text, text, date, text, jsonb, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_record(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_save_view(uuid, uuid, text, jsonb, uuid[]) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
  app_create_module(uuid, uuid, text, text, text, uuid[]),
  app_update_module(uuid, text, text, uuid[]),
  app_archive_module(uuid, boolean, uuid[]),
  app_create_item(uuid, uuid, text, text, uuid[]),
  app_update_item(uuid, text, text, text, uuid[]),
  app_update_item_ids(uuid),
  app_archive_item(uuid, boolean, uuid[]),
  app_copy_item(uuid, uuid, uuid, text, boolean, uuid[]),
  app_copy_item_ids(uuid),
  app_set_item_members(uuid, uuid[], uuid[], uuid[]),
  app_set_item_members_ids(uuid),
  app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb, uuid[]),
  app_set_item_template(uuid, boolean, uuid[]),
  app_set_course_details(uuid, text, text, text, text, text, date, date, date, uuid[]),
  app_add_episodes(uuid, jsonb, uuid[]),
  app_delete_episode(uuid, uuid[]),
  app_add_study_units(uuid, jsonb, uuid[]),
  app_delete_study_unit(uuid, uuid[]),
  app_create_record(uuid, uuid, text, text, date, text, jsonb, jsonb, uuid[]),
  app_delete_record(uuid, uuid[]),
  app_save_view(uuid, uuid, text, jsonb, uuid[])
TO fvoci_app;
