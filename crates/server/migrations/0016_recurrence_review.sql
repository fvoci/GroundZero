-- 반복 리뷰 반영 (D-31, D-50).
--
-- 1. 규칙을 바꾸거나 다시 켜면 그날(규칙 시간대)부터만 회차를 만들고 보인다(effective_from). 지난 날짜를
--    한꺼번에 채우지 않는다. 횟수(count)는 여전히 기준일부터 센다.
-- 2. 펼치기는 날짜를 하루씩 훑지 않고 회차 단위로 계산한다(먼 미래·먼 과거 기간도 회차 수만큼만 든다).
--    기준일은 언제나 첫 회차다(요일이 규칙에 없어도). 끝 날짜는 기준일보다 앞설 수 없다.
-- 3. 항목 전체 동기화는 세 열을 한 번에 먼저 잠근다(반복마다 열을 따로 잡다 교착하던 것).
-- 4. 반복의 첫 회차(틀)는 하위 업무가 될 수 없고, 반복이 켜진 동안 날짜를 모두 지울 수 없다.
-- 5. 반복 아닌 일정은 저장된 시각을 그대로 쓴다(여름시간이 끝나는 날의 겹친 시각). 펼치기 끝을 하루 더 둔다
--    (시간대 차이가 하루를 넘는 경우). 일정 날짜를 옮기면 뺀 회차도 같이 옮긴다.
-- 6. 보관된 항목의 예정 회차는 보이지 않는다(만들 수도 없으므로).

ALTER TABLE task_series ADD COLUMN effective_from date;
UPDATE task_series SET effective_from = anchor;
ALTER TABLE task_series ALTER COLUMN effective_from SET NOT NULL;

CREATE OR REPLACE FUNCTION app_check_recurrence(p_rule jsonb) RETURNS void
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
    IF p_rule ->> 'until' IS NOT NULL AND p_rule ->> 'count' IS NOT NULL THEN
      RAISE EXCEPTION 'use either until or count' USING ERRCODE = '22023';
    END IF;
  END
  $$;

-- 규칙과 기준일로 [p_from, p_to] 안의 회차 날짜. 기준일이 첫 회차이고 count는 그것을 포함한다.
-- 횟수가 없으면 p_from 근처부터 계산하고, 있으면 기준일부터 count개까지만 만든다.
CREATE OR REPLACE FUNCTION app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date)
  RETURNS TABLE (occurrence date)
  LANGUAGE plpgsql STABLE
  SET search_path = fvoci, pg_temp
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

-- 규칙 정하기·바꾸기·멈추기: 바꾸거나 다시 켜면 그날부터만 적용한다. 끝 날짜는 기준일보다 앞설 수 없다.
CREATE OR REPLACE FUNCTION app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      UPDATE task_series SET
        rule = p_rule, timezone = p_timezone, updated_at = now(),
        effective_from = CASE WHEN p_rule IS DISTINCT FROM v_series.rule
                              THEN greatest(v_series.anchor, v_today) ELSE effective_from END
      WHERE id = v_series.id;
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
    IF (p_rule ->> 'until')::date < v_anchor THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    INSERT INTO task_series (workspace_id, item_id, root_task_id, rule, timezone, anchor, effective_from, created_by)
    VALUES (v_task.workspace_id, v_task.item_id, p_task, p_rule, p_timezone, v_anchor, v_anchor, app_user_id())
    RETURNING * INTO v_series;
    UPDATE tasks SET series_id = v_series.id, occurrence_date = v_anchor WHERE id = p_task;
    PERFORM app_audit(v_task.workspace_id, 'set_recurrence', 'task', p_task,
                      jsonb_build_object('from', NULL, 'to', p_rule, 'timezone', p_timezone));
    RETURN v_series.id;
  END
  $$;

-- 회차 만들기: effective_from 이후만. 틀에 날짜가 없으면 만들지 않는다.
CREATE OR REPLACE FUNCTION app_sync_task_series(p_series uuid) RETURNS integer
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
    IF v_series.rule IS NULL OR (v_root.due_date IS NULL AND v_root.start_date IS NULL) THEN
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

-- 항목 전체 동기화: 세 열을 (상태 순으로) 먼저 잠근 뒤 반복마다 맞춘다.
CREATE OR REPLACE FUNCTION app_sync_item_series(p_item uuid) RETURNS integer
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
    PERFORM app_lock_lane(p_item, 'done');
    PERFORM app_lock_lane(p_item, 'in_progress');
    PERFORM app_lock_lane(p_item, 'todo');
    FOR v_id IN SELECT id FROM task_series WHERE item_id = p_item AND rule IS NOT NULL ORDER BY id LOOP
      v_made := v_made + app_sync_task_series(v_id);
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 예정 회차: effective_from 이후, 보관되지 않은 항목만.
CREATE OR REPLACE FUNCTION app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date)
  RETURNS TABLE (series_id uuid, root_task_id uuid, item_id uuid, title text, milestone boolean,
                 occurrence date, start_date date, due_date date)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT s.id, r.id, r.item_id, r.title, r.milestone, o.occurrence,
           r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)
    FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    CROSS JOIN LATERAL app_recurrence_dates(s.rule, s.anchor, greatest(s.effective_from, p_from - 400), p_to) o
    WHERE s.rule IS NOT NULL
      AND (p_workspace IS NULL OR s.workspace_id = p_workspace)
      AND (p_item IS NULL OR s.item_id = p_item)
      AND app_task_access(r.id) IS NOT NULL
      AND NOT app_item_archived(r.item_id)
      AND coalesce(r.due_date + (o.occurrence - s.anchor), r.start_date + (o.occurrence - s.anchor)) >= p_from
      AND coalesce(r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)) <= p_to
      AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = s.id AND t.occurrence_date = o.occurrence)
  $$;

-- 반복의 첫 회차 지키기: 하위 업무가 되거나 날짜를 모두 잃으면 반복이 화면에서 사라지거나 날짜 없는 회차를
-- 만든다. 반복이 켜진 동안은 막는다(반복을 멈추면 할 수 있다).
CREATE FUNCTION app_guard_series_root() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
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
CREATE TRIGGER tasks_series_root_guard BEFORE UPDATE OF parent_id, start_date, due_date ON tasks
  FOR EACH ROW EXECUTE FUNCTION app_guard_series_root();

-- 반복 일정 펼치기: 반복 아닌 일정은 저장된 시각 그대로, 펼치기 끝은 이틀 더.
CREATE OR REPLACE FUNCTION app_event_occurrences(p_workspace uuid, p_from timestamptz, p_to timestamptz,
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
    shaped AS (
      SELECT v.id, v.anchor AS occ, v.start_date AS sd, v.end_date AS ed, v.starts_at AS sa, v.ends_at AS ea
      FROM visible v WHERE v.recurrence IS NULL
      UNION ALL
      SELECT v.id, o.occurrence,
             CASE WHEN v.all_day THEN v.start_date + (o.occurrence - v.anchor) END,
             CASE WHEN v.all_day THEN v.end_date + (o.occurrence - v.anchor) END,
             CASE WHEN NOT v.all_day
                  THEN ((o.occurrence + (v.starts_at AT TIME ZONE v.timezone)::time) AT TIME ZONE v.timezone) END,
             CASE WHEN NOT v.all_day
                  THEN ((o.occurrence + (v.starts_at AT TIME ZONE v.timezone)::time) AT TIME ZONE v.timezone)
                       + (v.ends_at - v.starts_at) END
      FROM visible v
      CROSS JOIN LATERAL app_recurrence_dates(v.recurrence, v.anchor, p_from_date - 400, p_to_date + 2) o
      WHERE v.recurrence IS NOT NULL AND NOT (o.occurrence = ANY (v.excluded_dates))
    )
    SELECT id, occ, sd, ed, sa, ea FROM shaped
    WHERE (sd IS NOT NULL AND sd <= p_to_date AND ed >= p_from_date)
       OR (sa IS NOT NULL AND sa < p_to AND (ea > p_from OR sa >= p_from))
    ORDER BY coalesce(sa, sd::timestamp AT TIME ZONE 'UTC'), id
  $$;

-- 반복 일정의 시작 날짜(일정 시간대 기준)가 바뀌면 뺀 회차도 같은 날수만큼 옮긴다.
CREATE FUNCTION app_shift_excluded_dates() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old date := coalesce(OLD.start_date, (OLD.starts_at AT TIME ZONE OLD.timezone)::date);
    v_new date := coalesce(NEW.start_date, (NEW.starts_at AT TIME ZONE NEW.timezone)::date);
  BEGIN
    IF cardinality(NEW.excluded_dates) > 0 AND v_old IS DISTINCT FROM v_new THEN
      NEW.excluded_dates := ARRAY(SELECT d + (v_new - v_old) FROM unnest(NEW.excluded_dates) d);
    END IF;
    RETURN NEW;
  END
  $$;
CREATE TRIGGER events_shift_excluded BEFORE UPDATE OF start_date, starts_at, timezone ON events
  FOR EACH ROW EXECUTE FUNCTION app_shift_excluded_dates();

-- 반복 일정 규칙: 끝 날짜는 시작 날짜보다 앞설 수 없다.
CREATE OR REPLACE FUNCTION app_set_event_recurrence(p_event uuid, p_rule jsonb) RETURNS void
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
    IF (p_rule ->> 'until')::date < coalesce(v_event.start_date, (v_event.starts_at AT TIME ZONE v_event.timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    UPDATE events SET recurrence = p_rule, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(v_event.workspace_id, 'set_recurrence', 'event', p_event,
                      jsonb_build_object('from', v_event.recurrence, 'to', p_rule));
  END
  $$;

REVOKE ALL ON FUNCTION app_guard_series_root() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_shift_excluded_dates() FROM PUBLIC;
