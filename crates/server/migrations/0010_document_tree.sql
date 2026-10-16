-- 문서 트리 (D-27): 위키·항목 문서·개인 노트를 같은 자리(container) 안에서 부모·자식으로 묶고 순서를 둔다.
-- 자리 = (workspace_id, item_id, owner_user_id). 부모는 같은 자리에 있어야 한다.
-- 순서는 numeric 위치로 두고 끼워 넣을 때 옮기는 문서 한 행만 바꾼다. 가운데 값은 정확한 곱셈(* 0.5)으로
-- 구해 반복해 끼워 넣어도 값이 겹치지 않는다. 트리 변경은 권한을 바꾸지 않는다. 부모를 볼 수 없는 사람에게는
-- 자식이 맨 위에 보인다.
--
-- 잠금 순서(모든 경로 공통): 자리 잠금(advisory) → 문서 행 → 항목·모듈(공유) → 번호 카운터.
-- 자리 잠금은 같은 자리의 트리 변경을 직렬화한다(행 잠금만으로는 서로의 조상 경로를 못 봐 순환이 생길 수 있다).

ALTER TABLE documents ADD COLUMN parent_id uuid REFERENCES documents (id);
ALTER TABLE documents ADD COLUMN position numeric NOT NULL DEFAULT 0;
CREATE INDEX documents_parent ON documents (parent_id, position);
CREATE INDEX documents_wiki_roots ON documents (workspace_id, position)
  WHERE item_id IS NULL AND owner_user_id IS NULL AND parent_id IS NULL;
CREATE INDEX documents_item_roots ON documents (item_id, position) WHERE parent_id IS NULL;
CREATE INDEX documents_note_roots ON documents (owner_user_id, position) WHERE parent_id IS NULL;

-- 기존 문서는 자리마다 번호(없으면 만든 시각) 순으로 1, 2, 3...
UPDATE documents d SET position = r.pos
FROM (
  SELECT id, row_number() OVER (
    PARTITION BY workspace_id, item_id, owner_user_id ORDER BY number NULLS LAST, created_at, id) AS pos
  FROM documents
) r
WHERE r.id = d.id;

------------------------------------------------------------------------------
-- 형제 조회: 자리·부모별로 인덱스를 쓰도록 나눈다(IS NOT DISTINCT FROM은 인덱스를 못 쓴다).
------------------------------------------------------------------------------

CREATE FUNCTION app_tree_siblings(p_workspace uuid, p_item uuid, p_owner uuid, p_parent uuid)
  RETURNS SETOF documents
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_parent IS NOT NULL THEN
      RETURN QUERY SELECT * FROM documents WHERE parent_id = p_parent;
    ELSIF p_owner IS NOT NULL THEN
      RETURN QUERY SELECT * FROM documents WHERE owner_user_id = p_owner AND parent_id IS NULL;
    ELSIF p_item IS NOT NULL THEN
      RETURN QUERY SELECT * FROM documents WHERE item_id = p_item AND parent_id IS NULL;
    ELSE
      RETURN QUERY SELECT * FROM documents
      WHERE workspace_id = p_workspace AND item_id IS NULL AND owner_user_id IS NULL AND parent_id IS NULL;
    END IF;
  END
  $$;

------------------------------------------------------------------------------
-- 불변식
------------------------------------------------------------------------------

-- 새 문서는 자리(와 부모)의 맨 끝에 놓는다.
CREATE FUNCTION documents_initial_position() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    SELECT coalesce(max(s.position), 0) + 1 INTO NEW.position
    FROM app_tree_siblings(NEW.workspace_id, NEW.item_id, NEW.owner_user_id, NEW.parent_id) s;
    RETURN NEW;
  END
  $$;
CREATE TRIGGER documents_initial_position BEFORE INSERT ON documents
  FOR EACH ROW EXECUTE FUNCTION documents_initial_position();

-- 부모는 같은 자리에 있어야 하고, 자리를 옮기는 문서에는 자식이 남아 있으면 안 된다(이동 함수가 먼저 떼어 낸다).
-- CHECK·복합 외래 키로는 표현할 수 없어(부모 행 참조, NULL 열) 제약 트리거로 강제한다.
CREATE FUNCTION documents_tree_check() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_parent documents%ROWTYPE;
  BEGIN
    IF NEW.parent_id IS NOT NULL THEN
      IF NEW.parent_id = NEW.id THEN
        RAISE EXCEPTION 'a document cannot be its own parent' USING ERRCODE = '23514';
      END IF;
      SELECT * INTO v_parent FROM documents WHERE id = NEW.parent_id;
      IF v_parent.workspace_id IS DISTINCT FROM NEW.workspace_id
         OR v_parent.item_id IS DISTINCT FROM NEW.item_id
         OR v_parent.owner_user_id IS DISTINCT FROM NEW.owner_user_id THEN
        RAISE EXCEPTION 'the parent must be in the same place' USING ERRCODE = '23514';
      END IF;
    END IF;
    IF TG_OP = 'UPDATE'
       AND (NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
            OR NEW.item_id IS DISTINCT FROM OLD.item_id
            OR NEW.owner_user_id IS DISTINCT FROM OLD.owner_user_id)
       AND EXISTS (SELECT 1 FROM documents c WHERE c.parent_id = NEW.id) THEN
      RAISE EXCEPTION 'children must be detached before moving a document' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
  END
  $$;
CREATE CONSTRAINT TRIGGER documents_tree_check
  AFTER INSERT OR UPDATE OF parent_id, workspace_id, item_id, owner_user_id ON documents
  FOR EACH ROW EXECUTE FUNCTION documents_tree_check();

------------------------------------------------------------------------------
-- 잠금
------------------------------------------------------------------------------

CREATE FUNCTION app_lock_tree(p_workspace uuid, p_item uuid, p_owner uuid) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT pg_advisory_xact_lock(hashtextextended(
      'fvoci-tree:' || coalesce(p_workspace::text, '') || ':' || coalesce(p_item::text, '') || ':'
        || coalesce(p_owner::text, ''), 0))
  $$;

-- 이동 전 자리 잠금: 원래 자리와 새 자리를 키 문자열 순으로 잡는다(반대 방향 이동끼리 교착 방지).
-- 볼 수 없는 문서면 아무것도 하지 않는다(뒤의 확인이 404를 낸다). 아무 정보도 돌려주지 않는다.
CREATE FUNCTION app_lock_move(p_doc uuid, p_target uuid, p_item uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc  documents%ROWTYPE;
    v_from text;
    v_to   text;
  BEGIN
    IF app_document_access(p_doc) IS NULL THEN
      RETURN;
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    v_from := coalesce(v_doc.workspace_id::text, '') || ':' || coalesce(v_doc.item_id::text, '') || ':'
              || coalesce(v_doc.owner_user_id::text, '');
    v_to := coalesce(p_target::text, '') || ':' || coalesce(p_item::text, '') || ':';
    IF v_from <= v_to THEN
      PERFORM app_lock_tree(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id);
      PERFORM app_lock_tree(p_target, p_item, NULL);
    ELSE
      PERFORM app_lock_tree(p_target, p_item, NULL);
      PERFORM app_lock_tree(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id);
    END IF;
  END
  $$;

-- 깊이(최상위 = 1)와 하위 트리 높이(자신만 = 1). 설정 한도(최대 100)보다 넉넉히 끊는다.
CREATE FUNCTION app_document_depth(p_doc uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH RECURSIVE up(id, parent_id, depth) AS (
      SELECT id, parent_id, 1 FROM documents WHERE id = p_doc
      UNION ALL
      SELECT d.id, d.parent_id, up.depth + 1 FROM documents d JOIN up ON d.id = up.parent_id
      WHERE up.depth < 1000
    )
    SELECT max(depth) FROM up
  $$;

CREATE FUNCTION app_document_height(p_doc uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH RECURSIVE down(id, height) AS (
      SELECT id, 1 FROM documents WHERE id = p_doc
      UNION ALL
      SELECT d.id, down.height + 1 FROM documents d JOIN down ON d.parent_id = down.id
      WHERE down.height < 1000
    )
    SELECT max(height) FROM down
  $$;

------------------------------------------------------------------------------
-- 배치(내부): 자리 잠금과 문서 행 잠금을 이미 잡은 상태에서 부르는 공통 부분. 앱 역할에는 주지 않는다.
------------------------------------------------------------------------------

CREATE FUNCTION app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      PERFORM app_audit_document(p_doc, 'reparent', jsonb_build_object('parent', p_parent));
    END IF;
  END
  $$;

------------------------------------------------------------------------------
-- 부모·순서 바꾸기 (쓰기 권한). p_parent가 NULL이면 최상위, p_before가 있으면 그 형제 바로 앞, 없으면 맨 끝.
------------------------------------------------------------------------------

CREATE FUNCTION app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_place_document(p_doc, p_parent, p_before, p_max_depth);
  END
  $$;

------------------------------------------------------------------------------
-- 생성: 부모를 함께 받는다. 새로 만들 때만 배치한다(같은 request_id 재시도는 기존 문서를 그대로 돌려준다).
-- 부모가 있으면 자리 잠금을 번호 할당보다 먼저 잡는다(잠금 순서).
------------------------------------------------------------------------------

DROP FUNCTION app_create_wiki_document(uuid, uuid, text, integer, jsonb);
DROP FUNCTION app_create_module_document(uuid, uuid, text, integer, jsonb);
DROP FUNCTION app_create_personal_note(uuid, text, integer, jsonb);

CREATE FUNCTION app_create_wiki_document(
  p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb,
  p_parent uuid, p_max_depth integer
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
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(p_workspace, NULL, NULL);
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
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth);
    END IF;
    RETURN v_id;
  END
  $$;

CREATE FUNCTION app_create_module_document(
  p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb,
  p_parent uuid, p_max_depth integer
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
    v_id := uuidv7();
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'document', v_id);
    INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, 'module', v_item.module_id::text, v_number, btrim(p_title),
            p_schema_version, p_body, 1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth);
    END IF;
    RETURN v_id;
  END
  $$;

CREATE FUNCTION app_create_personal_note(
  p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer
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
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(NULL, NULL, v_user);
    END IF;
    INSERT INTO documents (owner_user_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id, visibility)
    VALUES (v_user, 'personal', 'NOTE', NULL, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id, 'private')
    RETURNING id INTO v_id;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth);
    END IF;
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 다른 자리로 이동: 자식은 원래 자리에 남기고, 이동한 문서가 있던 자리에 원래 순서대로 끼워 원래 부모에 붙인다.
-- 이동한 문서는 새 자리의 맨 끝.
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_move_document(p_doc uuid, p_target uuid, p_item uuid)
  RETURNS TABLE (workspace_id uuid, reference text)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
  BEGIN
    SELECT * INTO v_seen FROM documents d WHERE d.id = p_doc;
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
    PERFORM app_audit_document(p_doc, 'move_out', jsonb_build_object('reference', v_ref));
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
      PERFORM app_audit_document(v_child.id, 'reparent',
                                 jsonb_build_object('parent', v_doc.parent_id, 'reason', 'parent_moved'));
    END LOOP;

    UPDATE documents d
    SET workspace_id = p_target, owner_user_id = NULL, item_id = p_item,
        placement = CASE WHEN p_item IS NULL THEN 'wiki' ELSE 'module' END,
        scope = CASE WHEN p_item IS NULL THEN 'WIKI' ELSE v_scope::text END,
        number = v_number, parent_id = NULL,
        position = (SELECT coalesce(max(s.position), 0) + 1 FROM app_tree_siblings(p_target, p_item, NULL, NULL) s)
    WHERE d.id = p_doc;
    PERFORM app_audit_document(p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;

REVOKE ALL ON FUNCTION app_tree_siblings(uuid, uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_lock_tree(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_lock_move(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_document_depth(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_document_height(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_place_document(uuid, uuid, uuid, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_document_parent(uuid, uuid, uuid, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION documents_initial_position() FROM PUBLIC;
REVOKE ALL ON FUNCTION documents_tree_check() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_wiki_document(uuid, uuid, text, integer, jsonb, uuid, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_module_document(uuid, uuid, text, integer, jsonb, uuid, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_personal_note(uuid, text, integer, jsonb, uuid, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_set_document_parent(uuid, uuid, uuid, integer),
  app_lock_move(uuid, uuid, uuid),
  app_create_wiki_document(uuid, uuid, text, integer, jsonb, uuid, integer),
  app_create_module_document(uuid, uuid, text, integer, jsonb, uuid, integer),
  app_create_personal_note(uuid, text, integer, jsonb, uuid, integer)
TO fvoci_app;
