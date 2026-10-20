-- 스터디 계획과 개인별 진행 (D-36: 교육과정·챕터·단계, 일·주별 계획, 페이지·문제 수·목표 시간, 계획 대비 진행·남은
-- 분량·미완료 재배치, 개인 및 공동 계획).
--
-- 계획 단위(study_units)는 스터디 항목을 볼 수 있는 사람이 함께 보고, 쓸 수 있는 사람이 고친다. 단위마다 분량
-- (페이지·문제·분·개)과 공통 계획일을 둔다. 진행(unit_progress)은 사람마다 따로이며 본인만 읽는다(메모 포함).
-- 개인 계획일은 공통 계획일을 덮어쓴다. 밀린 단위의 재배치는 사용자가 정한 날짜로만 한다(X-07: 자동 추정 없음).
-- 함께 하는 사람의 진행은 지금 그 스터디를 볼 수 있는 사람의 집계만 보인다.

CREATE TABLE study_units (
  id           uuid PRIMARY KEY,
  item_id      uuid NOT NULL REFERENCES items (id) ON DELETE CASCADE,
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  number       integer NOT NULL CHECK (number > 0),
  title        text NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 300),
  chapter      text NOT NULL DEFAULT '' CHECK (length(chapter) <= 300),
  unit         text NOT NULL DEFAULT 'pages' CHECK (unit IN ('pages', 'problems', 'minutes', 'items')),
  amount       integer NOT NULL DEFAULT 0 CHECK (amount BETWEEN 0 AND 1000000),
  planned_on   date,
  created_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  request_id   uuid NOT NULL,
  CONSTRAINT study_units_request UNIQUE (item_id, created_by, request_id)
);
CREATE INDEX study_units_item ON study_units (item_id, number);

CREATE TABLE unit_progress (
  unit_id      uuid NOT NULL REFERENCES study_units (id) ON DELETE CASCADE,
  user_id      uuid NOT NULL REFERENCES users (id),
  status       text NOT NULL DEFAULT 'not_started' CHECK (status IN ('not_started', 'in_progress', 'done')),
  done_amount  integer NOT NULL DEFAULT 0 CHECK (done_amount BETWEEN 0 AND 1000000),
  planned_on   date,
  completed_on date,
  note         text NOT NULL DEFAULT '' CHECK (length(note) <= 20000),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (unit_id, user_id)
);
CREATE INDEX unit_progress_user ON unit_progress (user_id);

-- 스터디 항목인지 확인하고 항목 권한을 돌려준다(없거나 스터디가 아니면 404).
CREATE FUNCTION app_study_access(p_item uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_item_access(p_item);
  BEGIN
    IF v_access IS NULL OR NOT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = p_item AND m.kind = 'study') THEN
      RAISE EXCEPTION 'study not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_access;
  END
  $$;

CREATE FUNCTION app_require_study_write(p_item uuid) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_study_access(p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
  END
  $$;

-- 단위 여러 개 추가. 번호가 없으면 마지막 다음. 같은 요청 id는 한 번만.
-- p_rows: [{request_id, number?, title, chapter?, unit?, amount?, planned_on?}]
CREATE FUNCTION app_add_study_units(p_item uuid, p_rows jsonb) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
  BEGIN
    PERFORM app_require_study_write(p_item);
    IF jsonb_typeof(p_rows) <> 'array' OR jsonb_array_length(p_rows) NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'give 1 to 1000 units' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR NO KEY UPDATE;
    SELECT coalesce(max(number), 0) + 1 INTO v_next FROM study_units WHERE item_id = p_item;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_rows) LOOP
      INSERT INTO study_units (item_id, workspace_id, number, title, chapter, unit, amount, planned_on, created_by,
                               request_id)
      VALUES (p_item, v_ws, coalesce((v_row->>'number')::integer, v_next), btrim(v_row->>'title'),
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
      PERFORM app_audit(v_ws, 'add_study_units', 'item', p_item, jsonb_build_object('count', v_added));
    END IF;
    RETURN v_added;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow
         OR numeric_value_out_of_range OR check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE FUNCTION app_update_study_unit(
  p_unit uuid, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_item);
    UPDATE study_units SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      unit = p_kind, amount = p_amount, planned_on = p_planned, updated_at = now()
    WHERE id = p_unit;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE FUNCTION app_delete_study_unit(p_unit uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(v_unit.workspace_id, 'delete_study_unit', 'item', v_unit.item_id,
                      jsonb_build_object('number', v_unit.number, 'title', v_unit.title));
  END
  $$;

-- 내 진행(스터디를 볼 수 있으면 누구나 자기 것). 완료한 날은 사용자의 오늘, 완료를 풀면 지운다.
CREATE FUNCTION app_set_unit_progress(
  p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item  uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
    v_today date := coalesce(p_today, current_date);
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_status NOT IN ('not_started', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF coalesce(p_done, 0) NOT BETWEEN 0 AND 1000000 OR length(coalesce(p_note, '')) > 20000 THEN
      RAISE EXCEPTION 'invalid progress' USING ERRCODE = '22023';
    END IF;
    IF abs(v_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
    INSERT INTO unit_progress (unit_id, user_id, status, done_amount, planned_on, completed_on, note, updated_at)
    VALUES (p_unit, app_user_id(), p_status, coalesce(p_done, 0), p_planned,
            CASE WHEN p_status = 'done' THEN v_today END, coalesce(p_note, ''), now())
    ON CONFLICT (unit_id, user_id) DO UPDATE SET
      status = EXCLUDED.status,
      done_amount = EXCLUDED.done_amount,
      planned_on = EXCLUDED.planned_on,
      completed_on = CASE WHEN EXCLUDED.status <> 'done' THEN NULL
                          WHEN unit_progress.status = 'done' THEN unit_progress.completed_on
                          ELSE EXCLUDED.completed_on END,
      note = EXCLUDED.note,
      updated_at = now();
  END
  $$;

-- 내 계획일 여러 개를 한 번에(밀린 단위 재배치). p_entries: [{unit_id, planned_on}]. 진행 상태는 그대로.
CREATE FUNCTION app_plan_units(p_item uuid, p_entries jsonb) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row   jsonb;
    v_count integer := 0;
  BEGIN
    PERFORM app_study_access(p_item);
    IF jsonb_typeof(p_entries) <> 'array' OR jsonb_array_length(p_entries) > 1000 THEN
      RAISE EXCEPTION 'give at most 1000 entries' USING ERRCODE = '22023';
    END IF;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_entries) LOOP
      IF NOT EXISTS (SELECT 1 FROM study_units WHERE id = (v_row->>'unit_id')::uuid AND item_id = p_item) THEN
        RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
      END IF;
      INSERT INTO unit_progress (unit_id, user_id, planned_on)
      VALUES ((v_row->>'unit_id')::uuid, app_user_id(), (v_row->>'planned_on')::date)
      ON CONFLICT (unit_id, user_id) DO UPDATE SET planned_on = EXCLUDED.planned_on, updated_at = now();
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow THEN
      RAISE EXCEPTION 'invalid plan: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- 함께 하는 사람의 진행 집계(스터디를 볼 수 있는 사람). 메모·개인 날짜는 넣지 않는다.
CREATE FUNCTION app_study_participants(p_item uuid, p_today date)
  RETURNS TABLE (user_id uuid, display_name text, done integer, in_progress integer, overdue integer,
                 done_amount bigint)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_study_access(p_item);
    RETURN QUERY
      SELECT u.id, u.display_name,
             count(*) FILTER (WHERE p.status = 'done')::integer,
             count(*) FILTER (WHERE p.status = 'in_progress')::integer,
             (SELECT count(*)::integer FROM study_units s2
              LEFT JOIN unit_progress p2 ON p2.unit_id = s2.id AND p2.user_id = u.id
              WHERE s2.item_id = p_item AND coalesce(p2.status, 'not_started') <> 'done'
                AND coalesce(p2.planned_on, s2.planned_on) < p_today),
             coalesce(sum(p.done_amount), 0)::bigint
      FROM unit_progress p
      JOIN study_units s ON s.id = p.unit_id AND s.item_id = p_item
      JOIN users u ON u.id = p.user_id
      WHERE app_item_access_as(p_item, u.id) IS NOT NULL
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;

ALTER TABLE study_units ENABLE ROW LEVEL SECURITY;
CREATE POLICY study_units_select ON study_units FOR SELECT USING (app_item_access(item_id) IS NOT NULL);
ALTER TABLE unit_progress ENABLE ROW LEVEL SECURITY;
CREATE POLICY unit_progress_select ON unit_progress FOR SELECT USING (user_id = app_user_id());
GRANT SELECT ON study_units, unit_progress TO fvoci_app;

REVOKE ALL ON FUNCTION app_study_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_require_study_write(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_study_units(uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_study_unit(uuid, integer, text, text, text, integer, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_study_unit(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_unit_progress(uuid, text, integer, date, text, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_plan_units(uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_study_participants(uuid, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_study_access(uuid),
  app_add_study_units(uuid, jsonb),
  app_update_study_unit(uuid, integer, text, text, text, integer, date),
  app_delete_study_unit(uuid),
  app_set_unit_progress(uuid, text, integer, date, text, date),
  app_plan_units(uuid, jsonb),
  app_study_participants(uuid, date)
TO fvoci_app;
