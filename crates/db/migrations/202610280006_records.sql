-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

CREATE TABLE fvoci.course_details (
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    subject text DEFAULT ''::text NOT NULL,
    instructor text DEFAULT ''::text NOT NULL,
    platform text DEFAULT ''::text NOT NULL,
    url text DEFAULT ''::text NOT NULL,
    textbook text DEFAULT ''::text NOT NULL,
    starts_on date,
    expires_on date,
    target_on date,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT course_details_url CHECK (fvoci.app_web_url_ok(url))
);


ALTER TABLE fvoci.course_details OWNER TO fvoci_migrator;

CREATE TABLE fvoci.course_episodes (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    number integer NOT NULL,
    title text NOT NULL,
    chapter text DEFAULT ''::text NOT NULL,
    duration_seconds integer DEFAULT 0 NOT NULL,
    url text DEFAULT ''::text NOT NULL,
    materials text DEFAULT ''::text NOT NULL,
    planned_on date,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    CONSTRAINT course_episodes_duration_seconds_check CHECK (((duration_seconds >= 0) AND (duration_seconds <= 360000))),
    CONSTRAINT course_episodes_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT course_episodes_number_check CHECK ((number > 0)),
    CONSTRAINT course_episodes_title_check CHECK (((length(btrim(title)) >= 1) AND (length(btrim(title)) <= 300))),
    CONSTRAINT course_episodes_url CHECK (fvoci.app_web_url_ok(url)),
    CONSTRAINT course_episodes_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.course_episodes OWNER TO fvoci_migrator;

CREATE TABLE fvoci.episode_progress (
    episode_id uuid NOT NULL,
    user_id uuid NOT NULL,
    status text DEFAULT 'not_started'::text NOT NULL,
    planned_on date,
    completed_on date,
    watch_count integer DEFAULT 0 NOT NULL,
    last_position_seconds integer,
    note text DEFAULT ''::text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT episode_progress_last_position_seconds_check CHECK ((last_position_seconds >= 0)),
    CONSTRAINT episode_progress_status_check CHECK ((status = ANY (ARRAY['not_started'::text, 'in_progress'::text, 'done'::text]))),
    CONSTRAINT episode_progress_watch_count_check CHECK ((watch_count >= 0))
);


ALTER TABLE fvoci.episode_progress OWNER TO fvoci_migrator;

CREATE TABLE fvoci.item_records (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    kind text NOT NULL,
    title text NOT NULL,
    occurred_on date,
    body text DEFAULT ''::text NOT NULL,
    fields jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    request_id uuid NOT NULL,
    CONSTRAINT item_records_body_check CHECK ((length(body) <= 20000)),
    CONSTRAINT item_records_fields_check CHECK ((jsonb_typeof(fields) = 'object'::text)),
    CONSTRAINT item_records_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT item_records_kind_check CHECK ((kind ~ '^[a-z_]{2,32}$'::text)),
    CONSTRAINT item_records_title_check CHECK (((length(btrim(title)) >= 1) AND (length(btrim(title)) <= 300)))
);


ALTER TABLE fvoci.item_records OWNER TO fvoci_migrator;

CREATE TABLE fvoci.record_files (
    record_id uuid NOT NULL,
    "position" integer NOT NULL,
    role text NOT NULL,
    file_id uuid,
    version_id uuid,
    file_name text NOT NULL,
    version integer NOT NULL,
    sha256 text NOT NULL,
    size bigint NOT NULL,
    CONSTRAINT record_files_position_check CHECK (("position" >= 0)),
    CONSTRAINT record_files_role_check CHECK ((role = ANY (ARRAY['input'::text, 'code'::text, 'result'::text, 'reference'::text, 'other'::text])))
);


ALTER TABLE fvoci.record_files OWNER TO fvoci_migrator;

CREATE TABLE fvoci.study_units (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    number integer NOT NULL,
    title text NOT NULL,
    chapter text DEFAULT ''::text NOT NULL,
    unit text DEFAULT 'pages'::text NOT NULL,
    amount integer DEFAULT 0 NOT NULL,
    planned_on date,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    CONSTRAINT study_units_amount_check CHECK (((amount >= 0) AND (amount <= 1000000))),
    CONSTRAINT study_units_chapter_check CHECK ((length(chapter) <= 300)),
    CONSTRAINT study_units_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT study_units_number_check CHECK ((number > 0)),
    CONSTRAINT study_units_title_check CHECK (((length(btrim(title)) >= 1) AND (length(btrim(title)) <= 300))),
    CONSTRAINT study_units_unit_check CHECK ((unit = ANY (ARRAY['pages'::text, 'problems'::text, 'minutes'::text, 'items'::text]))),
    CONSTRAINT study_units_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.study_units OWNER TO fvoci_migrator;

CREATE TABLE fvoci.unit_progress (
    unit_id uuid NOT NULL,
    user_id uuid NOT NULL,
    status text DEFAULT 'not_started'::text NOT NULL,
    done_amount integer DEFAULT 0 NOT NULL,
    planned_on date,
    completed_on date,
    note text DEFAULT ''::text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT unit_progress_done_amount_check CHECK (((done_amount >= 0) AND (done_amount <= 1000000))),
    CONSTRAINT unit_progress_note_check CHECK ((length(note) <= 20000)),
    CONSTRAINT unit_progress_status_check CHECK ((status = ANY (ARRAY['not_started'::text, 'in_progress'::text, 'done'::text])))
);


ALTER TABLE fvoci.unit_progress OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
    v_at    integer := 0;
  BEGIN
    PERFORM app_require_course_write(p_item);
    -- 항목을 잠근 뒤 다시 본다(기다리는 사이 공개 범위·보관이 바뀌었을 수 있다).
    PERFORM app_lock_item(p_item);
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


ALTER FUNCTION fvoci.app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
    v_at    integer := 0;
  BEGIN
    PERFORM app_require_study_write(p_item);
    -- 항목을 잠근 뒤 다시 본다(기다리는 사이 공개 범위·보관이 바뀌었을 수 있다).
    PERFORM app_lock_item(p_item);
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


ALTER FUNCTION fvoci.app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_course_access(p_item uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
  BEGIN
    IF v_access = 'none' OR NOT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = p_item AND m.kind = 'course') THEN
      RAISE EXCEPTION 'course not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_course_access(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_course_participants(p_item uuid, p_today date) RETURNS TABLE(user_id uuid, display_name text, done integer, in_progress integer, overdue integer, done_seconds bigint)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_course_access(p_item);
    IF p_today IS NULL OR abs(p_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
    RETURN QUERY
      SELECT u.id, u.display_name,
             count(*) FILTER (WHERE p.status = 'done')::integer,
             count(*) FILTER (WHERE p.status = 'in_progress')::integer,
             (SELECT count(*)::integer FROM course_episodes e2
              LEFT JOIN episode_progress p2 ON p2.episode_id = e2.id AND p2.user_id = u.id
              WHERE e2.item_id = p_item AND coalesce(p2.status, 'not_started') <> 'done'
                AND coalesce(p2.planned_on, e2.planned_on) < p_today),
             coalesce(sum(e.duration_seconds) FILTER (WHERE p.status = 'done'), 0)::bigint
      FROM episode_progress p
      JOIN course_episodes e ON e.id = p.episode_id AND e.item_id = p_item
      JOIN users u ON u.id = p.user_id
      -- 지금 이 강좌를 볼 수 있는 사람만(나간 멤버, 비공개로 바뀐 항목을 볼 수 없는 멤버는 빠진다).
      WHERE app_access_as('item', p_item, u.id) <> 'none'
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;


ALTER FUNCTION fvoci.app_course_participants(p_item uuid, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_episode(p_episode uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ep course_episodes%ROWTYPE;
  BEGIN
    SELECT * INTO v_ep FROM course_episodes WHERE id = p_episode;
    IF v_ep.id IS NULL OR app_access('item', v_ep.item_id) = 'none' THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_ep.item_id);
    DELETE FROM course_episodes WHERE id = p_episode;
    PERFORM app_audit(app_id(p_ids, 1), v_ep.workspace_id, 'delete_episode', 'item', v_ep.item_id,
                      jsonb_build_object('number', v_ep.number, 'title', v_ep.title));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_episode(p_episode uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_record(p_record uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_rec    item_records%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    v_access := app_access('item', v_rec.item_id);
    IF v_rec.id IS NULL OR v_access = 'none' THEN
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


ALTER FUNCTION fvoci.app_delete_record(p_record uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_study_unit(p_unit uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_unit study_units%ROWTYPE;
  BEGIN
    SELECT * INTO v_unit FROM study_units WHERE id = p_unit;
    IF v_unit.id IS NULL OR app_access('item', v_unit.item_id) = 'none' THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_unit.item_id);
    DELETE FROM study_units WHERE id = p_unit;
    PERFORM app_audit(app_id(p_ids, 1), v_unit.workspace_id, 'delete_study_unit', 'item', v_unit.item_id,
                      jsonb_build_object('number', v_unit.number, 'title', v_unit.title));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_study_unit(p_unit uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_plan_units(p_item uuid, p_entries jsonb) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row   jsonb;
    v_count integer := 0;
  BEGIN
    PERFORM app_study_access(p_item);
    IF app_access('item', p_item) = 'read' THEN
      RAISE EXCEPTION 'read-only members cannot record progress' USING ERRCODE = '42501';
    END IF;
    -- D-113(개정 ⑯)·D-20(⑧): 본인 진도도 쓰기라 보관된 항목·모듈 아래에서는 남기지 못한다. 보관과 줄 세우려 잠근 뒤 본다.
    PERFORM app_lock_item(p_item);
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
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


ALTER FUNCTION fvoci.app_plan_units(p_item uuid, p_entries jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_record_write_check(p_item uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
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
  END
  $$;


ALTER FUNCTION fvoci.app_record_write_check(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_episode_progress(p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item  uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_today date := coalesce(p_today, current_date);
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    -- 개정 2026-10-26(⑧, D-20): 읽기 전용은 본인 진도도 남기지 못한다.
    IF app_access('item', v_item) = 'read' THEN
      RAISE EXCEPTION 'read-only members cannot record progress' USING ERRCODE = '42501';
    END IF;
    -- D-113(개정 ⑯)·D-20(⑧): 본인 진도도 쓰기라 보관된 항목·모듈 아래에서는 남기지 못한다. 보관과 줄 세우려 잠근 뒤 본다.
    PERFORM app_lock_item(v_item);
    IF app_item_archived(v_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_status NOT IN ('not_started', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF length(coalesce(p_note, '')) > 20000 OR p_position < 0 THEN
      RAISE EXCEPTION 'invalid progress' USING ERRCODE = '22023';
    END IF;
    IF abs(v_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
    INSERT INTO episode_progress (episode_id, user_id, status, planned_on, completed_on, watch_count,
                                  last_position_seconds, note, updated_at)
    VALUES (p_episode, app_user_id(), p_status, p_planned,
            CASE WHEN p_status = 'done' THEN v_today END,
            CASE WHEN p_status = 'done' THEN 1 ELSE 0 END, p_position, coalesce(p_note, ''), now())
    ON CONFLICT (episode_id, user_id) DO UPDATE SET
      status = EXCLUDED.status,
      planned_on = EXCLUDED.planned_on,
      -- 이미 완료였으면 그날을 지키고, 완료를 풀면 지운다.
      completed_on = CASE WHEN EXCLUDED.status <> 'done' THEN NULL
                          WHEN episode_progress.status = 'done' THEN episode_progress.completed_on
                          ELSE EXCLUDED.completed_on END,
      watch_count = episode_progress.watch_count
        + CASE WHEN EXCLUDED.status = 'done' AND episode_progress.status <> 'done' THEN 1 ELSE 0 END,
      last_position_seconds = EXCLUDED.last_position_seconds,
      note = EXCLUDED.note,
      updated_at = now();
  END
  $$;


ALTER FUNCTION fvoci.app_set_episode_progress(p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row  jsonb;
    v_pos  integer := 0;
    v_file files%ROWTYPE;
    v_ver  file_versions%ROWTYPE;
    v_had  record_files%ROWTYPE;
    v_before record_files[];
    v_used integer[] := '{}';
  BEGIN
    IF jsonb_typeof(coalesce(p_files, '[]'::jsonb)) <> 'array' OR jsonb_array_length(coalesce(p_files, '[]')) > 50 THEN
      RAISE EXCEPTION 'give at most 50 linked files' USING ERRCODE = '22023';
    END IF;
    -- 지난 연결을 기억해 두고, 다시 보낸 것은 그대로 쓰고, 파일이 영구 삭제된 연결은 끝에 그대로 둔다.
    SELECT coalesce(array_agg(rf ORDER BY rf.position), '{}') INTO v_before
    FROM record_files rf WHERE rf.record_id = p_record;
    DELETE FROM record_files WHERE record_id = p_record;
    FOR v_row IN SELECT * FROM jsonb_array_elements(coalesce(p_files, '[]'::jsonb)) LOOP
      v_had := NULL;
      SELECT * INTO v_had FROM unnest(v_before) b
      WHERE b.file_id = (v_row->>'file_id')::uuid AND b.version = (v_row->>'version')::integer
        AND b.position <> ALL (v_used)
      ORDER BY b.position LIMIT 1;
      IF v_had.record_id IS NOT NULL THEN
        -- 이미 연결된 파일·버전: 파일이 휴지통에 있어도 연결을 그대로 쓴다(되살릴 수 있고, 연결이 지킨다, D-56).
        INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
        VALUES (p_record, v_pos, coalesce(v_row->>'role', v_had.role), v_had.file_id, v_had.version_id,
                v_had.file_name, v_had.version, v_had.sha256, v_had.size);
        v_used := v_used || v_had.position;
        v_pos := v_pos + 1;
        CONTINUE;
      END IF;
      SELECT * INTO v_file FROM files
      WHERE id = (v_row->>'file_id')::uuid AND item_id = p_item AND deleted_at IS NULL FOR KEY SHARE;
      IF v_file.id IS NULL THEN
        RAISE EXCEPTION 'linked files must be attachments of this item' USING ERRCODE = '22023';
      END IF;
      SELECT * INTO v_ver FROM file_versions
      WHERE file_id = v_file.id AND version = coalesce((v_row->>'version')::integer, v_file.current_version)
      FOR KEY SHARE;
      IF v_ver.id IS NULL THEN
        RAISE EXCEPTION 'file version not found' USING ERRCODE = '22023';
      END IF;
      INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
      VALUES (p_record, v_pos, coalesce(v_row->>'role', 'other'), v_file.id, v_ver.id, v_file.name, v_ver.version,
              v_ver.sha256, v_ver.size);
      v_pos := v_pos + 1;
    END LOOP;
    -- 파일이 영구 삭제되어 이름·버전·SHA-256만 남은 연결은 화면이 다시 보낼 수 없으므로 그대로 둔다(D-35).
    INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
    SELECT p_record, v_pos + row_number() OVER (ORDER BY b.position) - 1, b.role, NULL, NULL,
           b.file_name, b.version, b.sha256, b.size
    FROM unnest(v_before) b WHERE b.file_id IS NULL;
  EXCEPTION
    WHEN invalid_text_representation OR check_violation OR numeric_value_out_of_range THEN
      RAISE EXCEPTION 'invalid linked file: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_unit_progress(p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item  uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
    v_today date := coalesce(p_today, current_date);
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    -- 개정 2026-10-26(⑧, D-20): 읽기 전용은 본인 진도도 남기지 못한다.
    IF app_access('item', v_item) = 'read' THEN
      RAISE EXCEPTION 'read-only members cannot record progress' USING ERRCODE = '42501';
    END IF;
    -- D-113(개정 ⑯)·D-20(⑧): 본인 진도도 쓰기라 보관된 항목·모듈 아래에서는 남기지 못한다. 보관과 줄 세우려 잠근 뒤 본다.
    PERFORM app_lock_item(v_item);
    IF app_item_archived(v_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
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


ALTER FUNCTION fvoci.app_set_unit_progress(p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_study_access(p_item uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
  BEGIN
    IF v_access = 'none' OR NOT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = p_item AND m.kind = 'study') THEN
      RAISE EXCEPTION 'study not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_study_access(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_study_participants(p_item uuid, p_today date) RETURNS TABLE(user_id uuid, display_name text, done integer, in_progress integer, overdue integer, done_amount bigint)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_study_access(p_item);
    IF p_today IS NULL OR abs(p_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
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
      WHERE app_access_as('item', p_item, u.id) <> 'none'
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;


ALTER FUNCTION fvoci.app_study_participants(p_item uuid, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_episode(p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text, p_materials text, p_planned date) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_version integer;
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_item);
    UPDATE course_episodes SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      duration_seconds = p_duration, url = btrim(coalesce(p_url, '')), materials = coalesce(p_materials, ''),
      planned_on = p_planned, version = version + 1, updated_at = now()
    WHERE id = p_episode AND version = p_version
    RETURNING version INTO v_version;
    IF v_version IS NULL THEN
      RAISE EXCEPTION 'the episode changed; reload it' USING ERRCODE = '55000';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_episode(p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text, p_materials text, p_planned date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_record(p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_rec item_records%ROWTYPE;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    IF v_rec.id IS NULL OR app_access('item', v_rec.item_id) = 'none' THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.version <> p_version THEN
      RAISE EXCEPTION 'the record changed; reload it' USING ERRCODE = '55000';
    END IF;
    UPDATE item_records SET title = btrim(p_title), occurred_on = p_occurred, body = coalesce(p_body, ''),
                            fields = coalesce(p_fields, '{}'), updated_by = app_user_id(), updated_at = now(),
                            version = version + 1
    WHERE id = p_record;
    PERFORM app_set_record_files(p_record, v_rec.item_id, p_files);
    RETURN v_rec.version + 1;
  EXCEPTION
    WHEN check_violation THEN
      RAISE EXCEPTION 'invalid record: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_record(p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_study_unit(p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
    v_version integer;
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_item);
    UPDATE study_units SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      unit = p_kind, amount = p_amount, planned_on = p_planned, version = version + 1, updated_at = now()
    WHERE id = p_unit AND version = p_version
    RETURNING version INTO v_version;
    IF v_version IS NULL THEN
      RAISE EXCEPTION 'the unit changed; reload it' USING ERRCODE = '55000';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_study_unit(p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date) OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.course_details
    ADD CONSTRAINT course_details_pkey PRIMARY KEY (item_id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.episode_progress
    ADD CONSTRAINT episode_progress_pkey PRIMARY KEY (episode_id, user_id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_pkey PRIMARY KEY (record_id, "position");

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.unit_progress
    ADD CONSTRAINT unit_progress_pkey PRIMARY KEY (unit_id, user_id);

CREATE INDEX course_episodes_item ON fvoci.course_episodes USING btree (item_id, number);

CREATE INDEX episode_progress_user ON fvoci.episode_progress USING btree (user_id);

CREATE INDEX item_records_item ON fvoci.item_records USING btree (item_id, kind, occurred_on);

CREATE INDEX record_files_version ON fvoci.record_files USING btree (version_id) WHERE (version_id IS NOT NULL);

CREATE INDEX study_units_item ON fvoci.study_units USING btree (item_id, number);

CREATE INDEX unit_progress_user ON fvoci.unit_progress USING btree (user_id);

CREATE TRIGGER item_records_search AFTER INSERT OR DELETE OR UPDATE OF title, body, fields, occurred_on ON fvoci.item_records FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('record');

ALTER TABLE ONLY fvoci.course_details
    ADD CONSTRAINT course_details_item FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.course_details
    ADD CONSTRAINT course_details_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_item FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.episode_progress
    ADD CONSTRAINT episode_progress_episode_id_fkey FOREIGN KEY (episode_id) REFERENCES fvoci.course_episodes(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.episode_progress
    ADD CONSTRAINT episode_progress_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_file_id_fkey FOREIGN KEY (file_id) REFERENCES fvoci.files(id) ON DELETE SET NULL;

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_record_id_fkey FOREIGN KEY (record_id) REFERENCES fvoci.item_records(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_version_id_fkey FOREIGN KEY (version_id) REFERENCES fvoci.file_versions(id) ON DELETE SET NULL;

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.unit_progress
    ADD CONSTRAINT unit_progress_unit_id_fkey FOREIGN KEY (unit_id) REFERENCES fvoci.study_units(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.unit_progress
    ADD CONSTRAINT unit_progress_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
