-- 첫 사용자 흐름: 사용자·세션·워크스페이스·멤버·위키 문서·리비전·감사 로그.
-- 실행 역할: fvoci_migrator (스키마 fvoci 소유자). 앱은 fvoci_app으로 접속하며 RLS가 적용된다.
-- 역할 생성은 `fvoci-server db bootstrap`이 한다. 결정: D-08, D-17, D-40, D-45, D-57, D-62, O-09.

-- 현재 요청의 사용자. 설정이 없거나 비어 있으면 NULL을 돌려 모든 정책이 거부되게 한다.
CREATE FUNCTION app_user_id() RETURNS uuid
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$ SELECT nullif(current_setting('app.user_id', true), '')::uuid $$;

------------------------------------------------------------------------------
-- 사용자와 세션
------------------------------------------------------------------------------

CREATE TABLE users (
  id            uuid PRIMARY KEY DEFAULT uuidv7(),
  issuer        text NOT NULL,
  subject       text NOT NULL,
  email         text,
  display_name  text NOT NULL,
  created_at    timestamptz NOT NULL DEFAULT now(),
  last_login_at timestamptz NOT NULL DEFAULT now(),
  -- 사용자 식별은 발급자 + subject. 이메일로 병합하지 않는다 (D-45).
  CONSTRAINT users_identity UNIQUE (issuer, subject)
);

-- 로그인 진행 중 상태(PKCE verifier, nonce, 돌아갈 경로). 쿠키에는 id만 담는다.
CREATE TABLE oidc_login_states (
  id            text PRIMARY KEY,
  pkce_verifier text NOT NULL,
  nonce         text NOT NULL,
  return_to     text NOT NULL,
  expires_at    timestamptz NOT NULL
);

-- 서버 측 세션. 토큰 원문은 저장하지 않고 SHA-256 해시만 저장한다.
CREATE TABLE sessions (
  id         uuid PRIMARY KEY DEFAULT uuidv7(),
  token_hash bytea NOT NULL UNIQUE,
  user_id    uuid NOT NULL REFERENCES users (id),
  id_token   text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  expires_at timestamptz NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);

------------------------------------------------------------------------------
-- 워크스페이스와 멤버
------------------------------------------------------------------------------

CREATE TABLE workspaces (
  id         uuid PRIMARY KEY DEFAULT uuidv7(),
  name       text NOT NULL CHECK (length(btrim(name)) > 0),
  created_by uuid NOT NULL REFERENCES users (id),
  request_id uuid NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  -- 재시도로 같은 워크스페이스가 두 번 생기지 않게 한다 (D-17).
  CONSTRAINT workspaces_request UNIQUE (created_by, request_id)
);

CREATE TABLE workspace_members (
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  user_id      uuid NOT NULL REFERENCES users (id),
  role         text NOT NULL CHECK (role IN ('admin', 'writer', 'reader')),
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user ON workspace_members (user_id);

-- 현재 사용자의 워크스페이스 역할. 정책 안에서 workspace_members를 다시 RLS로 읽으면 재귀가 되므로
-- 정의자 권한으로 읽는다. 멤버가 아니면 NULL.
CREATE FUNCTION app_member_role(ws uuid) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT role FROM workspace_members WHERE workspace_id = ws AND user_id = app_user_id()
  $$;

-- 대상 사용자가 현재 사용자와 워크스페이스를 하나라도 공유하는지.
CREATE FUNCTION app_shares_workspace(other uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (
      SELECT 1
      FROM workspace_members mine
      JOIN workspace_members theirs ON theirs.workspace_id = mine.workspace_id
      WHERE mine.user_id = app_user_id() AND theirs.user_id = other
    )
  $$;

------------------------------------------------------------------------------
-- 번호열과 문서
------------------------------------------------------------------------------

-- 사람이 보는 번호. 범위(scope)는 모듈 인스턴스 키 또는 WIKI. 원자적으로 증가하고 재사용하지 않는다 (D-17, D-48).
CREATE TABLE reference_counters (
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  scope        text NOT NULL,
  next_number  bigint NOT NULL CHECK (next_number > 0),
  PRIMARY KEY (workspace_id, scope)
);

-- 문서. 첫 흐름은 Tiptap JSON 본문을 리비전으로 저장한다 (D-62).
-- CRDT 도입 시 상태 열을 body 옆에 추가하며 id·number·revision은 그대로 이어진다.
CREATE TABLE documents (
  id             uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id   uuid NOT NULL REFERENCES workspaces (id),
  placement      text NOT NULL CHECK (placement IN ('wiki')),
  scope          text NOT NULL,
  number         bigint NOT NULL,
  title          text NOT NULL CHECK (length(btrim(title)) > 0),
  schema_version integer NOT NULL CHECK (schema_version > 0),
  body           jsonb NOT NULL,
  revision       integer NOT NULL CHECK (revision > 0),
  created_by     uuid NOT NULL REFERENCES users (id),
  created_at     timestamptz NOT NULL DEFAULT now(),
  updated_by     uuid NOT NULL REFERENCES users (id),
  updated_at     timestamptz NOT NULL DEFAULT now(),
  request_id     uuid NOT NULL,
  CONSTRAINT documents_number UNIQUE (workspace_id, scope, number),
  CONSTRAINT documents_request UNIQUE (workspace_id, created_by, request_id)
);

CREATE TABLE document_revisions (
  document_id    uuid NOT NULL REFERENCES documents (id),
  revision       integer NOT NULL CHECK (revision > 0),
  title          text NOT NULL,
  schema_version integer NOT NULL,
  body           jsonb NOT NULL,
  author         uuid NOT NULL REFERENCES users (id),
  request_id     uuid NOT NULL,
  created_at     timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (document_id, revision),
  -- 같은 저장 요청을 재시도하면 같은 리비전을 돌려준다.
  CONSTRAINT document_revisions_request UNIQUE (document_id, request_id)
);

------------------------------------------------------------------------------
-- 감사 로그 (D-57). 본문과 자격 증명은 기록하지 않는다.
------------------------------------------------------------------------------

CREATE TABLE audit_log (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id uuid REFERENCES workspaces (id),
  actor        uuid NOT NULL REFERENCES users (id),
  action       text NOT NULL,
  target_type  text NOT NULL,
  target_id    uuid NOT NULL,
  detail       jsonb NOT NULL DEFAULT '{}'::jsonb,
  created_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_workspace ON audit_log (workspace_id, created_at);

------------------------------------------------------------------------------
-- 정의자 함수: 사용자 문맥이 생기기 전이거나 여러 테이블을 원자적으로 만드는 작업만 둔다.
------------------------------------------------------------------------------

-- 로그인 성공 시 사용자 생성 또는 갱신. (issuer, subject)로만 찾는다.
CREATE FUNCTION app_upsert_user(p_issuer text, p_subject text, p_email text, p_display_name text)
  RETURNS uuid
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO users (issuer, subject, email, display_name)
    VALUES (p_issuer, p_subject, p_email, p_display_name)
    ON CONFLICT ON CONSTRAINT users_identity
      DO UPDATE SET email = EXCLUDED.email,
                    display_name = EXCLUDED.display_name,
                    last_login_at = now()
    RETURNING id
  $$;

-- 워크스페이스 생성: 워크스페이스, 생성자 관리자 멤버십, 감사 로그를 한 번에 만든다.
-- 같은 사용자가 같은 request_id로 다시 부르면 기존 id를 돌려준다.
CREATE FUNCTION app_create_workspace(p_request_id uuid, p_name text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'no current user' USING ERRCODE = '42501';
    END IF;

    INSERT INTO workspaces (name, created_by, request_id)
    VALUES (btrim(p_name), v_user, p_request_id)
    ON CONFLICT ON CONSTRAINT workspaces_request DO NOTHING
    RETURNING id INTO v_id;

    IF v_id IS NULL THEN
      SELECT id INTO v_id FROM workspaces WHERE created_by = v_user AND request_id = p_request_id;
      RETURN v_id;
    END IF;

    INSERT INTO workspace_members (workspace_id, user_id, role) VALUES (v_id, v_user, 'admin');
    INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
    VALUES (v_id, v_user, 'create', 'workspace', v_id, jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- RLS
------------------------------------------------------------------------------

ALTER TABLE users ENABLE ROW LEVEL SECURITY;
CREATE POLICY users_select ON users FOR SELECT
  USING (id = app_user_id() OR app_shares_workspace(id));

ALTER TABLE workspaces ENABLE ROW LEVEL SECURITY;
CREATE POLICY workspaces_select ON workspaces FOR SELECT
  USING (app_member_role(id) IS NOT NULL);
CREATE POLICY workspaces_update ON workspaces FOR UPDATE
  USING (app_member_role(id) = 'admin')
  WITH CHECK (app_member_role(id) = 'admin');

ALTER TABLE workspace_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY workspace_members_select ON workspace_members FOR SELECT
  USING (app_member_role(workspace_id) IS NOT NULL);

ALTER TABLE reference_counters ENABLE ROW LEVEL SECURITY;
CREATE POLICY reference_counters_all ON reference_counters FOR ALL
  USING (app_member_role(workspace_id) IN ('admin', 'writer'))
  WITH CHECK (app_member_role(workspace_id) IN ('admin', 'writer'));

-- 관리자는 워크스페이스의 모든 문서를 읽는다 (D-40). 개인 노트는 이 테이블이 아니라 별도 영역이다 (D-49, D-59).
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;
CREATE POLICY documents_select ON documents FOR SELECT
  USING (app_member_role(workspace_id) IS NOT NULL);
CREATE POLICY documents_insert ON documents FOR INSERT
  WITH CHECK (
    app_member_role(workspace_id) IN ('admin', 'writer')
    AND created_by = app_user_id()
    AND updated_by = app_user_id()
  );
CREATE POLICY documents_update ON documents FOR UPDATE
  USING (app_member_role(workspace_id) IN ('admin', 'writer'))
  WITH CHECK (app_member_role(workspace_id) IN ('admin', 'writer') AND updated_by = app_user_id());

ALTER TABLE document_revisions ENABLE ROW LEVEL SECURITY;
CREATE POLICY document_revisions_select ON document_revisions FOR SELECT
  USING (EXISTS (SELECT 1 FROM documents d WHERE d.id = document_id));
CREATE POLICY document_revisions_insert ON document_revisions FOR INSERT
  WITH CHECK (
    author = app_user_id()
    AND EXISTS (
      SELECT 1 FROM documents d
      WHERE d.id = document_id AND app_member_role(d.workspace_id) IN ('admin', 'writer')
    )
  );

ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
CREATE POLICY audit_log_select ON audit_log FOR SELECT
  USING (app_member_role(workspace_id) = 'admin');
CREATE POLICY audit_log_insert ON audit_log FOR INSERT
  WITH CHECK (
    actor = app_user_id()
    AND (workspace_id IS NULL OR app_member_role(workspace_id) IS NOT NULL)
  );

------------------------------------------------------------------------------
-- 앱 역할 권한. 삭제 권한은 주지 않는다 (휴지통은 별도 흐름에서 상태 열로 다룬다).
------------------------------------------------------------------------------

GRANT USAGE ON SCHEMA fvoci TO fvoci_app;
GRANT SELECT ON users TO fvoci_app;
GRANT SELECT, INSERT, DELETE ON oidc_login_states TO fvoci_app;
GRANT SELECT, INSERT, DELETE ON sessions TO fvoci_app;
GRANT SELECT, UPDATE (name) ON workspaces TO fvoci_app;
GRANT SELECT ON workspace_members TO fvoci_app;
GRANT SELECT, INSERT, UPDATE ON reference_counters TO fvoci_app;
GRANT SELECT, INSERT, UPDATE (title, body, schema_version, revision, updated_by, updated_at) ON documents TO fvoci_app;
GRANT SELECT, INSERT ON document_revisions TO fvoci_app;
GRANT SELECT, INSERT ON audit_log TO fvoci_app;

REVOKE ALL ON FUNCTION app_user_id() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_member_role(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_shares_workspace(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_upsert_user(text, text, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_workspace(uuid, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_user_id() TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_member_role(uuid) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_shares_workspace(uuid) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_upsert_user(text, text, text, text) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_create_workspace(uuid, text) TO fvoci_app;
