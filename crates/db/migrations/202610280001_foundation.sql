-- GroundZero 새 설치 기준(오버홀 '달리는 아비시니안' 1단계, D-126·D-131). 역할·스키마는 먼저 db bootstrap으로 준비한다.
-- PostgreSQL 17 이상에서 환경 공통 SQL을 쓴다(D-100: docs/decisions/database.md#d-100).
-- 기존 개발·시험 DB는 대상·백업·삭제 승인을 거쳐 재생성한다(D-107: docs/decisions/database.md#d-107).
-- 비교용 과거 기준 SQL: 355fe26407b91b5b560a4f13dd34981f74c0ee6f의
-- crates/server/migrations/202610230001_initial_state.sql과 202610250001_user_settings_theme.sql(여기에 합침).
--
-- 계층은 서버 › 조직 › 워크스페이스 › 모듈 › 항목이고(D-111) 접근 등급은 app_access(kind, id) 하나가 계산한다(D-113).
-- D-08·D-88: 소유자는 fvoci_migrator, 앱 접속은 소유권·BYPASSRLS 없는 fvoci_app이다.
-- SECURITY DEFINER는 소유자 권한으로 불변식을 지키므로 search_path를 fvoci, pg_temp로 고정하고
-- PUBLIC 실행 권한을 회수한다. 사용자 권한·소유 범위는 함수와 RLS에서 확인한다.
-- 트리거 전용·내부 함수의 실행 권한도 마지막 ACL에서 제한한다.
-- 순수 도우미 app_local_instant·app_normalize_key의 PUBLIC 실행은 기존 계약대로 유지한다.
--
-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

ALTER SCHEMA fvoci OWNER TO fvoci_migrator;

-- 테이블 CHECK가 사용하는 순수 도우미.

CREATE FUNCTION fvoci.app_web_url_ok(p_url text) RETURNS boolean
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $_$
    SELECT coalesce(p_url, '') = '' OR p_url ~* '^https?://[^[:space:]]+$'
  $_$;


ALTER FUNCTION fvoci.app_web_url_ok(p_url text) OWNER TO fvoci_migrator;

-- 테이블·행 타입. 새 UUID는 앱이 공급하며 최종 기본값·CHECK를 처음부터 적용한다.
-- 필수 초기 행은 없으며 개발 예제 seed는 별도다.

CREATE TABLE fvoci.instance_state (
    key text NOT NULL,
    value jsonb DEFAULT '{}'::jsonb NOT NULL,
    set_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.instance_state OWNER TO fvoci_migrator;

CREATE TABLE fvoci.oidc_login_states (
    secret_hash bytea NOT NULL,
    state text NOT NULL,
    pkce_verifier text NOT NULL,
    nonce text NOT NULL,
    return_to text NOT NULL,
    expires_at timestamp with time zone NOT NULL
);


ALTER TABLE fvoci.oidc_login_states OWNER TO fvoci_migrator;

CREATE TABLE fvoci.search_queue (
    kind text NOT NULL,
    target_id uuid NOT NULL,
    queued_at timestamp with time zone DEFAULT now() NOT NULL,
    claimed_until timestamp with time zone,
    attempts integer DEFAULT 0 NOT NULL,
    task_uid bigint,
    task_queued_at timestamp with time zone,
    CONSTRAINT search_queue_kind_check CHECK ((kind = ANY (ARRAY['document'::text, 'task'::text, 'item'::text, 'event'::text, 'file'::text, 'record'::text]))),
    CONSTRAINT search_queue_task_pair CHECK (((task_uid IS NULL) = (task_queued_at IS NULL)))
);


ALTER TABLE fvoci.search_queue OWNER TO fvoci_migrator;

CREATE TABLE fvoci.sessions (
    id uuid NOT NULL,
    token_hash bytea NOT NULL,
    user_id uuid NOT NULL,
    id_token text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    CONSTRAINT sessions_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.sessions OWNER TO fvoci_migrator;

-- 서버 관리자(D-112). 운영자가 지정한 OIDC 식별자의 사용자를 시드하며 조직 생성·목록·서버 로그 같은 메타데이터만 다룬다.
CREATE TABLE fvoci.server_admins (
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL
);


ALTER TABLE fvoci.server_admins OWNER TO fvoci_migrator;

CREATE TABLE fvoci.users (
    id uuid NOT NULL,
    issuer text NOT NULL,
    subject text NOT NULL,
    email text,
    display_name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_login_at timestamp with time zone DEFAULT now() NOT NULL,
    email_verified boolean DEFAULT false NOT NULL,
    CONSTRAINT users_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.users OWNER TO fvoci_migrator;

-- 함수. 행 타입은 위에서 준비했고 각 최종 서명·본문을 한 번만 정의한다.

-- 상태 이름·범주가 바뀌면 그 상태의 업무를 다시 색인한다(D-101·D-102의 색인 일치).
CREATE FUNCTION fvoci.app_search_enqueue_status_tasks() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'task', t.id FROM tasks t WHERE t.status_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_status_tasks() OWNER TO fvoci_migrator;

-- 서버 관리자·조직 관리자(D-112)는 메타데이터 함수에서만 쓴다. 내용 등급과 무관하다.
CREATE FUNCTION fvoci.app_is_server_admin() RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT EXISTS (SELECT 1 FROM server_admins WHERE user_id = app_user_id()) $$;


ALTER FUNCTION fvoci.app_is_server_admin() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_server_admin() RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
  BEGIN
    IF v_user IS NULL OR NOT app_is_server_admin() THEN
      RAISE EXCEPTION 'server admin required' USING ERRCODE = '42501';
    END IF;
    RETURN v_user;
  END
  $$;


ALTER FUNCTION fvoci.app_require_server_admin() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_server_admins() RETURNS TABLE(user_id uuid, display_name text, created_at timestamp with time zone)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_require_server_admin();
    RETURN QUERY
      SELECT a.user_id, u.display_name, a.created_at
      FROM server_admins a JOIN users u ON u.id = a.user_id
      ORDER BY a.created_at, a.user_id;
  END
  $$;


ALTER FUNCTION fvoci.app_server_admins() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_timezone(p_tz text) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_tz IS NULL OR p_tz ~ '^(posix|right|SystemV)/' OR p_tz IN ('Factory', 'localtime', 'posixrules')
       OR NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = p_tz) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_timezone(p_tz text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_login_state(p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    -- 만료 행은 한 번에 일부만 지운다(요청마다 전체 스캔을 피한다).
    DELETE FROM oidc_login_states
    WHERE secret_hash IN (
      SELECT secret_hash FROM oidc_login_states WHERE expires_at < now() LIMIT 100
    );
    INSERT INTO oidc_login_states (secret_hash, state, pkce_verifier, nonce, return_to, expires_at)
    VALUES (p_secret_hash, p_state, p_verifier, p_nonce, p_return_to,
            now() + make_interval(secs => p_ttl_secs));
  END
  $$;


ALTER FUNCTION fvoci.app_create_login_state(p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    -- 로그인 중이던 세션이 있으면 회전한다(이전 토큰 무효).
    IF p_replaces IS NOT NULL THEN
      DELETE FROM sessions WHERE token_hash = p_replaces;
    END IF;
    DELETE FROM sessions
    WHERE id IN (SELECT id FROM sessions WHERE expires_at < now() LIMIT 100);
    INSERT INTO sessions (id, token_hash, user_id, id_token, expires_at)
    VALUES (app_id(p_ids, 1), p_token_hash, p_user, p_id_token, now() + make_interval(secs => p_ttl_secs));
  END
  $$;


ALTER FUNCTION fvoci.app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_end_session(p_token_hash bytea) RETURNS text
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ DELETE FROM sessions WHERE token_hash = p_token_hash RETURNING id_token $$;


ALTER FUNCTION fvoci.app_end_session(p_token_hash bytea) OWNER TO fvoci_migrator;

-- D-88: 앱이 공급한 UUIDv7만 소비한다. DB 생성이나 부족한 ID의 대체는 없다. docs/decisions/database.md#d-88.
-- FV001은 개수 조회 뒤 대상이 늘어난 경합으로 재시도할 수 있는 부족, FV002는 잘못된 ID·위치다.
-- HTTP는 부족을 409로 돌려주고 작업 프로세스는 개수 조회부터 트랜잭션 전체를 재시도한다.
CREATE FUNCTION fvoci.app_id(p_ids uuid[], p_at integer) RETURNS uuid
    LANGUAGE plpgsql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_at IS NULL OR p_at < 1 THEN
      RAISE EXCEPTION 'invalid new id position %', p_at USING ERRCODE = 'FV002';
    END IF;
    IF p_at > coalesce(cardinality(p_ids), 0) THEN
      RAISE EXCEPTION 'new id #% was not supplied by the application', p_at USING ERRCODE = 'FV001';
    END IF;
    IF p_ids[p_at] IS NULL OR uuid_extract_version(p_ids[p_at]) IS DISTINCT FROM 7 THEN
      RAISE EXCEPTION 'new ids must be UUIDv7' USING ERRCODE = 'FV002';
    END IF;
    RETURN p_ids[p_at];
  END
  $$;


ALTER FUNCTION fvoci.app_id(p_ids uuid[], p_at integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_ids_from(p_ids uuid[], p_from integer) RETURNS uuid[]
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT coalesce(p_ids[p_from:], '{}'::uuid[]) $$;


ALTER FUNCTION fvoci.app_ids_from(p_ids uuid[], p_from integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_local_instant(p_local timestamp without time zone, p_zone text) RETURNS timestamp with time zone
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT min(c) FROM (
      SELECT (p_local - ((t AT TIME ZONE p_zone) - (t AT TIME ZONE 'UTC'))) AT TIME ZONE 'UTC' AS c
      FROM (VALUES ((p_local - interval '1 day') AT TIME ZONE 'UTC'),
                   ((p_local + interval '1 day') AT TIME ZONE 'UTC')) AS around (t)
    ) candidates
    WHERE (c AT TIME ZONE p_zone) = p_local
  $$;


ALTER FUNCTION fvoci.app_local_instant(p_local timestamp without time zone, p_zone text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_normalize_key(p_key text) RETURNS text
    LANGUAGE plpgsql IMMUTABLE
    AS $_$
  DECLARE
    v text := upper(btrim(p_key));
  BEGIN
    IF v !~ '^[A-Z][A-Z0-9]{1,9}$' THEN
      RAISE EXCEPTION 'key must be 2-10 letters or digits starting with a letter' USING ERRCODE = '22023';
    END IF;
    IF v = 'WIKI' THEN
      RAISE EXCEPTION 'WIKI is reserved' USING ERRCODE = '22023';
    END IF;
    RETURN v;
  END
  $_$;


ALTER FUNCTION fvoci.app_normalize_key(p_key text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_kind text := TG_ARGV[0];
    v_id   uuid := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
  BEGIN
    INSERT INTO search_queue (kind, target_id) VALUES (v_kind, v_id)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_assignee() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    VALUES ('task', CASE WHEN TG_OP = 'DELETE' THEN OLD.task_id ELSE NEW.task_id END)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_assignee() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_document_files() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'file', id FROM files WHERE document_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_document_files() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_item_children() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE item_id = NEW.id
    UNION ALL SELECT 'task', id FROM tasks WHERE item_id = NEW.id
    UNION ALL SELECT 'event', id FROM events WHERE item_id = NEW.id
    UNION ALL SELECT 'file', id FROM files WHERE item_id = NEW.id
    UNION ALL SELECT 'file', f.id FROM files f JOIN documents d ON d.id = f.document_id WHERE d.item_id = NEW.id
    UNION ALL SELECT 'record', id FROM item_records WHERE item_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_item_children() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_module() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'item', i.id FROM items i WHERE i.module_id = NEW.module_id
    UNION ALL SELECT 'task', t.id FROM tasks t JOIN items i ON i.id = t.item_id WHERE i.module_id = NEW.module_id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_module() OWNER TO fvoci_migrator;

-- 검색 제목 대체 경로의 집계 읽기(D-131·태비 초안 1). 호출자 RLS로 후보를 읽는다.
-- 날짜·작성자·조직 범위와 60개 후보 상한은 색인 경로와 같다(D-101·D-102).
-- 응답 직전 app_search_visible의 최종 접근 확인은 호출부에서 그대로 한다.
CREATE FUNCTION fvoci.app_search_titles(p_pattern text, p_kinds text[], p_workspace uuid, p_module uuid, p_status_category text, p_assignee uuid, p_from date, p_to date, p_start timestamp with time zone, p_end timestamp with time zone, p_author uuid, p_organization uuid)
    RETURNS TABLE(kind text, id uuid, title text, workspace_id uuid, item_id uuid)
    LANGUAGE sql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
SELECT kind, id, title, workspace_id,
                CASE WHEN r.item_id IS NOT NULL AND app_access('item', r.item_id) <> 'none' THEN r.item_id END
         FROM (
           SELECT 'document' AS kind, d.id, d.title, d.workspace_id, d.item_id, d.updated_at,
                  (SELECT module_id FROM items WHERE id = d.item_id) AS module_id, NULL::text AS status_category,
                  NULL::uuid[] AS assignees, d.updated_at AS at, NULL::date AS on_day, d.created_by
           FROM documents d WHERE d.title ILIKE p_pattern AND d.deleted_at IS NULL
           UNION ALL
           SELECT 'task', t.id, t.title, t.workspace_id, t.item_id, t.updated_at,
                  (SELECT module_id FROM items WHERE id = t.item_id), s.category,
                  ARRAY(SELECT user_id FROM task_assignees WHERE task_id = t.id), NULL,
                  coalesce(t.due_date, t.start_date), t.created_by
           FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.title ILIKE p_pattern AND t.deleted_at IS NULL
           UNION ALL
           SELECT 'item', i.id, i.title, i.workspace_id, NULL, i.updated_at, i.module_id, NULL, NULL,
                  i.updated_at, NULL, i.created_by
           FROM items i WHERE i.title ILIKE p_pattern AND i.deleted_at IS NULL
           UNION ALL
           SELECT 'event', e.id, e.title, e.workspace_id, e.item_id, e.updated_at,
                  (SELECT module_id FROM items WHERE id = e.item_id), NULL, NULL,
                  e.starts_at, e.start_date, e.created_by
           FROM events e WHERE e.title ILIKE p_pattern AND e.deleted_at IS NULL
           UNION ALL
           SELECT 'file', f.id, f.name, f.workspace_id, coalesce(f.item_id, d.item_id), f.updated_at,
                  (SELECT module_id FROM items WHERE id = coalesce(f.item_id, d.item_id)), NULL, NULL,
                  f.updated_at, NULL, f.created_by
           FROM files f LEFT JOIN documents d ON d.id = f.document_id
           WHERE f.name ILIKE p_pattern AND f.deleted_at IS NULL
           UNION ALL
           SELECT 'record', rc.id, rc.title, rc.workspace_id, rc.item_id, rc.updated_at,
                  (SELECT module_id FROM items WHERE id = rc.item_id), NULL, NULL,
                  CASE WHEN rc.occurred_on IS NULL THEN rc.updated_at END, rc.occurred_on, rc.created_by
           FROM item_records rc WHERE rc.title ILIKE p_pattern
         ) r
         WHERE r.kind = ANY(p_kinds) AND (p_workspace::uuid IS NULL OR r.workspace_id = p_workspace)
           AND (p_module::uuid IS NULL OR r.module_id = p_module)
           AND (p_status_category::text IS NULL OR r.status_category = p_status_category)
           AND (p_assignee::uuid IS NULL OR p_assignee = ANY (r.assignees))
           AND ((p_from::date IS NULL AND p_to::date IS NULL)
                OR (r.at >= coalesce(p_start::timestamptz, '-infinity') AND r.at < coalesce(p_end::timestamptz, 'infinity'))
                OR (r.on_day >= coalesce(p_from::date, '-infinity') AND r.on_day <= coalesce(p_to::date, 'infinity')))
           AND (p_author::uuid IS NULL OR r.created_by = p_author)
           AND (r.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
                OR (r.workspace_id IS NULL AND (
                    (r.kind = 'document' AND EXISTS (SELECT 1 FROM documents d WHERE d.id = r.id AND d.owner_user_id = app_user_id()))
                    OR (r.kind = 'file' AND EXISTS (SELECT 1 FROM files f JOIN documents d ON d.id = f.document_id WHERE f.id = r.id AND d.owner_user_id = app_user_id())))))
         ORDER BY r.updated_at DESC LIMIT 60
    $$;

ALTER FUNCTION fvoci.app_search_titles(p_pattern text, p_kinds text[], p_workspace uuid, p_module uuid, p_status_category text, p_assignee uuid, p_from date, p_to date, p_start timestamp with time zone, p_end timestamp with time zone, p_author uuid, p_organization uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_visible(p_kinds text[], p_ids uuid[]) RETURNS TABLE(kind text, id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT w.kind, w.id FROM unnest(p_kinds, p_ids) AS w(kind, id)
    WHERE CASE w.kind
      WHEN 'event' THEN app_access('event', w.id) <> 'none'
      WHEN 'file' THEN app_access('file', w.id) <> 'none'
      WHEN 'record' THEN EXISTS (SELECT 1 FROM item_records r
                                 WHERE r.id = w.id AND app_access('item', r.item_id) <> 'none')
      ELSE app_access(w.kind, w.id) <> 'none'
    END
  $$;


ALTER FUNCTION fvoci.app_search_visible(p_kinds text[], p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_session_user(p_token_hash bytea) RETURNS uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT user_id FROM sessions WHERE token_hash = p_token_hash AND expires_at > now() $$;


ALTER FUNCTION fvoci.app_session_user(p_token_hash bytea) OWNER TO fvoci_migrator;

-- 반복은 항목·ID 순으로 처리하고 항목의 세 열은 done → in_progress → todo 순서로 먼저 잠근다.
-- 사용자 이동·항목 동기화와 순서를 맞추며 잠금은 트랜잭션 끝까지 유지한다.
-- 실패한 항목·반복의 ID 몫도 소비해 다음 반복의 몫을 지킨다. 항목별 부분 커밋은 하지 않는다(P-35 보류).
CREATE FUNCTION fvoci.app_system_search_claim(p_limit integer) RETURNS TABLE(kind text, target_id uuid, queued_at timestamp with time zone, attempts integer)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes', attempts = q.attempts + 1
    WHERE (q.kind, q.target_id) IN (
      SELECT s.kind, s.target_id FROM search_queue s
      WHERE app_user_id() IS NULL
        AND (s.claimed_until IS NULL OR s.claimed_until < now())
        AND s.attempts < 10
        -- 보낸 작업을 기다리는 행은 그 작업이 끝날 때까지 꺼내지 않는다.
        AND s.task_uid IS NULL
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING q.kind, q.target_id, q.queued_at, q.attempts
  $$;


ALTER FUNCTION fvoci.app_system_search_claim(p_limit integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    DELETE FROM search_queue q
    USING unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE search_queue q SET claimed_until = NULL
    FROM unnest(p_kinds, p_ids) AS d(kind, id)
    WHERE q.kind = d.kind AND q.target_id = d.id;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_enqueue_all() RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE deleted_at IS NULL
    UNION ALL SELECT 'task', id FROM tasks WHERE deleted_at IS NULL
    UNION ALL SELECT 'item', id FROM items WHERE deleted_at IS NULL
    UNION ALL SELECT 'event', id FROM events WHERE deleted_at IS NULL
    UNION ALL SELECT 'file', id FROM files WHERE deleted_at IS NULL
    UNION ALL SELECT 'record', id FROM item_records
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_enqueue_all() OWNER TO fvoci_migrator;

-- 검색 색인과 대체 SQL의 작성자·날짜 의미를 맞춘다. D-101·D-102: docs/decisions/notify-audit.md#d-101.
-- 시각은 사용자 시간대의 하루 경계로, 날짜 전용 값은 날짜 그대로 비교한다. 최종 접근 권한은 서버가 다시 검사한다.
CREATE FUNCTION fvoci.app_system_search_payload(p_kinds text[], p_ids uuid[]) RETURNS TABLE(kind text, id uuid, live boolean, workspace_id uuid, owner_user_id uuid, item_id uuid, title text, reference text, body jsonb, description text, created_by uuid, module_id uuid, status text, status_category text, assignee_ids uuid[], at timestamp with time zone, on_day date, file_key text, file_type text, file_size bigint)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH wanted AS (SELECT * FROM unnest(p_kinds, p_ids) AS w(kind, id)),
    rows AS (
      SELECT w.kind, w.id,
        CASE w.kind
          WHEN 'document' THEN d.id IS NOT NULL AND d.deleted_at IS NULL
            AND (d.item_id IS NULL OR EXISTS (SELECT 1 FROM items i WHERE i.id = d.item_id AND i.deleted_at IS NULL))
          WHEN 'task' THEN t.id IS NOT NULL AND t.deleted_at IS NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = t.item_id AND i.deleted_at IS NULL)
          WHEN 'item' THEN it.id IS NOT NULL AND it.deleted_at IS NULL
          WHEN 'event' THEN e.id IS NOT NULL AND e.deleted_at IS NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = e.item_id AND i.deleted_at IS NULL)
          WHEN 'file' THEN fl.id IS NOT NULL AND fl.deleted_at IS NULL
            AND (fl.document_id IS NULL OR EXISTS (SELECT 1 FROM documents fd WHERE fd.id = fl.document_id AND fd.deleted_at IS NULL))
            AND NOT EXISTS (SELECT 1 FROM items i WHERE i.id = coalesce(fl.item_id, (SELECT item_id FROM documents WHERE id = fl.document_id)) AND i.deleted_at IS NOT NULL)
          WHEN 'record' THEN rc.id IS NOT NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = rc.item_id AND i.deleted_at IS NULL)
        END AS live,
        coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id, fl.workspace_id, rc.workspace_id)
          AS workspace_id,
        coalesce(d.owner_user_id, fl.owner_user_id) AS owner_user_id,
        coalesce(d.item_id, t.item_id, e.item_id, fl.item_id, rc.item_id,
                 (SELECT item_id FROM documents WHERE id = fl.document_id))
          AS item_id,
        coalesce(d.title, t.title, it.title, e.title, fl.name, rc.title) AS title,
        CASE w.kind
          WHEN 'task' THEN app_task_reference(t.id)
          WHEN 'item' THEN app_item_reference(it.id)
        END AS reference,
        d.body,
        coalesce(t.description, it.description, e.description,
          rc.body || ' ' || coalesce((SELECT string_agg(f.value, ' ') FROM jsonb_each_text(rc.fields) f), ''))
          AS description,
        coalesce(d.created_by, t.created_by, it.created_by, e.created_by, fl.created_by, rc.created_by) AS created_by,
        it.module_id AS item_module,
        (SELECT s.name FROM task_statuses s WHERE s.id = t.status_id) AS status,
        (SELECT s.category FROM task_statuses s WHERE s.id = t.status_id) AS status_category,
        CASE WHEN w.kind = 'task' THEN ARRAY(SELECT a.user_id FROM task_assignees a WHERE a.task_id = t.id ORDER BY 1) END
          AS assignee_ids,
        -- 검색 API의 대체 경로(`search_api.rs` `fallback`)와 같은 규칙.
        CASE w.kind
          WHEN 'task' THEN NULL
          WHEN 'event' THEN e.starts_at
          WHEN 'record' THEN CASE WHEN rc.occurred_on IS NULL THEN rc.updated_at END
          ELSE coalesce(d.updated_at, it.updated_at, fl.updated_at)
        END AS at,
        CASE w.kind
          WHEN 'task' THEN coalesce(t.due_date, t.start_date)
          WHEN 'event' THEN e.start_date
          WHEN 'record' THEN rc.occurred_on
        END AS on_day,
        v.storage_key AS file_key, v.content_type AS file_type, v.size AS file_size
      FROM wanted w
      LEFT JOIN documents d ON w.kind = 'document' AND d.id = w.id
      LEFT JOIN tasks t ON w.kind = 'task' AND t.id = w.id
      LEFT JOIN items it ON w.kind = 'item' AND it.id = w.id
      LEFT JOIN events e ON w.kind = 'event' AND e.id = w.id
      LEFT JOIN files fl ON w.kind = 'file' AND fl.id = w.id
      LEFT JOIN item_records rc ON w.kind = 'record' AND rc.id = w.id
      LEFT JOIN file_versions v ON v.file_id = fl.id AND v.version = fl.current_version
      WHERE app_user_id() IS NULL
    )
    SELECT r.kind, r.id, r.live, r.workspace_id, r.owner_user_id, r.item_id, r.title, r.reference, r.body,
           r.description, r.created_by,
           coalesce(r.item_module, (SELECT module_id FROM items WHERE id = r.item_id)),
           r.status, r.status_category, r.assignee_ids, r.at, r.on_day, r.file_key, r.file_type, r.file_size
    FROM rows r
  $$;


ALTER FUNCTION fvoci.app_system_search_payload(p_kinds text[], p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_pending(p_limit integer) RETURNS TABLE(task_uid bigint)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH picked AS (
      UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
      WHERE (q.kind, q.target_id) IN (
        SELECT s.kind, s.target_id FROM search_queue s
        WHERE app_user_id() IS NULL AND s.task_uid IS NOT NULL
          AND (s.claimed_until IS NULL OR s.claimed_until < now())
        ORDER BY s.queued_at
        LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
      RETURNING q.task_uid)
    SELECT DISTINCT picked.task_uid FROM picked
  $$;


ALTER FUNCTION fvoci.app_system_search_pending(p_limit integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) RETURNS TABLE(kind text, target_id uuid)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
    FROM unnest(p_kinds, p_ids, p_queued, p_attempts) AS d(kind, id, queued_at, attempts)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at AND q.attempts = d.attempts
      AND q.task_uid IS NULL
    RETURNING q.kind, q.target_id
  $$;


ALTER FUNCTION fvoci.app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE search_queue q SET claimed_until = NULL
    FROM unnest(p_kinds, p_ids, p_queued, p_attempts) AS d(kind, id, queued_at, attempts)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.task_uid IS NULL
      AND ((q.queued_at = d.queued_at AND q.attempts = d.attempts) OR q.attempts = 0)
  $$;


ALTER FUNCTION fvoci.app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_settled(p_task bigint, p_outcome text) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    IF p_outcome NOT IN ('succeeded', 'failed', 'pending') THEN
      RAISE EXCEPTION 'unknown search task outcome' USING ERRCODE = '22023';
    END IF;
    IF p_outcome = 'pending' THEN
      UPDATE search_queue SET claimed_until = NULL WHERE task_uid = p_task;
      RETURN 0;
    END IF;
    IF p_outcome = 'succeeded' THEN
      DELETE FROM search_queue WHERE task_uid = p_task AND queued_at = task_queued_at;
      GET DIAGNOSTICS v_count = ROW_COUNT;
    END IF;
    UPDATE search_queue SET task_uid = NULL, task_queued_at = NULL, claimed_until = NULL WHERE task_uid = p_task;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_settled(p_task bigint, p_outcome text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_task bigint) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN;
    END IF;
    UPDATE search_queue q SET task_uid = p_task, task_queued_at = d.queued_at, claimed_until = NULL
    FROM unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE q.kind = d.kind AND q.target_id = d.id;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_task bigint) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_take_login_state(p_secret_hash bytea, p_state text) RETURNS TABLE(pkce_verifier text, nonce text, return_to text)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    DELETE FROM oidc_login_states
    WHERE secret_hash = p_secret_hash AND state = p_state AND expires_at > now()
    RETURNING pkce_verifier, nonce, return_to
  $$;


ALTER FUNCTION fvoci.app_take_login_state(p_secret_hash bytea, p_state text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text, p_ids uuid[]) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO users (id, issuer, subject, email, email_verified, display_name)
    VALUES (app_id(p_ids, 1), p_issuer, p_subject, p_email, coalesce(p_email_verified, false), p_display_name)
    ON CONFLICT ON CONSTRAINT users_identity
      DO UPDATE SET email = EXCLUDED.email,
                    email_verified = EXCLUDED.email_verified,
                    display_name = EXCLUDED.display_name,
                    last_login_at = now()
    RETURNING id
  $$;


ALTER FUNCTION fvoci.app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 요청 사용자는 트랜잭션의 app.user_id다. 미설정·빈 값은 NULL이며 사용자 RLS·권한 검사는 거부한다.
-- app_system_*의 명시적인 사용자 없음 경로는 서버 작업용이다. docs/decisions/access.md#d-08.
CREATE FUNCTION fvoci.app_user_id() RETURNS uuid
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT nullif(current_setting('app.user_id', true), '')::uuid $$;


ALTER FUNCTION fvoci.app_user_id() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_utc_text(p_at timestamp with time zone) RETURNS text
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT to_char(p_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
  $$;


ALTER FUNCTION fvoci.app_utc_text(p_at timestamp with time zone) OWNER TO fvoci_migrator;

-- 기본키·고유 제약.

ALTER TABLE ONLY fvoci.instance_state
    ADD CONSTRAINT instance_state_pkey PRIMARY KEY (key);

ALTER TABLE ONLY fvoci.oidc_login_states
    ADD CONSTRAINT oidc_login_states_pkey PRIMARY KEY (secret_hash);

ALTER TABLE ONLY fvoci.search_queue
    ADD CONSTRAINT search_queue_pkey PRIMARY KEY (kind, target_id);

ALTER TABLE ONLY fvoci.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.sessions
    ADD CONSTRAINT sessions_token_hash_key UNIQUE (token_hash);

ALTER TABLE ONLY fvoci.server_admins
    ADD CONSTRAINT server_admins_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY fvoci.users
    ADD CONSTRAINT users_identity UNIQUE (issuer, subject);

ALTER TABLE ONLY fvoci.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);

-- 조회·멱등성·중복 방지 인덱스.

CREATE INDEX oidc_login_states_expires ON fvoci.oidc_login_states USING btree (expires_at);

CREATE INDEX search_queue_order ON fvoci.search_queue USING btree (queued_at);

CREATE INDEX search_queue_task ON fvoci.search_queue USING btree (task_uid) WHERE (task_uid IS NOT NULL);

CREATE INDEX sessions_expires ON fvoci.sessions USING btree (expires_at);

CREATE INDEX sessions_user ON fvoci.sessions USING btree (user_id);

-- 불변식·감사·검색 대기열 트리거.


-- 외래 키. 참조 대상 테이블과 키를 준비한 뒤 연결한다.

ALTER TABLE ONLY fvoci.server_admins
    ADD CONSTRAINT server_admins_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.sessions
    ADD CONSTRAINT sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

-- 기준 적용 중 표시. SQLx는 파일마다 커밋하므로 중간 파일에서 멈추면 이 표시가 남고, 앱은 재생성이 아니라
-- db migrate를 다시 돌리라고 알린다. 마지막 파일이 최종 세대 표시로 덮어쓴다.
COMMENT ON TABLE fvoci._sqlx_migrations IS 'fvoci baseline 202610280001 applying';

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
