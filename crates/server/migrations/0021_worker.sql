-- 백그라운드 작업 (O-19 해소: 권장안대로 한 바이너리에 API와 작업을 두고 실행 역할로 나누며, 작업 기록은
-- PostgreSQL 테이블에 둔다. 새 서비스 없음).
--
-- 작업 프로세스는 사용자 문맥 없이 `app_system_*` 함수만 부른다. 이 함수들은 정책이 이미 정한 일만 한다:
-- 보존 기간이 지난 휴지통 묶음 영구 삭제(D-41), 반복 업무의 오늘까지와 다음 회차 만들기(D-31). HTTP 경로는
-- 이 함수들을 부르지 않는다. 같은 일의 사용자 경로(관리자 비우기, 항목 열 때의 동기화)는 그대로 남는다.
-- 앱 역할과 작업 역할의 DB 계정 분리는 아직 하지 않았다(같은 앱 역할로 실행, docs/development.md 27절).
-- 감사 기록의 주체가 없는 작업은 detail에 system을 남긴다.

ALTER TABLE audit_log ALTER COLUMN actor DROP NOT NULL;
ALTER TABLE audit_log ADD CONSTRAINT audit_log_actor CHECK (actor IS NOT NULL OR detail ? 'system');

-- 주기 작업이 마지막으로 돈 시각. 여러 프로세스가 떠 있어도 이름마다 한 번만 돈다.
CREATE TABLE scheduled_runs (
  name        text PRIMARY KEY,
  last_run_at timestamptz NOT NULL
);

-- 때가 됐으면 지금으로 표시하고 true. 이름별 잠금으로 동시에 두 번 돌지 않는다(트랜잭션이 끝나면 풀린다).
CREATE FUNCTION app_claim_schedule(p_name text, p_every interval) RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_last timestamptz;
  BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-schedule:' || p_name, 0));
    SELECT last_run_at INTO v_last FROM scheduled_runs WHERE name = p_name;
    IF v_last IS NOT NULL AND v_last + p_every > now() THEN
      RETURN false;
    END IF;
    INSERT INTO scheduled_runs (name, last_run_at) VALUES (p_name, now())
    ON CONFLICT (name) DO UPDATE SET last_run_at = now();
    RETURN true;
  END
  $$;

CREATE FUNCTION app_sync_task_series_as(p_series uuid, p_system boolean) RETURNS integer
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
    v_user   uuid := CASE WHEN p_system THEN NULL ELSE app_user_id() END;
    v_module uuid;
    v_uid    uuid;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    PERFORM app_lock_task_rows(ARRAY[v_series.root_task_id], 'todo');
    SELECT * INTO v_root FROM tasks WHERE id = v_series.root_task_id;
    PERFORM app_lock_item(v_root.item_id);
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    -- 작업 프로세스는 만든 사람 이름으로 회차를 만들고 감사 기록에는 system으로 남긴다.
    v_user := coalesce(v_user, v_series.created_by);
    IF v_series.rule IS NULL OR (v_root.due_date IS NULL AND v_root.start_date IS NULL)
       OR v_root.deleted_at IS NOT NULL OR app_item_archived(v_root.item_id) THEN
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
      INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
      VALUES (v_root.workspace_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'create', 'task', v_id,
              jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'series', p_series,
                                 'occurrence', v_date)
                || CASE WHEN p_system THEN jsonb_build_object('system', true) ELSE '{}'::jsonb END);
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;


-- 사용자 경로: 권한을 확인하고 같은 본문을 쓴다.
CREATE OR REPLACE FUNCTION app_sync_task_series(p_series uuid) RETURNS integer
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT app_sync_task_series_as(p_series, false) $$;

CREATE FUNCTION app_purge_batch_as(p_batch uuid, p_system boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_item  uuid;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR v_batch.restored_at IS NOT NULL OR v_batch.purged_at IS NOT NULL
       OR (NOT p_system AND app_trash_role(v_batch) IS NULL) THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system AND app_trash_role(v_batch) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can delete permanently' USING ERRCODE = '42501';
    END IF;
    -- 작업 프로세스는 만료된 묶음만 지운다.
    IF p_system AND app_trash_expires_at(v_batch) > now() THEN
      RETURN;
    END IF;
    IF v_batch.root_kind = 'document' THEN
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'event' THEN
      DELETE FROM events WHERE trash_batch_id = p_batch;
    ELSE
      v_item := v_batch.root_id;
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE item_id = v_item));
      DELETE FROM task_dependencies WHERE item_id = v_item;
      DELETE FROM task_series WHERE item_id = v_item;
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE item_id = v_item));
      DELETE FROM events WHERE item_id = v_item;
      DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = v_item;
      -- 이 항목 안의 것이 다른 휴지통 묶음에 있었으면 그 묶음도 함께 끝난다.
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND NOT EXISTS (SELECT 1 FROM documents WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM tasks WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM events WHERE trash_batch_id = trash_batches.id)
        AND root_kind <> 'item';
      DELETE FROM items WHERE id = v_item;
    END IF;
    UPDATE trash_batches SET purged_at = now() WHERE id = p_batch;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_batch.workspace_id, v_batch.owner_user_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
  END
  $$;


CREATE OR REPLACE FUNCTION app_purge_batch(p_batch uuid) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT app_purge_batch_as(p_batch, false) $$;

-- 작업: 보존 기간이 지난 묶음을 모두 영구 삭제한다. 하나가 실패해도 나머지는 계속한다.
CREATE FUNCTION app_system_purge_expired() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_made  integer := 0;
  BEGIN
    FOR v_batch IN
      SELECT * FROM trash_batches WHERE restored_at IS NULL AND purged_at IS NULL ORDER BY deleted_at
    LOOP
      CONTINUE WHEN app_trash_expires_at(v_batch) > now();
      BEGIN
        PERFORM app_purge_batch_as(v_batch.id, true);
        v_made := v_made + 1;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of trash batch % failed: %', v_batch.id, SQLERRM;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 작업: 반복이 켜진 모든 업무의 회차를 맞춘다(지워지거나 보관된 것은 건너뛴다). 하나가 실패해도 계속한다.
CREATE FUNCTION app_system_recurrence_tick() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
  BEGIN
    FOR v_id IN
      SELECT s.id FROM task_series s
      JOIN tasks r ON r.id = s.root_task_id
      JOIN items i ON i.id = s.item_id
      WHERE s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      BEGIN
        v_made := v_made + app_sync_task_series_as(v_id, true);
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'recurrence sync of series % failed: %', v_id, SQLERRM;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

REVOKE ALL ON FUNCTION app_claim_schedule(text, interval) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_sync_task_series_as(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_batch_as(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_purge_expired() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_recurrence_tick() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_claim_schedule(text, interval),
  app_system_purge_expired(),
  app_system_recurrence_tick()
TO fvoci_app;
