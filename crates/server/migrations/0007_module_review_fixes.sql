-- 0006 검토 반영 (별도 검토자 결과). 0006은 이미 적용된 환경이 있으므로 고치지 않고 여기서 바꾼다.
--  1. 보관 아래에서도 소유 권한자는 노출을 줄이는 동작(공유 철회, 공개 링크 철회, 비공개로 바꾸기)을 할 수 있다.
--  2. 비공개 항목 아래 문서의 사본은 비공개로 만든다.
--  4. 항목·모듈을 잠근 뒤 확인한다(확인과 쓰기 사이에 보관·비공개로 바뀌지 않게).
--  5. 같은 모듈 안 항목 사이 이동은 번호를 유지한다.
--  6. request_id 재시도가 다른 위치의 문서를 돌려주지 않는다.
--  7. 항목 문서·항목·키의 워크스페이스 일치를 외래 키로 강제한다.
-- (3, 8, 9는 서버 코드, 10은 문서에 기록.)

------------------------------------------------------------------------------
-- 7. 워크스페이스 일치 외래 키
------------------------------------------------------------------------------

ALTER TABLE modules ADD CONSTRAINT modules_workspace UNIQUE (id, workspace_id);
ALTER TABLE items ADD CONSTRAINT items_workspace UNIQUE (id, workspace_id);
ALTER TABLE items ADD CONSTRAINT items_module_workspace
  FOREIGN KEY (module_id, workspace_id) REFERENCES modules (id, workspace_id);
ALTER TABLE module_keys ADD CONSTRAINT module_keys_module_workspace
  FOREIGN KEY (module_id, workspace_id) REFERENCES modules (id, workspace_id);
ALTER TABLE documents ADD CONSTRAINT documents_item_workspace
  FOREIGN KEY (item_id, workspace_id) REFERENCES items (id, workspace_id);

------------------------------------------------------------------------------
-- 1. 보관 상한을 뺀 기본 권한과, 상한을 적용한 권한
------------------------------------------------------------------------------

-- 기본 권한(보관 상한 없음). 노출을 줄이는 관리 동작과 그 목록에만 쓴다.
CREATE FUNCTION app_document_base_access(p_doc uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_doc   documents%ROWTYPE;
    v_role  text;
    v_base  text;
    v_item  text;
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
        v_base := 'owner';
      ELSIF v_role IS NOT NULL AND (v_doc.visibility = 'workspace' OR v_doc.created_by = v_user) THEN
        v_base := CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
      END IF;
      IF v_doc.item_id IS NOT NULL AND v_base IS NOT NULL THEN
        v_item := app_item_access(v_doc.item_id);
        -- 낮은 쪽: owner > write > read > NULL.
        v_base := CASE
          WHEN v_item IS NULL THEN NULL
          WHEN v_item = 'read' THEN 'read'
          WHEN v_item = 'write' AND v_base = 'owner' THEN 'write'
          ELSE v_base
        END;
      END IF;
    END IF;

    SELECT s.permission INTO v_share
    FROM document_shares s JOIN users u ON u.id = v_user
    WHERE s.document_id = p_doc AND u.email_verified AND lower(u.email) = s.grantee_email;

    RETURN CASE
      WHEN v_base = 'owner' THEN 'owner'
      WHEN v_base = 'write' OR v_share = 'write' THEN 'write'
      ELSE coalesce(v_base, v_share)
    END;
  END
  $$;

-- 일반 권한: 보관된 항목·모듈 아래 문서는 read로 제한한다.
CREATE OR REPLACE FUNCTION app_document_access(p_doc uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_document_base_access(p_doc);
    v_item   uuid;
  BEGIN
    IF v_access IS NULL OR v_access = 'read' THEN
      RETURN v_access;
    END IF;
    SELECT item_id INTO v_item FROM documents WHERE id = p_doc;
    IF v_item IS NOT NULL AND app_item_archived(v_item) THEN
      RETURN 'read';
    END IF;
    RETURN v_access;
  END
  $$;

-- 노출을 줄이는 동작용 소유 권한 확인(보관 상한 없음). 볼 수 없으면 P0002, 소유 권한이 아니면 42501.
CREATE FUNCTION app_require_document_manager(p_doc uuid) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_document_base_access(p_doc);
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'insufficient permission' USING ERRCODE = '42501';
    END IF;
  END
  $$;

-- 공유·공개 링크 목록은 관리할 수 있는 사람(보관 상한 없는 소유 권한자)에게 보인다.
DROP POLICY document_shares_select ON document_shares;
CREATE POLICY document_shares_select ON document_shares FOR SELECT
  USING (app_document_base_access(document_id) = 'owner');
DROP POLICY public_links_select ON public_links;
CREATE POLICY public_links_select ON public_links FOR SELECT
  USING (app_document_base_access(document_id) = 'owner');

CREATE OR REPLACE FUNCTION app_unshare_document(p_share uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_share document_shares%ROWTYPE;
  BEGIN
    SELECT * INTO v_share FROM document_shares WHERE id = p_share;
    PERFORM 1 FROM documents WHERE id = v_share.document_id FOR UPDATE;
    IF v_share.id IS NULL OR app_document_base_access(v_share.document_id) IS NULL THEN
      RAISE EXCEPTION 'share not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_document_manager(v_share.document_id);
    DELETE FROM document_shares WHERE id = p_share;
    PERFORM app_audit_document(v_share.document_id, 'unshare',
                               jsonb_build_object('email', v_share.grantee_email));
  END
  $$;

CREATE OR REPLACE FUNCTION app_revoke_public_link(p_link uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc uuid;
  BEGIN
    SELECT document_id INTO v_doc FROM public_links WHERE id = p_link AND revoked_at IS NULL;
    IF v_doc IS NULL OR app_document_base_access(v_doc) IS NULL THEN
      RAISE EXCEPTION 'link not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM 1 FROM documents WHERE id = v_doc FOR UPDATE;
    PERFORM app_require_document_manager(v_doc);
    UPDATE public_links SET revoked_at = now() WHERE id = p_link;
    PERFORM app_audit_document(v_doc, 'revoke_public_link', jsonb_build_object('link', p_link));
  END
  $$;

-- 보관 아래에서는 좁히는 변경(비공개로, 복제 허용 끄기)만 허용한다.
CREATE OR REPLACE FUNCTION app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE;
  BEGIN
    -- 행을 먼저 잠그고 권한을 본다. 동시에 이동된 문서에 이전 소유 권한으로 설정을 바꾸지 못하게 한다.
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    IF p_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF v_doc.owner_user_id IS NOT NULL AND p_visibility <> 'private' THEN
      RAISE EXCEPTION 'personal notes are always private' USING ERRCODE = '22023';
    END IF;
    IF app_document_access(p_doc) <> 'owner'
       AND ((p_visibility = 'workspace' AND v_doc.visibility <> 'workspace')
            OR (p_allow_copy AND NOT v_doc.allow_copy)) THEN
      RAISE EXCEPTION 'archived documents can only be restricted' USING ERRCODE = '55000';
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

------------------------------------------------------------------------------
-- 4. 항목·모듈 잠금
------------------------------------------------------------------------------

-- 항목과 그 모듈을 공유 잠금한다. 보관·공개 범위 변경(FOR UPDATE)은 이 트랜잭션이 끝날 때까지 기다린다.
-- 아무 정보도 돌려주지 않는다.
CREATE FUNCTION app_lock_item(p_item uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_item IS NULL THEN
      RETURN;
    END IF;
    PERFORM 1 FROM modules m JOIN items i ON i.module_id = m.id WHERE i.id = p_item FOR SHARE OF m;
    PERFORM 1 FROM items WHERE id = p_item FOR SHARE;
  END
  $$;

CREATE OR REPLACE FUNCTION app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_mod  modules%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR SHARE;
    IF v_user IS NULL OR v_mod.id IS NULL OR app_member_role(v_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(v_mod.workspace_id) NOT IN ('admin', 'writer') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF v_mod.archived_at IS NOT NULL THEN
      RAISE EXCEPTION 'the module is archived' USING ERRCODE = '55000';
    END IF;
    SELECT id INTO v_id FROM items WHERE module_id = p_module AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    v_id := uuidv7();
    PERFORM app_allocate_number(v_mod.workspace_id, p_module, 'item', v_id);
    INSERT INTO items (id, workspace_id, module_id, title, description, created_by, updated_by, request_id)
    VALUES (v_id, v_mod.workspace_id, p_module, btrim(p_title), coalesce(p_description, ''),
            v_user, v_user, p_request_id);
    PERFORM app_audit(v_mod.workspace_id, 'create', 'item', v_id, jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 6. 생성 재시도는 같은 위치의 문서만 돌려준다
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_create_module_document(
  p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb
) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_item     items%ROWTYPE;
    v_access   text;
    v_id       uuid;
    v_existing uuid;
    v_number   bigint;
  BEGIN
    PERFORM app_lock_item(p_item);
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    SELECT id, item_id INTO v_id, v_existing FROM documents
    WHERE workspace_id = v_item.workspace_id AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      IF v_existing IS DISTINCT FROM p_item THEN
        RAISE EXCEPTION 'request_id was already used for another document' USING ERRCODE = '55000';
      END IF;
      RETURN v_id;
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    v_id := uuidv7();
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'document', v_id);
    INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, 'module', v_item.module_id::text, v_number, btrim(p_title),
            p_schema_version, p_body, 1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

CREATE OR REPLACE FUNCTION app_create_wiki_document(
  p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb
) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user      uuid := app_user_id();
    v_id        uuid;
    v_placement text;
    v_number    bigint;
  BEGIN
    IF v_user IS NULL OR app_member_role(p_workspace) IS NULL
       OR app_member_role(p_workspace) NOT IN ('admin', 'writer') THEN
      RAISE EXCEPTION 'not allowed' USING ERRCODE = '42501';
    END IF;
    SELECT id, placement INTO v_id, v_placement FROM documents
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      IF v_placement <> 'wiki' THEN
        RAISE EXCEPTION 'request_id was already used for another document' USING ERRCODE = '55000';
      END IF;
      RETURN v_id;
    END IF;
    v_id := uuidv7();
    v_number := app_allocate_number(p_workspace, p_workspace, 'document', v_id);
    INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, p_workspace, 'wiki', 'WIKI', v_number, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 2. 비공개 항목 아래 문서의 사본은 비공개
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_access  text := app_require_document(p_doc, 'read');
    v_src     documents%ROWTYPE;
    v_private boolean;
    v_id      uuid;
    v_number  bigint;
  BEGIN
    SELECT * INTO v_src FROM documents WHERE id = p_doc;
    IF v_access <> 'owner' AND NOT v_src.allow_copy THEN
      RAISE EXCEPTION 'copying is not allowed for this document' USING ERRCODE = '42501';
    END IF;
    -- 사본으로 볼 수 있는 사람이 조용히 늘지 않게: 비공개 문서, 개인 노트, 비공개 항목 아래 문서의 사본은 비공개.
    v_private := v_src.visibility = 'private'
      OR EXISTS (SELECT 1 FROM items i WHERE i.id = v_src.item_id AND i.visibility = 'private');

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
      v_id := uuidv7();
      v_number := app_allocate_number(p_target_workspace, p_target_workspace, 'document', v_id);
      INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (v_id, p_target_workspace, 'wiki', 'WIKI', v_number,
              btrim(p_title), v_src.schema_version, v_src.body, 1, v_user, v_user, p_request_id,
              CASE WHEN v_private THEN 'private' ELSE 'workspace' END);
    END IF;

    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), v_src.schema_version, v_src.body, v_user, p_request_id);
    PERFORM app_audit_document(p_doc, 'copy',
      jsonb_build_object('target', CASE WHEN p_target_workspace IS NULL THEN 'personal' ELSE 'workspace' END));
    PERFORM app_audit_document(v_id, 'create',
      jsonb_build_object('copied', true, 'from', CASE WHEN v_src.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 5. 같은 범위 안 이동은 번호 유지 / 4. 이동 대상 잠금
------------------------------------------------------------------------------

-- 이동해도 번호가 그대로인지(같은 워크스페이스, 같은 번호 범위). 이동 확인을 통과한 경우에만 답한다.
CREATE FUNCTION app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) RETURNS boolean
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE := app_check_move(p_doc, p_target, p_item);
  BEGIN
    RETURN v_doc.number IS NOT NULL
      AND v_doc.workspace_id = p_target
      AND v_doc.scope = CASE WHEN p_item IS NULL THEN 'WIKI'
                             ELSE (SELECT module_id::text FROM items WHERE id = p_item) END;
  END
  $$;

CREATE OR REPLACE FUNCTION app_move_document(p_doc uuid, p_target uuid, p_item uuid)
  RETURNS TABLE (workspace_id uuid, reference text)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc    documents%ROWTYPE;
    v_ref    text;
    v_scope  uuid;
    v_number bigint;
  BEGIN
    PERFORM 1 FROM documents d WHERE d.id = p_doc FOR UPDATE;
    PERFORM app_lock_item((SELECT d.item_id FROM documents d WHERE d.id = p_doc));
    PERFORM app_lock_item(p_item);
    v_doc := app_check_move(p_doc, p_target, p_item);
    v_ref := app_document_reference(p_doc);
    PERFORM app_audit_document(p_doc, 'move_out', jsonb_build_object('reference', v_ref));
    v_scope := CASE WHEN p_item IS NULL THEN p_target ELSE (SELECT i.module_id FROM items i WHERE i.id = p_item) END;
    IF app_move_keeps_number(p_doc, p_target, p_item) THEN
      v_number := v_doc.number;
    ELSE
      v_number := app_allocate_number(p_target, v_scope, 'document', p_doc);
    END IF;
    UPDATE documents d
    SET workspace_id = p_target, owner_user_id = NULL, item_id = p_item,
        placement = CASE WHEN p_item IS NULL THEN 'wiki' ELSE 'module' END,
        scope = CASE WHEN p_item IS NULL THEN 'WIKI' ELSE v_scope::text END,
        number = v_number
    WHERE d.id = p_doc;
    PERFORM app_audit_document(p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_document_base_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_require_document_manager(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_lock_item(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_move_keeps_number(uuid, uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_document_base_access(uuid),
  app_lock_item(uuid),
  app_move_keeps_number(uuid, uuid, uuid)
TO fvoci_app;
