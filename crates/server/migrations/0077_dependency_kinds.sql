-- 선행 관계의 종류와 지연일(D-91).
--
-- 1. 관계 행에 종류(`kind`: FS 종료 후 시작, SS 시작 후 시작, FF 종료 후 종료, SF 시작 후 종료)와 지연일
--    (`lag_days`, 0 이상 정수, 달력 날짜)을 더한다. 있던 관계는 기본값으로 FS·0이 되어 뜻이 그대로다. 관계는 여전히
--    업무 쌍마다 하나(기본 키 그대로)이고 id가 없어 새로 만들 id도 없다.
-- 2. `app_set_task_dependency`: 종류·지연일을 주고 관계를 만들거나 고친다. 사용자가 승인한 후속 재배치를 같은
--    트랜잭션에서 함께 옮긴다(관계를 고친 후속 업무 자신과 그 뒤 업무). 기준 버전이 하나라도 다르면 아무것도 바꾸지
--    않고 `shift_conflict`. 승인 목록이 지금 상태로 다시 계산한 계획과 같은지는 서버(Rust)가 같은 트랜잭션에서 본다.
-- 3. `app_add_task_dependency`(옛 경로, 종류 없이)는 바꾸지 않는다: 없으면 FS·0으로 만들고, 있으면 그대로 둔다(옛
--    클라이언트가 고친 종류·지연일을 FS·0으로 되돌리지 않는다).
-- 4. 항목 복제(`app_copy_item`)가 종류·지연일을 함께 복사한다. 함수 본문은 0072와 같고 선행 관계 복사만 다르다.

ALTER TABLE task_dependencies
  ADD COLUMN kind text NOT NULL DEFAULT 'FS',
  ADD COLUMN lag_days integer NOT NULL DEFAULT 0,
  ADD CONSTRAINT task_dependencies_kind CHECK (kind IN ('FS', 'SS', 'FF', 'SF')),
  ADD CONSTRAINT task_dependencies_lag CHECK (lag_days BETWEEN 0 AND 3650);

-- id: 관계 감사 1개, 옮긴 업무마다 감사 1개. 결과: `created`·`updated`·`unchanged`·`shift_conflict`와 잠근 뒤 읽은
-- 이전 관계(없었으면 NULL). 서버가 승인 목록을 다시 계산할 때 그 이전 관계를 기준으로 쓴다.
CREATE FUNCTION app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer,
                                        p_shifts jsonb, p_ids uuid[])
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

    -- 승인한 재배치: 관계를 고친 후속 업무 자신이거나 그 뒤 업무여야 하고, 기준 버전이 모두 맞아야 한다.
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      IF v_other.id IS NULL OR v_other.item_id <> v_succ.item_id OR NOT EXISTS (
        WITH RECURSIVE reach(id) AS (
          SELECT p_successor
          UNION
          SELECT d.successor_id FROM task_dependencies d JOIN reach r ON d.predecessor_id = r.id
        )
        SELECT 1 FROM reach WHERE id = v_other.id
      ) THEN
        RAISE EXCEPTION 'a shifted task must be the successor or come after it' USING ERRCODE = '22023';
      END IF;
      IF v_other.version IS DISTINCT FROM (v_shift ->> 'base_version')::integer THEN
        RETURN QUERY SELECT 'shift_conflict', v_old.kind, v_old.lag_days;
        RETURN;
      END IF;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      IF (v_start IS NULL) <> (v_other.start_date IS NULL) OR (v_due IS NULL) <> (v_other.due_date IS NULL)
         OR (v_start IS NOT NULL AND v_due IS NOT NULL AND v_start > v_due) THEN
        RAISE EXCEPTION 'a shift moves existing dates only' USING ERRCODE = '22023';
      END IF;
    END LOOP;

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

REVOKE ALL ON FUNCTION app_set_task_dependency(uuid, uuid, text, integer, jsonb, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_task_dependency(uuid, uuid, text, integer, jsonb, uuid[]) TO fvoci_app;

-- id: 새 항목과 그 감사(2), 업무마다 id·요청 id(2), 체크리스트 줄마다 id·요청 id(2), 문서마다 id·요청 id(2, 첫
-- 리비전도 같은 요청 id), 회차·스터디 단위마다 id·요청 id(2), 복제 감사(1). 개수는 `_ids`.
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
