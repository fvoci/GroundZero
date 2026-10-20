-- 스톱워치·시간 기록 (D-08, D-32, D-50).
--
-- 시간 기록은 사용자 소유다. 워크스페이스 관리자에게도 보이지 않으며(D-08 개인 시간 기록 비공개) 감사 기록
-- (워크스페이스 관리자가 보는 기록)에도 남기지 않는다. 고친 내용은 본인만 보는 수정 이력에 남긴다.
-- 사용자당 진행 중(측정 중 또는 일시정지)인 타이머는 모든 워크스페이스를 통틀어 하나다. 측정은 서버 시각으로
-- 하므로 앱을 닫아도 계속되고 다시 접속하면 그대로 이어 보인다.
-- 길이 = 끝 − 시작 − 멈춘 시간. 일시정지 중이면 paused_at부터 지금까지도 멈춘 시간이다.
-- 연결 대상(업무·문서·항목)은 측정 중에도 뒤에도 정할 수 있다(측정 후 분류). 연결할 때 그 대상을 볼 수
-- 있어야 하고, 나중에 볼 수 없게 되면 기록은 남되 대상 이름은 보이지 않는다.
-- 종류: work(작업)·study(순공부: 강의 시청 제외)·lecture(강의 시청)·rest(휴식)·other.
-- 공유: 사용자가 워크스페이스마다 고르면 그 워크스페이스 업무에 연결된 자기 시간의 합계만 함께 보인다
-- (기록·메모·시각은 공유하지 않는다, D-32 "선택한 집계만 공유").

CREATE TABLE time_entries (
  id             uuid PRIMARY KEY,
  user_id        uuid NOT NULL REFERENCES users (id),
  started_at     timestamptz NOT NULL,
  ended_at       timestamptz,
  paused_at      timestamptz,
  paused_seconds integer NOT NULL DEFAULT 0 CHECK (paused_seconds >= 0),
  kind           text NOT NULL DEFAULT 'work' CHECK (kind IN ('work', 'study', 'lecture', 'rest', 'other')),
  memo           text NOT NULL DEFAULT '',
  target_kind    text CHECK (target_kind IN ('task', 'document', 'item')),
  target_id      uuid,
  workspace_id   uuid REFERENCES workspaces (id),
  version        integer NOT NULL DEFAULT 1 CHECK (version > 0),
  request_id     uuid NOT NULL,
  created_at     timestamptz NOT NULL DEFAULT now(),
  updated_at     timestamptz NOT NULL DEFAULT now(),
  deleted_at     timestamptz,
  CONSTRAINT time_entries_target CHECK ((target_kind IS NULL) = (target_id IS NULL)),
  CONSTRAINT time_entries_order CHECK (ended_at IS NULL OR ended_at >= started_at),
  CONSTRAINT time_entries_pause CHECK (paused_at IS NULL OR ended_at IS NULL),
  CONSTRAINT time_entries_request UNIQUE (user_id, request_id)
);
-- 진행 중인 타이머는 사용자당 하나.
CREATE UNIQUE INDEX time_entries_one_active ON time_entries (user_id) WHERE ended_at IS NULL AND deleted_at IS NULL;
CREATE INDEX time_entries_user_started ON time_entries (user_id, started_at) WHERE deleted_at IS NULL;
CREATE INDEX time_entries_target ON time_entries (target_kind, target_id) WHERE deleted_at IS NULL;

-- 본인만 보는 수정 이력(고치기 전 값).
CREATE TABLE time_entry_revisions (
  id             uuid PRIMARY KEY,
  entry_id       uuid NOT NULL REFERENCES time_entries (id),
  user_id        uuid NOT NULL REFERENCES users (id),
  started_at     timestamptz NOT NULL,
  ended_at       timestamptz,
  paused_seconds integer NOT NULL,
  kind           text NOT NULL,
  memo           text NOT NULL,
  target_kind    text,
  target_id      uuid,
  changed_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX time_entry_revisions_entry ON time_entry_revisions (entry_id, changed_at);

-- 워크스페이스별 합계 공유 선택(있으면 공유).
CREATE TABLE time_sharing (
  user_id      uuid NOT NULL REFERENCES users (id),
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, workspace_id)
);

-- 연결 대상 확인: 지금 사용자가 볼 수 있어야 한다. 대상의 워크스페이스를 돌려준다(개인 노트는 NULL).
CREATE FUNCTION app_time_target_workspace(p_kind text, p_id uuid) RETURNS uuid
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
    END IF;
    RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
  END
  $$;

-- 대상 표시 이름: 지금 볼 수 있을 때만(아니면 NULL).
CREATE FUNCTION app_time_target_label(p_kind text, p_id uuid) RETURNS text
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
    END
  $$;

-- 본인 기록 행 잠금(없거나 지웠거나 남의 것이면 404).
CREATE FUNCTION app_lock_own_entry(p_entry uuid) RETURNS time_entries
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_entry time_entries%ROWTYPE;
  BEGIN
    SELECT * INTO v_entry FROM time_entries
    WHERE id = p_entry AND user_id = app_user_id() AND deleted_at IS NULL FOR UPDATE;
    IF v_entry.id IS NULL THEN
      RAISE EXCEPTION 'time entry not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_entry;
  END
  $$;

-- 타이머 시작. 이미 진행 중인 타이머가 있으면 409(사용자당 하나). 같은 요청 id로 다시 보내면 같은 기록.
CREATE FUNCTION app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid)
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
    IF EXISTS (SELECT 1 FROM time_entries WHERE user_id = v_user AND ended_at IS NULL AND deleted_at IS NULL) THEN
      RAISE EXCEPTION 'a timer is already running' USING ERRCODE = '55000';
    END IF;
    INSERT INTO time_entries (user_id, started_at, kind, memo, target_kind, target_id, workspace_id, request_id)
    VALUES (v_user, now(), p_kind, coalesce(p_memo, ''), p_target_kind, p_target, v_ws, p_request_id)
    RETURNING id INTO v_id;
    RETURN v_id;
  END
  $$;

-- 타이머 일시정지·재개·종료. 이미 그 상태면 그대로(재시도 안전).
CREATE FUNCTION app_timer_action(p_entry uuid, p_action text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
  BEGIN
    IF v_entry.ended_at IS NOT NULL THEN
      IF p_action = 'stop' THEN
        RETURN;
      END IF;
      RAISE EXCEPTION 'the timer has stopped' USING ERRCODE = '55000';
    END IF;
    IF p_action = 'pause' THEN
      IF v_entry.paused_at IS NULL THEN
        UPDATE time_entries SET paused_at = now(), version = version + 1, updated_at = now() WHERE id = p_entry;
      END IF;
    ELSIF p_action = 'resume' THEN
      IF v_entry.paused_at IS NOT NULL THEN
        UPDATE time_entries SET
          paused_seconds = paused_seconds + floor(extract(epoch FROM now() - paused_at))::integer,
          paused_at = NULL, version = version + 1, updated_at = now()
        WHERE id = p_entry;
      END IF;
    ELSIF p_action = 'stop' THEN
      UPDATE time_entries SET
        paused_seconds = paused_seconds
          + CASE WHEN paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM now() - paused_at))::integer END,
        paused_at = NULL, ended_at = now(), version = version + 1, updated_at = now()
      WHERE id = p_entry;
    ELSE
      RAISE EXCEPTION 'unknown timer action' USING ERRCODE = '22023';
    END IF;
  END
  $$;

-- 수동 기록(끝난 기록). 같은 요청 id면 같은 기록.
CREATE FUNCTION app_add_time_entry(
  p_request_id uuid, p_started timestamptz, p_ended timestamptz, p_paused integer, p_kind text, p_memo text,
  p_target_kind text, p_target uuid
) RETURNS uuid
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
    IF p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    INSERT INTO time_entries (user_id, started_at, ended_at, paused_seconds, kind, memo, target_kind, target_id,
                              workspace_id, request_id)
    VALUES (v_user, p_started, p_ended, p_paused, p_kind, coalesce(p_memo, ''), p_target_kind, p_target, v_ws,
            p_request_id)
    RETURNING id INTO v_id;
    RETURN v_id;
  END
  $$;

-- 기록 고치기(본인, 기준 버전). 고치기 전 값을 수정 이력에 남긴다. 진행 중이면 시작 시각·종류·메모·대상만.
CREATE FUNCTION app_update_time_entry(
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
      IF p_ended IS NOT NULL OR p_paused IS DISTINCT FROM v_entry.paused_seconds OR p_started > now() THEN
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

-- 기록 지우기(본인, 표시만).
CREATE FUNCTION app_delete_time_entry(p_entry uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
  BEGIN
    UPDATE time_entries SET deleted_at = now(), version = version + 1 WHERE id = v_entry.id;
  END
  $$;

-- 합계 공유 켜기·끄기(그 워크스페이스 멤버만).
CREATE FUNCTION app_set_time_sharing(p_workspace uuid, p_enabled boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_enabled THEN
      INSERT INTO time_sharing (user_id, workspace_id) VALUES (app_user_id(), p_workspace) ON CONFLICT DO NOTHING;
    ELSE
      DELETE FROM time_sharing WHERE user_id = app_user_id() AND workspace_id = p_workspace;
    END IF;
  END
  $$;

-- 기록 길이(초). 진행 중이면 지금까지.
CREATE FUNCTION app_entry_seconds(e time_entries) RETURNS bigint
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT greatest(0, floor(extract(epoch FROM coalesce(e.ended_at, now()) - e.started_at))::bigint
      - e.paused_seconds
      - CASE WHEN e.paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM now() - e.paused_at))::bigint END)
  $$;

-- 업무의 실제 시간: 내 시간과, 그 워크스페이스에 공유를 켠 지금 멤버들의 합계(나 포함, 사람 수).
-- 업무를 볼 수 있어야 한다. 기록 하나하나는 돌려주지 않는다.
CREATE FUNCTION app_task_time(p_task uuid)
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
        coalesce(sum(app_entry_seconds(e)) FILTER (WHERE e.user_id = app_user_id()), 0)::bigint,
        coalesce(sum(app_entry_seconds(e)) FILTER (WHERE s.user_id IS NOT NULL), 0)::bigint,
        count(DISTINCT e.user_id) FILTER (WHERE s.user_id IS NOT NULL)::integer
      FROM time_entries e
      LEFT JOIN time_sharing s ON s.user_id = e.user_id AND s.workspace_id = v_ws
        AND EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_ws AND m.user_id = e.user_id)
      WHERE e.target_kind = 'task' AND e.target_id = p_task AND e.deleted_at IS NULL;
  END
  $$;

ALTER TABLE time_entries ENABLE ROW LEVEL SECURITY;
CREATE POLICY time_entries_select ON time_entries FOR SELECT USING (user_id = app_user_id());
ALTER TABLE time_entry_revisions ENABLE ROW LEVEL SECURITY;
CREATE POLICY time_entry_revisions_select ON time_entry_revisions FOR SELECT USING (user_id = app_user_id());
ALTER TABLE time_sharing ENABLE ROW LEVEL SECURITY;
CREATE POLICY time_sharing_select ON time_sharing FOR SELECT USING (user_id = app_user_id());
GRANT SELECT ON time_entries, time_entry_revisions, time_sharing TO fvoci_app;

REVOKE ALL ON FUNCTION app_time_target_workspace(text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_time_target_label(text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_lock_own_entry(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_start_timer(uuid, text, text, text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_timer_action(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_time_entry(uuid, timestamptz, timestamptz, integer, text, text, text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_time_entry(uuid, integer, timestamptz, timestamptz, integer, text, text, text, uuid)
  FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_time_entry(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_time_sharing(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_entry_seconds(time_entries) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_task_time(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_time_target_label(text, uuid),
  app_start_timer(uuid, text, text, text, uuid),
  app_timer_action(uuid, text),
  app_add_time_entry(uuid, timestamptz, timestamptz, integer, text, text, text, uuid),
  app_update_time_entry(uuid, integer, timestamptz, timestamptz, integer, text, text, text, uuid),
  app_delete_time_entry(uuid),
  app_set_time_sharing(uuid, boolean),
  app_entry_seconds(time_entries),
  app_task_time(uuid)
TO fvoci_app;
