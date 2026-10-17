-- 항목 복제와 재사용 템플릿 (D-33 "생성·수정·복제", "회고와 재사용 템플릿", D-20 복제 허용, D-48 번호).
--
-- 복제는 같은 워크스페이스의 같은 종류 모듈 안에 새 항목을 만든다. 새 항목은 비공개이고 복제한 사람이 만든
-- 사람이 된다(D-20 새 항목 기본 비공개). 옮기는 것: 항목 정보(리더 제외), 업무(하위 업무·체크리스트·선행 관계),
-- 항목 문서(트리 구조), 강좌 정보·회차, 스터디 단위. 업무·문서는 대상 모듈에서 새 번호를 받는다(D-48).
-- 옮기지 않는 것: 담당자(새 항목은 비공개라 볼 수 없음), 반복 규칙, 기록·첨부 파일, 사람마다의 진도·진행,
-- 복제를 허용하지 않은 문서(소유 권한자가 아니면 건너뛰고 수를 알린다).
-- "새로 시작"(템플릿 사용)이면 업무 상태·날짜·체크 표시와 항목 상태·기간을 비운다.
-- 템플릿 표시는 쓸 수 있는 사람이 항목에 단다. 모듈 화면에서 "템플릿에서 만들기"로 쓴다.

ALTER TABLE items ADD COLUMN is_template boolean NOT NULL DEFAULT false;

CREATE FUNCTION app_set_item_template(p_item uuid, p_template boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR UPDATE;
    PERFORM app_record_write_check(p_item);
    UPDATE items SET is_template = p_template, updated_at = now() WHERE id = p_item;
    PERFORM app_audit(v_ws, CASE WHEN p_template THEN 'mark_template' ELSE 'unmark_template' END, 'item', p_item,
                      '{}'::jsonb);
  END
  $$;

-- 복제. 돌려주는 값: 새 항목 id와 건너뛴 문서 수.
CREATE FUNCTION app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text, p_reset boolean)
  RETURNS TABLE (copied_item uuid, skipped_count integer)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    v_new := app_create_item(p_module, p_request, coalesce(nullif(btrim(p_title), ''), v_src.title), v_src.description);
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
      v_id := uuidv7();
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
              v_user, v_user, uuidv7());
      v_map := v_map || jsonb_build_object(v_row.id::text, v_id);
      INSERT INTO checklist_items (task_id, text, checked, position, request_id, created_by)
        SELECT v_id, c.text, CASE WHEN p_reset THEN false ELSE c.checked END, c.position, uuidv7(), v_user
        FROM checklist_items c WHERE c.task_id = v_row.id;
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
      v_id := uuidv7();
      INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility, parent_id, position)
      VALUES (v_id, v_src.workspace_id, v_new, 'module', p_module::text,
              app_allocate_number(v_src.workspace_id, p_module, 'document', v_id), v_row.title,
              v_row.schema_version, v_row.body, 1, v_user, v_user, uuidv7(),
              v_row.visibility,
              (v_map->>v_row.parent_id::text)::uuid, v_row.position);
      INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
      VALUES (v_id, 1, v_row.title, v_row.schema_version, v_row.body, v_user, uuidv7());
      v_map := v_map || jsonb_build_object(v_row.id::text, v_id);
    END LOOP;

    -- 강좌 정보·회차(진도 제외), 스터디 단위(진행 제외).
    INSERT INTO course_details (item_id, workspace_id, subject, instructor, platform, url, textbook, starts_on,
                                expires_on, target_on, updated_by)
      SELECT v_new, workspace_id, subject, instructor, platform, url, textbook,
             CASE WHEN p_reset THEN NULL ELSE starts_on END, CASE WHEN p_reset THEN NULL ELSE expires_on END,
             CASE WHEN p_reset THEN NULL ELSE target_on END, v_user
      FROM course_details WHERE item_id = p_item;
    INSERT INTO course_episodes (item_id, workspace_id, number, title, chapter, duration_seconds, url, materials,
                                 planned_on, created_by, request_id)
      SELECT v_new, workspace_id, number, title, chapter, duration_seconds, url, materials,
             CASE WHEN p_reset THEN NULL ELSE planned_on END, v_user, uuidv7()
      FROM course_episodes WHERE item_id = p_item;
    INSERT INTO study_units (item_id, workspace_id, number, title, chapter, unit, amount, planned_on, created_by,
                             request_id)
      SELECT v_new, workspace_id, number, title, chapter, unit, amount,
             CASE WHEN p_reset THEN NULL ELSE planned_on END, v_user, uuidv7()
      FROM study_units WHERE item_id = p_item;

    PERFORM app_audit(v_src.workspace_id, 'copy', 'item', v_new,
                      jsonb_build_object('from', p_item, 'reset', p_reset, 'skipped_documents', v_skipped));
    RETURN QUERY SELECT v_new, v_skipped;
  END
  $$;

REVOKE ALL ON FUNCTION app_set_item_template(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_copy_item(uuid, uuid, uuid, text, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_set_item_template(uuid, boolean),
  app_copy_item(uuid, uuid, uuid, text, boolean)
TO fvoci_app;
