-- 실시간 공동 편집(D-27 워크스페이스별 ON/OFF, D-70 Rust/yrs).
-- 문서의 진실은 지금처럼 documents.body와 리비전이다. 실시간 ON이면 편집 중인 Yjs 상태를 document_collab_states에
-- 두고, 일정 시간 조용하거나 마지막 사람이 나가면 같은 저장 경로로 리비전을 만든다. Yjs 상태는 그 상태를 만든
-- 리비전 번호를 함께 적어, 문서가 다른 길(복원·실시간 OFF 중 저장)로 바뀌었으면 다음에 열 때 본문에서 다시 만든다.

ALTER TABLE workspaces ADD COLUMN realtime_enabled boolean NOT NULL DEFAULT false;

CREATE TABLE document_collab_states (
  document_id uuid PRIMARY KEY REFERENCES documents (id) ON DELETE CASCADE,
  -- 이 상태가 이어받은 문서 리비전. documents.revision과 다르면 상태를 버리고 본문에서 다시 만든다.
  revision    integer NOT NULL,
  state       bytea NOT NULL,
  updated_at  timestamptz NOT NULL DEFAULT now()
);
ALTER TABLE document_collab_states ENABLE ROW LEVEL SECURITY;

-- 실시간 협업 켜고 끄기(관리자).
CREATE FUNCTION app_set_workspace_realtime(p_workspace uuid, p_enabled boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old boolean;
  BEGIN
    IF app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_workspace) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can change realtime editing' USING ERRCODE = '42501';
    END IF;
    SELECT realtime_enabled INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET realtime_enabled = p_enabled WHERE id = p_workspace;
    IF v_old IS DISTINCT FROM p_enabled THEN
      PERFORM app_audit(p_workspace, 'set_realtime', 'workspace', p_workspace,
                        jsonb_build_object('enabled', p_enabled));
    END IF;
  END
  $$;

-- 문서가 실시간 편집 중인지(워크스페이스 문서이고 워크스페이스가 켰다). 개인 노트는 실시간이 아니다.
CREATE FUNCTION app_document_realtime(p_doc uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce((SELECT w.realtime_enabled FROM documents d JOIN workspaces w ON w.id = d.workspace_id
                     WHERE d.id = p_doc), false)
  $$;

-- 실시간 연결의 권한: 볼 수 없으면 404, 실시간이 꺼져 있으면 409. 읽기(`read`) 또는 쓰기(`write`·`owner`).
CREATE FUNCTION app_collab_access(p_doc uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_document_access(p_doc);
  BEGIN
    IF app_user_id() IS NULL OR v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT app_document_realtime(p_doc) THEN
      RAISE EXCEPTION 'realtime editing is off for this document' USING ERRCODE = '55000';
    END IF;
    RETURN v_access;
  END
  $$;

-- 방을 열 때 읽는 것: 저장된 Yjs 상태(문서 리비전과 같을 때만)와 문서 본문·리비전. 볼 수 있는 사람만.
CREATE FUNCTION app_collab_load(p_doc uuid)
  RETURNS TABLE (doc_revision integer, title text, body jsonb, state bytea)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_collab_access(p_doc);
    RETURN QUERY
    SELECT d.revision, d.title, d.body,
           (SELECT s.state FROM document_collab_states s WHERE s.document_id = d.id AND s.revision = d.revision)
    FROM documents d WHERE d.id = p_doc;
  END
  $$;

-- 편집 중 상태 저장(서버 작업만, 사용자 문맥 없음). 리비전이 이미 바뀌었으면 저장하지 않는다(거짓).
CREATE FUNCTION app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea) RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RAISE EXCEPTION 'system function' USING ERRCODE = '42501';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND revision = p_revision AND deleted_at IS NULL) THEN
      RETURN false;
    END IF;
    INSERT INTO document_collab_states (document_id, revision, state, updated_at)
    VALUES (p_doc, p_revision, p_state, now())
    ON CONFLICT (document_id) DO UPDATE SET revision = excluded.revision, state = excluded.state, updated_at = now();
    RETURN true;
  END
  $$;

REVOKE ALL ON FUNCTION app_set_workspace_realtime(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_document_realtime(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_collab_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_collab_load(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_collab_store(uuid, integer, bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_set_workspace_realtime(uuid, boolean),
  app_document_realtime(uuid),
  app_collab_access(uuid),
  app_collab_load(uuid),
  app_system_collab_store(uuid, integer, bytea)
TO fvoci_app;
