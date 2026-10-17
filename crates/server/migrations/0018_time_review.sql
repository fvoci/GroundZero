-- 시간 기록·반복 리뷰 반영 (D-32, D-31).
--
-- 1. 업무의 공유 합계는 끝난 기록만 센다(진행 중인 남의 타이머가 늘어나는 것으로 시작·멈춤 시각이 드러나지 않게).
--    내 시간도 끝난 기록만 세고, 화면이 진행 중인 내 타이머를 더한다.
-- 2. 같은 요청 id로 다시 보낸 시작은 잠금 뒤에도 다시 찾아 같은 기록을 돌려준다(409가 아니라).
-- 3. 일시정지 중인 타이머의 시작을 멈춘 시각 뒤로 옮길 수 없다.
-- 4. 워크스페이스에서 빠지면 그 워크스페이스의 합계 공유 선택도 지운다(다시 초대돼도 묻지 않고 공유되지 않게).
-- 5. 멈춘 반복을 다시 켤 때도 첫 회차가 하위 업무면 거부한다.
-- 6. 일정을 옮길 때 뺀 회차는 모든 회차가 같은 날수만큼 움직이는 규칙(매일, 요일을 정하지 않은 매주)에서만 옮긴다.

CREATE OR REPLACE FUNCTION app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
    v_ws   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM time_entries WHERE user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    -- 같은 사용자의 시작 요청을 줄 세운다(하나만 진행 중이라는 규칙을 고유 색인과 함께 지킨다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || v_user::text, 0));
    SELECT id INTO v_id FROM time_entries WHERE user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF EXISTS (SELECT 1 FROM time_entries WHERE user_id = v_user AND ended_at IS NULL AND deleted_at IS NULL) THEN
      RAISE EXCEPTION 'a timer is already running' USING ERRCODE = '55000';
    END IF;
    INSERT INTO time_entries (user_id, started_at, kind, memo, target_kind, target_id, workspace_id, request_id)
    VALUES (v_user, now(), p_kind, coalesce(p_memo, ''), p_target_kind, p_target, v_ws, p_request_id)
    RETURNING id INTO v_id;
    RETURN v_id;
  END
  $$;

CREATE OR REPLACE FUNCTION app_update_time_entry(
  p_entry uuid, p_base_version integer, p_started timestamptz, p_ended timestamptz, p_paused integer,
  p_kind text, p_memo text, p_target_kind text, p_target uuid
) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
    v_ws    uuid;
  BEGIN
    IF v_entry.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    IF v_entry.ended_at IS NULL THEN
      IF p_ended IS NOT NULL OR p_paused IS DISTINCT FROM v_entry.paused_seconds
         OR p_started > coalesce(v_entry.paused_at, now()) THEN
        RAISE EXCEPTION 'a running timer only changes its start, kind, memo and target' USING ERRCODE = '22023';
      END IF;
    ELSIF p_ended IS NULL OR p_ended < p_started OR p_paused < 0
          OR p_paused > extract(epoch FROM p_ended - p_started) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    -- 대상을 바꿀 때만 지금 볼 수 있는지 확인한다(이미 연결된 대상은 볼 수 없게 되어도 그대로 둘 수 있다).
    IF (p_target_kind, p_target) IS DISTINCT FROM (v_entry.target_kind, v_entry.target_id) THEN
      v_ws := app_time_target_workspace(p_target_kind, p_target);
    ELSE
      v_ws := v_entry.workspace_id;
    END IF;
    INSERT INTO time_entry_revisions (entry_id, user_id, started_at, ended_at, paused_seconds, kind, memo,
                                      target_kind, target_id)
    VALUES (p_entry, v_entry.user_id, v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds, v_entry.kind,
            v_entry.memo, v_entry.target_kind, v_entry.target_id);
    UPDATE time_entries SET
      started_at = p_started, ended_at = p_ended, paused_seconds = p_paused, kind = p_kind,
      memo = coalesce(p_memo, ''), target_kind = p_target_kind, target_id = p_target, workspace_id = v_ws,
      version = version + 1, updated_at = now()
    WHERE id = p_entry;
    RETURN 'updated';
  END
  $$;

CREATE OR REPLACE FUNCTION app_task_time(p_task uuid)
  RETURNS TABLE (mine_seconds bigint, shared_seconds bigint, shared_people integer)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    IF app_task_access(p_task) IS NULL THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT workspace_id INTO v_ws FROM tasks WHERE id = p_task;
    RETURN QUERY
      SELECT
        coalesce(sum(app_entry_seconds(e)) FILTER (WHERE e.user_id = app_user_id() AND e.ended_at IS NOT NULL), 0)::bigint,
        coalesce(sum(app_entry_seconds(e)) FILTER (WHERE s.user_id IS NOT NULL AND e.ended_at IS NOT NULL), 0)::bigint,
        count(DISTINCT e.user_id) FILTER (WHERE s.user_id IS NOT NULL AND e.ended_at IS NOT NULL)::integer
      FROM time_entries e
      LEFT JOIN time_sharing s ON s.user_id = e.user_id AND s.workspace_id = v_ws
        AND EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_ws AND m.user_id = e.user_id)
      WHERE e.target_kind = 'task' AND e.target_id = p_task AND e.deleted_at IS NULL;
  END
  $$;

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
      IF p_rule IS NOT NULL AND v_task.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'only top-level tasks can repeat' USING ERRCODE = '22023';
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

CREATE OR REPLACE FUNCTION app_shift_excluded_dates() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old date := coalesce(OLD.start_date, (OLD.starts_at AT TIME ZONE OLD.timezone)::date);
    v_new date := coalesce(NEW.start_date, (NEW.starts_at AT TIME ZONE NEW.timezone)::date);
  BEGIN
    IF cardinality(NEW.excluded_dates) > 0 AND v_old IS DISTINCT FROM v_new AND NEW.recurrence IS NOT NULL
       AND (NEW.recurrence ->> 'freq' = 'daily'
            OR (NEW.recurrence ->> 'freq' = 'weekly' AND jsonb_array_length(NEW.recurrence -> 'weekdays') = 0)) THEN
      NEW.excluded_dates := ARRAY(SELECT d + (v_new - v_old) FROM unnest(NEW.excluded_dates) d);
    END IF;
    RETURN NEW;
  END
  $$;

-- 워크스페이스에서 빠지면 그 워크스페이스의 합계 공유 선택을 지운다.
CREATE FUNCTION app_forget_time_sharing() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    DELETE FROM time_sharing WHERE user_id = OLD.user_id AND workspace_id = OLD.workspace_id;
    RETURN OLD;
  END
  $$;
CREATE TRIGGER workspace_members_forget_time_sharing AFTER DELETE ON workspace_members
  FOR EACH ROW EXECUTE FUNCTION app_forget_time_sharing();
REVOKE ALL ON FUNCTION app_forget_time_sharing() FROM PUBLIC;
