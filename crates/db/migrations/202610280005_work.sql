-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

CREATE TABLE fvoci.checklist_items (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    text text NOT NULL,
    checked boolean DEFAULT false NOT NULL,
    "position" numeric NOT NULL,
    request_id uuid NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT checklist_items_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT checklist_items_text_check CHECK ((length(btrim(text)) > 0))
);


ALTER TABLE fvoci.checklist_items OWNER TO fvoci_migrator;

CREATE TABLE fvoci.events (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid NOT NULL,
    kind text NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    all_day boolean NOT NULL,
    start_date date,
    end_date date,
    starts_at timestamp with time zone,
    ends_at timestamp with time zone,
    timezone text NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    deleted_by uuid,
    request_id uuid NOT NULL,
    recurrence jsonb,
    excluded_dates date[] DEFAULT '{}'::date[] NOT NULL,
    trash_batch_id uuid,
    CONSTRAINT events_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT events_kind_check CHECK ((kind = ANY (ARRAY['meeting'::text, 'presentation'::text, 'event'::text]))),
    CONSTRAINT events_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT events_version_check CHECK ((version > 0)),
    CONSTRAINT events_when CHECK (((all_day AND (start_date IS NOT NULL) AND (end_date IS NOT NULL) AND (start_date <= end_date) AND (starts_at IS NULL) AND (ends_at IS NULL)) OR ((NOT all_day) AND (starts_at IS NOT NULL) AND (ends_at IS NOT NULL) AND (starts_at <= ends_at) AND (start_date IS NULL) AND (end_date IS NULL))))
);


ALTER TABLE fvoci.events OWNER TO fvoci_migrator;

CREATE TABLE fvoci.saved_views (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    item_id uuid NOT NULL,
    name text NOT NULL,
    config jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    updated_request_id uuid,
    CONSTRAINT saved_views_config_check CHECK (((jsonb_typeof(config) = 'object'::text) AND (length((config)::text) <= 4000))),
    CONSTRAINT saved_views_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT saved_views_name_check CHECK (((length(btrim(name)) >= 1) AND (length(btrim(name)) <= 100)))
);


ALTER TABLE fvoci.saved_views OWNER TO fvoci_migrator;

CREATE TABLE fvoci.task_assignees (
    task_id uuid NOT NULL,
    user_id uuid NOT NULL,
    assigned_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.task_assignees OWNER TO fvoci_migrator;

CREATE TABLE fvoci.task_dependencies (
    predecessor_id uuid NOT NULL,
    successor_id uuid NOT NULL,
    item_id uuid NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    kind text DEFAULT 'FS'::text NOT NULL,
    lag_days integer DEFAULT 0 NOT NULL,
    CONSTRAINT task_dependencies_distinct CHECK ((predecessor_id <> successor_id)),
    CONSTRAINT task_dependencies_kind CHECK ((kind = ANY (ARRAY['FS'::text, 'SS'::text, 'FF'::text, 'SF'::text]))),
    CONSTRAINT task_dependencies_lag CHECK (((lag_days >= 0) AND (lag_days <= 3650)))
);


ALTER TABLE fvoci.task_dependencies OWNER TO fvoci_migrator;

CREATE TABLE fvoci.task_series (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid NOT NULL,
    root_task_id uuid NOT NULL,
    rule jsonb,
    timezone text NOT NULL,
    anchor date NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    effective_from date NOT NULL,
    skipped_dates date[] DEFAULT '{}'::date[] NOT NULL,
    CONSTRAINT task_series_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.task_series OWNER TO fvoci_migrator;

-- 항목별 업무 상태 정의(D-116). 범주 todo·doing·done만 집계·알림·간트가 본다. 새 항목은 기본 세 상태로 시작한다.
CREATE TABLE fvoci.task_statuses (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    name text NOT NULL,
    color text DEFAULT ''::text NOT NULL,
    "position" numeric NOT NULL,
    category text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT task_statuses_category_check CHECK ((category = ANY (ARRAY['todo'::text, 'doing'::text, 'done'::text]))),
    CONSTRAINT task_statuses_color_check CHECK (((color = ''::text) OR (color ~ '^#[0-9a-f]{6}$'::text))),
    CONSTRAINT task_statuses_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT task_statuses_name_check CHECK (((name = btrim(name)) AND (length(name) >= 1) AND (length(name) <= 50)))
);

CREATE TABLE fvoci.tasks (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid NOT NULL,
    parent_id uuid,
    number bigint NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    status_id uuid NOT NULL,
    priority text DEFAULT 'none'::text NOT NULL,
    start_date date,
    due_date date,
    estimate_minutes integer,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    "position" numeric DEFAULT 0 NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    completed_at timestamp with time zone,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    milestone boolean DEFAULT false NOT NULL,
    series_id uuid,
    occurrence_date date,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    move_version integer DEFAULT 0 NOT NULL,
    CONSTRAINT tasks_dates CHECK (((start_date IS NULL) OR (due_date IS NULL) OR (start_date <= due_date))),
    CONSTRAINT tasks_estimate_minutes_check CHECK ((estimate_minutes >= 0)),
    CONSTRAINT tasks_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT tasks_milestone_single_date CHECK (((NOT milestone) OR (start_date IS NULL))),
    CONSTRAINT tasks_priority_check CHECK ((priority = ANY (ARRAY['none'::text, 'low'::text, 'medium'::text, 'high'::text, 'urgent'::text]))),
    CONSTRAINT tasks_series_occurrence CHECK (((series_id IS NULL) = (occurrence_date IS NULL))),
    CONSTRAINT tasks_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT tasks_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.task_statuses OWNER TO fvoci_migrator;

ALTER TABLE fvoci.tasks OWNER TO fvoci_migrator;

-- 상태 정의 관리(D-116). 항목 소유 권한이 정의를 만들고 고치고 지운다. 지울 때는 그 상태의 업무를 다른 상태로 옮긴다.
CREATE FUNCTION fvoci.app_task_statuses(p_item uuid) RETURNS TABLE(id uuid, name text, color text, "position" numeric, category text, task_count integer)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      SELECT s.id, s.name, s.color, s.position, s.category,
             (SELECT count(*)::integer FROM tasks t WHERE t.status_id = s.id AND t.deleted_at IS NULL)
      FROM task_statuses s WHERE s.item_id = p_item
      ORDER BY s.position, s.created_at, s.id;
  END
  $$;


ALTER FUNCTION fvoci.app_task_statuses(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_task_status(p_item uuid, p_name text, p_color text, p_category text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item items%ROWTYPE;
    v_id   uuid;
  BEGIN
    -- 항목의 상태 집합 변경(생성·수정·삭제)을 줄 세운다: 개수·이름 중복 검사가 잠금 뒤에 유효하도록.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-statuses:' || p_item::text, 0));
    SELECT * INTO v_item FROM items WHERE id = p_item FOR SHARE;
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    -- 보관된 항목은 볼 수 있는 누구에게나 보관 상태(409)가 먼저다(문서·업무·파일과 같은 규칙).
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    IF p_category NOT IN ('todo', 'doing', 'done') THEN
      RAISE EXCEPTION 'invalid category' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM task_statuses WHERE item_id = p_item AND name = btrim(coalesce(p_name, ''))) THEN
      RAISE EXCEPTION 'a status with this name exists' USING ERRCODE = '55000';
    END IF;
    IF (SELECT count(*) FROM task_statuses WHERE item_id = p_item) >= 30 THEN
      RAISE EXCEPTION 'too many statuses' USING ERRCODE = '22023';
    END IF;
    INSERT INTO task_statuses (id, item_id, name, color, position, category)
    VALUES (app_id(p_ids, 1), p_item, btrim(p_name), coalesce(p_color, ''),
            (SELECT coalesce(max(position), 0) + 1 FROM task_statuses WHERE item_id = p_item), p_category)
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create_status', 'item', p_item,
                      jsonb_build_object('status', v_id, 'name', btrim(p_name), 'category', p_category));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_task_status(p_item uuid, p_name text, p_color text, p_category text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 이름·색·순서·범주 바꾸기. 범주가 done으로 바뀌거나 done에서 벗어나면 그 상태의 업무 완료 시각을 맞춘다. 이름이 바뀌면 그
-- 상태의 업무를 다시 색인한다(task_statuses_search 트리거).
CREATE FUNCTION fvoci.app_update_task_status(p_status uuid, p_name text, p_color text, p_category text, p_position numeric, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
    v_item   items%ROWTYPE;
    v_name   text;
    v_cat    text;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status;
    IF v_status.id IS NULL OR app_access('item', v_status.item_id) = 'none' THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    -- 항목의 상태 집합 변경을 줄 세운 뒤(생성·삭제와 같은 잠금) 열을 잠그고, 잠금 뒤 등급·보관·존재를 다시 본다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-statuses:' || v_status.item_id::text, 0));
    PERFORM app_lock_lane(p_status);
    SELECT * INTO v_item FROM items WHERE id = v_status.item_id FOR SHARE;
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status FOR UPDATE;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    v_name := coalesce(btrim(p_name), v_status.name);
    v_cat := coalesce(p_category, v_status.category);
    IF v_cat NOT IN ('todo', 'doing', 'done') THEN
      RAISE EXCEPTION 'invalid category' USING ERRCODE = '22023';
    END IF;
    IF v_name <> v_status.name AND EXISTS (SELECT 1 FROM task_statuses WHERE item_id = v_status.item_id AND name = v_name) THEN
      RAISE EXCEPTION 'a status with this name exists' USING ERRCODE = '55000';
    END IF;
    UPDATE task_statuses SET name = v_name, color = coalesce(p_color, color), category = v_cat,
                             position = coalesce(p_position, position), updated_at = now()
    WHERE id = p_status;
    IF v_cat <> v_status.category THEN
      UPDATE tasks SET completed_at = CASE WHEN v_cat = 'done' THEN coalesce(completed_at, now()) END
      WHERE status_id = p_status;
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'update_status', 'item', v_status.item_id,
                      jsonb_build_object('status', p_status, 'from', jsonb_build_object('name', v_status.name,
                        'color', v_status.color, 'category', v_status.category, 'position', v_status.position),
                        'to', jsonb_build_object('name', v_name, 'color', coalesce(p_color, v_status.color),
                        'category', v_cat, 'position', coalesce(p_position, v_status.position))));
  END
  $$;


ALTER FUNCTION fvoci.app_update_task_status(p_status uuid, p_name text, p_color text, p_category text, p_position numeric, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 상태 지우기. 그 상태의 업무가 있으면 같은 항목의 p_move_to로 옮긴 뒤 지운다(D-116). 마지막 상태는 지울 수 없다.
CREATE FUNCTION fvoci.app_delete_task_status(p_status uuid, p_move_to uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
    v_to     task_statuses%ROWTYPE;
    v_item   items%ROWTYPE;
    v_lane   uuid;
    v_moved  integer := 0;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status;
    IF v_status.id IS NULL OR app_access('item', v_status.item_id) = 'none' THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    IF p_move_to IS NOT NULL THEN
      SELECT * INTO v_to FROM task_statuses WHERE id = p_move_to;
      IF v_to.id IS NULL OR v_to.item_id <> v_status.item_id OR v_to.id = p_status THEN
        RAISE EXCEPTION 'move the tasks to another status of the same item' USING ERRCODE = '22023';
      END IF;
    END IF;
    -- 항목의 상태 집합 변경을 줄 세운다(생성·수정과 같은 잠금): 마지막 상태 보장이 동시 삭제에서도 지켜지도록 개수 검사는
    -- 이 잠금 뒤에 한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-statuses:' || v_status.item_id::text, 0));
    IF (SELECT count(*) FROM task_statuses WHERE item_id = v_status.item_id) <= 1 THEN
      RAISE EXCEPTION 'an item needs at least one status' USING ERRCODE = '55000';
    END IF;
    -- 두 열을 id 순서로 잠근 뒤(이동·생성과 같은 순서) 항목을 공유 잠금하고, 잠금 뒤 등급·보관·존재를 다시 본다.
    FOR v_lane IN SELECT x FROM unnest(ARRAY[p_status, p_move_to]) x WHERE x IS NOT NULL ORDER BY x LOOP
      PERFORM app_lock_lane(v_lane);
    END LOOP;
    SELECT * INTO v_item FROM items WHERE id = v_status.item_id FOR SHARE;
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status FOR UPDATE;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    IF EXISTS (SELECT 1 FROM tasks WHERE status_id = p_status) THEN
      IF p_move_to IS NULL THEN
        RAISE EXCEPTION 'choose a status for the tasks in this column' USING ERRCODE = '22023';
      END IF;
      PERFORM 1 FROM tasks WHERE status_id = p_status ORDER BY id FOR NO KEY UPDATE;
      UPDATE tasks t SET
        status_id = p_move_to,
        position = r.pos + (SELECT coalesce(max(position), 0) FROM tasks WHERE status_id = p_move_to),
        version = version + 1, updated_at = now(),
        completed_at = CASE WHEN v_to.category = 'done' THEN coalesce(t.completed_at, now()) END
      FROM (SELECT id, row_number() OVER (ORDER BY position, number) AS pos FROM tasks WHERE status_id = p_status) r
      WHERE t.id = r.id;
      GET DIAGNOSTICS v_moved = ROW_COUNT;
    END IF;
    DELETE FROM task_statuses WHERE id = p_status;
    PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'delete_status', 'item', v_status.item_id,
                      jsonb_build_object('status', p_status, 'name', v_status.name, 'moved_to', p_move_to, 'moved', v_moved));
    RETURN v_moved;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_task_status(p_status uuid, p_move_to uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 업무 반복 규칙(D-31·D-125). 일정 반복과 같은 기준 판 검사: 지금 규칙이 보낸 규칙과도, 편집을 시작할 때 본 규칙(p_base)과도
-- 다르면 아무것도 바꾸지 않고 conflict를 돌려준다(409). 새 id 2: 계열, 감사.
CREATE FUNCTION fvoci.app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_base jsonb, p_ids uuid[]) RETURNS TABLE(series_id uuid, outcome text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task    tasks%ROWTYPE;
    v_series  task_series%ROWTYPE;
    v_current jsonb;
    v_anchor  date;
    v_today   date;
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
    END IF;
    v_current := v_series.rule;
    IF jsonb_strip_nulls(coalesce(v_current, 'null'::jsonb)) IS DISTINCT FROM jsonb_strip_nulls(coalesce(p_rule, 'null'::jsonb))
       AND jsonb_strip_nulls(coalesce(v_current, 'null'::jsonb)) IS DISTINCT FROM jsonb_strip_nulls(coalesce(p_base, 'null'::jsonb)) THEN
      RETURN QUERY SELECT v_series.id, 'conflict'::text;
      RETURN;
    END IF;
    IF v_series.id IS NOT NULL THEN
      IF (p_rule ->> 'until')::date < v_series.anchor THEN
        RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
      END IF;
      IF p_rule IS NOT NULL AND v_task.due_date IS NULL AND v_task.start_date IS NULL THEN
        RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
      END IF;
      IF p_rule IS NOT NULL AND v_task.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'only top-level tasks can repeat' USING ERRCODE = '22023';
      END IF;
      IF jsonb_strip_nulls(coalesce(v_current, 'null'::jsonb)) IS NOT DISTINCT FROM jsonb_strip_nulls(coalesce(p_rule, 'null'::jsonb))
         AND v_series.timezone = p_timezone THEN
        RETURN QUERY SELECT v_series.id, 'updated'::text;
        RETURN;
      END IF;
      UPDATE task_series SET
        rule = p_rule, timezone = p_timezone, updated_at = now(),
        effective_from = CASE WHEN p_rule IS DISTINCT FROM v_series.rule
                              THEN greatest(v_series.anchor, v_today) ELSE effective_from END
      WHERE id = v_series.id;
      PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'set_recurrence', 'task', p_task,
                        jsonb_build_object('from', v_series.rule, 'to', p_rule, 'timezone', p_timezone));
      RETURN QUERY SELECT v_series.id, 'updated'::text;
      RETURN;
    END IF;
    IF p_rule IS NULL THEN
      RETURN QUERY SELECT NULL::uuid, 'updated'::text;
      RETURN;
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
    RETURN QUERY SELECT v_series.id, 'updated'::text;
  END
  $$;


ALTER FUNCTION fvoci.app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_base jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 업무 상태 정의(D-116). API는 상태를 이름으로 가리키고 이름은 항목 안에서 고유하다. 범주(todo·doing·done)만 집계·알림·
-- 간트·검색 필터가 본다. 새 항목은 기본 세 상태(todo·in_progress·done, 언어 중립 키)로 시작한다.
CREATE FUNCTION fvoci.app_task_status(p_status uuid) RETURNS fvoci.task_statuses
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    RETURN v_status;
  END
  $$;


ALTER FUNCTION fvoci.app_task_status(p_status uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_status_by_name(p_item uuid, p_name text) RETURNS fvoci.task_statuses
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE item_id = p_item AND name = btrim(coalesce(p_name, ''));
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    RETURN v_status;
  END
  $$;


ALTER FUNCTION fvoci.app_task_status_by_name(p_item uuid, p_name text) OWNER TO fvoci_migrator;

-- 항목의 기본 상태: todo 범주의 첫 상태, 없으면 순서상 첫 상태. 새 업무·반복 회차·초기화 복제가 쓴다.
CREATE FUNCTION fvoci.app_default_task_status(p_item uuid) RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id uuid;
  BEGIN
    SELECT id INTO v_id FROM task_statuses WHERE item_id = p_item
    ORDER BY (category <> 'todo'), position, created_at, id LIMIT 1;
    IF v_id IS NULL THEN
      RAISE EXCEPTION 'the item has no task status' USING ERRCODE = '22023';
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_default_task_status(p_item uuid) OWNER TO fvoci_migrator;

-- 새 항목의 기본 세 상태. 새 id 셋을 차례로 쓴다.
CREATE FUNCTION fvoci.app_seed_task_statuses(p_item uuid, p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO task_statuses (id, item_id, name, color, position, category)
    VALUES (app_id(p_ids, 1), p_item, '할 일', '', 1, 'todo'),
           (app_id(p_ids, 2), p_item, '진행 중', '', 2, 'doing'),
           (app_id(p_ids, 3), p_item, '완료', '', 3, 'done')
  $$;


ALTER FUNCTION fvoci.app_seed_task_statuses(p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_assignee(p_workspace uuid, p_item uuid, p_user uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_user FOR SHARE;
    IF app_access_as('item', p_item, p_user) = 'none' THEN
      RAISE EXCEPTION 'an assignee cannot see this item' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_assignee(p_workspace uuid, p_item uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_recurrence(p_rule jsonb) RETURNS void
    LANGUAGE plpgsql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_interval integer;
  BEGIN
    IF jsonb_typeof(p_rule) <> 'object' OR p_rule ->> 'freq' NOT IN ('daily', 'weekly', 'monthly', 'yearly') THEN
      RAISE EXCEPTION 'invalid recurrence frequency' USING ERRCODE = '22023';
    END IF;
    v_interval := (p_rule ->> 'interval')::integer;
    IF v_interval IS NULL OR v_interval NOT BETWEEN 1 AND 99 THEN
      RAISE EXCEPTION 'interval must be 1 to 99' USING ERRCODE = '22023';
    END IF;
    IF jsonb_typeof(p_rule -> 'weekdays') <> 'array'
       OR EXISTS (SELECT 1 FROM jsonb_array_elements(p_rule -> 'weekdays') w
                  WHERE jsonb_typeof(w) <> 'number' OR w::integer NOT BETWEEN 1 AND 7)
       OR (p_rule ->> 'freq' <> 'weekly' AND jsonb_array_length(p_rule -> 'weekdays') > 0) THEN
      RAISE EXCEPTION 'weekdays must be ISO weekdays and only for weekly rules' USING ERRCODE = '22023';
    END IF;
    IF p_rule ? 'count' AND p_rule -> 'count' <> 'null'::jsonb
       AND (p_rule ->> 'count')::integer NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'count must be 1 to 1000' USING ERRCODE = '22023';
    END IF;
    IF p_rule ->> 'until' IS NOT NULL AND p_rule ->> 'count' IS NOT NULL THEN
      RAISE EXCEPTION 'use either until or count' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_recurrence(p_rule jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    -- 볼 수 없는 업무는 잠그지 않는다(잠금 대기로 존재가 드러나지 않게). 잠근 뒤 다시 확인한다.
    IF app_access('task', p_parent) = 'none' THEN
      RAISE EXCEPTION 'parent task not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_task IS NOT NULL THEN
      -- 두 업무의 열을 먼저, 그다음 행(app_lock_task_rows).
      PERFORM app_lock_task_rows(ARRAY[p_task, p_parent], NULL);
    ELSE
      -- 새 업무: 만드는 쪽은 이미 할 일 열을 잡았고 이 행 뒤로는 다른 업무 행을 기다리지 않는다.
      PERFORM 1 FROM tasks WHERE id = p_parent FOR NO KEY UPDATE;
    END IF;
    SELECT * INTO v_parent FROM tasks WHERE id = p_parent;
    IF v_parent.id IS NULL OR app_access('task', p_parent) = 'none' THEN
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


ALTER FUNCTION fvoci.app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_access text := app_access('item', p_item);
    v_item   items%ROWTYPE;
    v_id     uuid;
  BEGIN
    IF v_access = 'none' THEN
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
    -- 잠금 뒤 등급을 다시 본다(기다리는 사이 공개 범위·멤버가 바뀌었을 수 있다).
    IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    INSERT INTO events (id, workspace_id, item_id, kind, title, description, all_day, start_date, end_date,
                        starts_at, ends_at, timezone, created_by, updated_by, request_id)
    VALUES (app_id(p_ids, 1), v_item.workspace_id, p_item, p_kind, btrim(p_title), coalesce(p_description, ''),
            p_all_day, p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone, v_user, v_user, p_request_id)
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'event', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    PERFORM app_activity(app_id(p_ids, 3), v_item.workspace_id, p_item, 'event', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-105: 업무 한 건의 필드·담당·번호·감사 기록을 같은 트랜잭션으로 만든다. docs/decisions/tasks.md#d-105.
-- 같은 요청 ID는 입력을 덮거나 비교하지 않고 기존 업무를 반환한다. 삭제된 결과는 FV004이며 권한 검사 뒤에만 알린다.
-- 잠금은 담당 멤버 → 열 → 상위 업무 → 항목 공유 → 번호 순서다. D-105: docs/decisions/tasks.md#d-105.
CREATE FUNCTION fvoci.app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_item   items%ROWTYPE;
    v_access text := app_access('item', p_item);
    v_status task_statuses%ROWTYPE;
    v_id     uuid;
    v_number bigint;
    v_uid    uuid;
    v_people uuid[];
    v_at     integer := 3;
    v_deleted boolean;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT id, deleted_at IS NOT NULL INTO v_id, v_deleted
    FROM tasks WHERE item_id = p_item AND created_by = v_user AND request_id = p_request_id;
    IF v_deleted THEN
      RAISE EXCEPTION 'the task made by this request was deleted' USING ERRCODE = 'FV004';
    END IF;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    -- 상태는 항목의 상태 정의 이름이다(D-116). 비우면 기본 상태.
    IF p_status IS NULL THEN
      v_status := app_task_status(app_default_task_status(p_item));
    ELSE
      v_status := app_task_status_by_name(p_item, p_status);
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
    SELECT * INTO v_item FROM items WHERE id = p_item;
    SELECT coalesce(array_agg(u ORDER BY u), '{}') INTO v_people
    FROM (SELECT DISTINCT u FROM unnest(coalesce(p_assignees, '{}')) AS u) s;
    -- 담당자 후보의 멤버 행을 id 순으로 먼저 공유 잠금한다(멤버 행 → 열 → 항목 행: 내보내기·역할 변경과 같은 순서).
    PERFORM 1 FROM workspace_members WHERE workspace_id = v_item.workspace_id AND user_id = ANY (v_people)
    ORDER BY user_id FOR SHARE;
    PERFORM app_lock_lane(v_status.id);
    PERFORM app_check_task_parent(NULL, p_parent, p_item);
    PERFORM app_lock_item(p_item);
    -- 잠금 뒤의 상태로 다시 본다: 권한·휴지통·보관·상태 정의(상태 지우기는 그 열을 잠근 뒤 지운다).
    IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 열 잠금 뒤 상태 정의를 다시 읽는다(기다리는 사이 이름·범주가 바뀌었거나 지워졌을 수 있다).
    SELECT * INTO v_status FROM task_statuses WHERE id = v_status.id;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    FOREACH v_uid IN ARRAY v_people LOOP
      PERFORM app_check_assignee(v_item.workspace_id, p_item, v_uid);
    END LOOP;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'task', v_id);
    INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, description, status_id, priority,
                       start_date, due_date, estimate_minutes, tags, milestone, completed_at, position, created_by,
                       updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, p_parent, v_number, btrim(p_title), coalesce(p_description, ''),
            v_status.id, p_priority, p_start, p_due, p_estimate, coalesce(p_tags, '{}'), coalesce(p_milestone, false),
            CASE WHEN v_status.category = 'done' THEN now() END, app_task_lane_end(v_status.id), v_user, v_user,
            p_request_id);
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'task', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    PERFORM app_activity(app_id(p_ids, 3), v_item.workspace_id, p_item, 'task', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    FOREACH v_uid IN ARRAY v_people LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_item.workspace_id, 'assign', 'task', v_id,
                        jsonb_build_object('user', v_uid));
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_item.workspace_id, p_item, 'task', v_id, 'assign',
                           jsonb_build_object('user', v_uid));
    END LOOP;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) RETURNS TABLE(view_id uuid, outcome text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_view saved_views%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_view FROM saved_views
    WHERE user_id = app_user_id() AND (request_id = p_request OR updated_request_id = p_request);
    IF v_view.id IS NOT NULL THEN
      IF v_view.request_id <> p_request OR v_view.item_id <> p_item OR v_view.name <> btrim(p_name) THEN
        RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
      END IF;
      RETURN QUERY SELECT v_view.id, 'created'::text;
      RETURN;
    END IF;
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO saved_views (id, user_id, item_id, name, config, request_id)
    VALUES (app_id(p_ids, 1), app_user_id(), p_item, btrim(p_name), p_config, p_request)
    ON CONFLICT (user_id, item_id, name) DO NOTHING
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      RETURN QUERY SELECT id, 'exists'::text FROM saved_views
      WHERE user_id = app_user_id() AND item_id = p_item AND name = btrim(p_name);
      RETURN;
    END IF;
    RETURN QUERY SELECT v_id, 'created'::text;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_checklist_item(p_id uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_access('task', v_task) = 'none' THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    PERFORM app_lock_item((SELECT item_id FROM tasks WHERE id = v_task));
    PERFORM app_require_task_write(v_task);
    DELETE FROM checklist_items WHERE id = p_id;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_checklist_item(p_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_event(p_event uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_delete_event(p_event uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_view(p_view uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    DELETE FROM saved_views WHERE id = p_view AND user_id = app_user_id();
    IF NOT FOUND THEN
      RAISE EXCEPTION 'view not found' USING ERRCODE = 'P0002';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_view(p_view uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row record;
    v_ws  uuid := coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item));
    v_at  integer := 0;
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
      -- 휴지통이라 지금 볼 수 없는 것이 아니라 정말 권한을 잃었을 때만 뺀다.
      IF app_grade_item(v_row.item_id, v_row.user_id, true) = 'none' THEN
        DELETE FROM task_assignees WHERE task_id = v_row.task_id AND user_id = v_row.user_id;
        IF FOUND THEN
          v_at := v_at + 1;
          PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unassign', 'task', v_row.task_id,
                            jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
          -- 공간 제거·역할 변경에 따른 담당 해제는 활동 피드에도 남긴다(D-114·D-119).
          v_at := v_at + 1;
          PERFORM app_activity(app_id(p_ids, v_at), v_row.workspace_id, v_row.item_id, 'task', v_row.task_id, 'unassign',
                               jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
        END IF;
      END IF;
    END LOOP;
    -- 리더도 그 항목을 볼 수 있는 사람이어야 한다(휴지통 항목도 같은 규칙이며 휴지통이라는 이유로는 빼지 않는다).
    FOR v_row IN
      SELECT i.id, i.workspace_id, i.leader_id FROM items i
      WHERE i.leader_id IS NOT NULL AND i.workspace_id = v_ws
        AND (p_item IS NULL OR i.id = p_item)
        AND (p_user IS NULL OR i.leader_id = p_user)
      FOR UPDATE
    LOOP
      IF app_grade_item(v_row.id, v_row.leader_id, true) = 'none' THEN
        UPDATE items SET leader_id = NULL WHERE id = v_row.id;
        v_at := v_at + 1;
        PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unset_leader', 'item', v_row.id,
                          jsonb_build_object('user', v_row.leader_id, 'reason', p_reason));
      END IF;
    END LOOP;
    RETURN v_at;
  END
  $$;


ALTER FUNCTION fvoci.app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_event_occurrences(p_workspace uuid, p_from timestamp with time zone, p_to timestamp with time zone, p_from_date date, p_to_date date) RETURNS TABLE(event_id uuid, occurrence date, start_date date, end_date date, starts_at timestamp with time zone, ends_at timestamp with time zone, skipped boolean)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH visible AS (
      SELECT e.*, coalesce(e.start_date, (e.starts_at AT TIME ZONE e.timezone)::date) AS anchor
      FROM events e
      WHERE e.workspace_id = p_workspace AND e.deleted_at IS NULL AND app_access('event', e.id) <> 'none'
    ),
    shaped AS (
      SELECT v.id, v.anchor AS occ, v.start_date AS sd, v.end_date AS ed, v.starts_at AS sa, v.ends_at AS ea,
             false AS skipped
      FROM visible v WHERE v.recurrence IS NULL
      UNION ALL
      SELECT v.id, o.occurrence,
             CASE WHEN v.all_day THEN v.start_date + (o.occurrence - v.anchor) END,
             CASE WHEN v.all_day THEN v.end_date + (o.occurrence - v.anchor) END,
             l.at,
             l.at + (v.ends_at - v.starts_at),
             NOT v.all_day AND l.at IS NULL
      FROM visible v
      CROSS JOIN LATERAL app_recurrence_dates(v.recurrence, v.anchor, p_from_date - 400, p_to_date + 2) o
      CROSS JOIN LATERAL (
        SELECT CASE WHEN NOT v.all_day
                    THEN app_local_instant(o.occurrence + (v.starts_at AT TIME ZONE v.timezone)::time, v.timezone)
               END AS at
      ) l
      WHERE v.recurrence IS NOT NULL AND NOT (o.occurrence = ANY (v.excluded_dates))
    )
    SELECT id, occ, sd, ed, sa, ea, skipped FROM shaped
    WHERE (sd IS NOT NULL AND sd <= p_to_date AND ed >= p_from_date)
       OR (sa IS NOT NULL AND sa < p_to AND (ea > p_from OR sa >= p_from))
       OR (skipped AND occ BETWEEN p_from_date AND p_to_date)
    ORDER BY coalesce(sa, sd::timestamp AT TIME ZONE 'UTC', occ::timestamp AT TIME ZONE 'UTC'), id
  $$;


ALTER FUNCTION fvoci.app_event_occurrences(p_workspace uuid, p_from timestamp with time zone, p_to timestamp with time zone, p_from_date date, p_to_date date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_guard_series_root() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF EXISTS (SELECT 1 FROM task_series s WHERE s.root_task_id = NEW.id AND s.rule IS NOT NULL) THEN
      IF NEW.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'a repeating task cannot become a subtask' USING ERRCODE = '22023';
      END IF;
      IF NEW.due_date IS NULL AND NEW.start_date IS NULL THEN
        RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
      END IF;
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_guard_series_root() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_dependencies(p_item uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT pg_advisory_xact_lock(hashtextextended('fvoci-deps:' || p_item::text, 0))
  $$;


ALTER FUNCTION fvoci.app_lock_dependencies(p_item uuid) OWNER TO fvoci_migrator;

-- 칸반 열 잠금. 열은 상태 정의 하나(status_id)이고 상태는 항목에 속하므로 항목 id는 따로 넣지 않는다.
CREATE FUNCTION fvoci.app_lock_lane(p_status uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT pg_advisory_xact_lock(hashtextextended('fvoci-lane:' || p_status::text, 0))
  $$;


ALTER FUNCTION fvoci.app_lock_lane(p_status uuid) OWNER TO fvoci_migrator;

-- 교착을 피하려고 항목·상태 순서로 열을 잠근 뒤 업무 행을 ID 순서로 잠근다. 호출자는 잠근 뒤 권한·관계를 다시 확인한다.
-- 업무 키는 바꾸지 않으므로 FOR NO KEY UPDATE로 외래 키 검사의 KEY SHARE와 불필요하게 충돌하지 않게 한다.
-- 업무 행을 잠그기 전에 관련 열(지금 열과 옮겨 갈 열)을 열 id 순서로 먼저 잠근다(다시 매기기·반복 생성과 같은 순서).
CREATE FUNCTION fvoci.app_lock_task_rows(p_ids uuid[], p_extra_status uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_lane  uuid;
    v_lanes uuid[] := '{}';
  BEGIN
    FOR v_lane IN
      SELECT DISTINCT l.status_id FROM (
        SELECT status_id FROM tasks WHERE id = ANY (p_ids)
        UNION
        SELECT p_extra_status WHERE p_extra_status IS NOT NULL
      ) l
      ORDER BY l.status_id
    LOOP
      PERFORM app_lock_lane(v_lane);
      v_lanes := array_append(v_lanes, v_lane);
    END LOOP;
    PERFORM 1 FROM tasks WHERE id = ANY (p_ids) ORDER BY id FOR NO KEY UPDATE;
    IF EXISTS (SELECT 1 FROM tasks WHERE id = ANY (p_ids) AND NOT (status_id = ANY (v_lanes))) THEN
      RAISE EXCEPTION 'a task moved while it was being changed; try again' USING ERRCODE = '40001';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_lock_task_rows(p_ids uuid[], p_extra_status uuid) OWNER TO fvoci_migrator;

-- 칸반 이동(D-79·D-89). 열은 항목의 상태 정의 이름이다(D-116). 활동 피드에 kanban_move를 남긴다(D-119).
CREATE FUNCTION fvoci.app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task     tasks%ROWTYPE;
    v_before   tasks%ROWTYPE;
    v_status   task_statuses%ROWTYPE;
    v_old      task_statuses%ROWTYPE;
    v_lower    numeric;
    v_position numeric;
  BEGIN
    PERFORM app_require_task_write(p_task);
    v_status := app_task_status_by_name((SELECT item_id FROM tasks WHERE id = p_task), p_status);
    -- 지금 열과 옮겨 갈 열을 먼저, 그다음 행(다시 매기기와의 교착 방지).
    PERFORM app_lock_task_rows(ARRAY[p_task], v_status.id);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- D-89: 읽은 뒤 그사이 이 업무의 열·위치가 바뀌었으면(이동 판이 다르면) 아무것도 바꾸지 않는다. 행을 잠근 뒤라
    -- 비교와 옮기기 사이에 다른 이동이 끼지 못한다.
    IF p_base_move_version IS NOT NULL AND v_task.move_version IS DISTINCT FROM p_base_move_version THEN
      RAISE EXCEPTION 'the task moved since it was read' USING ERRCODE = 'FV003';
    END IF;
    -- 열 잠금 뒤 상태 정의를 다시 읽는다(기다리는 사이 이름·범주가 바뀌었거나 지워졌을 수 있다).
    SELECT * INTO v_status FROM task_statuses WHERE id = v_status.id;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    v_old := app_task_status(v_task.status_id);
    IF p_before IS NOT NULL THEN
      SELECT * INTO v_before FROM tasks WHERE id = p_before;
      IF v_before.id IS NULL OR v_before.id = p_task OR v_before.item_id <> v_task.item_id
         OR v_before.status_id <> v_status.id OR app_access('task', p_before) = 'none' THEN
        RAISE EXCEPTION 'before must be a task in the same column' USING ERRCODE = '22023';
      END IF;
      SELECT max(position) INTO v_lower FROM tasks
      WHERE status_id = v_status.id AND id <> p_task AND position < v_before.position;
      v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
      IF scale(v_position) > 40 THEN
        -- 같은 틈에 너무 많이 끼워 자릿수가 길어졌다. 열을 다시 매긴 뒤 계산한다.
        PERFORM app_renumber_lane(v_status.id);
        SELECT position INTO v_before.position FROM tasks WHERE id = p_before;
        SELECT max(position) INTO v_lower FROM tasks
        WHERE status_id = v_status.id AND id <> p_task AND position < v_before.position;
        v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
      END IF;
    ELSE
      SELECT coalesce(max(position), 0) + 1 INTO v_position FROM tasks
      WHERE status_id = v_status.id AND id <> p_task;
    END IF;
    UPDATE tasks SET
      status_id = v_status.id, position = v_position,
      version = CASE WHEN v_status.id <> v_task.status_id THEN version + 1 ELSE version END,
      completed_at = CASE WHEN v_status.category = 'done' AND v_old.category <> 'done' THEN now()
                          WHEN v_status.category <> 'done' THEN NULL ELSE completed_at END,
      updated_by = CASE WHEN v_status.id <> v_task.status_id THEN app_user_id() ELSE updated_by END,
      updated_at = CASE WHEN v_status.id <> v_task.status_id THEN now() ELSE updated_at END
    WHERE id = p_task;
    PERFORM app_activity(app_id(p_ids, 1), v_task.workspace_id, v_task.item_id, 'task', p_task, 'kanban_move',
                         jsonb_build_object('from', v_old.name, 'to', v_status.name, 'before', p_before));
  END
  $$;


ALTER FUNCTION fvoci.app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date) RETURNS TABLE(series_id uuid, root_task_id uuid, item_id uuid, title text, milestone boolean, occurrence date, start_date date, due_date date)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT s.id, r.id, r.item_id, r.title, r.milestone, o.occurrence,
           r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)
    FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    CROSS JOIN LATERAL app_recurrence_dates(s.rule, s.anchor, greatest(s.effective_from, p_from - 400), p_to) o
    WHERE s.rule IS NOT NULL
      AND (p_workspace IS NULL OR s.workspace_id = p_workspace)
      AND (p_item IS NULL OR s.item_id = p_item)
      AND app_access('task', r.id) <> 'none'
      AND NOT app_item_archived(r.item_id)
      AND coalesce(r.due_date + (o.occurrence - s.anchor), r.start_date + (o.occurrence - s.anchor)) >= p_from
      AND coalesce(r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)) <= p_to
      AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = s.id AND t.occurrence_date = o.occurrence)
      AND NOT (o.occurrence = ANY (s.skipped_dates))
  $$;


ALTER FUNCTION fvoci.app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date) RETURNS TABLE(occurrence date)
    LANGUAGE plpgsql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_freq     text := p_rule ->> 'freq';
    v_step     integer := (p_rule ->> 'interval')::integer;
    v_until    date := (p_rule ->> 'until')::date;
    v_count    integer := (p_rule ->> 'count')::integer;
    v_days     integer[];
    v_last     date := least(p_to, coalesce(v_until, p_to));
    v_monday   date := p_anchor - (extract(isodow FROM p_anchor)::integer - 1);
    v_k0       integer := 0;
    v_k1       integer;
  BEGIN
    IF v_last < p_anchor OR p_to < p_from THEN
      RETURN;
    END IF;
    SELECT coalesce(array_agg(w::integer), '{}') INTO v_days FROM jsonb_array_elements(p_rule -> 'weekdays') w;
    IF cardinality(v_days) = 0 THEN
      v_days := ARRAY[extract(isodow FROM p_anchor)::integer];
    END IF;
    -- 회차 번호 k의 범위. 횟수가 없으면 p_from 바로 앞에서 시작한다(앞의 회차는 셀 필요가 없다).
    IF v_freq = 'daily' THEN
      IF v_count IS NULL THEN v_k0 := greatest(0, (p_from - p_anchor) / v_step); END IF;
      v_k1 := (v_last - p_anchor) / v_step;
      IF v_count IS NOT NULL THEN v_k1 := least(v_k1, v_count); END IF;
      RETURN QUERY
        WITH c AS (SELECT p_anchor + k * v_step AS d FROM generate_series(v_k0, v_k1) k),
             n AS (SELECT d, row_number() OVER (ORDER BY d) AS i FROM c)
        SELECT d FROM n WHERE (v_count IS NULL OR i <= v_count) AND d BETWEEN greatest(p_from, p_anchor) AND v_last
        ORDER BY d;
      RETURN;
    END IF;
    IF v_freq = 'weekly' THEN
      IF v_count IS NULL THEN v_k0 := greatest(0, (p_from - v_monday) / (7 * v_step)); END IF;
      v_k1 := (v_last - v_monday) / (7 * v_step);
      IF v_count IS NOT NULL THEN v_k1 := least(v_k1, v_count); END IF;
      RETURN QUERY
        WITH c AS (
          SELECT p_anchor AS d
          UNION
          SELECT v_monday + 7 * v_step * k + (wd - 1)
          FROM generate_series(v_k0, v_k1) k, unnest(v_days) wd
        ),
        n AS (SELECT d, row_number() OVER (ORDER BY d) AS i FROM c WHERE d >= p_anchor)
        SELECT d FROM n
        WHERE (v_count IS NULL OR i <= v_count) AND d BETWEEN greatest(p_from, p_anchor) AND v_last
        ORDER BY d;
      RETURN;
    END IF;
    -- 매달·매년: 없는 날(31일·2월 29일)은 건너뛴다. 횟수가 있으면 건너뛸 수 있는 몫까지 넉넉히(×8) 센다.
    DECLARE
      v_unit   integer := CASE v_freq WHEN 'monthly' THEN 1 ELSE 12 END;
      v_months integer := ((extract(year FROM v_last) - extract(year FROM p_anchor)) * 12
                           + extract(month FROM v_last) - extract(month FROM p_anchor))::integer;
      v_from_m integer := ((extract(year FROM p_from) - extract(year FROM p_anchor)) * 12
                           + extract(month FROM p_from) - extract(month FROM p_anchor))::integer;
      v_start  date := date_trunc('month', p_anchor)::date;
      v_day    integer := extract(day FROM p_anchor)::integer;
    BEGIN
      IF v_count IS NULL THEN v_k0 := greatest(0, v_from_m / (v_step * v_unit)); END IF;
      v_k1 := v_months / (v_step * v_unit);
      IF v_count IS NOT NULL THEN v_k1 := least(v_k1, v_count * 8); END IF;
      RETURN QUERY
        WITH m AS (
          SELECT (v_start + make_interval(months => k * v_step * v_unit))::date AS first_day
          FROM generate_series(v_k0, v_k1) k
        ),
        c AS (
          SELECT first_day + (v_day - 1) AS d, first_day FROM m
        ),
        n AS (
          SELECT d, row_number() OVER (ORDER BY d) AS i FROM c
          WHERE date_trunc('month', d)::date = first_day
        )
        SELECT d FROM n
        WHERE (v_count IS NULL OR i <= v_count) AND d BETWEEN greatest(p_from, p_anchor) AND v_last
        ORDER BY d;
    END;
  END
  $$;


ALTER FUNCTION fvoci.app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_renumber_lane(p_status uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE tasks t SET position = r.pos
    FROM (SELECT id, row_number() OVER (ORDER BY position, number) AS pos
          FROM tasks WHERE status_id = p_status) r
    WHERE t.id = r.id AND t.status_id = p_status
  $$;


ALTER FUNCTION fvoci.app_renumber_lane(p_status uuid) OWNER TO fvoci_migrator;

-- 현재 규칙이 요청과 이미 같으면 재시도 성공으로 끝낸다. 다른 경우에만 잠근 행의 규칙과 읽은 기준을 비교한다.
-- 기준이 지난 요청은 규칙·판·감사를 덮지 않는다. D-94·D-125: docs/decisions/tasks.md#d-94.
CREATE FUNCTION fvoci.app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    IF jsonb_strip_nulls(v_event.recurrence) IS DISTINCT FROM jsonb_strip_nulls(p_rule)
       AND jsonb_strip_nulls(v_event.recurrence) IS DISTINCT FROM jsonb_strip_nulls(p_base) THEN
      RETURN 'conflict';
    END IF;
    IF jsonb_strip_nulls(v_event.recurrence) IS NOT DISTINCT FROM jsonb_strip_nulls(p_rule) THEN
      RETURN 'updated';
    END IF;
    IF (p_rule ->> 'until')::date < coalesce(v_event.start_date, (v_event.starts_at AT TIME ZONE v_event.timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    UPDATE events SET recurrence = p_rule, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'set_recurrence', 'event', p_event,
                      jsonb_build_object('from', v_event.recurrence, 'to', p_rule));
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer, p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[]) RETURNS TABLE(outcome text, old_kind text, old_lag integer)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    IF app_access('task', p_predecessor) = 'none' THEN
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
    -- 화면이 편집을 시작한 관계(새 관계면 없음)와 잠근 뒤의 관계가 다르면 아무것도 바꾸지 않는다. 지금 관계가
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

    -- 승인한 재배치: 관계를 고친 후속 업무 자신이거나 그 뒤 업무여야 하고, 기준 버전이 모두 맞아야 한다. 같은
    -- 항목인데 지금은 이어지지 않는 업무(미리본 뒤 관계가 지워짐)는 오래된 계획이라 `shift_conflict`(아무것도 바꾸지
    -- 않음), 처음부터 옮길 수 없는 업무(없음·다른 항목·관계의 선행 업무)는 400 그대로다(지금 선행 쪽인 업무도 오래된 계획).
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_state := CASE WHEN v_other.id = p_predecessor THEN 'invalid'
                      ELSE app_shift_target(p_successor, v_other.id, true) END;
      IF v_state = 'invalid' THEN
        RAISE EXCEPTION 'a shifted task must be the successor or come after it' USING ERRCODE = '22023';
      END IF;
      -- 오래된 항목은 날짜 모양을 보기 전에 거른다.
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


ALTER FUNCTION fvoci.app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer, p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_shift_excluded_dates() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old date := coalesce(OLD.start_date, (OLD.starts_at AT TIME ZONE OLD.timezone)::date);
    v_new date := coalesce(NEW.start_date, (NEW.starts_at AT TIME ZONE NEW.timezone)::date);
  BEGIN
    IF cardinality(NEW.excluded_dates) > 0 AND v_old IS DISTINCT FROM v_new AND NEW.recurrence IS NOT NULL
       AND (NEW.recurrence ->> 'freq' = 'daily'
            OR (NEW.recurrence ->> 'freq' = 'weekly' AND coalesce(jsonb_array_length(NEW.recurrence -> 'weekdays'), 0) = 0)) THEN
      NEW.excluded_dates := ARRAY(SELECT d + (v_new - v_old) FROM unnest(NEW.excluded_dates) d);
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_shift_excluded_dates() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_shift_target(p_from uuid, p_task uuid, p_include_from boolean) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_shift_target(p_from uuid, p_task uuid, p_include_from boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_item_series(p_item uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
    v_id     uuid;
    v_made   integer := 0;
    v_at     integer := 0;
    v_need   integer;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 모든 계열의 루트 담당 멤버를 열보다 먼저 잠근다(멤버 제거·강등과 직렬화).
    PERFORM 1 FROM workspace_members m
    WHERE EXISTS (
      SELECT 1 FROM task_series s JOIN tasks r ON r.id = s.root_task_id
      JOIN task_assignees a ON a.task_id = r.id
      WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL
        AND m.workspace_id = r.workspace_id AND m.user_id = a.user_id
    )
    ORDER BY m.user_id, m.workspace_id FOR SHARE;
    -- 항목의 모든 열을 사용자 요청과 같은 순서(열 id 순)로 먼저 잠근다.
    FOR v_id IN SELECT id FROM task_statuses WHERE item_id = p_item ORDER BY id LOOP
      PERFORM app_lock_lane(v_id);
    END LOOP;
    FOR v_id IN
      SELECT s.id FROM task_series s JOIN tasks r ON r.id = s.root_task_id
      WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      v_need := app_task_series_bound(v_id);
      v_made := v_made + app_sync_task_series_as(v_id, false, coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
      v_at := v_at + v_need;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_sync_item_series(p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_item_series_ids(p_item uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('item', p_item) = 'none' THEN 0 ELSE
      coalesce((SELECT sum(app_task_series_bound(s.id)) FROM task_series s JOIN tasks r ON r.id = s.root_task_id
                WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL), 0)::integer
    END
  $$;


ALTER FUNCTION fvoci.app_sync_item_series_ids(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_task_series(p_series uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_root uuid;
  BEGIN
    SELECT root_task_id INTO v_root FROM task_series WHERE id = p_series;
    IF v_root IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_root);
    PERFORM 1 FROM workspace_members m
    WHERE EXISTS (
      SELECT 1 FROM tasks r JOIN task_assignees a ON a.task_id = r.id
      WHERE r.id = v_root AND m.workspace_id = r.workspace_id AND m.user_id = a.user_id
    )
    ORDER BY m.user_id, m.workspace_id FOR SHARE;
    RETURN app_sync_task_series_as(p_series, false, p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_sync_task_series(p_series uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_task_series_as(p_series uuid, p_system boolean, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    v_todo   uuid;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    -- 새 회차는 항목의 기본 상태(todo 범주 첫 상태, D-116) 열에 들어간다.
    v_todo := app_default_task_status((SELECT item_id FROM tasks WHERE id = v_series.root_task_id));
    PERFORM app_lock_task_rows(ARRAY[v_series.root_task_id], v_todo);
    SELECT * INTO v_root FROM tasks WHERE id = v_series.root_task_id;
    PERFORM app_lock_item(v_root.item_id);
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM task_statuses WHERE id = v_todo) THEN
      RAISE EXCEPTION 'the default status changed while syncing; try again' USING ERRCODE = '40001';
    END IF;
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    -- 작업 프로세스는 만든 사람 이름으로 회차를 만들고 감사 기록에는 system으로 남긴다.
    v_user := coalesce(v_user, v_series.created_by);
    -- 잠근 뒤의 상태로 다시 본다: 규칙·날짜·휴지통(루트 업무·항목·모듈·워크스페이스)·보관.
    IF v_series.rule IS NULL OR (v_root.due_date IS NULL AND v_root.start_date IS NULL)
       OR v_root.deleted_at IS NOT NULL OR app_item_archived(v_root.item_id)
       OR EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id JOIN workspaces w ON w.id = i.workspace_id
                  WHERE i.id = v_root.item_id
                    AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL OR w.deleted_at IS NOT NULL)) THEN
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
      INSERT INTO tasks (id, workspace_id, item_id, number, title, description, status_id, priority, start_date,
                         due_date, estimate_minutes, tags, milestone, position, completed_at, created_by, updated_by,
                         request_id, series_id, occurrence_date)
      VALUES (v_id, v_root.workspace_id, v_root.item_id,
              app_allocate_number(v_root.workspace_id, v_module, 'task', v_id),
              v_root.title, v_root.description, v_todo, v_root.priority,
              v_root.start_date + (v_date - v_series.anchor), v_root.due_date + (v_date - v_series.anchor),
              v_root.estimate_minutes, v_root.tags, v_root.milestone, app_task_lane_end(v_todo),
              CASE WHEN (SELECT category FROM task_statuses WHERE id = v_todo) = 'done' THEN now() END,
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
        -- 선행 잠금 뒤 바뀐 담당은 열을 쥔 채 기다리지 않는다. 55P03이면 전체 동기화를 다시 한다.
        PERFORM 1 FROM workspace_members m
          WHERE m.workspace_id = v_root.workspace_id AND m.user_id = v_uid FOR SHARE NOWAIT;
        IF FOUND AND app_access_as('item', v_root.item_id, v_uid) <> 'none' THEN
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
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_root.workspace_id, v_root.item_id, 'task', v_id, 'create',
                           jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'occurrence', v_date));
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_sync_task_series_as(p_series uuid, p_system boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_task_series_ids(p_series uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('task', s.root_task_id) = 'none' THEN 0 ELSE app_task_series_bound(s.id) END
    FROM task_series s WHERE s.id = p_series
  $$;


ALTER FUNCTION fvoci.app_sync_task_series_ids(p_series uuid) OWNER TO fvoci_migrator;

-- 반복 회차 만들기(D-31, #95). 작업 프로세스는 계열마다 트랜잭션 하나로 app_system_sync_task_series를 부른다. 계열 하나의
-- 실패가 다른 계열의 커밋을 막지 않고, 처리한 계열의 잠금을 작업이 끝날 때까지 쥐지 않는다. 사용자 요청의 동기화
-- (app_sync_item_series)는 그대로 둔다.
CREATE FUNCTION fvoci.app_system_recurrence_due() RETURNS SETOF uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT s.id FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    JOIN items i ON i.id = s.item_id
    JOIN modules m ON m.id = i.module_id
    JOIN workspaces w ON w.id = s.workspace_id
    WHERE app_user_id() IS NULL AND s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL
      AND m.deleted_at IS NULL AND w.deleted_at IS NULL
    ORDER BY s.item_id, s.id
  $$;


ALTER FUNCTION fvoci.app_system_recurrence_due() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_sync_task_series(p_series uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    PERFORM 1 FROM workspace_members m
    WHERE EXISTS (
      SELECT 1 FROM task_series s JOIN tasks r ON r.id = s.root_task_id
      JOIN task_assignees a ON a.task_id = r.id
      WHERE s.id = p_series AND m.workspace_id = r.workspace_id AND m.user_id = a.user_id
    )
    ORDER BY m.user_id, m.workspace_id FOR SHARE;
    RETURN app_sync_task_series_as(p_series, true, p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_system_sync_task_series(p_series uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_sync_task_series_ids(p_series uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE app_task_series_bound(p_series) END
  $$;


ALTER FUNCTION fvoci.app_system_sync_task_series_ids(p_series uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_checklist_counts(p_task uuid) RETURNS TABLE(done bigint, total bigint)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT count(*) FILTER (WHERE checked), count(*) FROM checklist_items
    WHERE task_id = p_task AND app_access('task', p_task) <> 'none'
  $$;


ALTER FUNCTION fvoci.app_task_checklist_counts(p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_lane_end(p_status uuid) RETURNS numeric
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce(max(position), 0) + 1 FROM tasks WHERE status_id = p_status
  $$;


ALTER FUNCTION fvoci.app_task_lane_end(p_status uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_reference(p_task uuid) RETURNS text
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT k.key || '-' || n.number
    FROM numbered_targets n
    JOIN module_keys k ON k.module_id = n.scope_id AND k.retired_at IS NULL
    WHERE n.target_kind = 'task' AND n.target_id = p_task
  $$;


ALTER FUNCTION fvoci.app_task_reference(p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_series_bound(p_series uuid) RETURNS integer
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    RETURN v_missing * (4 + 2 * v_lines);
  END
  $$;


ALTER FUNCTION fvoci.app_task_series_bound(p_series uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_tasks_move_version() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF NEW.status_id IS DISTINCT FROM OLD.status_id OR NEW.position IS DISTINCT FROM OLD.position THEN
      NEW.move_version := OLD.move_version + 1;
    ELSE
      NEW.move_version := OLD.move_version;
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_tasks_move_version() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT (SELECT count(*) FROM task_assignees a JOIN tasks t ON t.id = a.task_id
            WHERE (p_item IS NULL OR t.item_id = p_item)
              AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
              AND (p_user IS NULL OR a.user_id = p_user))::integer
         + (SELECT count(*) FROM items i
            WHERE i.leader_id IS NOT NULL
              AND i.workspace_id = coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item))
              AND (p_item IS NULL OR i.id = p_item)
              AND (p_user IS NULL OR i.leader_id = p_user))::integer
  $$;


ALTER FUNCTION fvoci.app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_access('task', v_task) = 'none' THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    PERFORM app_lock_item((SELECT item_id FROM tasks WHERE id = v_task));
    PERFORM app_require_task_write(v_task);
    UPDATE checklist_items SET text = coalesce(btrim(p_text), text), checked = coalesce(p_checked, checked),
      updated_at = now()
    WHERE id = p_id;
  END
  $$;


ALTER FUNCTION fvoci.app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    -- 규칙 설정(app_set_event_recurrence)과 같은 조건: 첫 회차가 반복의 끝보다 뒤면 회차가 하나도 없다.
    IF (v_event.recurrence ->> 'until')::date < coalesce(p_start_date, (p_starts_at AT TIME ZONE p_timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
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


ALTER FUNCTION fvoci.app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    v_status task_statuses%ROWTYPE;
    v_old    task_statuses%ROWTYPE;
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
    -- 상태는 항목의 상태 정의 이름이다(D-116).
    v_status := app_task_status_by_name((SELECT item_id FROM tasks WHERE id = p_task), p_status);
    -- 담당자 후보의 멤버 행을 id 순으로 먼저 공유 잠금한다(멤버 행 → 열 → 항목 행: 내보내기·역할 변경과 같은 순서).
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = (SELECT workspace_id FROM tasks WHERE id = p_task) AND user_id = ANY (coalesce(p_assignees, '{}'))
    ORDER BY user_id FOR SHARE;
    -- 이 업무와 옮길 후속 업무(같은 항목만, 다른 항목 id는 잠그지 않고 아래에서 거부)를 열 → 행 순으로 잠근다.
    SELECT array_agg(id) INTO v_lock FROM tasks
    WHERE id = ANY (coalesce(v_ids, '{}')) AND item_id = (SELECT item_id FROM tasks WHERE id = p_task);
    PERFORM app_lock_task_rows(array_prepend(p_task, coalesce(v_lock, '{}')), v_status.id);
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
    -- 열 잠금 뒤 상태 정의를 다시 읽는다(기다리는 사이 이름·범주가 바뀌었거나 지워졌을 수 있다).
    SELECT * INTO v_status FROM task_statuses WHERE id = v_status.id;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    v_old := app_task_status(v_task.status_id);
    IF p_priority NOT IN ('none', 'low', 'medium', 'high', 'urgent') THEN
      RAISE EXCEPTION 'invalid priority' USING ERRCODE = '22023';
    END IF;
    IF p_start IS NOT NULL AND p_due IS NOT NULL AND p_start > p_due THEN
      RAISE EXCEPTION 'the start date must not be after the due date' USING ERRCODE = '22023';
    END IF;
    IF p_milestone AND p_start IS NOT NULL THEN
      RAISE EXCEPTION 'a milestone has only a due date' USING ERRCODE = '22023';
    END IF;

    -- 승인한 후속 업무: 이 업무에서 이어지는 후속이어야 하고, 기준 버전이 모두 맞아야 한다. 같은 항목인데 지금은
    -- 이어지지 않는 업무(미리본 뒤 관계가 지워짐)는 오래된 계획이라 `shift_conflict`(아무것도 바꾸지 않음), 처음부터 옮길
    -- 수 없는 업무(없음·다른 항목)는 400 그대로다(지금 선행 쪽인 업무도 오래된 계획).
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_state := app_shift_target(p_task, v_other.id, false);
      IF v_state = 'invalid' THEN
        RAISE EXCEPTION 'a shifted task must be a successor of the edited task' USING ERRCODE = '22023';
      END IF;
      -- 오래된 항목은 날짜 모양을 보기 전에 거른다(미리본 뒤 시작일을 지운 후속도 400이 아니라 409).
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
      title = btrim(p_title), description = coalesce(p_description, ''), status_id = v_status.id,
      priority = p_priority, start_date = p_start, due_date = p_due, estimate_minutes = p_estimate,
      tags = coalesce(p_tags, '{}'), milestone = p_milestone, version = version + 1, updated_by = v_user,
      updated_at = now(),
      completed_at = CASE WHEN v_status.category = 'done' AND v_old.category <> 'done' THEN now()
                          WHEN v_status.category <> 'done' THEN NULL ELSE completed_at END,
      position = CASE WHEN v_status.id <> v_task.status_id THEN app_task_lane_end(v_status.id)
                      ELSE position END
    WHERE id = p_task;

    IF v_task.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_task.workspace_id, 'rename', 'task', p_task,
                        jsonb_build_object('from', v_task.title, 'to', btrim(p_title)));
    END IF;
    IF v_status.id <> v_task.status_id THEN
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_task.workspace_id, v_task.item_id, 'task', p_task, 'status',
                           jsonb_build_object('from', v_old.name, 'to', v_status.name,
                                              'from_category', v_old.category, 'to_category', v_status.category));
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
        v_at := v_at + 1;
        PERFORM app_activity(app_id(p_ids, v_at), v_task.workspace_id, v_task.item_id, 'task', p_task, 'unassign',
                             jsonb_build_object('user', v_uid));
      END IF;
    END LOOP;
    FOREACH v_uid IN ARRAY v_new LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (p_task, v_uid, v_user);
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'assign', 'task', p_task,
                        jsonb_build_object('user', v_uid));
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_task.workspace_id, v_task.item_id, 'task', p_task, 'assign',
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


ALTER FUNCTION fvoci.app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 이름 바꾸기 1 + 상태 변경 활동 1 + 담당 해제·지정마다 감사와 활동 2 + 밀린 후속마다 1.
    SELECT CASE WHEN app_access('task', p_task) = 'none' THEN 0 ELSE
      2 + 2 * (SELECT count(*) FROM task_assignees WHERE task_id = p_task)::integer
        + 2 * coalesce(cardinality(p_assignees), 0)
        + CASE WHEN jsonb_typeof(p_shifts) = 'array' THEN jsonb_array_length(p_shifts) ELSE 0 END
    END
  $$;


ALTER FUNCTION fvoci.app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) OWNER TO fvoci_migrator;

-- D-103: 마지막 수정 요청 ID만 재시도 성공으로 인정한다. 이후 저장이 있으면 옛 기준 판으로 되돌릴 수 없다.
-- 판 확인과 갱신은 같은 행 잠금 안에서 한다. D-89: docs/decisions/tasks.md#d-89.
CREATE FUNCTION fvoci.app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_view saved_views%ROWTYPE;
  BEGIN
    SELECT * INTO v_view FROM saved_views WHERE id = p_view AND user_id = app_user_id() FOR UPDATE;
    IF v_view.id IS NULL OR app_access('item', v_view.item_id) = 'none' THEN
      RAISE EXCEPTION 'view not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_view.updated_request_id = p_request THEN
      RETURN 'replayed';
    END IF;
    IF EXISTS (SELECT 1 FROM saved_views
               WHERE user_id = app_user_id() AND (request_id = p_request OR updated_request_id = p_request)) THEN
      RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
    END IF;
    IF v_view.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    UPDATE saved_views SET config = p_config, version = version + 1, updated_request_id = p_request
    WHERE id = p_view;
    RETURN 'updated';
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb) OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_request UNIQUE (task_id, request_id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_name UNIQUE (user_id, item_id, name);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_request UNIQUE (user_id, request_id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_updated_request UNIQUE (user_id, updated_request_id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_pkey PRIMARY KEY (task_id, user_id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_pkey PRIMARY KEY (predecessor_id, successor_id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_root_task_id_key UNIQUE (root_task_id);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_item UNIQUE (id, item_id);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_item_name UNIQUE (item_id, name);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_occurrence UNIQUE (series_id, occurrence_date);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_request UNIQUE (item_id, created_by, request_id);

CREATE INDEX checklist_items_task ON fvoci.checklist_items USING btree (task_id, "position");

CREATE INDEX events_item ON fvoci.events USING btree (item_id) WHERE (deleted_at IS NULL);

CREATE INDEX events_trash ON fvoci.events USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX events_workspace_dates ON fvoci.events USING btree (workspace_id, start_date, end_date) WHERE (deleted_at IS NULL);

CREATE INDEX events_workspace_times ON fvoci.events USING btree (workspace_id, starts_at, ends_at) WHERE (deleted_at IS NULL);

CREATE INDEX saved_views_item ON fvoci.saved_views USING btree (user_id, item_id);

CREATE INDEX task_assignees_user ON fvoci.task_assignees USING btree (user_id);

CREATE INDEX task_dependencies_item ON fvoci.task_dependencies USING btree (item_id);

CREATE INDEX task_dependencies_successor ON fvoci.task_dependencies USING btree (successor_id);

CREATE INDEX task_series_item ON fvoci.task_series USING btree (item_id);

CREATE INDEX task_statuses_item_position ON fvoci.task_statuses USING btree (item_id, "position");

CREATE INDEX tasks_due ON fvoci.tasks USING btree (due_date) WHERE (due_date IS NOT NULL);

CREATE INDEX tasks_item_updated ON fvoci.tasks USING btree (item_id, updated_at DESC);

CREATE INDEX tasks_lane ON fvoci.tasks USING btree (status_id, "position");

CREATE INDEX tasks_parent ON fvoci.tasks USING btree (parent_id);

CREATE INDEX tasks_trash ON fvoci.tasks USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX tasks_updated_by ON fvoci.tasks USING btree (updated_by, updated_at DESC);

CREATE INDEX tasks_workspace_updated ON fvoci.tasks USING btree (workspace_id, updated_at DESC);

CREATE TRIGGER events_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, item_id, start_date, starts_at ON fvoci.events FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('event');

CREATE TRIGGER events_shift_excluded BEFORE UPDATE OF start_date, starts_at, timezone ON fvoci.events FOR EACH ROW EXECUTE FUNCTION fvoci.app_shift_excluded_dates();

CREATE TRIGGER task_assignees_search AFTER INSERT OR DELETE ON fvoci.task_assignees FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue_assignee();

CREATE TRIGGER tasks_move_version BEFORE UPDATE ON fvoci.tasks FOR EACH ROW EXECUTE FUNCTION fvoci.app_tasks_move_version();

CREATE TRIGGER tasks_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, status_id, item_id, due_date, start_date ON fvoci.tasks FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('task');

CREATE TRIGGER task_statuses_search AFTER UPDATE OF name, category ON fvoci.task_statuses FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue_status_tasks();

CREATE TRIGGER tasks_series_root_guard BEFORE UPDATE OF parent_id, start_date, due_date ON fvoci.tasks FOR EACH ROW EXECUTE FUNCTION fvoci.app_guard_series_root();

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_task_id_fkey FOREIGN KEY (task_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_deleted_by_fkey FOREIGN KEY (deleted_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_assigned_by_fkey FOREIGN KEY (assigned_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_task_id_fkey FOREIGN KEY (task_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_predecessor_id_fkey FOREIGN KEY (predecessor_id) REFERENCES fvoci.tasks(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_successor_id_fkey FOREIGN KEY (successor_id) REFERENCES fvoci.tasks(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_root_task_id_fkey FOREIGN KEY (root_task_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;
ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_series_id_fkey FOREIGN KEY (series_id) REFERENCES fvoci.task_series(id);
ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_status_id_fkey FOREIGN KEY (status_id) REFERENCES fvoci.task_statuses(id);
ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_status_item FOREIGN KEY (status_id, item_id) REFERENCES fvoci.task_statuses(id, item_id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

-- 저장된 YYYY-MM-DD가 잘못됐으면 그 날짜만 건너뛴다(#218).
-- 문자열의 ::date 변환은 DateStyle에 의존하므로 숫자 인자를 받는 make_date를 쓴다.
CREATE FUNCTION fvoci.app_iso_date(p_value text) RETURNS date
    LANGUAGE plpgsql IMMUTABLE STRICT PARALLEL SAFE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
BEGIN
    IF p_value !~ '^[0-9]{4}-[0-9]{2}-[0-9]{2}$' THEN
        RETURN NULL;
    END IF;
    RETURN make_date(substring(p_value FROM 1 FOR 4)::integer,
                     substring(p_value FROM 6 FOR 2)::integer,
                     substring(p_value FROM 9 FOR 2)::integer);
EXCEPTION WHEN datetime_field_overflow THEN
    RETURN NULL;
END;
$$;

ALTER FUNCTION fvoci.app_iso_date(p_value text) OWNER TO fvoci_migrator;

-- 달력 집계(D-80). SECURITY INVOKER로 앱 역할의 RLS를 그대로 적용한다.
-- 학습 표는 다음 기준 파일에서 만들어지므로 PL/pgSQL의 실행 시 이름 해석을 쓴다.
CREATE FUNCTION fvoci.app_calendar(p_workspace uuid, p_from date, p_to date, p_tz text, p_module uuid DEFAULT NULL, p_item uuid DEFAULT NULL) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
    SET timezone TO 'UTC'
    AS $$
DECLARE
    v_from_at timestamptz;
    v_to_at timestamptz;
BEGIN
    IF p_from IS NULL OR p_to IS NULL OR p_to < p_from OR p_to - p_from >= 62
       OR p_from < DATE '1900-01-01' OR p_to > DATE '2200-12-31' THEN
        RAISE EXCEPTION 'the range must be 1 to 62 days between 1900 and 2200' USING ERRCODE = '22023';
    END IF;
    IF app_access('workspace', p_workspace) = 'none' THEN
        RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_check_timezone(p_tz);
    IF p_item IS NOT NULL THEN
        IF NOT EXISTS (SELECT 1 FROM items i WHERE i.id = p_item AND i.workspace_id = p_workspace
                       AND (p_module IS NULL OR i.module_id = p_module)
                       AND app_access('item', i.id) <> 'none') THEN
            RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
        END IF;
    ELSIF p_module IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM modules m WHERE m.id = p_module AND m.workspace_id = p_workspace
    ) THEN
        RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    -- 날짜 경계는 처음 오는 0시이며, 0시가 없으면 건너뛴 직후다(D-50·D-81).
    v_from_at := coalesce(app_local_instant(p_from::timestamp, p_tz), p_from::timestamp AT TIME ZONE p_tz);
    v_to_at := coalesce(app_local_instant((p_to + 1)::timestamp, p_tz), (p_to + 1)::timestamp AT TIME ZONE p_tz);
    RETURN (
        WITH scope AS MATERIALIZED (
            SELECT i.id FROM items i
            WHERE i.workspace_id = p_workspace AND app_access('item', i.id) <> 'none'
              AND (p_module IS NULL OR i.module_id = p_module) AND (p_item IS NULL OR i.id = p_item)
        ),
        task_rows AS MATERIALIZED (
            SELECT t.id, t.item_id, app_task_reference(t.id) AS reference, t.title,
                   s.name AS status, s.category AS status_category, t.milestone, t.start_date, t.due_date,
                   coalesce(t.start_date, t.due_date) AS sort_date, t.number
            FROM tasks t JOIN task_statuses s ON s.id = t.status_id JOIN scope ON scope.id = t.item_id
            WHERE t.workspace_id = p_workspace
              AND coalesce(t.start_date, t.due_date) <= p_to AND coalesce(t.due_date, t.start_date) >= p_from
        ),
        projected_rows AS MATERIALIZED (
            SELECT o.* FROM app_projected_occurrences(p_workspace, NULL, p_from, p_to) o
            JOIN scope ON scope.id = o.item_id
        ),
        occurrence_rows AS MATERIALIZED (
            SELECT o.* FROM app_event_occurrences(p_workspace, v_from_at, v_to_at, p_from, p_to) o
            JOIN events e ON e.id = o.event_id AND e.deleted_at IS NULL
            JOIN scope ON scope.id = e.item_id
        ),
        event_rows AS MATERIALIZED (
            SELECT e.id, e.item_id, e.workspace_id, e.kind, e.title, e.description, e.all_day,
                   e.start_date, e.end_date, e.starts_at, e.ends_at, e.timezone, e.recurrence,
                   e.excluded_dates, e.version, app_access('event', e.id) AS access
            FROM events e JOIN scope ON scope.id = e.item_id
            WHERE e.deleted_at IS NULL AND EXISTS (SELECT 1 FROM occurrence_rows o WHERE o.event_id = e.id)
        ),
        date_rows AS MATERIALIZED (
            SELECT 'episode'::text AS kind, e.id, e.item_id, e.number::text || '. ' || e.title AS title,
                   coalesce(p.planned_on, e.planned_on) AS date,
                   p.planned_on IS NOT NULL AS personal, coalesce(p.status = 'done', false) AS done
            FROM course_episodes e JOIN scope ON scope.id = e.item_id
            LEFT JOIN episode_progress p ON p.episode_id = e.id AND p.user_id = app_user_id()
            WHERE e.workspace_id = p_workspace AND coalesce(p.planned_on, e.planned_on) BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'study_unit', u.id, u.item_id, u.number::text || '. ' || u.title,
                   coalesce(p.planned_on, u.planned_on), p.planned_on IS NOT NULL, coalesce(p.status = 'done', false)
            FROM study_units u JOIN scope ON scope.id = u.item_id
            LEFT JOIN unit_progress p ON p.unit_id = u.id AND p.user_id = app_user_id()
            WHERE u.workspace_id = p_workspace AND coalesce(p.planned_on, u.planned_on) BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'submission', r.id, r.item_id, r.title, r.occurred_on,
                   false, coalesce(r.fields->>'status' IN ('published', 'rejected'), false)
            FROM item_records r JOIN scope ON scope.id = r.item_id
            WHERE r.workspace_id = p_workspace AND r.kind = 'submission' AND r.occurred_on BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'revision_due', r.id, r.item_id, r.title, app_iso_date(r.fields->>'revision_due'),
                   false, coalesce(r.fields->>'status' IN ('accepted', 'published', 'rejected'), false)
            FROM item_records r JOIN scope ON scope.id = r.item_id
            WHERE r.workspace_id = p_workspace AND r.kind = 'submission'
              AND app_iso_date(r.fields->>'revision_due') BETWEEN p_from AND p_to
            UNION ALL
            SELECT 'deliverable', r.id, r.item_id, r.title, r.occurred_on,
                   false, coalesce(r.fields->>'status' IN ('done', 'accepted'), false)
            FROM item_records r JOIN scope ON scope.id = r.item_id
            WHERE r.workspace_id = p_workspace AND r.kind = 'deliverable' AND r.occurred_on BETWEEN p_from AND p_to
        ),
        item_ids AS (
            SELECT item_id FROM task_rows UNION SELECT item_id FROM projected_rows
            UNION SELECT item_id FROM event_rows UNION SELECT item_id FROM date_rows
        ),
        item_rows AS (
            SELECT i.id, app_item_reference(i.id) AS reference, i.title FROM items i JOIN item_ids ON item_ids.item_id = i.id
        )
        SELECT jsonb_build_object(
            'tasks', (SELECT coalesce(jsonb_agg(to_jsonb(t) - 'sort_date' - 'number' ORDER BY t.sort_date, t.number), '[]'::jsonb) FROM task_rows t),
            'projected_tasks', (SELECT coalesce(jsonb_agg(to_jsonb(p) ORDER BY p.occurrence, p.series_id), '[]'::jsonb) FROM projected_rows p),
            'events', (SELECT coalesce(jsonb_agg(to_jsonb(e) ORDER BY coalesce(e.starts_at, e.start_date::timestamp AT TIME ZONE e.timezone), e.id), '[]'::jsonb) FROM event_rows e),
            'occurrences', (SELECT coalesce(jsonb_agg(to_jsonb(o) ORDER BY coalesce(o.starts_at, o.start_date::timestamp AT TIME ZONE 'UTC', o.occurrence::timestamp AT TIME ZONE 'UTC'), o.event_id), '[]'::jsonb) FROM occurrence_rows o),
            'dates', (SELECT coalesce(jsonb_agg(to_jsonb(d) ORDER BY d.date, d.kind, d.title), '[]'::jsonb) FROM date_rows d),
            'items', (SELECT coalesce(jsonb_agg(to_jsonb(i) ORDER BY i.title, i.id), '[]'::jsonb) FROM item_rows i)
        )
    );
END;
$$;

ALTER FUNCTION fvoci.app_calendar(p_workspace uuid, p_from date, p_to date, p_tz text, p_module uuid, p_item uuid) OWNER TO fvoci_migrator;

-- #291 task-screen reads. The caller retains fvoci_app and every child table's RLS.
-- This shared aggregate is also callable only as the current application user.
CREATE FUNCTION fvoci.app_task_bundle_read(p_item uuid) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
AS $$
DECLARE v_grade text := app_access('item', p_item);
BEGIN
  IF v_grade = 'none' THEN
    RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
  END IF;
  RETURN jsonb_build_object(
    'viewer_id', app_user_id(), 'item_id', p_item, 'item_access', v_grade,
    'archived', (SELECT i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL
                 FROM items i JOIN modules m ON m.id=i.module_id WHERE i.id=p_item),
    'tasks', coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM (
      SELECT t.id,t.item_id,i.module_id,t.workspace_id,t.parent_id,
        app_task_reference(t.id) AS reference,t.title,t.description,s.name AS status,
        s.category AS status_category,t.priority,t.start_date,t.due_date,t.estimate_minutes,
        t.tags,t.milestone,t.series_id,t.occurrence_date,t.version,t.move_version,t.completed_at,t.updated_at
      FROM tasks t JOIN items i ON i.id=t.item_id JOIN task_statuses s ON s.id=t.status_id
      WHERE t.item_id=p_item ORDER BY s.position,t.position,t.number
    ) r),'[]'::jsonb),
    'assignees', coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM (
      SELECT a.task_id,a.user_id,coalesce(u.display_name,'') AS display_name
      FROM task_assignees a JOIN tasks t ON t.id=a.task_id LEFT JOIN users u ON u.id=a.user_id
      WHERE t.item_id=p_item ORDER BY a.task_id,u.display_name,a.user_id
    ) r),'[]'::jsonb),
    'dependencies', coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM (
      SELECT predecessor_id,successor_id,kind,lag_days FROM task_dependencies
      WHERE item_id=p_item ORDER BY created_at,predecessor_id,successor_id
    ) r),'[]'::jsonb),
    'series', coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM (
      SELECT s.id,s.root_task_id,s.rule,s.timezone FROM task_series s
      WHERE s.item_id=p_item ORDER BY s.id
    ) r),'[]'::jsonb),
    'checklist', coalesce((SELECT jsonb_agg(to_jsonb(r)) FROM (
      SELECT c.id,c.task_id,c.text,c.checked FROM checklist_items c JOIN tasks t ON t.id=c.task_id
      WHERE t.item_id=p_item ORDER BY c.task_id,c.position,c.created_at
    ) r),'[]'::jsonb)
  );
END $$;
ALTER FUNCTION fvoci.app_task_bundle_read(uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_context_read(p_item uuid) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
AS $$
DECLARE v_item jsonb; v_module jsonb; v_module_id uuid;
BEGIN
  SELECT jsonb_build_object('id',i.id,'module_id',i.module_id,'workspace_id',i.workspace_id,
    'reference',app_item_reference(i.id),'title',i.title,'description',i.description,
    'visibility',i.visibility,'access',app_access('item',i.id),'archived',i.archived_at IS NOT NULL,
    'updated_at',i.updated_at,'status',i.status,'starts_on',i.starts_on,'ends_on',i.ends_on,
    'is_template',i.is_template,'version',i.version),i.module_id
    INTO v_item,v_module_id FROM items i WHERE i.id=p_item;
  IF NOT FOUND THEN RAISE EXCEPTION 'item not found' USING ERRCODE='P0002'; END IF;
  SELECT jsonb_build_object('id',m.id,'workspace_id',m.workspace_id,'kind',m.kind,'name',m.name,
    'key',k.key,'archived',m.archived_at IS NOT NULL,'previous_keys',coalesce((
      SELECT jsonb_agg(o.key ORDER BY o.created_at) FROM module_keys o WHERE o.module_id=m.id AND o.retired_at IS NOT NULL
    ),'[]'::jsonb),'icon',m.icon,'description',m.description,'access',app_access('module',m.id))
    INTO v_module FROM modules m JOIN module_keys k ON k.module_id=m.id AND k.retired_at IS NULL
    WHERE m.id=v_module_id;
  IF NOT FOUND THEN RAISE EXCEPTION 'module not found' USING ERRCODE='P0002'; END IF;
  RETURN jsonb_build_object('viewer_id',app_user_id(),'item',v_item,'module',v_module,'documents',(
    WITH visible AS MATERIALIZED (
      SELECT id,title,revision,updated_at,parent_id,number,position,created_at FROM documents WHERE item_id=p_item
    )
    SELECT coalesce(jsonb_agg(to_jsonb(r)),'[]'::jsonb) FROM (
      SELECT d.id,(SELECT k.key FROM items i JOIN module_keys k ON k.module_id=i.module_id AND k.retired_at IS NULL
        WHERE i.id=p_item)||'-'||d.number AS reference,d.title,d.revision,d.updated_at,p.id AS parent_id
      FROM visible d LEFT JOIN visible p ON p.id=d.parent_id
      ORDER BY d.position,d.number NULLS LAST,d.created_at,d.id
    ) r
  ));
END $$;
ALTER FUNCTION fvoci.app_task_context_read(uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_board_read(p_item uuid) RETURNS jsonb
    LANGUAGE sql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
AS $$
  SELECT app_task_bundle_read(p_item) || jsonb_build_object('statuses',
    (SELECT coalesce(jsonb_agg(to_jsonb(s)),'[]'::jsonb) FROM app_task_statuses(p_item) s))
$$;
ALTER FUNCTION fvoci.app_task_board_read(uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_detail_read(p_task uuid) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
AS $$
DECLARE v_task jsonb; v_item uuid; v_bundle jsonb; v_statuses jsonb; v_assignable jsonb; v_item_json jsonb;
BEGIN
  SELECT to_jsonb(r),r.item_id INTO v_task,v_item FROM (
    SELECT t.id,t.item_id,i.module_id,t.workspace_id,t.parent_id,
      app_task_reference(t.id) AS reference,t.title,t.description,s.name AS status,
      s.category AS status_category,t.priority,t.start_date,t.due_date,t.estimate_minutes,
      t.tags,t.milestone,t.series_id,t.occurrence_date,t.version,t.move_version,t.completed_at,t.updated_at
    FROM tasks t JOIN items i ON i.id=t.item_id JOIN task_statuses s ON s.id=t.status_id WHERE t.id=p_task
  ) r;
  IF NOT FOUND THEN RAISE EXCEPTION 'task not found' USING ERRCODE='P0002'; END IF;
  v_bundle := app_task_bundle_read(v_item);
  SELECT jsonb_build_object('id',i.id,'module_id',i.module_id,'workspace_id',i.workspace_id,
    'reference',app_item_reference(i.id),'title',i.title,'description',i.description,
    'visibility',i.visibility,'access',app_access('item',i.id),'archived',i.archived_at IS NOT NULL,
    'updated_at',i.updated_at,'status',i.status,'starts_on',i.starts_on,'ends_on',i.ends_on,
    'is_template',i.is_template,'version',i.version)
    INTO v_item_json FROM items i WHERE i.id=v_item;
  BEGIN
    SELECT jsonb_build_object('state','ready','value',coalesce(jsonb_agg(to_jsonb(s)),'[]'::jsonb))
      INTO v_statuses FROM app_task_statuses(v_item) s;
  EXCEPTION WHEN OTHERS THEN
    IF SQLSTATE IN ('42501','P0002') OR left(SQLSTATE,2)='28' THEN RAISE; END IF;
    v_statuses := jsonb_build_object('state','failed','error','unavailable');
  END;
  BEGIN
    SELECT jsonb_build_object('state','ready','value',coalesce(jsonb_agg(to_jsonb(a)),'[]'::jsonb))
      INTO v_assignable FROM app_item_assignable(v_item) a;
  EXCEPTION WHEN OTHERS THEN
    IF SQLSTATE IN ('42501','P0002') OR left(SQLSTATE,2)='28' THEN RAISE; END IF;
    v_assignable := jsonb_build_object('state','failed','error','unavailable');
  END;
  RETURN v_bundle || jsonb_build_object('task',v_task,'item',v_item_json,'statuses',v_statuses,'assignable',v_assignable);
END $$;
ALTER FUNCTION fvoci.app_task_detail_read(uuid) OWNER TO fvoci_migrator;

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
