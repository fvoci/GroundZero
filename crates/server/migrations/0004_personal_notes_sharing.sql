-- 개인 노트·명시적 공유·문서별 비공개·복제 허용·복제·이동 (D-20, D-22, D-23, D-24, D-25, D-40, D-49, D-57, D-59).
-- 문서 접근 판정은 app_document_access() 하나로 모은다. RLS와 서버 코드가 같은 함수를 쓴다.

------------------------------------------------------------------------------
-- 문서: 워크스페이스 소유 또는 사용자 소유(개인 노트) 중 정확히 하나
------------------------------------------------------------------------------

ALTER TABLE documents ALTER COLUMN workspace_id DROP NOT NULL;
ALTER TABLE documents ADD COLUMN owner_user_id uuid REFERENCES users (id);
ALTER TABLE documents ADD CONSTRAINT documents_one_owner
  CHECK ((workspace_id IS NULL) <> (owner_user_id IS NULL));
ALTER TABLE documents DROP CONSTRAINT documents_placement_check;
ALTER TABLE documents ADD CONSTRAINT documents_placement_check
  CHECK (placement IN ('wiki', 'personal'));
ALTER TABLE documents ALTER COLUMN number DROP NOT NULL;
ALTER TABLE documents ADD CONSTRAINT documents_personal_shape CHECK (
  (placement = 'personal') = (owner_user_id IS NOT NULL)
  AND (placement = 'personal') = (number IS NULL)
);
-- 문서별 비공개 (D-20, D-40): private 팀 문서는 작성자·관리자·명시적 공유 대상만 본다.
ALTER TABLE documents ADD COLUMN visibility text NOT NULL DEFAULT 'workspace'
  CHECK (visibility IN ('workspace', 'private'));
-- 복제 허용 (D-20). 소유 권한자는 항상 복제할 수 있다.
ALTER TABLE documents ADD COLUMN allow_copy boolean NOT NULL DEFAULT false;
-- 개인 노트의 생성 재시도 방지(워크스페이스 문서는 documents_request가 막는다).
CREATE UNIQUE INDEX documents_personal_request ON documents (owner_user_id, request_id)
  WHERE owner_user_id IS NOT NULL;

------------------------------------------------------------------------------
-- 명시적 공유 (D-23, D-49). 대상은 IdP가 검증한 이메일로 맞춘다(사용자 목록 노출 방지).
------------------------------------------------------------------------------

CREATE TABLE document_shares (
  id            uuid PRIMARY KEY,
  document_id   uuid NOT NULL REFERENCES documents (id),
  grantee_email text NOT NULL CHECK (grantee_email = lower(btrim(grantee_email))),
  permission    text NOT NULL CHECK (permission IN ('read', 'write')),
  granted_by    uuid NOT NULL REFERENCES users (id),
  created_at    timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT document_shares_target UNIQUE (document_id, grantee_email)
);
CREATE INDEX document_shares_email ON document_shares (grantee_email);

------------------------------------------------------------------------------
-- 접근 판정: 'owner' | 'write' | 'read' | NULL
--   개인 노트 소유자 → owner, 워크스페이스 관리자 → owner (D-40),
--   작성자 → write (private이면 본인이 만든 문서만), 독자 → read (같은 private 규칙),
--   명시적 공유 → 그 권한. 개인 노트에는 워크스페이스가 없으므로 다른 사람의 관리자에게 권한이 가지 않는다 (D-49).
------------------------------------------------------------------------------

CREATE FUNCTION app_document_access(p_doc uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_doc   documents%ROWTYPE;
    v_role  text;
    v_base  text;
    v_share text;
  BEGIN
    IF v_user IS NULL THEN
      RETURN NULL;
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF v_doc.id IS NULL THEN
      RETURN NULL;
    END IF;

    IF v_doc.owner_user_id IS NOT NULL THEN
      IF v_doc.owner_user_id = v_user THEN
        RETURN 'owner';
      END IF;
    ELSE
      v_role := app_member_role(v_doc.workspace_id);
      IF v_role = 'admin' THEN
        RETURN 'owner';
      ELSIF v_role IS NOT NULL AND (v_doc.visibility = 'workspace' OR v_doc.created_by = v_user) THEN
        v_base := CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
      END IF;
    END IF;

    SELECT s.permission INTO v_share
    FROM document_shares s JOIN users u ON u.id = v_user
    WHERE s.document_id = p_doc AND u.email_verified AND lower(u.email) = s.grantee_email;

    IF v_base = 'write' OR v_share = 'write' THEN
      RETURN 'write';
    END IF;
    RETURN coalesce(v_base, v_share);
  END
  $$;

------------------------------------------------------------------------------
-- RLS 교체
------------------------------------------------------------------------------

DROP POLICY documents_select ON documents;
DROP POLICY documents_insert ON documents;
DROP POLICY documents_update ON documents;
CREATE POLICY documents_select ON documents FOR SELECT
  USING (app_document_access(id) IS NOT NULL);
CREATE POLICY documents_update ON documents FOR UPDATE
  USING (app_document_access(id) IN ('owner', 'write'))
  WITH CHECK (app_document_access(id) IN ('owner', 'write') AND updated_by = app_user_id());

DROP POLICY document_revisions_insert ON document_revisions;
CREATE POLICY document_revisions_insert ON document_revisions FOR INSERT
  WITH CHECK (author = app_user_id() AND app_document_access(document_id) IN ('owner', 'write'));
-- 리비전 읽기는 기존 정책(문서를 볼 수 있으면)이 그대로 맞다.

ALTER TABLE document_shares ENABLE ROW LEVEL SECURITY;
-- 공유 목록은 공유를 관리할 수 있는 사람(소유 권한자)만 본다.
CREATE POLICY document_shares_select ON document_shares FOR SELECT
  USING (app_document_access(document_id) = 'owner');
GRANT SELECT ON document_shares TO fvoci_app;

------------------------------------------------------------------------------
-- 리비전 트리거: 내용(제목·본문·스키마)이 바뀔 때만 정확히 1 오른다. 이동·설정 변경은 리비전을 바꾸지 않는다.
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION documents_revision_step() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_content_changed boolean :=
      NEW.title IS DISTINCT FROM OLD.title
      OR NEW.body IS DISTINCT FROM OLD.body
      OR NEW.schema_version IS DISTINCT FROM OLD.schema_version;
  BEGIN
    IF v_content_changed AND NEW.revision <> OLD.revision + 1 THEN
      RAISE EXCEPTION 'content change must advance the revision by one' USING ERRCODE = '23514';
    END IF;
    -- 내용 없이 리비전만 올리면 실제로 없던 리비전 기록을 끼워 넣을 수 있다. 막는다.
    IF NOT v_content_changed AND NEW.revision <> OLD.revision THEN
      RAISE EXCEPTION 'revision changes only with content' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
  END
  $$;

------------------------------------------------------------------------------
-- 감사 기록: 개인 영역 기록은 소유자가 본다 (D-57).
------------------------------------------------------------------------------

ALTER TABLE audit_log ADD COLUMN owner_user_id uuid REFERENCES users (id);
DROP POLICY audit_log_select ON audit_log;
CREATE POLICY audit_log_select ON audit_log FOR SELECT
  USING (app_member_role(workspace_id) = 'admin' OR owner_user_id = app_user_id());

-- 문서 대상 감사 기록. 문서의 소유 영역(워크스페이스 또는 개인)에 남긴다.
CREATE FUNCTION app_audit_document(p_doc uuid, p_action text, p_detail jsonb) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    SELECT d.workspace_id, d.owner_user_id, app_user_id(), p_action, 'document', d.id,
           coalesce(p_detail, '{}'::jsonb)
    FROM documents d WHERE d.id = p_doc
  $$;

CREATE OR REPLACE FUNCTION documents_audit_rename() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF NEW.title IS DISTINCT FROM OLD.title THEN
      INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (NEW.workspace_id, NEW.owner_user_id, NEW.updated_by, 'rename', 'document', NEW.id,
              jsonb_build_object('from', OLD.title, 'to', NEW.title));
    END IF;
    RETURN NULL;
  END
  $$;

------------------------------------------------------------------------------
-- 공통: 소유 권한 확인
------------------------------------------------------------------------------

-- 문서에 대한 현재 사용자의 권한. 볼 수 없으면 P0002, 기대보다 낮으면 42501.
CREATE FUNCTION app_require_document(p_doc uuid, p_need text) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_document_access(p_doc);
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF (p_need = 'owner' AND v_access <> 'owner')
       OR (p_need = 'write' AND v_access NOT IN ('owner', 'write')) THEN
      RAISE EXCEPTION 'insufficient permission' USING ERRCODE = '42501';
    END IF;
    RETURN v_access;
  END
  $$;

-- 다음 위키 번호(호출자가 작성 권한을 이미 확인했다고 가정하는 내부 함수).
CREATE FUNCTION app_next_wiki_number(p_workspace uuid) RETURNS bigint
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO reference_counters (workspace_id, scope, next_number) VALUES (p_workspace, 'WIKI', 2)
    ON CONFLICT (workspace_id, scope)
      DO UPDATE SET next_number = reference_counters.next_number + 1
    RETURNING next_number - 1
  $$;

------------------------------------------------------------------------------
-- 개인 노트 생성
------------------------------------------------------------------------------

CREATE FUNCTION app_create_personal_note(
  p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb
) RETURNS uuid
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
    SELECT id INTO v_id FROM documents WHERE owner_user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    INSERT INTO documents (owner_user_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id, visibility)
    VALUES (v_user, 'personal', 'NOTE', NULL, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id, 'private')
    RETURNING id INTO v_id;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 설정·공유 (소유 권한자)
------------------------------------------------------------------------------

CREATE FUNCTION app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE;
  BEGIN
    -- 행을 먼저 잠그고 권한을 본다. 동시에 이동된 문서에 이전 소유 권한으로 설정을 바꾸지 못하게 한다.
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document(p_doc, 'owner');
    IF p_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF v_doc.owner_user_id IS NOT NULL AND p_visibility <> 'private' THEN
      RAISE EXCEPTION 'personal notes are always private' USING ERRCODE = '22023';
    END IF;
    UPDATE documents SET visibility = p_visibility, allow_copy = p_allow_copy WHERE id = p_doc;
    IF v_doc.visibility IS DISTINCT FROM p_visibility THEN
      PERFORM app_audit_document(p_doc, 'change_visibility',
                                 jsonb_build_object('from', v_doc.visibility, 'to', p_visibility));
    END IF;
    IF v_doc.allow_copy IS DISTINCT FROM p_allow_copy THEN
      PERFORM app_audit_document(p_doc, 'change_copy_policy',
                                 jsonb_build_object('allow_copy', p_allow_copy));
    END IF;
  END
  $$;

CREATE FUNCTION app_share_document(p_doc uuid, p_email text, p_permission text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_email text := lower(btrim(p_email));
    v_id    uuid;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document(p_doc, 'owner');
    IF p_permission NOT IN ('read', 'write') THEN
      RAISE EXCEPTION 'invalid permission' USING ERRCODE = '22023';
    END IF;
    IF position('@' in v_email) < 2 THEN
      RAISE EXCEPTION 'invalid email' USING ERRCODE = '22023';
    END IF;
    INSERT INTO document_shares (document_id, grantee_email, permission, granted_by)
    VALUES (p_doc, v_email, p_permission, app_user_id())
    ON CONFLICT ON CONSTRAINT document_shares_target
      DO UPDATE SET permission = EXCLUDED.permission, granted_by = EXCLUDED.granted_by
    RETURNING id INTO v_id;
    PERFORM app_audit_document(p_doc, 'share',
                               jsonb_build_object('email', v_email, 'permission', p_permission));
    RETURN v_id;
  END
  $$;

CREATE FUNCTION app_unshare_document(p_share uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_share document_shares%ROWTYPE;
  BEGIN
    SELECT * INTO v_share FROM document_shares WHERE id = p_share;
    PERFORM 1 FROM documents WHERE id = v_share.document_id FOR UPDATE;
    IF v_share.id IS NULL OR app_document_access(v_share.document_id) IS NULL THEN
      RAISE EXCEPTION 'share not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_document(v_share.document_id, 'owner');
    DELETE FROM document_shares WHERE id = p_share;
    PERFORM app_audit_document(v_share.document_id, 'unshare',
                               jsonb_build_object('email', v_share.grantee_email));
  END
  $$;

------------------------------------------------------------------------------
-- 복제: 독립된 새 문서(리비전 1, 이력 없음). 이후 원본과 동기화하지 않는다 (D-23).
------------------------------------------------------------------------------

CREATE FUNCTION app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_access text := app_require_document(p_doc, 'read');
    v_src    documents%ROWTYPE;
    v_id     uuid;
  BEGIN
    SELECT * INTO v_src FROM documents WHERE id = p_doc;
    IF v_access <> 'owner' AND NOT v_src.allow_copy THEN
      RAISE EXCEPTION 'copying is not allowed for this document' USING ERRCODE = '42501';
    END IF;

    IF p_target_workspace IS NULL THEN
      SELECT id INTO v_id FROM documents WHERE owner_user_id = v_user AND request_id = p_request_id;
      IF v_id IS NOT NULL THEN RETURN v_id; END IF;
      INSERT INTO documents (owner_user_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (v_user, 'personal', 'NOTE', NULL, btrim(p_title), v_src.schema_version, v_src.body,
              1, v_user, v_user, p_request_id, 'private')
      RETURNING id INTO v_id;
    ELSE
      IF app_member_role(p_target_workspace) IS NULL THEN
        RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
      END IF;
      IF app_member_role(p_target_workspace) NOT IN ('admin', 'writer') THEN
        RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
      END IF;
      SELECT id INTO v_id FROM documents
      WHERE workspace_id = p_target_workspace AND created_by = v_user AND request_id = p_request_id;
      IF v_id IS NOT NULL THEN RETURN v_id; END IF;
      -- 비공개 문서·개인 노트의 사본은 비공개로 만든다(복제로 볼 수 있는 사람이 조용히 늘지 않게).
      INSERT INTO documents (workspace_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (p_target_workspace, 'wiki', 'WIKI', app_next_wiki_number(p_target_workspace),
              btrim(p_title), v_src.schema_version, v_src.body, 1, v_user, v_user, p_request_id,
              CASE WHEN v_src.visibility = 'private' THEN 'private' ELSE 'workspace' END)
      RETURNING id INTO v_id;
    END IF;

    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), v_src.schema_version, v_src.body, v_user, p_request_id);
    -- 감사 기록에는 다른 영역의 ID를 남기지 않는다(각 영역 관리자가 서로의 존재를 알지 않게).
    PERFORM app_audit_document(p_doc, 'copy',
      jsonb_build_object('target', CASE WHEN p_target_workspace IS NULL THEN 'personal' ELSE 'workspace' END));
    PERFORM app_audit_document(v_id, 'create',
      jsonb_build_object('copied', true, 'from', CASE WHEN v_src.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 이동 (D-24, D-59): 개인 노트 → 워크스페이스, 워크스페이스 → 다른 워크스페이스.
-- 팀 문서를 개인 노트로 빼는 이동은 막는다(D-25, 복제를 쓴다).
------------------------------------------------------------------------------

-- 문서를 볼 수 있는 사용자 집합(이동 전후 비교용). 공유 대상은 이메일로 맞춘다.
CREATE FUNCTION app_document_audience(p_workspace uuid, p_owner uuid, p_creator uuid, p_visibility text, p_doc uuid)
  RETURNS TABLE (user_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT p_owner WHERE p_owner IS NOT NULL
    UNION
    SELECT m.user_id FROM workspace_members m
    WHERE p_workspace IS NOT NULL AND m.workspace_id = p_workspace
      AND (m.role = 'admin' OR p_visibility = 'workspace' OR m.user_id = p_creator)
    UNION
    SELECT u.id FROM document_shares s JOIN users u
      ON u.email_verified AND lower(u.email) = s.grantee_email
    WHERE s.document_id = p_doc
  $$;

CREATE FUNCTION app_check_move(p_doc uuid, p_target uuid) RETURNS documents
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE;
  BEGIN
    PERFORM app_require_document(p_doc, 'owner');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF p_target IS NULL THEN
      RAISE EXCEPTION 'team documents stay in the workspace; make a copy instead' USING ERRCODE = '55000';
    END IF;
    IF v_doc.workspace_id = p_target THEN
      RAISE EXCEPTION 'the document is already in that workspace' USING ERRCODE = '55000';
    END IF;
    IF app_member_role(p_target) IS NULL THEN
      RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_target) NOT IN ('admin', 'writer') THEN
      RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
    END IF;
    RETURN v_doc;
  END
  $$;

-- 이동 미리보기: 접근이 생기는 사람과 사라지는 사람(표시 이름).
CREATE FUNCTION app_move_preview(p_doc uuid, p_target uuid)
  RETURNS TABLE (change text, display_name text)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE := app_check_move(p_doc, p_target);
  BEGIN
    RETURN QUERY
    WITH old_set AS (
      SELECT a.user_id AS uid FROM app_document_audience(v_doc.workspace_id, v_doc.owner_user_id,
                                                         v_doc.created_by, v_doc.visibility, p_doc) a
    ), new_set AS (
      SELECT a.user_id AS uid FROM app_document_audience(p_target, NULL, v_doc.created_by,
                                                         v_doc.visibility, p_doc) a
    )
    SELECT 'gain'::text, u.display_name FROM new_set n JOIN users u ON u.id = n.uid
      WHERE n.uid NOT IN (SELECT o.uid FROM old_set o)
    UNION ALL
    SELECT 'lose'::text, u.display_name FROM old_set o JOIN users u ON u.id = o.uid
      WHERE o.uid NOT IN (SELECT n.uid FROM new_set n)
    ORDER BY 1, 2;
  END
  $$;

-- 이동. 행을 먼저 잠근 뒤 권한·대상을 확인한다(동시에 다른 곳으로 옮겨진 문서를 이전 권한으로 옮기지 않게).
-- 이동 후 이동한 사람이 문서를 볼 수 없을 수도 있으므로(비공개 문서를 작성 권한만 있는 곳으로 옮길 때)
-- 새 위치와 참조를 돌려준다.
CREATE FUNCTION app_move_document(p_doc uuid, p_target uuid)
  RETURNS TABLE (workspace_id uuid, reference text)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc    documents%ROWTYPE;
    v_ref    text;
    v_number bigint;
  BEGIN
    PERFORM 1 FROM documents d WHERE d.id = p_doc FOR UPDATE;
    v_doc := app_check_move(p_doc, p_target);
    v_ref := CASE WHEN v_doc.number IS NULL THEN NULL ELSE v_doc.scope || '-' || v_doc.number END;
    -- 원래 영역에 떠남을 남긴다. 다른 영역의 ID는 남기지 않는다.
    PERFORM app_audit_document(p_doc, 'move_out', jsonb_build_object('reference', v_ref));
    v_number := app_next_wiki_number(p_target);
    UPDATE documents d
    SET workspace_id = p_target, owner_user_id = NULL, placement = 'wiki', scope = 'WIKI', number = v_number
    WHERE d.id = p_doc;
    -- 새 영역에 들어옴을 남긴다. 링크는 id 기준이라 이전 참조로 걸린 링크도 유지된다 (D-18).
    PERFORM app_audit_document(p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN QUERY SELECT p_target, 'WIKI-' || v_number;
  END
  $$;

------------------------------------------------------------------------------
-- 내 노트·공유받은 문서 목록 (소유자 표시 이름 포함)
------------------------------------------------------------------------------

-- 공유한 사람의 이름을 보여준다. 워크스페이스 이름은 멤버가 아닌 사람에게 알리지 않는다.
CREATE FUNCTION app_shared_with_me()
  RETURNS TABLE (id uuid, title text, permission text, shared_by text, updated_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT d.id, d.title, s.permission, granter.display_name, d.updated_at
    FROM document_shares s
    JOIN users me ON me.id = app_user_id() AND me.email_verified AND lower(me.email) = s.grantee_email
    JOIN documents d ON d.id = s.document_id
    JOIN users granter ON granter.id = s.granted_by
    ORDER BY d.updated_at DESC
  $$;

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

DO $$
DECLARE
  f text;
BEGIN
  FOREACH f IN ARRAY ARRAY[
    'app_document_access(uuid)',
    'app_audit_document(uuid, text, jsonb)',
    'app_require_document(uuid, text)',
    'app_next_wiki_number(uuid)',
    'app_create_personal_note(uuid, text, integer, jsonb)',
    'app_set_document_settings(uuid, text, boolean)',
    'app_share_document(uuid, text, text)',
    'app_unshare_document(uuid)',
    'app_copy_document(uuid, uuid, uuid, text)',
    'app_document_audience(uuid, uuid, uuid, text, uuid)',
    'app_check_move(uuid, uuid)',
    'app_move_preview(uuid, uuid)',
    'app_move_document(uuid, uuid)',
    'app_shared_with_me()'
  ] LOOP
    EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', f);
  END LOOP;
END
$$;

GRANT EXECUTE ON FUNCTION
  app_document_access(uuid),
  app_create_personal_note(uuid, text, integer, jsonb),
  app_set_document_settings(uuid, text, boolean),
  app_share_document(uuid, text, text),
  app_unshare_document(uuid),
  app_copy_document(uuid, uuid, uuid, text),
  app_move_preview(uuid, uuid),
  app_move_document(uuid, uuid),
  app_shared_with_me()
TO fvoci_app;
