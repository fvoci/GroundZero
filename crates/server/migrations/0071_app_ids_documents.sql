-- 앱이 만든 id로 행을 만든다(D-88, 0070 머리말). 이 파일: 문서·트리·이동·공유·공개 링크·리비전 복원·참조와 언급,
-- 휴지통 넣기(문서·업무·항목·모듈), 댓글. 함수 본문은 마지막 정의와 같고 id 부분만 다르다.
--
-- 문서 제목 변경 감사는 지금까지 트리거(`documents_audit_rename`, 0075에서 지움)가 남겼다. 트리거는 앱이 만든 id를
-- 받을 수 없으므로, 제목을 바꾸는 두 길(저장·실시간 편집의 `write_revision`, 리비전 복원)이 리비전을 남긴 뒤
-- `app_audit_document_rename`을 부른다. 이전·새 제목은 함수가 두 리비전에서 읽으므로 앱이 꾸밀 수 없고, 한 리비전에
-- 한 번만 남는다(아래 고유 색인).

CREATE UNIQUE INDEX audit_log_document_rename ON audit_log (target_id, (detail ->> 'revision'))
  WHERE action = 'rename' AND target_type = 'document' AND detail ? 'revision';

-- id 1개(제목이 바뀐 리비전일 때만 쓴다). 지금 사용자가 막 남긴 그 문서의 현재 리비전이어야 한다.
CREATE FUNCTION app_audit_document_rename(p_id uuid, p_doc uuid, p_revision integer) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE;
    v_new document_revisions%ROWTYPE;
    v_old text;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    SELECT * INTO v_new FROM document_revisions WHERE document_id = p_doc AND revision = p_revision;
    IF v_new.document_id IS NULL OR v_doc.revision <> p_revision OR v_new.author IS DISTINCT FROM app_user_id() THEN
      RAISE EXCEPTION 'revision not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT title INTO v_old FROM document_revisions WHERE document_id = p_doc AND revision = p_revision - 1;
    IF v_old IS NULL OR v_old = v_new.title THEN
      RETURN;
    END IF;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (p_id, v_doc.workspace_id, v_doc.owner_user_id, app_user_id(), 'rename', 'document', p_doc,
            jsonb_build_object('from', v_old, 'to', v_new.title, 'revision', p_revision))
    ON CONFLICT DO NOTHING;
  END
  $$;

-- id 1개(부모가 바뀔 때 감사 기록).
CREATE FUNCTION app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc      documents%ROWTYPE;
    v_parent   documents%ROWTYPE;
    v_before   documents%ROWTYPE;
    v_ancestor uuid;
    v_lower    numeric;
    v_position numeric;
  BEGIN
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF p_parent IS NOT NULL THEN
      IF app_document_access(p_parent) IS NULL THEN
        RAISE EXCEPTION 'parent not found' USING ERRCODE = 'P0002';
      END IF;
      SELECT * INTO v_parent FROM documents WHERE id = p_parent;
      IF v_parent.workspace_id IS DISTINCT FROM v_doc.workspace_id
         OR v_parent.item_id IS DISTINCT FROM v_doc.item_id
         OR v_parent.owner_user_id IS DISTINCT FROM v_doc.owner_user_id THEN
        RAISE EXCEPTION 'the parent must be in the same place' USING ERRCODE = '22023';
      END IF;
      -- 자기 자신이나 자기 하위 문서 아래로는 옮길 수 없다(자리 잠금 아래에서 조상 경로를 본다).
      v_ancestor := p_parent;
      WHILE v_ancestor IS NOT NULL LOOP
        IF v_ancestor = p_doc THEN
          RAISE EXCEPTION 'a document cannot be placed under itself' USING ERRCODE = '22023';
        END IF;
        SELECT parent_id INTO v_ancestor FROM documents WHERE id = v_ancestor;
      END LOOP;
      IF app_document_depth(p_parent) + app_document_height(p_doc) > p_max_depth THEN
        RAISE EXCEPTION 'the tree would be deeper than % levels', p_max_depth USING ERRCODE = '22023';
      END IF;
    END IF;

    IF p_before IS NOT NULL THEN
      SELECT * INTO v_before FROM documents WHERE id = p_before;
      IF v_before.id IS NULL OR v_before.id = p_doc OR app_document_access(p_before) IS NULL
         -- 형제 판단은 호출한 사람이 보는 부모 기준(볼 수 없는 부모 아래 문서는 최상위로 보인다).
         OR (CASE WHEN v_before.parent_id IS NOT NULL AND app_document_access(v_before.parent_id) IS NOT NULL
                  THEN v_before.parent_id END) IS DISTINCT FROM p_parent
         OR v_before.workspace_id IS DISTINCT FROM v_doc.workspace_id
         OR v_before.item_id IS DISTINCT FROM v_doc.item_id
         OR v_before.owner_user_id IS DISTINCT FROM v_doc.owner_user_id THEN
        RAISE EXCEPTION 'before must be a sibling under the new parent' USING ERRCODE = '22023';
      END IF;
      SELECT max(s.position) INTO v_lower
      FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, p_parent) s
      WHERE s.id <> p_doc AND s.position < v_before.position;
      v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
    ELSE
      IF v_doc.parent_id IS NOT DISTINCT FROM p_parent AND NOT EXISTS (
           SELECT 1 FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, p_parent) s
           WHERE s.id <> p_doc AND s.position > v_doc.position) THEN
        RETURN;  -- 이미 그 부모의 맨 끝: 바꿀 것이 없다.
      END IF;
      SELECT coalesce(max(s.position), 0) + 1 INTO v_position
      FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, p_parent) s
      WHERE s.id <> p_doc;
    END IF;

    UPDATE documents SET parent_id = p_parent, position = v_position WHERE id = p_doc;
    IF v_doc.parent_id IS DISTINCT FROM p_parent THEN
      PERFORM app_audit_document(app_id(p_ids, 1), p_doc, 'reparent', jsonb_build_object('parent', p_parent));
    END IF;
  END
  $$;

-- id 3개: 문서, 감사 기록, 부모 지정 감사 기록.
CREATE FUNCTION app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer,
                                         p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
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
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(p_workspace, NULL, NULL);
    END IF;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(p_workspace, p_workspace, 'document', v_id);
    INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, p_workspace, 'wiki', 'WIKI', v_number, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 3));
    END IF;
    RETURN v_id;
  END
  $$;

-- id 3개: 문서, 감사 기록, 부모 지정 감사 기록.
CREATE FUNCTION app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer,
                                           p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
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
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(v_item.workspace_id, p_item, NULL);
    END IF;
    PERFORM app_lock_item(p_item);
    IF app_item_access(p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'document', v_id);
    INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, 'module', v_item.module_id::text, v_number, btrim(p_title),
            p_schema_version, p_body, 1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 3));
    END IF;
    RETURN v_id;
  END
  $$;

-- id 3개: 문서, 감사 기록, 부모 지정 감사 기록.
CREATE FUNCTION app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb,
                                         p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
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
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(NULL, NULL, v_user);
    END IF;
    INSERT INTO documents (id, owner_user_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id, visibility)
    VALUES (app_id(p_ids, 1), v_user, 'personal', 'NOTE', NULL, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id, 'private')
    RETURNING id INTO v_id;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 3));
    END IF;
    RETURN v_id;
  END
  $$;

-- id 3개: 사본, 원본의 복제 감사 기록, 사본의 만들기 감사 기록.
CREATE FUNCTION app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text,
                                  p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
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
      INSERT INTO documents (id, owner_user_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (app_id(p_ids, 1), v_user, 'personal', 'NOTE', NULL, btrim(p_title), v_src.schema_version, v_src.body,
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
      v_id := app_id(p_ids, 1);
      v_number := app_allocate_number(p_target_workspace, p_target_workspace, 'document', v_id);
      INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (v_id, p_target_workspace, 'wiki', 'WIKI', v_number,
              btrim(p_title), v_src.schema_version, v_src.body, 1, v_user, v_user, p_request_id,
              CASE WHEN v_private THEN 'private' ELSE 'workspace' END);
    END IF;

    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), v_src.schema_version, v_src.body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'copy',
      jsonb_build_object('target', CASE WHEN p_target_workspace IS NULL THEN 'personal' ELSE 'workspace' END));
    PERFORM app_audit_document(app_id(p_ids, 3), v_id, 'create',
      jsonb_build_object('copied', true, 'from', CASE WHEN v_src.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN v_id;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_seen documents%ROWTYPE;
    v_doc  documents%ROWTYPE;
  BEGIN
    -- 권한을 먼저 확인한 뒤에만 잠근다. 자리 잠금 → 행 잠금 순서. 잠근 뒤 자리가 바뀌었으면(그사이 이동) 다시 하게 한다.
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_seen FROM documents WHERE id = p_doc;
    PERFORM app_lock_tree(v_seen.workspace_id, v_seen.item_id, v_seen.owner_user_id);
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_item(v_doc.item_id);
    PERFORM app_require_document(p_doc, 'write');
    PERFORM app_place_document(p_doc, p_parent, p_before, p_max_depth, p_ids);
  END
  $$;

-- id: 옮겨 나가기 감사 1개, 자식마다 부모 바뀜 감사 1개, 옮겨 들어오기 감사 1개(개수는 `_ids`).
CREATE FUNCTION app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[])
  RETURNS TABLE (workspace_id uuid, reference text)
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_seen   documents%ROWTYPE;
    v_doc    documents%ROWTYPE;
    v_ref    text;
    v_scope  uuid;
    v_number bigint;
    v_next   numeric;
    v_child  record;
    v_at     integer := 1;
  BEGIN
    SELECT * INTO v_seen FROM documents d WHERE d.id = p_doc;
    -- 업로드 마치기와 같은 순서(업로드 행 → 문서)로 먼저 잠근다.
    PERFORM 1 FROM uploads u
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc))
    ORDER BY u.id
    FOR NO KEY UPDATE;
    PERFORM app_lock_move(p_doc, p_target, p_item);
    PERFORM 1 FROM documents d WHERE d.id = p_doc FOR UPDATE;
    PERFORM app_lock_item((SELECT d.item_id FROM documents d WHERE d.id = p_doc));
    PERFORM app_lock_item(p_item);
    v_doc := app_check_move(p_doc, p_target, p_item);
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    v_ref := app_document_reference(p_doc);
    PERFORM app_audit_document(app_id(p_ids, v_at), p_doc, 'move_out', jsonb_build_object('reference', v_ref));
    v_scope := CASE WHEN p_item IS NULL THEN p_target ELSE (SELECT i.module_id FROM items i WHERE i.id = p_item) END;
    IF app_move_keeps_number(p_doc, p_target, p_item) THEN
      v_number := v_doc.number;
    ELSE
      v_number := app_allocate_number(p_target, v_scope, 'document', p_doc);
    END IF;

    -- 자식: 원래 부모 아래, 이동한 문서가 있던 위치와 그다음 형제 사이에 원래 순서대로.
    SELECT min(s.position) INTO v_next
    FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, v_doc.parent_id) s
    WHERE s.id <> p_doc AND s.position > v_doc.position;
    v_next := coalesce(v_next, v_doc.position + 1);
    FOR v_child IN
      SELECT c.id, row_number() OVER (ORDER BY c.position, c.number NULLS LAST, c.created_at, c.id) AS k,
             count(*) OVER () AS n
      FROM documents c WHERE c.parent_id = p_doc
    LOOP
      UPDATE documents d
      SET parent_id = v_doc.parent_id,
          position = v_doc.position + (v_next - v_doc.position) * v_child.k / (v_child.n + 1)
      WHERE d.id = v_child.id;
      v_at := v_at + 1;
      PERFORM app_audit_document(app_id(p_ids, v_at), v_child.id, 'reparent',
                                 jsonb_build_object('parent', v_doc.parent_id, 'reason', 'parent_moved'));
    END LOOP;

    UPDATE documents d
    SET workspace_id = p_target, owner_user_id = NULL, item_id = p_item,
        placement = CASE WHEN p_item IS NULL THEN 'wiki' ELSE 'module' END,
        scope = CASE WHEN p_item IS NULL THEN 'WIKI' ELSE v_scope::text END,
        number = v_number, parent_id = NULL,
        position = (SELECT coalesce(max(s.position), 0) + 1 FROM app_tree_siblings(p_target, p_item, NULL, NULL) s)
    WHERE d.id = p_doc;
    -- 첨부(휴지통에 있는 것 포함)·그 휴지통 묶음·열린 업로드도 새 자리로. 권한·휴지통·감사·저장 용량·
    -- 내보내기가 모두 이 자리를 본다.
    UPDATE files f SET workspace_id = p_target, owner_user_id = NULL WHERE f.document_id = p_doc;
    UPDATE trash_batches b SET workspace_id = p_target, owner_user_id = NULL
    WHERE b.root_kind = 'file' AND b.restored_at IS NULL AND b.purged_at IS NULL
      AND b.root_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc);
    UPDATE uploads u SET workspace_id = p_target, owner_user_id = NULL
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc));
    v_at := v_at + 1;
    PERFORM app_audit_document(app_id(p_ids, v_at), p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;

CREATE FUNCTION app_move_document_ids(p_doc uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_document_access(p_doc) IS NULL THEN 0
                ELSE 2 + (SELECT count(*) FROM documents c WHERE c.parent_id = p_doc)::integer END
  $$;

-- id 2개(공개 범위, 복제 허용 각각 바뀔 때).
CREATE FUNCTION app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
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
      PERFORM app_audit_document(app_id(p_ids, 1), p_doc, 'change_visibility',
                                 jsonb_build_object('from', v_doc.visibility, 'to', p_visibility));
    END IF;
    IF v_doc.allow_copy IS DISTINCT FROM p_allow_copy THEN
      PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'change_copy_policy',
                                 jsonb_build_object('allow_copy', p_allow_copy));
    END IF;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR NO KEY UPDATE;
    v_access := app_document_access(p_doc);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the owner can mark a template' USING ERRCODE = '42501';
    END IF;
    UPDATE documents SET is_template = p_template WHERE id = p_doc;
    PERFORM app_audit_document(app_id(p_ids, 1), p_doc,
                               CASE WHEN p_template THEN 'mark_template' ELSE 'unmark_template' END, '{}'::jsonb);
  END
  $$;

-- id 2개: 공유(새로 줄 때), 감사 기록.
CREATE FUNCTION app_share_document(p_doc uuid, p_email text, p_permission text, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
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
    INSERT INTO document_shares (id, document_id, grantee_email, permission, granted_by)
    VALUES (app_id(p_ids, 1), p_doc, v_email, p_permission, app_user_id())
    ON CONFLICT ON CONSTRAINT document_shares_target
      DO UPDATE SET permission = EXCLUDED.permission, granted_by = EXCLUDED.granted_by
    RETURNING id INTO v_id;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'share',
                               jsonb_build_object('email', v_email, 'permission', p_permission));
    RETURN v_id;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_unshare_document(p_share uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
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
    PERFORM app_audit_document(app_id(p_ids, 1), v_share.document_id, 'unshare',
                               jsonb_build_object('email', v_share.grantee_email));
  END
  $$;

-- id 2개: 링크, 감사 기록.
CREATE FUNCTION app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamptz, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document(p_doc, 'owner');
    IF p_expires_at IS NOT NULL AND p_expires_at <= now() THEN
      RAISE EXCEPTION 'expiry must be in the future' USING ERRCODE = '22023';
    END IF;
    INSERT INTO public_links (id, document_id, token_hash, created_by, expires_at)
    VALUES (app_id(p_ids, 1), p_doc, p_token_hash, app_user_id(), p_expires_at)
    RETURNING id INTO v_id;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'create_public_link',
                               jsonb_build_object('link', v_id, 'expires_at', p_expires_at));
    RETURN v_id;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_revoke_public_link(p_link uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
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
    PERFORM app_audit_document(app_id(p_ids, 1), v_doc, 'revoke_public_link', jsonb_build_object('link', p_link));
  END
  $$;

-- id 2개: 제목 변경 감사(제목이 바뀌면), 복원 감사.
CREATE FUNCTION app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid,
                                              p_base_revision integer, p_schema_version integer, p_ids uuid[])
  RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_doc      documents%ROWTYPE;
    v_source   document_revisions%ROWTYPE;
    v_replay   document_revisions%ROWTYPE;
    v_next     integer;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_lock_item(v_doc.item_id);
    PERFORM app_require_document(p_doc, 'write');

    -- 같은 사람의 같은 요청 재시도: 같은 리비전의 복원이고 그 결과가 아직 최신일 때만 성공.
    SELECT * INTO v_replay FROM document_revisions
    WHERE document_id = p_doc AND author = v_user AND request_id = p_request_id;
    IF v_replay.document_id IS NOT NULL THEN
      IF v_replay.restored_from IS DISTINCT FROM p_revision THEN
        RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
      END IF;
      RETURN CASE WHEN v_replay.revision = v_doc.revision THEN 'restored' ELSE 'conflict' END;
    END IF;
    IF v_doc.revision <> p_base_revision THEN
      RETURN 'conflict';
    END IF;

    SELECT * INTO v_source FROM document_revisions WHERE document_id = p_doc AND revision = p_revision;
    IF v_source.document_id IS NULL THEN
      RAISE EXCEPTION 'revision not found' USING ERRCODE = 'P0002';
    END IF;
    -- 옛 저장 형식으로 문서를 되돌리지 않는다(형식 변환이 생기면 변환 뒤 복원).
    IF v_source.schema_version <> p_schema_version THEN
      RAISE EXCEPTION 'this revision uses an older document format' USING ERRCODE = '55000';
    END IF;
    IF v_source.title = v_doc.title AND v_source.body = v_doc.body
       AND v_source.schema_version = v_doc.schema_version THEN
      RETURN 'unchanged';
    END IF;

    v_next := v_doc.revision + 1;
    UPDATE documents
    SET title = v_source.title, body = v_source.body, schema_version = v_source.schema_version,
        revision = v_next, updated_by = v_user, updated_at = now()
    WHERE id = p_doc;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id,
                                    restored_from)
    VALUES (p_doc, v_next, v_source.title, v_source.schema_version, v_source.body, v_user, p_request_id,
            p_revision);
    -- 바뀐 곳: 제목 변경 감사는 트리거가 아니라 여기서(0071 머리말).
    PERFORM app_audit_document_rename(app_id(p_ids, 1), p_doc, v_next);
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'restore_revision',
                               jsonb_build_object('from', p_revision, 'to', v_next));
    PERFORM app_record_document_references(p_doc, false, '{}'::uuid[]);
    RETURN 'restored';
  END
  $$;

-- id: 새로 언급한 사람마다 알림 1개(개수는 `_ids`, 알리지 않으면 0개).
CREATE FUNCTION app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_body     jsonb;
    v_revision integer;
    v_previous jsonb;
    v_uid      uuid;
    v_new      integer;
    v_at       integer := 0;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT body, revision INTO v_body, v_revision FROM documents WHERE id = p_doc;

    DELETE FROM document_links WHERE document_id = p_doc;
    INSERT INTO document_links (document_id, target_kind, target_id)
    SELECT p_doc, r.kind, r.id FROM app_reference_nodes(v_body) r
    WHERE r.kind IN ('item', 'document', 'task') AND NOT (r.kind = 'document' AND r.id = p_doc);

    IF NOT p_notify THEN
      RETURN;
    END IF;
    SELECT body INTO v_previous FROM document_revisions
    WHERE document_id = p_doc AND revision = v_revision - 1;
    FOR v_uid IN
      SELECT r.id FROM app_reference_nodes(v_body) r JOIN users u ON u.id = r.id
      WHERE r.kind = 'user' AND r.id <> v_user
        AND NOT EXISTS (SELECT 1 FROM app_reference_nodes(coalesce(v_previous, '{}'::jsonb)) p
                        WHERE p.kind = 'user' AND p.id = r.id)
    LOOP
      IF app_document_access_as(p_doc, v_uid) IS NOT NULL THEN
        INSERT INTO document_mentions (document_id, user_id, mentioned_by)
        VALUES (p_doc, v_uid, v_user)
        ON CONFLICT DO NOTHING;
        GET DIAGNOSTICS v_new = ROW_COUNT;
        IF v_new > 0 THEN
          v_at := v_at + 1;
          INSERT INTO notifications (id, user_id, kind, document_id, actor)
          VALUES (app_id(p_ids, v_at), v_uid, 'mention', p_doc, v_user);
        END IF;
      END IF;
    END LOOP;
  END
  $$;

-- 지금 본문에서 사람을 가리키는 참조 수(새로 언급한 사람 수의 상한).
CREATE FUNCTION app_record_document_references_ids(p_doc uuid, p_notify boolean) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN NOT p_notify OR app_document_access(p_doc) IS NULL THEN 0
                ELSE (SELECT count(*) FROM documents d, app_reference_nodes(d.body) r
                      WHERE d.id = p_doc AND r.kind = 'user')::integer END
  $$;

-- id 1개.
CREATE FUNCTION app_audit_document_export(p_doc uuid, p_revision integer, p_format text, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_workspace uuid;
    v_owner     uuid;
  BEGIN
    IF app_document_access(p_doc) IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT app_document_can_export(p_doc) THEN
      RAISE EXCEPTION 'exporting this document is not allowed' USING ERRCODE = '42501';
    END IF;
    IF p_format NOT IN ('markdown', 'zip', 'pdf') THEN
      RAISE EXCEPTION 'unknown export format' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id, owner_user_id INTO v_workspace, v_owner FROM documents WHERE id = p_doc;
    IF v_owner IS NULL THEN
      PERFORM app_audit(app_id(p_ids, 1), v_workspace, 'export', 'document', p_doc,
                        jsonb_build_object('revision', p_revision, 'format', p_format));
    ELSE
      INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), NULL, v_owner, app_user_id(), 'export', 'document', p_doc,
              jsonb_build_object('revision', p_revision, 'format', p_format));
    END IF;
  END
  $$;

-- id 2개: 휴지통 묶음, 감사 기록.
CREATE FUNCTION app_trash_document(p_doc uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_doc    documents%ROWTYPE;
    v_access text := app_document_access(p_doc);
    v_batch  uuid;
    v_count  integer;
    v_tree   uuid[];
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF NOT (v_access = 'owner' OR (v_access = 'write' AND v_doc.created_by = v_user)) THEN
      IF v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id)
         AND app_document_base_access(p_doc) IS NOT NULL THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'only the owner or the author can delete this document' USING ERRCODE = '42501';
    END IF;
    -- 트리 잠금 → 문서 행 → 항목 공유(문서 잠금 순서).
    PERFORM app_lock_tree(CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END,
                          v_doc.item_id, v_doc.owner_user_id);
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_lock_item(v_doc.item_id);
    IF app_document_access(p_doc) IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END, v_doc.owner_user_id,
            'document', p_doc, v_doc.title, 0, v_user)
    RETURNING id INTO v_batch;
    -- 지우는 사람이 지울 수 있는 하위 문서만 함께 지운다. 볼 수 없거나 지울 수 없는 하위 문서는 이 문서의
    -- 부모 아래로 옮겨 남긴다(남의 문서를 몰래 휴지통에 넣지 않게).
    v_tree := ARRAY(
    WITH RECURSIVE tree(id) AS (
      SELECT p_doc
      UNION
      SELECT d.id FROM documents d JOIN tree t ON d.parent_id = t.id
      WHERE d.deleted_at IS NULL
        AND (app_document_access(d.id) = 'owner'
             OR (app_document_access(d.id) = 'write' AND d.created_by = v_user))
    )
    SELECT id FROM tree);
    UPDATE documents SET parent_id = v_doc.parent_id
    WHERE parent_id = ANY (v_tree) AND NOT (id = ANY (v_tree)) AND deleted_at IS NULL;
    UPDATE documents SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_tree);
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE trash_batches SET member_count = v_count WHERE id = v_batch;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'delete', jsonb_build_object('batch', v_batch, 'count', v_count));
    RETURN v_batch;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 댓글
------------------------------------------------------------------------------------------------------------------

-- id: 댓글 1개, 받는 사람마다 알림 1개(개수는 `_ids`).
CREATE FUNCTION app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text,
                                p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_id     uuid;
    v_parent comments%ROWTYPE;
    v_owner  uuid;
    v_body   text := btrim(coalesce(p_body, ''));
    v_uid    uuid;
    v_at     integer := 1;
  BEGIN
    SELECT id INTO v_id FROM comments WHERE created_by = v_user AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF (p_document IS NULL) = (p_task IS NULL) THEN
      RAISE EXCEPTION 'comment on a document or a task' USING ERRCODE = '22023';
    END IF;
    IF app_comment_target_access(p_document, p_task) IS NULL THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_comment_target_archived(p_document, p_task) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF length(v_body) NOT BETWEEN 1 AND 10000 THEN
      RAISE EXCEPTION 'a comment needs 1 to 10000 characters' USING ERRCODE = '22023';
    END IF;
    IF p_parent IS NOT NULL THEN
      -- 지우기(FOR NO KEY UPDATE)와 겨루지 않게 잠근다. 먼저 지워졌으면 찾지 못한다(404).
      SELECT * INTO v_parent FROM comments WHERE id = p_parent FOR SHARE;
      IF v_parent.id IS NULL OR v_parent.document_id IS DISTINCT FROM p_document
         OR v_parent.task_id IS DISTINCT FROM p_task THEN
        RAISE EXCEPTION 'thread not found' USING ERRCODE = 'P0002';
      END IF;
      IF v_parent.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'reply to the first comment of the thread' USING ERRCODE = '22023';
      END IF;
    END IF;
    IF p_document IS NOT NULL THEN
      SELECT coalesce(owner_user_id, created_by) INTO v_owner FROM documents WHERE id = p_document;
    ELSE
      SELECT created_by INTO v_owner FROM tasks WHERE id = p_task;
    END IF;
    INSERT INTO comments (id, document_id, task_id, parent_id, body, created_by, request_id)
    VALUES (app_id(p_ids, 1), p_document, p_task, p_parent, v_body, v_user, p_request)
    RETURNING id INTO v_id;
    IF p_parent IS NULL THEN
      -- 새 스레드: 대상을 만든 사람과(업무면) 담당자에게, 사람마다 한 번.
      FOR v_uid IN
        SELECT v_owner WHERE v_owner IS NOT NULL
        UNION SELECT user_id FROM task_assignees WHERE task_id = p_task
      LOOP
        v_at := v_at + 1;
        PERFORM app_notify(app_id(p_ids, v_at), v_uid, 'comment', p_document, p_task, v_id, v_user, '{}'::jsonb);
      END LOOP;
    ELSE
      -- 답글: 스레드를 연 사람과 그 스레드에 답한 사람에게.
      FOR v_uid IN
        SELECT DISTINCT created_by FROM comments WHERE (id = p_parent OR parent_id = p_parent) AND deleted_at IS NULL
      LOOP
        v_at := v_at + 1;
        PERFORM app_notify(app_id(p_ids, v_at), v_uid, 'reply', p_document, p_task, v_id, v_user, '{}'::jsonb);
      END LOOP;
    END IF;
    RETURN v_id;
  END
  $$;

-- 댓글 1개 + 알림 받을 사람 수의 상한(같은 대상 고르기, 권한 확인 전). 답글의 대상에는 방금 쓴 답글의 작성자도
-- 들어가므로(본인에게는 알리지 않지만 차례는 쓴다) 1을 더한다.
CREATE FUNCTION app_add_comment_ids(p_document uuid, p_task uuid, p_parent uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE
      WHEN app_comment_target_access(p_document, p_task) IS NULL THEN 0
      WHEN p_parent IS NULL THEN 2 + (SELECT count(*) FROM task_assignees WHERE task_id = p_task)::integer
      ELSE 2 + (SELECT count(DISTINCT created_by) FROM comments
                WHERE (id = p_parent OR parent_id = p_parent) AND deleted_at IS NULL)::integer
    END
  $$;

-- id 1개(남이 쓴 댓글을 지울 때 감사 기록).
CREATE FUNCTION app_delete_comment(p_comment uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
    v_ws     uuid;
    v_owner  uuid;
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_c.id IS NULL OR v_c.deleted_at IS NOT NULL OR v_access IS NULL THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.created_by <> app_user_id() AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the author or the owner can delete a comment' USING ERRCODE = '42501';
    END IF;
    IF EXISTS (SELECT 1 FROM comments WHERE parent_id = p_comment AND deleted_at IS NULL) THEN
      UPDATE comments SET deleted_at = now(), body = '(지운 댓글)', updated_at = now() WHERE id = p_comment;
    ELSE
      DELETE FROM comments WHERE id = p_comment;
    END IF;
    IF v_c.created_by <> app_user_id() THEN
      IF v_c.document_id IS NOT NULL THEN
        SELECT workspace_id, CASE WHEN workspace_id IS NULL THEN owner_user_id END INTO v_ws, v_owner
        FROM documents WHERE id = v_c.document_id;
      ELSE
        SELECT workspace_id INTO v_ws FROM tasks WHERE id = v_c.task_id;
      END IF;
      INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_ws, v_owner, app_user_id(), 'delete_comment',
              CASE WHEN v_c.document_id IS NOT NULL THEN 'document' ELSE 'task' END,
              coalesce(v_c.document_id, v_c.task_id), jsonb_build_object('author', v_c.created_by));
    END IF;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 업무·항목·모듈 휴지통 넣기(문서와 같은 서버 경로를 쓴다)
------------------------------------------------------------------------------------------------------------------

-- id 2개: 휴지통 묶음, 감사 기록.
CREATE FUNCTION app_trash_task(p_task uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task  tasks%ROWTYPE;
    v_batch uuid;
    v_ids   uuid[];
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    PERFORM app_lock_task_rows(v_ids, NULL);
    -- 잠그는 사이 새로 생긴 하위 업무도 함께(드문 경우, 다시 잠근다).
    IF EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task AND deleted_at IS NULL AND NOT (id = ANY (v_ids))) THEN
      SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
      PERFORM app_lock_task_rows(v_ids, NULL);
    END IF;
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- 잠근 뒤 다시 고른다. 기다리는 사이 다른 지우기가 끝낸 하위 업무는 이미 자기 묶음에 있다.
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE id = ANY (v_ids) AND deleted_at IS NULL;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_task.workspace_id, 'task', p_task, v_task.title, cardinality(v_ids), app_user_id())
    RETURNING id INTO v_batch;
    UPDATE tasks SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_ids);
    PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'delete', 'task', p_task,
                      jsonb_build_object('batch', v_batch, 'count', cardinality(v_ids)));
    RETURN v_batch;
  END
  $$;

-- id 2개: 휴지통 묶음, 감사 기록.
CREATE FUNCTION app_trash_item(p_item uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item  items%ROWTYPE;
    v_batch uuid;
  BEGIN
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_access(p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can delete it' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF v_item.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_item.workspace_id, 'item', p_item, v_item.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE items SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_item;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'delete', 'item', p_item,
                      jsonb_build_object('batch', v_batch));
    RETURN v_batch;
  END
  $$;

-- id 2개: 휴지통 묶음, 감사 기록.
CREATE FUNCTION app_trash_module(p_module uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_module modules%ROWTYPE;
    v_batch  uuid;
    v_count  integer;
  BEGIN
    SELECT * INTO v_module FROM modules WHERE id = p_module FOR UPDATE;
    IF v_module.id IS NULL OR v_module.deleted_at IS NOT NULL OR app_member_role(v_module.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_module.workspace_id);
    -- 휴지통에 없는 항목 수(목록 표시용). 따로 지운 항목은 자기 묶음에 있다.
    SELECT count(*) INTO v_count FROM items WHERE module_id = p_module AND deleted_at IS NULL;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_module.workspace_id, 'module', p_module, v_module.name, v_count, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE modules SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_module;
    PERFORM app_audit(app_id(p_ids, 2), v_module.workspace_id, 'delete', 'module', p_module,
                      jsonb_build_object('batch', v_batch, 'items', v_count));
    RETURN v_batch;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_trash_task(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_item(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_module(uuid, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_trash_task(uuid, uuid[]), app_trash_item(uuid, uuid[]), app_trash_module(uuid, uuid[])
  TO fvoci_app;

REVOKE ALL ON FUNCTION app_audit_document_rename(uuid, uuid, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_place_document(uuid, uuid, uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_wiki_document(uuid, uuid, text, integer, jsonb, uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_module_document(uuid, uuid, text, integer, jsonb, uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_personal_note(uuid, text, integer, jsonb, uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_copy_document(uuid, uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_document_parent(uuid, uuid, uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_move_document(uuid, uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_move_document_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_document_settings(uuid, text, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_document_template(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_share_document(uuid, text, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_unshare_document(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_public_link(uuid, bytea, timestamptz, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_revoke_public_link(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_restore_document_revision(uuid, integer, uuid, integer, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_record_document_references(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_record_document_references_ids(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit_document_export(uuid, integer, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_document(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_comment(uuid, uuid, uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_comment_ids(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_comment(uuid, uuid[]) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
  app_audit_document_rename(uuid, uuid, integer),
  app_create_wiki_document(uuid, uuid, text, integer, jsonb, uuid, integer, uuid[]),
  app_create_module_document(uuid, uuid, text, integer, jsonb, uuid, integer, uuid[]),
  app_create_personal_note(uuid, text, integer, jsonb, uuid, integer, uuid[]),
  app_copy_document(uuid, uuid, uuid, text, uuid[]),
  app_set_document_parent(uuid, uuid, uuid, integer, uuid[]),
  app_move_document(uuid, uuid, uuid, uuid[]),
  app_move_document_ids(uuid),
  app_set_document_settings(uuid, text, boolean, uuid[]),
  app_set_document_template(uuid, boolean, uuid[]),
  app_share_document(uuid, text, text, uuid[]),
  app_unshare_document(uuid, uuid[]),
  app_create_public_link(uuid, bytea, timestamptz, uuid[]),
  app_revoke_public_link(uuid, uuid[]),
  app_restore_document_revision(uuid, integer, uuid, integer, integer, uuid[]),
  app_record_document_references(uuid, boolean, uuid[]),
  app_record_document_references_ids(uuid, boolean),
  app_audit_document_export(uuid, integer, text, uuid[]),
  app_trash_document(uuid, uuid[]),
  app_add_comment(uuid, uuid, uuid, uuid, text, uuid[]),
  app_add_comment_ids(uuid, uuid, uuid),
  app_delete_comment(uuid, uuid[])
TO fvoci_app;

-- 제목 변경 감사는 위의 두 길이 남긴다. 트리거를 두면 같은 변경이 두 번 남는다.
DROP TRIGGER documents_audit_rename ON documents;
DROP FUNCTION documents_audit_rename();
