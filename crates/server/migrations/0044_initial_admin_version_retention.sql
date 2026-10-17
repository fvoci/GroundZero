-- 초기 관리자 (D-51, X-10)와 자료 버전 보존 (D-56).
--
-- 초기 관리자: 운영자가 설정(`FVOCI_INITIAL_ADMIN_SUBJECT`)으로 지정한 OIDC 식별자가 처음 로그인하면 첫 워크스페이스를
-- 만들고 그 사람을 소유자(관리자)로 둔다. 서버 인스턴스에서 한 번만 한다. "첫 로그인 사용자 자동 관리자"는 쓰지 않는다.
--
-- 자료 버전 보존: 워크스페이스 관리자가 이전 파일 버전의 보존 기간(일)을 정할 수 있다(비우면 영구 보존, 기본).
-- 작업 프로세스가 기간이 지난 이전 버전(지금 버전이 아닌 것)을 지우되, 분석 등 기록이 연결한 버전과 문서 리비전이
-- 본문에서 가리키는 버전(이미지 등)은 지우지 않는다. 그런 버전은 사람이 하나씩 지우려 해도 409로 막는다.

CREATE TABLE instance_state (
  key    text PRIMARY KEY,
  value  jsonb NOT NULL DEFAULT '{}'::jsonb,
  set_at timestamptz NOT NULL DEFAULT now()
);

-- 지정된 초기 관리자의 첫 워크스페이스(서버가 식별자를 확인한 뒤 부른다). 이미 했으면 NULL.
CREATE FUNCTION app_system_claim_initial_workspace(p_user uuid, p_name text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_me text := current_setting('app.user_id', true);
    v_ws uuid;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN NULL;
    END IF;
    INSERT INTO instance_state (key, value) VALUES ('initial_admin', jsonb_build_object('user', p_user))
    ON CONFLICT (key) DO NOTHING;
    IF NOT FOUND THEN
      RETURN NULL;
    END IF;
    PERFORM set_config('app.user_id', p_user::text, true);
    v_ws := app_create_workspace(uuidv7(), coalesce(nullif(btrim(p_name), ''), 'fvoci'));
    PERFORM set_config('app.user_id', coalesce(v_me, ''), true);
    UPDATE instance_state SET value = value || jsonb_build_object('workspace', v_ws) WHERE key = 'initial_admin';
    RETURN v_ws;
  END
  $$;

ALTER TABLE workspaces ADD COLUMN version_retention_days integer
  CHECK (version_retention_days IS NULL OR version_retention_days BETWEEN 1 AND 3650);

CREATE FUNCTION app_set_version_retention(p_workspace uuid, p_days integer) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old integer;
  BEGIN
    IF app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_workspace) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can change version retention' USING ERRCODE = '42501';
    END IF;
    IF p_days IS NOT NULL AND p_days NOT BETWEEN 1 AND 3650 THEN
      RAISE EXCEPTION 'retention must be 1 to 3650 days' USING ERRCODE = '22023';
    END IF;
    SELECT version_retention_days INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET version_retention_days = p_days WHERE id = p_workspace;
    PERFORM app_audit(p_workspace, 'set_version_retention', 'workspace', p_workspace,
                      jsonb_build_object('from', v_old, 'to', p_days));
  END
  $$;

-- 버전을 붙잡고 있는 것: 기록 연결(D-35) 또는 문서 리비전 본문의 그 버전 주소(D-56).
CREATE FUNCTION app_version_referenced(p_file uuid, p_version integer, p_version_id uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (SELECT 1 FROM record_files WHERE version_id = p_version_id)
        OR EXISTS (SELECT 1 FROM document_revisions
                   WHERE strpos(body::text, '/api/files/' || p_file || '/content?version=' || p_version || '&') > 0
                      OR strpos(body::text, '/api/files/' || p_file || '/content?version=' || p_version || '"') > 0)
  $$;

CREATE OR REPLACE FUNCTION app_guard_pinned_version() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF current_setting('fvoci.purging_files', true) = 'on' THEN
      RETURN OLD;
    END IF;
    IF app_version_referenced(OLD.file_id, OLD.version, OLD.id) THEN
      RAISE EXCEPTION 'this version is referenced by a record or a document revision' USING ERRCODE = '55000';
    END IF;
    RETURN OLD;
  END
  $$;

-- 작업: 보존 기간이 지난 이전 버전 정리. 지운 수.
CREATE FUNCTION app_system_purge_old_versions() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row   record;
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_row IN
      SELECT v.id, v.file_id, v.version
      FROM file_versions v
      JOIN files f ON f.id = v.file_id
      JOIN workspaces w ON w.id = f.workspace_id
      WHERE w.version_retention_days IS NOT NULL
        AND v.version <> f.current_version
        AND v.created_at < now() - make_interval(days => w.version_retention_days)
      ORDER BY v.created_at
      LIMIT 500
    LOOP
      IF app_version_referenced(v_row.file_id, v_row.version, v_row.id) THEN
        CONTINUE;
      END IF;
      UPDATE uploads SET version_id = NULL WHERE version_id = v_row.id;
      DELETE FROM file_versions WHERE id = v_row.id;
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  END
  $$;

ALTER TABLE instance_state ENABLE ROW LEVEL SECURITY;
REVOKE ALL ON FUNCTION app_system_claim_initial_workspace(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_version_retention(uuid, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_version_referenced(uuid, integer, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_purge_old_versions() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_system_claim_initial_workspace(uuid, text),
  app_set_version_retention(uuid, integer),
  app_system_purge_old_versions()
TO fvoci_app;
