-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

CREATE TABLE fvoci.time_entries (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    started_at timestamp with time zone NOT NULL,
    ended_at timestamp with time zone,
    paused_at timestamp with time zone,
    paused_seconds integer DEFAULT 0 NOT NULL,
    kind text DEFAULT 'work'::text NOT NULL,
    memo text DEFAULT ''::text NOT NULL,
    target_kind text,
    target_id uuid,
    workspace_id uuid,
    version integer DEFAULT 1 NOT NULL,
    request_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    CONSTRAINT time_entries_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT time_entries_kind_check CHECK ((kind = ANY (ARRAY['work'::text, 'study'::text, 'lecture'::text, 'hobby'::text, 'rest'::text, 'other'::text]))),
    CONSTRAINT time_entries_order CHECK (((ended_at IS NULL) OR (ended_at >= started_at))),
    CONSTRAINT time_entries_pause CHECK (((paused_at IS NULL) OR (ended_at IS NULL))),
    CONSTRAINT time_entries_paused_seconds_check CHECK ((paused_seconds >= 0)),
    CONSTRAINT time_entries_target CHECK (((target_kind IS NULL) = (target_id IS NULL))),
    CONSTRAINT time_entries_target_kind_check CHECK ((target_kind = ANY (ARRAY['task'::text, 'document'::text, 'item'::text, 'episode'::text, 'study_unit'::text]))),
    CONSTRAINT time_entries_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.time_entries OWNER TO fvoci_migrator;

-- 시작·수동 기록·이어가기의 요청을 같은 사용자별 키로 기억한다. 이어가기도 최초 기록을 재시도 결과로 돌려준다.
-- 내부 멱등 대장이며 앱 역할의 직접 접근은 허용하지 않는다.
CREATE TABLE fvoci.time_entry_requests (
    user_id uuid NOT NULL,
    request_id uuid NOT NULL,
    entry_id uuid NOT NULL
);

ALTER TABLE fvoci.time_entry_requests OWNER TO fvoci_migrator;

CREATE TABLE fvoci.time_entry_revisions (
    id uuid NOT NULL,
    entry_id uuid NOT NULL,
    user_id uuid NOT NULL,
    started_at timestamp with time zone NOT NULL,
    ended_at timestamp with time zone,
    paused_seconds integer NOT NULL,
    kind text NOT NULL,
    memo text NOT NULL,
    target_kind text,
    target_id uuid,
    changed_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT time_entry_revisions_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.time_entry_revisions OWNER TO fvoci_migrator;

-- 타이머 시작(D-32·D-117, 개정 2026-10-26 ⑰). 사용자당 모든 조직·공간에서 실행 중인 기록은 하나다. 기록 중에 다른 대상에서 시작하면 한
-- 트랜잭션에서 진행 중 기록을 멈춰 저장하고 새 기록을 시작한다(stopped_entry_id). 새 대상을 볼 수 없으면 아무것도 바뀌지
-- 않는다(기존 기록은 계속 간다). 같은 request_id의 재시도는 같은 기록을 돌려주고 다시 멈추지 않는다.
CREATE FUNCTION fvoci.app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[], p_mode text DEFAULT NULL::text) RETURNS TABLE(entry_id uuid, stopped_entry_id uuid)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_id      uuid;
    v_ws      uuid;
    v_running time_entries%ROWTYPE;
    v_stopped uuid;
    v_now     timestamp with time zone;
  BEGIN
    IF p_mode IS NOT NULL AND p_mode NOT IN ('continue', 'new') THEN
      RAISE EXCEPTION 'mode must be continue or new' USING ERRCODE = '22023';
    END IF;
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    SELECT r.entry_id INTO v_id FROM time_entry_requests r
    WHERE r.user_id = v_user AND r.request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN QUERY SELECT v_id, NULL::uuid;
      RETURN;
    END IF;
    -- 새 대상을 먼저 확인한다. 못 보는 대상이면 여기서 끝나고 진행 중 기록은 그대로다.
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    -- 같은 사용자의 시작 요청을 줄 세운다(하나만 진행 중이라는 규칙을 고유 색인과 함께 지킨다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || v_user::text, 0));
    -- 전환 시각은 잠금을 얻은 뒤의 실제 시각이다(now()는 트랜잭션 시작 시각이라, 먼저 시작했지만 잠금을 늦게 얻은 요청이
    -- 뒤에 만들어진 기록의 시작보다 이른 시각으로 멈추려다 time_entries_order에 걸린다).
    v_now := clock_timestamp();
    SELECT r.entry_id INTO v_id FROM time_entry_requests r
    WHERE r.user_id = v_user AND r.request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN QUERY SELECT v_id, NULL::uuid;
      RETURN;
    END IF;
    SELECT * INTO v_running FROM time_entries
    WHERE user_id = v_user AND ended_at IS NULL AND deleted_at IS NULL FOR UPDATE;
    -- 개정 2026-10-26(⑰, D-117): 같은 대상으로 기록 중에 다시 시작하면 웹이 "이어가기 / 새 기록"을 묻는다. 플래그 없이는
    -- 55000(→409)으로 거절하고, continue는 기존 기록을 그대로 돌려주며(새 기록 없음), new는 아래처럼 멈춰 저장하고 새로 시작한다.
    -- 다른 대상이면 전처럼 자동으로 전환한다.
    IF v_running.id IS NOT NULL AND v_running.target_kind = p_target_kind
       AND v_running.target_id IS NOT DISTINCT FROM p_target THEN
      IF p_mode = 'continue' THEN
        INSERT INTO time_entry_requests (user_id, request_id, entry_id)
        VALUES (v_user, p_request_id, v_running.id);
        RETURN QUERY SELECT v_running.id, NULL::uuid;
        RETURN;
      ELSIF p_mode IS DISTINCT FROM 'new' THEN
        RAISE EXCEPTION 'a timer for this target is already running' USING ERRCODE = '55000';
      END IF;
    END IF;
    IF v_running.id IS NOT NULL THEN
      -- 멈춤(app_timer_action 'stop')과 같은 계산: 일시정지 중이던 시간은 정지 시간에 더한다.
      UPDATE time_entries SET
        paused_seconds = paused_seconds
          + CASE WHEN paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM v_now - paused_at))::integer END,
        paused_at = NULL, ended_at = v_now, version = version + 1, updated_at = v_now
      WHERE id = v_running.id;
      v_stopped := v_running.id;
    END IF;
    INSERT INTO time_entries (id, user_id, started_at, kind, memo, target_kind, target_id, workspace_id, request_id)
    VALUES (app_id(p_ids, 1), v_user, v_now, p_kind, coalesce(p_memo, ''), p_target_kind, p_target, v_ws,
            p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO time_entry_requests (user_id, request_id, entry_id)
    VALUES (v_user, p_request_id, v_id);
    RETURN QUERY SELECT v_id, v_stopped;
  END
  $$;


ALTER FUNCTION fvoci.app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[], p_mode text) OWNER TO fvoci_migrator;

-- 항목·업무의 내 소요 시간(D-117: 기록은 본인만 보고 선택 공유 집계는 없다).
CREATE FUNCTION fvoci.app_item_time(p_item uuid) RETURNS TABLE(entry_kind text, mine_seconds bigint)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      WITH targets (target_kind, target_id) AS (
        SELECT 'item'::text, p_item
        UNION ALL SELECT 'task', t.id FROM tasks t WHERE t.item_id = p_item
        UNION ALL SELECT 'document', d.id FROM documents d WHERE d.item_id = p_item
        UNION ALL SELECT 'episode', c.id FROM course_episodes c WHERE c.item_id = p_item
        UNION ALL SELECT 'study_unit', u.id FROM study_units u WHERE u.item_id = p_item
      )
      SELECT e.kind, coalesce(sum(app_entry_seconds(e)), 0)::bigint
      FROM time_entries e
      JOIN targets g ON g.target_kind = e.target_kind AND g.target_id = e.target_id
      WHERE e.user_id = app_user_id() AND e.deleted_at IS NULL AND e.ended_at IS NOT NULL
      GROUP BY e.kind
      ORDER BY e.kind;
  END
  $$;


ALTER FUNCTION fvoci.app_item_time(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_time(p_task uuid) RETURNS bigint
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('task', p_task) = 'none' THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN (SELECT coalesce(sum(app_entry_seconds(e)), 0)::bigint
            FROM time_entries e
            WHERE e.target_kind = 'task' AND e.target_id = p_task AND e.user_id = app_user_id()
              AND e.deleted_at IS NULL AND e.ended_at IS NOT NULL);
  END
  $$;


ALTER FUNCTION fvoci.app_task_time(p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_time_entry(p_request_id uuid, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
    v_ws   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    -- 타이머 시작·이어가기와 요청 키를 공유한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || v_user::text, 0));
    SELECT r.entry_id INTO v_id FROM time_entry_requests r
    WHERE r.user_id = v_user AND r.request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    INSERT INTO time_entries (id, user_id, started_at, ended_at, paused_seconds, kind, memo, target_kind, target_id,
                              workspace_id, request_id)
    VALUES (app_id(p_ids, 1), v_user, p_started, p_ended, p_paused, p_kind, coalesce(p_memo, ''), p_target_kind,
            p_target, v_ws, p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO time_entry_requests (user_id, request_id, entry_id)
    VALUES (v_user, p_request_id, v_id);
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_add_time_entry(p_request_id uuid, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_time_entry(p_entry uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
  BEGIN
    UPDATE time_entries SET deleted_at = now(), version = version + 1 WHERE id = v_entry.id;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_time_entry(p_entry uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_entry_seconds(e fvoci.time_entries) RETURNS bigint
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT greatest(0, floor(extract(epoch FROM coalesce(e.ended_at, now()) - e.started_at))::bigint
      - e.paused_seconds
      - CASE WHEN e.paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM now() - e.paused_at))::bigint END)
  $$;


ALTER FUNCTION fvoci.app_entry_seconds(e fvoci.time_entries) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_own_entry(p_entry uuid) RETURNS fvoci.time_entries
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_lock_own_entry(p_entry uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_time_target_label(p_kind text, p_id uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE
      WHEN p_kind = 'task' AND app_access('task', p_id) <> 'none' THEN
        (SELECT concat_ws(' ', app_task_reference(id), title) FROM tasks WHERE id = p_id)
      WHEN p_kind = 'document' AND app_access('document', p_id) <> 'none' THEN
        (SELECT concat_ws(' ', app_document_reference(id), title) FROM documents WHERE id = p_id)
      WHEN p_kind = 'item' AND app_access('item', p_id) <> 'none' THEN
        (SELECT concat_ws(' ', app_item_reference(id), title) FROM items WHERE id = p_id)
      WHEN p_kind = 'episode'
           AND app_access('item', (SELECT item_id FROM course_episodes WHERE id = p_id)) <> 'none' THEN
        (SELECT concat_ws(' ', app_item_reference(e.item_id), e.number || '회', e.title)
         FROM course_episodes e WHERE e.id = p_id)
      WHEN p_kind = 'study_unit'
           AND app_access('item', (SELECT item_id FROM study_units WHERE id = p_id)) <> 'none' THEN
        (SELECT concat_ws(' ', app_item_reference(s.item_id), s.number || '.', s.title)
         FROM study_units s WHERE s.id = p_id)
    END
  $$;


ALTER FUNCTION fvoci.app_time_target_label(p_kind text, p_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_time_target_workspace(p_kind text, p_id uuid) RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_kind IS NULL THEN
      RETURN NULL;
    END IF;
    IF p_kind = 'task' AND app_access('task', p_id) <> 'none' THEN
      RETURN (SELECT workspace_id FROM tasks WHERE id = p_id);
    ELSIF p_kind = 'document' AND app_access('document', p_id) <> 'none' THEN
      RETURN (SELECT workspace_id FROM documents WHERE id = p_id);
    ELSIF p_kind = 'item' AND app_access('item', p_id) <> 'none' THEN
      RETURN (SELECT workspace_id FROM items WHERE id = p_id);
    ELSIF p_kind = 'episode'
          AND app_access('item', (SELECT item_id FROM course_episodes WHERE id = p_id)) <> 'none' THEN
      RETURN (SELECT workspace_id FROM course_episodes WHERE id = p_id);
    ELSIF p_kind = 'study_unit'
          AND app_access('item', (SELECT item_id FROM study_units WHERE id = p_id)) <> 'none' THEN
      RETURN (SELECT workspace_id FROM study_units WHERE id = p_id);
    END IF;
    RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
  END
  $$;


ALTER FUNCTION fvoci.app_time_target_workspace(p_kind text, p_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_timer_action(p_entry uuid, p_action text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_timer_action(p_entry uuid, p_action text) OWNER TO fvoci_migrator;

-- 구간을 바꿀 때만 멈춤 누계와 범위를 검사한다. 이미 연결된 대상을 볼 수 없어도 메모 등 다른 필드는 고칠 수 있다.
-- 길이 상한 추가나 기존 긴 기록 보정은 하지 않는다(P-29 보류).
CREATE FUNCTION fvoci.app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
      IF p_started IS DISTINCT FROM v_entry.started_at
         AND v_entry.paused_seconds > extract(epoch FROM coalesce(v_entry.paused_at, now()) - p_started) THEN
        RAISE EXCEPTION 'the start leaves less time than the timer was paused' USING ERRCODE = '22023';
      END IF;
    ELSIF p_ended IS NULL THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    ELSIF (p_started, p_ended, p_paused) IS DISTINCT FROM (v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds)
          AND (p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started)) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    -- 대상을 바꿀 때만 지금 볼 수 있는지 확인한다(이미 연결된 대상은 볼 수 없게 되어도 그대로 둘 수 있다).
    IF (p_target_kind, p_target) IS DISTINCT FROM (v_entry.target_kind, v_entry.target_id) THEN
      v_ws := app_time_target_workspace(p_target_kind, p_target);
    ELSE
      v_ws := v_entry.workspace_id;
    END IF;
    INSERT INTO time_entry_revisions (id, entry_id, user_id, started_at, ended_at, paused_seconds, kind, memo,
                                      target_kind, target_id)
    VALUES (app_id(p_ids, 1), p_entry, v_entry.user_id, v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds,
            v_entry.kind, v_entry.memo, v_entry.target_kind, v_entry.target_id);
    UPDATE time_entries SET
      started_at = p_started, ended_at = p_ended, paused_seconds = p_paused, kind = p_kind,
      memo = coalesce(p_memo, ''), target_kind = p_target_kind, target_id = p_target, workspace_id = v_ws,
      version = version + 1, updated_at = now()
    WHERE id = p_entry;
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_request UNIQUE (user_id, request_id);

ALTER TABLE ONLY fvoci.time_entry_requests
    ADD CONSTRAINT time_entry_requests_pkey PRIMARY KEY (user_id, request_id);

ALTER TABLE ONLY fvoci.time_entry_revisions
    ADD CONSTRAINT time_entry_revisions_pkey PRIMARY KEY (id);

CREATE UNIQUE INDEX time_entries_one_active ON fvoci.time_entries USING btree (user_id) WHERE ((ended_at IS NULL) AND (deleted_at IS NULL));

CREATE INDEX time_entries_target ON fvoci.time_entries USING btree (target_kind, target_id) WHERE (deleted_at IS NULL);

CREATE INDEX time_entries_user_started ON fvoci.time_entries USING btree (user_id, started_at) WHERE (deleted_at IS NULL);

CREATE INDEX time_entry_revisions_entry ON fvoci.time_entry_revisions USING btree (entry_id, changed_at);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.time_entry_requests
    ADD CONSTRAINT time_entry_requests_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.time_entry_requests
    ADD CONSTRAINT time_entry_requests_entry_id_fkey FOREIGN KEY (entry_id) REFERENCES fvoci.time_entries(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.time_entry_revisions
    ADD CONSTRAINT time_entry_revisions_entry_id_fkey FOREIGN KEY (entry_id) REFERENCES fvoci.time_entries(id);

ALTER TABLE ONLY fvoci.time_entry_revisions
    ADD CONSTRAINT time_entry_revisions_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
