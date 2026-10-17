-- 반복 업무·반복 일정 (D-31, D-50, D-57).
--
-- 규칙(구현 선택, RFC 5545 전체가 아님): {"freq": daily|weekly|monthly|yearly, "interval": 1..99,
-- "weekdays": [1..7] (ISO, 월=1, 매주에만), "until": 날짜 | null, "count": 1..1000 | null}.
-- 매달 31일·매년 2월 29일처럼 없는 날은 건너뛴다. 기준일(anchor)이 첫 회차이며 count는 첫 회차를 포함한다.
-- 펼치기는 `app_recurrence_dates` 하나로 하고 업무·일정이 함께 쓴다.
--
-- 반복 업무: 회차마다 실제 업무 행이 있다(완료 상태가 회차별). 미완료 회차는 그대로 두고 다음 회차는 원래
-- 일정대로 만든다. 백그라운드 작업 구조(O-19)가 정해지지 않았으므로 회차 만들기는 쓰기 요청 안에서만 한다:
-- 규칙을 정하거나 바꿀 때, 반복 업무를 고칠 때, 쓰기 권한이 있는 사람이 항목을 열 때 보내는 동기화 요청.
-- 오늘(규칙의 시간대 기준)까지의 회차와 그다음 한 회차를 만들고, 한 번에 50개까지 만든다(나머지는 다음 요청).
-- 읽기(목록·달력)는 아무것도 만들지 않고, 아직 없는 회차는 "예정"으로 계산해 보여 준다.
-- 반복 일정: 완료 상태가 없으므로 행을 만들지 않고 달력에서 펼친다. 회차 하나 빼기만 지원한다(excluded_dates).

-- 규칙 확인. 틀리면 22023.
CREATE FUNCTION app_check_recurrence(p_rule jsonb) RETURNS void
  LANGUAGE plpgsql IMMUTABLE
  SET search_path = fvoci, pg_temp
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
  END
  $$;

-- 규칙을 펼쳐 [p_from, p_to] 안의 날짜를 돌려준다. 기준일부터 세어 count·until을 적용한다.
CREATE FUNCTION app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date)
  RETURNS TABLE (occurrence date)
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$
    WITH rule AS (
      SELECT p_rule ->> 'freq' AS freq,
             (p_rule ->> 'interval')::integer AS step,
             coalesce((SELECT array_agg(w::integer) FROM jsonb_array_elements(p_rule -> 'weekdays') w),
                      '{}'::integer[]) AS weekdays,
             (p_rule ->> 'until')::date AS until,
             (p_rule ->> 'count')::integer AS cnt
    ),
    days AS (
      SELECT g::date AS d, rule.*
      FROM rule, generate_series(p_anchor, least(p_to, coalesce(rule.until, p_to)), interval '1 day') g
    ),
    hits AS (
      SELECT d, cnt FROM days
      WHERE CASE freq
        WHEN 'daily' THEN (d - p_anchor) % step = 0
        WHEN 'weekly' THEN
          extract(isodow FROM d)::integer = ANY (
            CASE WHEN cardinality(weekdays) = 0 THEN ARRAY[extract(isodow FROM p_anchor)::integer]
                 ELSE weekdays END)
          AND ((d - (p_anchor - (extract(isodow FROM p_anchor)::integer - 1))) / 7) % step = 0
        WHEN 'monthly' THEN
          extract(day FROM d) = extract(day FROM p_anchor)
          AND ((extract(year FROM d) * 12 + extract(month FROM d))
               - (extract(year FROM p_anchor) * 12 + extract(month FROM p_anchor)))::integer % step = 0
        WHEN 'yearly' THEN
          extract(day FROM d) = extract(day FROM p_anchor) AND extract(month FROM d) = extract(month FROM p_anchor)
          AND (extract(year FROM d) - extract(year FROM p_anchor))::integer % step = 0
      END
    ),
    numbered AS (
      SELECT d, cnt, row_number() OVER (ORDER BY d) AS n FROM hits
    )
    SELECT d FROM numbered
    WHERE (cnt IS NULL OR n <= cnt) AND d BETWEEN p_from AND p_to
    ORDER BY d
  $$;

------------------------------------------------------------------------------
-- 반복 업무
------------------------------------------------------------------------------

CREATE TABLE task_series (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  item_id      uuid NOT NULL REFERENCES items (id),
  root_task_id uuid NOT NULL UNIQUE REFERENCES tasks (id),
  -- 멈춘 반복은 규칙이 없다(이미 만든 회차는 남는다).
  rule         jsonb,
  timezone     text NOT NULL,
  anchor       date NOT NULL,
  created_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX task_series_item ON task_series (item_id);

ALTER TABLE tasks ADD COLUMN series_id uuid REFERENCES task_series (id);
ALTER TABLE tasks ADD COLUMN occurrence_date date;
ALTER TABLE tasks ADD CONSTRAINT tasks_occurrence UNIQUE (series_id, occurrence_date);
ALTER TABLE tasks ADD CONSTRAINT tasks_series_occurrence CHECK ((series_id IS NULL) = (occurrence_date IS NULL));

-- 반복 규칙 정하기·바꾸기·멈추기(업무 쓰기 권한). 처음 정하면 그 업무가 첫 회차(기준일 = 마감일, 없으면 시작일).
-- 최상위 업무만. 이미 만든 회차는 바꾸지 않는다. 버전과 상관없이 마지막 요청이 이긴다(감사 기록).
CREATE FUNCTION app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task   tasks%ROWTYPE;
    v_series task_series%ROWTYPE;
    v_anchor date;
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
    IF v_task.series_id IS NOT NULL THEN
      SELECT * INTO v_series FROM task_series WHERE id = v_task.series_id FOR UPDATE;
      IF v_series.root_task_id <> p_task THEN
        RAISE EXCEPTION 'change the recurrence on the first occurrence' USING ERRCODE = '22023';
      END IF;
      UPDATE task_series SET rule = p_rule, timezone = p_timezone, updated_at = now() WHERE id = v_series.id;
      PERFORM app_audit(v_task.workspace_id, 'set_recurrence', 'task', p_task,
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
    INSERT INTO task_series (workspace_id, item_id, root_task_id, rule, timezone, anchor, created_by)
    VALUES (v_task.workspace_id, v_task.item_id, p_task, p_rule, p_timezone, v_anchor, app_user_id())
    RETURNING * INTO v_series;
    UPDATE tasks SET series_id = v_series.id, occurrence_date = v_anchor WHERE id = p_task;
    PERFORM app_audit(v_task.workspace_id, 'set_recurrence', 'task', p_task,
                      jsonb_build_object('from', NULL, 'to', p_rule, 'timezone', p_timezone));
    RETURN v_series.id;
  END
  $$;

-- 회차 만들기(쓰기 요청 안에서만). 규칙 시간대의 오늘까지와 그다음 한 회차 중 없는 것을 만든다(한 번에 50개).
-- 내용은 첫 회차(틀)의 지금 내용을 따르고, 날짜는 기준일과의 간격을 유지한다. 담당자는 지금 그 항목을 볼 수
-- 있는 사람만 옮기고, 체크리스트는 체크를 푼 채 옮긴다. 하위 업무·선행 관계는 옮기지 않는다.
-- 잠금: 열(할 일, 첫 회차의 열) → 첫 회차 행 → 항목 공유 → 번호. 같은 반복의 동기화는 첫 회차 행에서 줄을 선다.
CREATE FUNCTION app_sync_task_series(p_series uuid) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_series task_series%ROWTYPE;
    v_root   tasks%ROWTYPE;
    v_today  date;
    v_date   date;
    v_id     uuid;
    v_made   integer := 0;
    v_user   uuid := app_user_id();
    v_module uuid;
    v_uid    uuid;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_series.root_task_id);
    PERFORM app_lock_task_rows(ARRAY[v_series.root_task_id], 'todo');
    SELECT * INTO v_root FROM tasks WHERE id = v_series.root_task_id;
    PERFORM app_lock_item(v_root.item_id);
    PERFORM app_require_task_write(v_series.root_task_id);
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.rule IS NULL THEN
      RETURN 0;
    END IF;
    v_today := (now() AT TIME ZONE v_series.timezone)::date;
    SELECT module_id INTO v_module FROM items WHERE id = v_root.item_id;
    FOR v_date IN
      WITH wanted AS (
        SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_series.anchor, v_today)
        UNION
        (SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_today + 1, v_today + 3660)
         ORDER BY occurrence LIMIT 1)
      )
      SELECT w.occurrence FROM wanted w
      WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = p_series AND t.occurrence_date = w.occurrence)
      ORDER BY w.occurrence
      LIMIT 50
    LOOP
      v_id := uuidv7();
      INSERT INTO tasks (id, workspace_id, item_id, number, title, description, priority, start_date, due_date,
                         estimate_minutes, tags, milestone, position, created_by, updated_by, request_id,
                         series_id, occurrence_date)
      VALUES (v_id, v_root.workspace_id, v_root.item_id,
              app_allocate_number(v_root.workspace_id, v_module, 'task', v_id),
              v_root.title, v_root.description, v_root.priority,
              v_root.start_date + (v_date - v_series.anchor), v_root.due_date + (v_date - v_series.anchor),
              v_root.estimate_minutes, v_root.tags, v_root.milestone, app_task_lane_end(v_root.item_id, 'todo'),
              v_user, v_user, uuidv7(), p_series, v_date)
      ON CONFLICT (series_id, occurrence_date) DO NOTHING;
      IF NOT FOUND THEN
        CONTINUE;
      END IF;
      INSERT INTO checklist_items (task_id, text, checked, position, request_id, created_by)
      SELECT v_id, c.text, false, c.position, uuidv7(), v_user FROM checklist_items c WHERE c.task_id = v_root.id;
      FOR v_uid IN SELECT a.user_id FROM task_assignees a WHERE a.task_id = v_root.id ORDER BY a.user_id LOOP
        IF EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_root.workspace_id AND m.user_id = v_uid)
           AND app_item_access_as(v_root.item_id, v_uid) IS NOT NULL THEN
          INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
        END IF;
      END LOOP;
      PERFORM app_audit(v_root.workspace_id, 'create', 'task', v_id,
                        jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'series', p_series,
                                           'occurrence', v_date));
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 항목의 모든 반복 업무 동기화(쓰기 권한이 있는 사람이 항목을 열 때). 만든 회차 수.
CREATE FUNCTION app_sync_item_series(p_item uuid) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_item_access(p_item);
    v_id     uuid;
    v_made   integer := 0;
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
    FOR v_id IN SELECT id FROM task_series WHERE item_id = p_item AND rule IS NOT NULL ORDER BY id LOOP
      v_made := v_made + app_sync_task_series(v_id);
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 아직 만들지 않은 회차(예정). 읽기 전용 계산이며 첫 회차를 볼 수 있을 때만 나온다.
CREATE FUNCTION app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date)
  RETURNS TABLE (series_id uuid, root_task_id uuid, item_id uuid, title text, milestone boolean,
                 occurrence date, start_date date, due_date date)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT s.id, r.id, r.item_id, r.title, r.milestone, o.occurrence,
           r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)
    FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    CROSS JOIN LATERAL app_recurrence_dates(s.rule, s.anchor, p_from - 400, p_to) o
    WHERE s.rule IS NOT NULL
      AND (p_workspace IS NULL OR s.workspace_id = p_workspace)
      AND (p_item IS NULL OR s.item_id = p_item)
      AND app_task_access(r.id) IS NOT NULL
      AND coalesce(r.due_date + (o.occurrence - s.anchor), r.start_date + (o.occurrence - s.anchor)) >= p_from
      AND coalesce(r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)) <= p_to
      AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = s.id AND t.occurrence_date = o.occurrence)
  $$;

ALTER TABLE task_series ENABLE ROW LEVEL SECURITY;
CREATE POLICY task_series_select ON task_series FOR SELECT USING (app_task_access(root_task_id) IS NOT NULL);
GRANT SELECT ON task_series TO fvoci_app;

------------------------------------------------------------------------------
-- 반복 일정
------------------------------------------------------------------------------

ALTER TABLE events ADD COLUMN recurrence jsonb;
ALTER TABLE events ADD COLUMN excluded_dates date[] NOT NULL DEFAULT '{}';

-- 반복 규칙 정하기·멈추기(쓰기 권한). 기준일은 일정 시간대의 시작 날짜. 마지막 요청이 이긴다(감사 기록).
CREATE FUNCTION app_set_event_recurrence(p_event uuid, p_rule jsonb) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    UPDATE events SET recurrence = p_rule, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(v_event.workspace_id, 'set_recurrence', 'event', p_event,
                      jsonb_build_object('from', v_event.recurrence, 'to', p_rule));
  END
  $$;

-- 반복 일정의 회차 하나 빼기(쓰기 권한). p_date는 일정 시간대의 회차 날짜.
CREATE FUNCTION app_exclude_event_occurrence(p_event uuid, p_date date) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(v_event.workspace_id, 'exclude_occurrence', 'event', p_event,
                      jsonb_build_object('date', p_date));
  END
  $$;

-- 기간 안의 일정 회차(반복이 아니면 그 일정 하나). 날짜는 일정 시간대 기준으로 펼치고, 시각 있는 일정은
-- 그 시간대의 같은 벽시계 시각을 UTC로 바꾼다(여름시간이 바뀌어도 09:00은 09:00). 볼 수 있는 일정만.
CREATE FUNCTION app_event_occurrences(p_workspace uuid, p_from timestamptz, p_to timestamptz,
                                      p_from_date date, p_to_date date)
  RETURNS TABLE (event_id uuid, occurrence date, start_date date, end_date date,
                 starts_at timestamptz, ends_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH visible AS (
      SELECT e.*, coalesce(e.start_date, (e.starts_at AT TIME ZONE e.timezone)::date) AS anchor
      FROM events e
      WHERE e.workspace_id = p_workspace AND e.deleted_at IS NULL AND app_event_access(e.id) IS NOT NULL
    ),
    expanded AS (
      SELECT v.id, v.anchor AS occ, v.all_day, v.start_date, v.end_date, v.starts_at, v.ends_at, v.timezone,
             v.anchor
      FROM visible v WHERE v.recurrence IS NULL
      UNION ALL
      SELECT v.id, o.occurrence, v.all_day, v.start_date, v.end_date, v.starts_at, v.ends_at, v.timezone, v.anchor
      FROM visible v
      CROSS JOIN LATERAL app_recurrence_dates(v.recurrence, v.anchor, p_from_date - 400, p_to_date + 1) o
      WHERE v.recurrence IS NOT NULL AND NOT (o.occurrence = ANY (v.excluded_dates))
    ),
    shaped AS (
      SELECT id, occ,
             CASE WHEN all_day THEN start_date + (occ - anchor) END AS sd,
             CASE WHEN all_day THEN end_date + (occ - anchor) END AS ed,
             CASE WHEN NOT all_day
                  THEN ((occ + (starts_at AT TIME ZONE timezone)::time) AT TIME ZONE timezone) END AS sa,
             CASE WHEN NOT all_day
                  THEN ((occ + (starts_at AT TIME ZONE timezone)::time) AT TIME ZONE timezone)
                       + (ends_at - starts_at) END AS ea
      FROM expanded
    )
    SELECT id, occ, sd, ed, sa, ea FROM shaped
    WHERE (sd IS NOT NULL AND sd <= p_to_date AND ed >= p_from_date)
       -- 자정에 끝나는 일정은 다음 날에 걸치지 않는다(끝 시각은 포함하지 않음, 길이 0이면 시작 시각으로).
       OR (sa IS NOT NULL AND sa < p_to AND (ea > p_from OR sa >= p_from))
    ORDER BY coalesce(sa, sd::timestamp AT TIME ZONE 'UTC'), id
  $$;

-- 감사 기록용 UTC ISO 시각(세션 시간대 설정과 무관).
CREATE FUNCTION app_utc_text(p_at timestamptz) RETURNS text
  LANGUAGE sql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT to_char(p_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
  $$;

------------------------------------------------------------------------------
-- 일정·달력 리뷰 반영
------------------------------------------------------------------------------

-- 업무 행 잠금은 FOR NO KEY UPDATE: 키는 바뀌지 않으므로 외래 키 확인(KEY SHARE)과 부딪히지 않는다
-- (선행 관계 추가의 외래 키 확인과 승인 재배치가 서로 기다리던 교착 방지).
CREATE OR REPLACE FUNCTION app_lock_task_rows(p_ids uuid[], p_extra_status text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_lane  record;
    v_lanes text[] := '{}';
  BEGIN
    FOR v_lane IN
      SELECT DISTINCT l.item_id, l.status FROM (
        SELECT item_id, status FROM tasks WHERE id = ANY (p_ids)
        UNION
        SELECT item_id, p_extra_status FROM tasks WHERE id = p_ids[1] AND p_extra_status IS NOT NULL
      ) l
      ORDER BY l.item_id, l.status
    LOOP
      PERFORM app_lock_lane(v_lane.item_id, v_lane.status);
      v_lanes := array_append(v_lanes, v_lane.item_id::text || ':' || v_lane.status);
    END LOOP;
    PERFORM 1 FROM tasks WHERE id = ANY (p_ids) ORDER BY id FOR NO KEY UPDATE;
    IF EXISTS (SELECT 1 FROM tasks WHERE id = ANY (p_ids)
               AND NOT (item_id::text || ':' || status = ANY (v_lanes))) THEN
      RAISE EXCEPTION 'a task moved while it was being changed; try again' USING ERRCODE = '40001';
    END IF;
  END
  $$;

CREATE OR REPLACE FUNCTION app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) RETURNS void
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
    -- 볼 수 없는 업무는 잠그지 않는다(잠금 대기로 존재가 드러나지 않게). 잠근 뒤 다시 확인한다.
    IF app_task_access(p_parent) IS NULL THEN
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

-- 시간대: 브라우저(Intl)가 모르는 이름(posix/·right/·Factory 등)은 받지 않는다.
CREATE OR REPLACE FUNCTION app_check_timezone(p_tz text) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_tz IS NULL OR p_tz ~ '^(posix|right|SystemV)/' OR p_tz IN ('Factory', 'localtime', 'posixrules')
       OR NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = p_tz) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
  END
  $$;

-- 날짜 변경 감사: 시간대만 바꾼 경우도 남기고, 시각은 세션 설정과 상관없이 UTC ISO로 적는다.
CREATE OR REPLACE FUNCTION app_update_event(
  p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean,
  p_start_date date, p_end_date date, p_starts_at timestamptz, p_ends_at timestamptz, p_timezone text
) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      PERFORM app_audit(v_event.workspace_id, 'rename', 'event', p_event,
                        jsonb_build_object('from', v_event.title, 'to', btrim(p_title)));
    END IF;
    IF (v_event.start_date, v_event.end_date, v_event.starts_at, v_event.ends_at, v_event.timezone)
       IS DISTINCT FROM (p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone) THEN
      PERFORM app_audit(v_event.workspace_id, 'reschedule', 'event', p_event, jsonb_build_object(
        'from', jsonb_build_array(coalesce(v_event.start_date::text, app_utc_text(v_event.starts_at)),
                                  coalesce(v_event.end_date::text, app_utc_text(v_event.ends_at)), v_event.timezone),
        'to', jsonb_build_array(coalesce(p_start_date::text, app_utc_text(p_starts_at)),
                                coalesce(p_end_date::text, app_utc_text(p_ends_at)), p_timezone)));
    END IF;
    RETURN 'updated';
  END
  $$;


REVOKE ALL ON FUNCTION app_utc_text(timestamptz) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_check_recurrence(jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_recurrence_dates(jsonb, date, date, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_task_recurrence(uuid, jsonb, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_task_series(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_item_series(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_projected_occurrences(uuid, uuid, date, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_event_recurrence(uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_exclude_event_occurrence(uuid, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_event_occurrences(uuid, timestamptz, timestamptz, date, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_check_recurrence(jsonb),
  app_recurrence_dates(jsonb, date, date, date),
  app_set_task_recurrence(uuid, jsonb, text),
  app_sync_task_series(uuid),
  app_sync_item_series(uuid),
  app_projected_occurrences(uuid, uuid, date, date),
  app_set_event_recurrence(uuid, jsonb),
  app_exclude_event_occurrence(uuid, date),
  app_event_occurrences(uuid, timestamptz, timestamptz, date, date)
TO fvoci_app;
