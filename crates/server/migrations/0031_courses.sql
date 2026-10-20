-- 강의 모듈: 강좌 정보·회차·개인별 진도 (D-37, D-32, D-39 CSV 일괄 등록).
--
-- 강좌는 강의 모듈의 항목이고 그 권한을 따른다. 강좌 정보와 목차(회차·길이·자료·공통 계획일)는 항목을 볼 수
-- 있는 사람이 함께 보고, 쓸 수 있는 사람이 고친다. 진도는 사람마다 따로다: 상태(미수강·수강 중·완료), 내 계획일
-- (공통 계획일을 덮어씀), 마지막 시청 위치, 다시 본 횟수, 개인 메모. 완료는 사람이 직접 표시하고, 다시 보아 완료해도
-- 완료 회차 수는 한 번만 센다. 함께 수강하는 사람의 진도는 집계(완료·수강 중·밀린 회차 수와 영상 길이)만
-- 보이고, 개인 메모·마지막 위치·시간 기록은 보이지 않는다. 회차를 시간 기록의 대상으로 고를 수 있다.

CREATE TABLE course_details (
  item_id      uuid PRIMARY KEY,
  workspace_id uuid NOT NULL,
  subject      text NOT NULL DEFAULT '',
  instructor   text NOT NULL DEFAULT '',
  platform     text NOT NULL DEFAULT '',
  url          text NOT NULL DEFAULT '',
  textbook     text NOT NULL DEFAULT '',
  starts_on    date,
  expires_on   date,
  target_on    date,
  updated_by   uuid REFERENCES users (id),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT course_details_item FOREIGN KEY (item_id, workspace_id) REFERENCES items (id, workspace_id)
);

CREATE TABLE course_episodes (
  id               uuid PRIMARY KEY,
  item_id          uuid NOT NULL,
  workspace_id     uuid NOT NULL,
  number           integer NOT NULL CHECK (number > 0),
  title            text NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 300),
  chapter          text NOT NULL DEFAULT '',
  duration_seconds integer NOT NULL DEFAULT 0 CHECK (duration_seconds BETWEEN 0 AND 360000),
  url              text NOT NULL DEFAULT '',
  materials        text NOT NULL DEFAULT '',
  planned_on       date,
  created_by       uuid NOT NULL REFERENCES users (id),
  created_at       timestamptz NOT NULL DEFAULT now(),
  updated_at       timestamptz NOT NULL DEFAULT now(),
  request_id       uuid NOT NULL,
  CONSTRAINT course_episodes_item FOREIGN KEY (item_id, workspace_id) REFERENCES items (id, workspace_id),
  CONSTRAINT course_episodes_request UNIQUE (item_id, created_by, request_id)
);
CREATE INDEX course_episodes_item ON course_episodes (item_id, number);

CREATE TABLE episode_progress (
  episode_id            uuid NOT NULL REFERENCES course_episodes (id) ON DELETE CASCADE,
  user_id               uuid NOT NULL REFERENCES users (id),
  status                text NOT NULL DEFAULT 'not_started' CHECK (status IN ('not_started', 'in_progress', 'done')),
  planned_on            date,
  completed_on          date,
  watch_count           integer NOT NULL DEFAULT 0 CHECK (watch_count >= 0),
  last_position_seconds integer CHECK (last_position_seconds >= 0),
  note                  text NOT NULL DEFAULT '',
  updated_at            timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (episode_id, user_id)
);
CREATE INDEX episode_progress_user ON episode_progress (user_id);

ALTER TABLE time_entries DROP CONSTRAINT time_entries_target_kind_check;
ALTER TABLE time_entries ADD CONSTRAINT time_entries_target_kind_check
  CHECK (target_kind IN ('task', 'document', 'item', 'episode'));

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

-- 강좌 항목인지 확인하고 항목 권한을 돌려준다(없거나 강좌가 아니면 404).
CREATE FUNCTION app_course_access(p_item uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_item_access(p_item);
  BEGIN
    IF v_access IS NULL OR NOT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = p_item AND m.kind = 'course') THEN
      RAISE EXCEPTION 'course not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_access;
  END
  $$;

-- 강좌를 고칠 수 있는지(쓰기 권한, 보관되지 않음).
CREATE FUNCTION app_require_course_write(p_item uuid) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_course_access(p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
  END
  $$;

------------------------------------------------------------------------------
-- 강좌 정보와 회차
------------------------------------------------------------------------------

CREATE FUNCTION app_set_course_details(
  p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text,
  p_starts date, p_expires date, p_target date) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_require_course_write(p_item);
    IF length(coalesce(p_url, '')) > 2000 OR length(coalesce(p_subject, '') || coalesce(p_instructor, '')
       || coalesce(p_platform, '') || coalesce(p_textbook, '')) > 2000 THEN
      RAISE EXCEPTION 'course details are too long' USING ERRCODE = '22023';
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
    PERFORM app_audit((SELECT workspace_id FROM items WHERE id = p_item), 'update_course', 'item', p_item,
                      '{}'::jsonb);
  END
  $$;

-- 회차 여러 개 추가(직접 입력·CSV 일괄 등록). 번호가 없으면 마지막 다음. 같은 요청 id는 한 번만.
-- p_rows: [{request_id, number?, title, chapter?, duration_seconds?, url?, materials?, planned_on?}]
CREATE FUNCTION app_add_episodes(p_item uuid, p_rows jsonb) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
  BEGIN
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
      INSERT INTO course_episodes (item_id, workspace_id, number, title, chapter, duration_seconds, url, materials,
                                   planned_on, created_by, request_id)
      VALUES (p_item, v_ws, coalesce((v_row->>'number')::integer, v_next), btrim(v_row->>'title'),
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
      PERFORM app_audit(v_ws, 'add_episodes', 'item', p_item, jsonb_build_object('count', v_added));
    END IF;
    RETURN v_added;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow
         OR numeric_value_out_of_range OR check_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE FUNCTION app_update_episode(
  p_episode uuid, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text,
  p_materials text, p_planned date) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_item);
    UPDATE course_episodes SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      duration_seconds = p_duration, url = btrim(coalesce(p_url, '')), materials = coalesce(p_materials, ''),
      planned_on = p_planned, updated_at = now()
    WHERE id = p_episode;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- 회차 지우기(쓸 수 있는 사람). 그 회차의 진도도 함께 지워진다. 시간 기록은 남고 대상 이름만 사라진다.
CREATE FUNCTION app_delete_episode(p_episode uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ep course_episodes%ROWTYPE;
  BEGIN
    SELECT * INTO v_ep FROM course_episodes WHERE id = p_episode;
    IF v_ep.id IS NULL OR app_item_access(v_ep.item_id) IS NULL THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_ep.item_id);
    DELETE FROM course_episodes WHERE id = p_episode;
    PERFORM app_audit(v_ep.workspace_id, 'delete_episode', 'item', v_ep.item_id,
                      jsonb_build_object('number', v_ep.number, 'title', v_ep.title));
  END
  $$;

------------------------------------------------------------------------------
-- 내 진도
------------------------------------------------------------------------------

-- 내 진도 바꾸기(강좌를 볼 수 있으면 누구나 자기 진도). 완료로 바꿀 때마다 본 횟수가 늘고, 처음 완료한 날을 남긴다.
CREATE FUNCTION app_set_episode_progress(
  p_episode uuid, p_status text, p_planned date, p_position integer, p_note text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_old  episode_progress%ROWTYPE;
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_status NOT IN ('not_started', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF length(coalesce(p_note, '')) > 20000 OR p_position < 0 THEN
      RAISE EXCEPTION 'invalid progress' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO v_old FROM episode_progress WHERE episode_id = p_episode AND user_id = app_user_id() FOR UPDATE;
    INSERT INTO episode_progress (episode_id, user_id, status, planned_on, completed_on, watch_count,
                                  last_position_seconds, note, updated_at)
    VALUES (p_episode, app_user_id(), p_status, p_planned,
            CASE WHEN p_status = 'done' THEN current_date END,
            CASE WHEN p_status = 'done' THEN 1 ELSE 0 END, p_position, coalesce(p_note, ''), now())
    ON CONFLICT (episode_id, user_id) DO UPDATE SET
      status = EXCLUDED.status,
      planned_on = EXCLUDED.planned_on,
      completed_on = coalesce(episode_progress.completed_on, EXCLUDED.completed_on),
      watch_count = episode_progress.watch_count
        + CASE WHEN EXCLUDED.status = 'done' AND episode_progress.status <> 'done' THEN 1 ELSE 0 END,
      last_position_seconds = EXCLUDED.last_position_seconds,
      note = EXCLUDED.note,
      updated_at = now();
  END
  $$;

-- 함께 수강하는 사람들의 진도 집계(강좌를 볼 수 있는 사람). 개인 메모·위치·시간은 넣지 않는다.
-- 진도를 한 번이라도 남긴 사람만 나온다.
CREATE FUNCTION app_course_participants(p_item uuid, p_today date)
  RETURNS TABLE (user_id uuid, display_name text, done integer, in_progress integer, overdue integer,
                 done_seconds bigint)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_course_access(p_item);
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
      -- 지금 이 강좌를 볼 수 있는 멤버만(나간 사람은 빠진다).
      WHERE EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = e.workspace_id AND m.user_id = u.id)
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;

------------------------------------------------------------------------------
-- 시간 기록 대상에 회차 더하기
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_time_target_workspace(p_kind text, p_id uuid) RETURNS uuid
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_kind IS NULL THEN
      RETURN NULL;
    END IF;
    IF p_kind = 'task' AND app_task_access(p_id) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM tasks WHERE id = p_id);
    ELSIF p_kind = 'document' AND app_document_access(p_id) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM documents WHERE id = p_id);
    ELSIF p_kind = 'item' AND app_item_access(p_id) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM items WHERE id = p_id);
    ELSIF p_kind = 'episode'
          AND app_item_access((SELECT item_id FROM course_episodes WHERE id = p_id)) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM course_episodes WHERE id = p_id);
    END IF;
    RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
  END
  $$;

CREATE OR REPLACE FUNCTION app_time_target_label(p_kind text, p_id uuid) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE
      WHEN p_kind = 'task' AND app_task_access(p_id) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_task_reference(id), title) FROM tasks WHERE id = p_id)
      WHEN p_kind = 'document' AND app_document_access(p_id) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_document_reference(id), title) FROM documents WHERE id = p_id)
      WHEN p_kind = 'item' AND app_item_access(p_id) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_item_reference(id), title) FROM items WHERE id = p_id)
      WHEN p_kind = 'episode'
           AND app_item_access((SELECT item_id FROM course_episodes WHERE id = p_id)) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_item_reference(e.item_id), e.number || '회', e.title)
         FROM course_episodes e WHERE e.id = p_id)
    END
  $$;

------------------------------------------------------------------------------
-- 영구 삭제: 항목을 지우면 강좌 정보·회차·진도도
------------------------------------------------------------------------------

CREATE FUNCTION app_delete_item_course_rows() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    DELETE FROM course_episodes WHERE item_id = OLD.id;
    DELETE FROM course_details WHERE item_id = OLD.id;
    RETURN OLD;
  END
  $$;
CREATE TRIGGER items_course BEFORE DELETE ON items
  FOR EACH ROW EXECUTE FUNCTION app_delete_item_course_rows();

------------------------------------------------------------------------------
-- 행 보안·권한
------------------------------------------------------------------------------

ALTER TABLE course_details ENABLE ROW LEVEL SECURITY;
CREATE POLICY course_details_select ON course_details FOR SELECT USING (app_item_access(item_id) IS NOT NULL);
ALTER TABLE course_episodes ENABLE ROW LEVEL SECURITY;
CREATE POLICY course_episodes_select ON course_episodes FOR SELECT USING (app_item_access(item_id) IS NOT NULL);
-- 진도 행은 본인만 본다(다른 사람의 진도는 집계 함수로만).
ALTER TABLE episode_progress ENABLE ROW LEVEL SECURITY;
CREATE POLICY episode_progress_select ON episode_progress FOR SELECT USING (user_id = app_user_id());

GRANT SELECT ON course_details, course_episodes, episode_progress TO fvoci_app;

REVOKE ALL ON FUNCTION app_course_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_require_course_write(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_course_details(uuid, text, text, text, text, text, date, date, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_episodes(uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_episode(uuid, integer, text, text, integer, text, text, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_episode(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_episode_progress(uuid, text, date, integer, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_course_participants(uuid, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_item_course_rows() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_course_access(uuid),
  app_set_course_details(uuid, text, text, text, text, text, date, date, date),
  app_add_episodes(uuid, jsonb),
  app_update_episode(uuid, integer, text, text, integer, text, text, date),
  app_delete_episode(uuid),
  app_set_episode_progress(uuid, text, date, integer, text),
  app_course_participants(uuid, date)
TO fvoci_app;
