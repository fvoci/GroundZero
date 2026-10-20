-- 회의·발표·행사 일정 (D-31, D-50, D-57).
--
-- 일정은 항목에 속하고 권한은 항목을 따른다(보관 아래 읽기 전용, 볼 수 없으면 없는 것과 같다). 번호는 붙이지
-- 않는다(D-48의 번호열은 항목·업무·문서만). 시각이 있는 일정은 UTC 시각과 일정에 지정한 시간대를 함께 둔다
-- (표시는 보는 사람의 시간대, 반복 계산은 일정의 시간대, D-50). 종일 일정은 날짜 그대로 둔다.
-- 지우기는 표시만 하고 행은 남긴다(휴지통·복원·영구 삭제는 D-39 조각에서). 반복은 다음 조각.

CREATE TABLE events (
  id           uuid PRIMARY KEY,
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  item_id      uuid NOT NULL,
  kind         text NOT NULL CHECK (kind IN ('meeting', 'presentation', 'event')),
  title        text NOT NULL CHECK (length(btrim(title)) > 0),
  description  text NOT NULL DEFAULT '',
  all_day      boolean NOT NULL,
  start_date   date,
  end_date     date,
  starts_at    timestamptz,
  ends_at      timestamptz,
  timezone     text NOT NULL,
  version      integer NOT NULL DEFAULT 1 CHECK (version > 0),
  created_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_by   uuid NOT NULL REFERENCES users (id),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  deleted_at   timestamptz,
  deleted_by   uuid REFERENCES users (id),
  request_id   uuid NOT NULL,
  CONSTRAINT events_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES items (id, workspace_id),
  CONSTRAINT events_request UNIQUE (item_id, created_by, request_id),
  CONSTRAINT events_when CHECK (
    (all_day AND start_date IS NOT NULL AND end_date IS NOT NULL AND start_date <= end_date
       AND starts_at IS NULL AND ends_at IS NULL)
    OR (NOT all_day AND starts_at IS NOT NULL AND ends_at IS NOT NULL AND starts_at <= ends_at
       AND start_date IS NULL AND end_date IS NULL))
);
CREATE INDEX events_item ON events (item_id) WHERE deleted_at IS NULL;
CREATE INDEX events_workspace_dates ON events (workspace_id, start_date, end_date) WHERE deleted_at IS NULL;
CREATE INDEX events_workspace_times ON events (workspace_id, starts_at, ends_at) WHERE deleted_at IS NULL;

-- 일정 권한: 항목 권한, 보관 아래 읽기. 지운 일정은 없는 것과 같다.
CREATE FUNCTION app_event_access(p_event uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   uuid;
    v_access text;
  BEGIN
    SELECT item_id INTO v_item FROM events WHERE id = p_event AND deleted_at IS NULL;
    IF v_item IS NULL THEN
      RETURN NULL;
    END IF;
    v_access := app_item_access(v_item);
    IF v_access IS NOT NULL AND v_access <> 'read' AND app_item_archived(v_item) THEN
      RETURN 'read';
    END IF;
    RETURN v_access;
  END
  $$;

CREATE FUNCTION app_require_event_write(p_event uuid) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_event_access(p_event);
    v_item   uuid;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'event not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      SELECT item_id INTO v_item FROM events WHERE id = p_event;
      IF app_item_archived(v_item) AND app_item_access(v_item) IN ('owner', 'write') THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
  END
  $$;

-- 시간대 이름 확인(IANA). 모르는 이름은 400.
CREATE FUNCTION app_check_timezone(p_tz text) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_tz IS NULL OR NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = p_tz) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
  END
  $$;

CREATE FUNCTION app_create_event(
  p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean,
  p_start_date date, p_end_date date, p_starts_at timestamptz, p_ends_at timestamptz, p_timezone text
) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_access text := app_item_access(p_item);
    v_item   items%ROWTYPE;
    v_id     uuid;
  BEGIN
    IF v_access IS NULL THEN
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
    SELECT * INTO v_item FROM items WHERE id = p_item;
    INSERT INTO events (workspace_id, item_id, kind, title, description, all_day, start_date, end_date,
                        starts_at, ends_at, timezone, created_by, updated_by, request_id)
    VALUES (v_item.workspace_id, p_item, p_kind, btrim(p_title), coalesce(p_description, ''), p_all_day,
            p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone, v_user, v_user, p_request_id)
    RETURNING id INTO v_id;
    PERFORM app_audit(v_item.workspace_id, 'create', 'event', v_id, jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

-- 일정 고치기. 기준 버전이 지났으면 'conflict'(아무것도 바꾸지 않음).
CREATE FUNCTION app_update_event(
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
    IF (v_event.start_date, v_event.end_date, v_event.starts_at, v_event.ends_at)
       IS DISTINCT FROM (p_start_date, p_end_date, p_starts_at, p_ends_at) THEN
      PERFORM app_audit(v_event.workspace_id, 'reschedule', 'event', p_event, jsonb_build_object(
        'from', jsonb_build_array(coalesce(v_event.start_date::text, v_event.starts_at::text),
                                  coalesce(v_event.end_date::text, v_event.ends_at::text)),
        'to', jsonb_build_array(coalesce(p_start_date::text, p_starts_at::text),
                                coalesce(p_end_date::text, p_ends_at::text))));
    END IF;
    RETURN 'updated';
  END
  $$;

-- 일정 지우기(표시만). 이미 지웠으면 404.
CREATE FUNCTION app_delete_event(p_event uuid) RETURNS void
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
    UPDATE events SET deleted_at = now(), deleted_by = app_user_id() WHERE id = p_event;
    PERFORM app_audit(v_event.workspace_id, 'delete', 'event', p_event, jsonb_build_object('title', v_event.title));
  END
  $$;

ALTER TABLE events ENABLE ROW LEVEL SECURITY;
CREATE POLICY events_select ON events FOR SELECT USING (app_event_access(id) IS NOT NULL);
GRANT SELECT ON events TO fvoci_app;

REVOKE ALL ON FUNCTION app_event_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_require_event_write(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_check_timezone(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_event(uuid, uuid, text, text, text, boolean, date, date, timestamptz, timestamptz,
                                        text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_event(uuid, integer, text, text, text, boolean, date, date, timestamptz,
                                        timestamptz, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_event(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_event_access(uuid),
  app_check_timezone(text),
  app_create_event(uuid, uuid, text, text, text, boolean, date, date, timestamptz, timestamptz, text),
  app_update_event(uuid, integer, text, text, text, boolean, date, date, timestamptz, timestamptz, text),
  app_delete_event(uuid)
TO fvoci_app;
