-- 합쳐진 열의 위치값(초기화 복제)과 겹친 위치에서의 이동 (D-31·D-79, 감사 후속).
--
-- 1. 초기화 복제(`app_copy_item`, `p_reset`)는 모든 업무를 "할 일"로 합치는데 위치값은 원본 열의 값을 그대로 복사했다.
--    원본 A(할 일, 1)·B(완료, 1)·C(할 일, 2)를 복제하면 A와 B가 같은 위치 1을 가진다. 보이는 순서(위치, 번호)는 그대로지만
--    그 뒤 "C를 B 앞으로"가 B의 위치 1 아래 최댓값을 찾지 못해 0.5가 되어 [C, A, B]로 간다(통합 시험으로 확인).
--    복제가 업무를 넣은 뒤 합쳐진 열을 다시 매긴다(`app_renumber_lane`은 보이는 순서(위치, 번호)대로 매기므로 순서는
--    바뀌지 않는다). 함수 본문은 0077과 같고 다시 매기기만 더했다.
-- 2. 앞선 복제가 남긴 겹친 위치는 이 마이그레이션이 한 번에 다시 매긴다(겹친 위치가 있는 열만, 보이는 순서 그대로). 위치가
--    실제로 바뀐 업무만 이동 판(`move_version`)이 오른다(0076 트리거). 이동 함수에 옛 데이터를 위한 분기를 두지 않는다
--    (v1.0.0 전에는 지난 개발 버전과의 호환 분기를 두지 않는다는 사용자 지시).

CREATE OR REPLACE FUNCTION app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text,
                                         p_reset boolean, p_ids uuid[])
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
    -- 0079: 초기화 복제는 모든 열을 "할 일"로 합치므로 원본 열마다의 위치값이 겹친다. 보이는 순서대로 다시 매긴다.
    IF p_reset THEN
      PERFORM app_renumber_lane(v_new, 'todo');
    END IF;
    -- 0077: 종류·지연일도 복사한다.
    INSERT INTO task_dependencies (predecessor_id, successor_id, item_id, created_by, kind, lag_days)
      SELECT (v_map->>d.predecessor_id::text)::uuid, (v_map->>d.successor_id::text)::uuid, v_new, v_user,
             d.kind, d.lag_days
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

REVOKE ALL ON FUNCTION app_copy_item(uuid, uuid, uuid, text, boolean, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_copy_item(uuid, uuid, uuid, text, boolean, uuid[]) TO fvoci_app;

-- 앞선 초기화 복제가 남긴 겹친 위치를 보이는 순서(위치, 번호)대로 다시 매긴다. 겹친 위치가 있는 열만 건드린다.
DO $$
  DECLARE
    v_lane record;
  BEGIN
    FOR v_lane IN
      SELECT DISTINCT item_id, status
      FROM (SELECT item_id, status FROM tasks GROUP BY item_id, status, position HAVING count(*) > 1) d
    LOOP
      PERFORM app_renumber_lane(v_lane.item_id, v_lane.status);
    END LOOP;
  END
$$;
