-- 첫 흐름 보안 보강 (검토 결과 반영).
-- 1. 세션·로그인 상태는 앱 역할이 테이블을 직접 읽지 못하게 하고 해시 키 정의자 함수로만 다룬다.
-- 2. 로그인 상태는 쿠키 비밀값의 해시로 찾고, OIDC state는 행에 따로 저장해 비교한다.
-- 3. 번호 할당·문서 생성·감사 기록은 정의자 함수와 트리거만 쓴다. 앱 역할은 번호·리비전을 임의로 정하지 못한다.
-- 4. 리비전 행은 문서의 현재 리비전과 같아야 하고, 문서 리비전은 1씩만 오른다.

------------------------------------------------------------------------------
-- 로그인 상태
------------------------------------------------------------------------------

DROP TABLE oidc_login_states;
CREATE TABLE oidc_login_states (
  secret_hash   bytea PRIMARY KEY,
  state         text NOT NULL,
  pkce_verifier text NOT NULL,
  nonce         text NOT NULL,
  return_to     text NOT NULL,
  expires_at    timestamptz NOT NULL
);
CREATE INDEX oidc_login_states_expires ON oidc_login_states (expires_at);
CREATE INDEX sessions_expires ON sessions (expires_at);

CREATE FUNCTION app_create_login_state(
  p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer
) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

-- 쿠키 비밀값의 해시로 로그인 상태를 꺼내며 지운다(한 번만 쓸 수 있다).
CREATE FUNCTION app_take_login_state(p_secret_hash bytea)
  RETURNS TABLE (state text, pkce_verifier text, nonce text, return_to text)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    DELETE FROM oidc_login_states
    WHERE secret_hash = p_secret_hash AND expires_at > now()
    RETURNING state, pkce_verifier, nonce, return_to
  $$;

------------------------------------------------------------------------------
-- 세션
------------------------------------------------------------------------------

CREATE FUNCTION app_create_session(
  p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea
) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    -- 로그인 중이던 세션이 있으면 회전한다(이전 토큰 무효).
    IF p_replaces IS NOT NULL THEN
      DELETE FROM sessions WHERE token_hash = p_replaces;
    END IF;
    DELETE FROM sessions
    WHERE id IN (SELECT id FROM sessions WHERE expires_at < now() LIMIT 100);
    INSERT INTO sessions (token_hash, user_id, id_token, expires_at)
    VALUES (p_token_hash, p_user, p_id_token, now() + make_interval(secs => p_ttl_secs));
  END
  $$;

CREATE FUNCTION app_session_user(p_token_hash bytea) RETURNS uuid
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT user_id FROM sessions WHERE token_hash = p_token_hash AND expires_at > now() $$;

-- 세션을 끝내고 제공자 로그아웃에 쓸 ID 토큰을 돌려준다.
CREATE FUNCTION app_end_session(p_token_hash bytea) RETURNS text
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ DELETE FROM sessions WHERE token_hash = p_token_hash RETURNING id_token $$;

REVOKE ALL ON oidc_login_states FROM fvoci_app;
REVOKE ALL ON sessions FROM fvoci_app;

------------------------------------------------------------------------------
-- 문서 생성: 번호 할당 + 문서 + 리비전 1 + 감사 기록을 한 함수로
------------------------------------------------------------------------------

CREATE FUNCTION app_create_wiki_document(
  p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb
) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_id     uuid;
    v_number bigint;
  BEGIN
    IF v_user IS NULL OR app_member_role(p_workspace) NOT IN ('admin', 'writer')
       OR app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'not allowed' USING ERRCODE = '42501';
    END IF;

    SELECT id INTO v_id FROM documents
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;

    INSERT INTO reference_counters (workspace_id, scope, next_number) VALUES (p_workspace, 'WIKI', 2)
    ON CONFLICT (workspace_id, scope)
      DO UPDATE SET next_number = reference_counters.next_number + 1
    RETURNING next_number - 1 INTO v_number;

    INSERT INTO documents (workspace_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (p_workspace, 'wiki', 'WIKI', v_number, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id)
    RETURNING id INTO v_id;

    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);

    INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
    VALUES (p_workspace, v_user, 'create', 'document', v_id, jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

REVOKE ALL ON reference_counters FROM fvoci_app;
REVOKE INSERT ON documents FROM fvoci_app;
REVOKE INSERT ON audit_log FROM fvoci_app;

------------------------------------------------------------------------------
-- 불변식 트리거
------------------------------------------------------------------------------

-- 문서 리비전은 저장마다 정확히 1 오른다.
CREATE FUNCTION documents_revision_step() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF NEW.revision <> OLD.revision + 1 THEN
      RAISE EXCEPTION 'revision must advance by one' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
  END
  $$;
CREATE TRIGGER documents_revision_step BEFORE UPDATE ON documents
  FOR EACH ROW EXECUTE FUNCTION documents_revision_step();

-- 리비전 행은 문서의 현재 리비전과 같은 번호여야 한다.
CREATE FUNCTION document_revisions_match() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF NEW.revision <> (SELECT revision FROM documents WHERE id = NEW.document_id) THEN
      RAISE EXCEPTION 'revision row must match the document revision' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
  END
  $$;
CREATE TRIGGER document_revisions_match BEFORE INSERT ON document_revisions
  FOR EACH ROW EXECUTE FUNCTION document_revisions_match();

-- 제목이 바뀌면 감사 기록(본문은 남기지 않는다). 앱 역할에는 감사 기록 INSERT 권한이 없다.
CREATE FUNCTION documents_audit_rename() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF NEW.title IS DISTINCT FROM OLD.title THEN
      INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
      VALUES (NEW.workspace_id, NEW.updated_by, 'rename', 'document', NEW.id,
              jsonb_build_object('from', OLD.title, 'to', NEW.title));
    END IF;
    RETURN NULL;
  END
  $$;
CREATE TRIGGER documents_audit_rename AFTER UPDATE ON documents
  FOR EACH ROW EXECUTE FUNCTION documents_audit_rename();

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_create_login_state(bytea, text, text, text, text, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_take_login_state(bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_session(bytea, uuid, text, bigint, bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_session_user(bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_end_session(bytea) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_wiki_document(uuid, uuid, text, integer, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION documents_revision_step() FROM PUBLIC;
REVOKE ALL ON FUNCTION document_revisions_match() FROM PUBLIC;
REVOKE ALL ON FUNCTION documents_audit_rename() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_create_login_state(bytea, text, text, text, text, integer) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_take_login_state(bytea) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_create_session(bytea, uuid, text, bigint, bytea) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_session_user(bytea) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_end_session(bytea) TO fvoci_app;
GRANT EXECUTE ON FUNCTION app_create_wiki_document(uuid, uuid, text, integer, jsonb) TO fvoci_app;
