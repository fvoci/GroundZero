-- 모듈 인스턴스·키 대장·항목·공통 번호 대장·모듈 문서 (D-14, D-15, D-16, D-17, D-18, D-20, D-48, D-57, D-59).
--
-- 번호: 범위(scope)마다 번호열 하나. 위키 범위 id = 워크스페이스 id, 모듈 범위 id = 모듈 id.
--       numbered_targets가 "범위·번호 → 대상"을 책임지고, 할당은 app_allocate_number()만 한다.
-- 참조 문자열(DEV-3)은 저장하지 않고 읽을 때 현재 키로 만든다. 키가 바뀌어도 번호와 링크(id)는 그대로다.

------------------------------------------------------------------------------
-- 번호 대장
------------------------------------------------------------------------------

CREATE TABLE numbered_targets (
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  scope_id     uuid NOT NULL,
  number       bigint NOT NULL CHECK (number > 0),
  target_kind  text NOT NULL CHECK (target_kind IN ('item', 'document', 'task')),
  target_id    uuid NOT NULL,
  PRIMARY KEY (scope_id, number),
  CONSTRAINT numbered_targets_target UNIQUE (target_kind, target_id)
);

INSERT INTO numbered_targets (workspace_id, scope_id, number, target_kind, target_id)
SELECT workspace_id, workspace_id, number, 'document', id FROM documents WHERE placement = 'wiki';

-- 번호 할당(내부). 호출자가 권한을 확인한 뒤 부른다. 범위 카운터를 올리고 대장에 기록한다.
CREATE FUNCTION app_allocate_number(p_workspace uuid, p_scope uuid, p_kind text, p_target uuid)
  RETURNS bigint
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_scope_key text := CASE WHEN p_scope = p_workspace THEN 'WIKI' ELSE p_scope::text END;
    v_number    bigint;
  BEGIN
    INSERT INTO reference_counters (workspace_id, scope, next_number) VALUES (p_workspace, v_scope_key, 2)
    ON CONFLICT (workspace_id, scope)
      DO UPDATE SET next_number = reference_counters.next_number + 1
    RETURNING next_number - 1 INTO v_number;
    -- 이미 번호가 있는 대상(이동)이면 새 범위·번호로 옮긴다. 옛 번호는 카운터가 되돌아가지 않으므로 재사용되지 않는다.
    INSERT INTO numbered_targets (workspace_id, scope_id, number, target_kind, target_id)
    VALUES (p_workspace, p_scope, v_number, p_kind, p_target)
    ON CONFLICT ON CONSTRAINT numbered_targets_target
      DO UPDATE SET workspace_id = EXCLUDED.workspace_id, scope_id = EXCLUDED.scope_id, number = EXCLUDED.number;
    RETURN v_number;
  END
  $$;

------------------------------------------------------------------------------
-- 모듈과 키 대장
------------------------------------------------------------------------------

CREATE TABLE modules (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  -- 기능 유형은 만든 뒤 바꾸지 않는다 (D-16).
  kind         text NOT NULL CHECK (kind IN ('project', 'research', 'study', 'course', 'hobby')),
  name         text NOT NULL CHECK (length(btrim(name)) > 0),
  created_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  request_id   uuid NOT NULL,
  archived_at  timestamptz,
  CONSTRAINT modules_request UNIQUE (workspace_id, created_by, request_id)
);

-- 현재 키와 과거 키. 과거 키도 남아 다른 모듈이 다시 쓰지 못한다 (D-18). WIKI는 예약.
CREATE TABLE module_keys (
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  key          text NOT NULL CHECK (key ~ '^[A-Z][A-Z0-9]{1,9}$' AND key <> 'WIKI'),
  module_id    uuid NOT NULL REFERENCES modules (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  retired_at   timestamptz,
  PRIMARY KEY (workspace_id, key)
);
CREATE UNIQUE INDEX module_keys_current ON module_keys (module_id) WHERE retired_at IS NULL;

CREATE FUNCTION app_normalize_key(p_key text) RETURNS text
  LANGUAGE plpgsql IMMUTABLE
  AS $$
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
  $$;

------------------------------------------------------------------------------
-- 항목: 개별 프로젝트·연구·스터디·강좌·취미 활동 (D-14). 새 항목은 기본 비공개 (D-20).
------------------------------------------------------------------------------

CREATE TABLE items (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  module_id    uuid NOT NULL REFERENCES modules (id),
  title        text NOT NULL CHECK (length(btrim(title)) > 0),
  description  text NOT NULL DEFAULT '',
  visibility   text NOT NULL DEFAULT 'private' CHECK (visibility IN ('workspace', 'private')),
  created_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_by   uuid NOT NULL REFERENCES users (id),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  request_id   uuid NOT NULL,
  archived_at  timestamptz,
  CONSTRAINT items_request UNIQUE (module_id, created_by, request_id)
);
CREATE INDEX items_module ON items (module_id);

-- 항목 권한: 관리자와 항목을 만든 사람 → owner. 공개 항목이면 작성자 → write, 독자 → read.
CREATE FUNCTION app_item_access(p_item uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_item items%ROWTYPE;
    v_role text;
  BEGIN
    IF v_user IS NULL THEN
      RETURN NULL;
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    IF v_item.id IS NULL THEN
      RETURN NULL;
    END IF;
    v_role := app_member_role(v_item.workspace_id);
    IF v_role IS NULL THEN
      RETURN NULL;
    END IF;
    IF v_role = 'admin' THEN
      RETURN 'owner';
    END IF;
    -- 만든 사람도 독자로 내려가면 읽기만 한다.
    IF v_item.created_by = v_user THEN
      RETURN CASE v_role WHEN 'writer' THEN 'owner' ELSE 'read' END;
    END IF;
    IF v_item.visibility <> 'workspace' THEN
      RETURN NULL;
    END IF;
    RETURN CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
  END
  $$;

-- 보관(D-41의 휴지통과 다르다): 항목이나 그 모듈이 보관되면 읽기 전용이다. 번호와 키는 그대로 남는다.
CREATE FUNCTION app_item_archived(p_item uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
      WHERE i.id = p_item AND (i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL))
  $$;

------------------------------------------------------------------------------
-- 문서: 모듈 항목 아래 배치 (D-59)
------------------------------------------------------------------------------

ALTER TABLE documents ADD COLUMN item_id uuid REFERENCES items (id);
ALTER TABLE documents DROP CONSTRAINT documents_placement_check;
ALTER TABLE documents ADD CONSTRAINT documents_placement_check
  CHECK (placement IN ('wiki', 'module', 'personal'));
ALTER TABLE documents ADD CONSTRAINT documents_module_shape
  CHECK ((placement = 'module') = (item_id IS NOT NULL));
-- 모듈 문서의 scope는 모듈 id 문자열이다(reference_counters와 같은 범위 키). documents_number는 문서끼리의
-- 겹침을, numbered_targets는 항목·문서·작업 전체의 겹침을 막는다.
CREATE INDEX documents_item ON documents (item_id);

-- 문서 참조 문자열. 호출자 권한으로 읽으므로 키를 볼 수 없는 사람(멤버가 아닌 공유 대상)에게는 NULL.
CREATE FUNCTION app_document_reference(p_doc uuid) RETURNS text
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE
      WHEN d.placement = 'wiki' THEN 'WIKI-' || d.number
      WHEN d.placement = 'module' THEN (
        SELECT k.key || '-' || d.number FROM items i
        JOIN module_keys k ON k.module_id = i.module_id AND k.retired_at IS NULL
        WHERE i.id = d.item_id)
    END
    FROM documents d WHERE d.id = p_doc
  $$;

CREATE FUNCTION app_item_reference(p_item uuid) RETURNS text
  LANGUAGE sql STABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT k.key || '-' || n.number
    FROM numbered_targets n
    JOIN module_keys k ON k.module_id = n.scope_id AND k.retired_at IS NULL
    WHERE n.target_kind = 'item' AND n.target_id = p_item
  $$;

-- 문서 권한: 0004 판정에 항목 권한을 더한다. 항목 아래 문서는 항목 권한과 문서 규칙 중 낮은 쪽.
CREATE OR REPLACE FUNCTION app_document_access(p_doc uuid) RETURNS text
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
        -- 낮은 쪽: owner > write > read > NULL. 항목 owner(생성자)는 문서 규칙의 권한을 그대로 갖는다.
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

    v_base := CASE
      WHEN v_base = 'owner' THEN 'owner'
      WHEN v_base = 'write' OR v_share = 'write' THEN 'write'
      ELSE coalesce(v_base, v_share)
    END;
    -- 보관된 항목·모듈 아래 문서는 읽기 전용이다(보이기는 그대로).
    IF v_base IS NOT NULL AND v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id) THEN
      RETURN 'read';
    END IF;
    RETURN v_base;
  END
  $$;

------------------------------------------------------------------------------
-- 위키 문서 생성·복제: 번호 대장을 쓰도록 다시 만든다.
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_create_wiki_document(
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
    IF v_user IS NULL OR app_member_role(p_workspace) IS NULL
       OR app_member_role(p_workspace) NOT IN ('admin', 'writer') THEN
      RAISE EXCEPTION 'not allowed' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM documents
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
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

CREATE OR REPLACE FUNCTION app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_access text := app_require_document(p_doc, 'read');
    v_src    documents%ROWTYPE;
    v_id     uuid;
    v_number bigint;
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
      v_id := uuidv7();
      v_number := app_allocate_number(p_target_workspace, p_target_workspace, 'document', v_id);
      -- 비공개 문서·개인 노트의 사본은 비공개로 만든다(복제로 볼 수 있는 사람이 조용히 늘지 않게).
      INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (v_id, p_target_workspace, 'wiki', 'WIKI', v_number,
              btrim(p_title), v_src.schema_version, v_src.body, 1, v_user, v_user, p_request_id,
              CASE WHEN v_src.visibility = 'private' THEN 'private' ELSE 'workspace' END);
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

DROP FUNCTION app_next_wiki_number(uuid);

------------------------------------------------------------------------------
-- 모듈 관리 (관리자)
------------------------------------------------------------------------------

CREATE FUNCTION app_create_module(p_workspace uuid, p_request_id uuid, p_kind text, p_name text, p_key text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
    v_key  text := app_normalize_key(p_key);
    v_id   uuid;
  BEGIN
    SELECT id INTO v_id FROM modules
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_kind NOT IN ('project', 'research', 'study', 'course', 'hobby') THEN
      RAISE EXCEPTION 'unknown module kind' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM module_keys WHERE workspace_id = p_workspace AND key = v_key) THEN
      RAISE EXCEPTION 'this key is in use or was used before' USING ERRCODE = '55000';
    END IF;
    INSERT INTO modules (workspace_id, kind, name, created_by, request_id)
    VALUES (p_workspace, p_kind, btrim(p_name), v_user, p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO module_keys (workspace_id, key, module_id) VALUES (p_workspace, v_key, v_id);
    PERFORM app_audit(p_workspace, 'create', 'module', v_id,
                      jsonb_build_object('kind', p_kind, 'name', btrim(p_name), 'key', v_key));
    RETURN v_id;
  END
  $$;

CREATE FUNCTION app_update_module(p_module uuid, p_name text, p_key text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_mod     modules%ROWTYPE;
    v_key     text := app_normalize_key(p_key);
    v_current text;
    v_owner   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR UPDATE;
    IF v_mod.id IS NULL OR app_member_role(v_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_mod.workspace_id);

    IF v_mod.name <> btrim(p_name) THEN
      UPDATE modules SET name = btrim(p_name) WHERE id = p_module;
      PERFORM app_audit(v_mod.workspace_id, 'rename', 'module', p_module,
                        jsonb_build_object('from', v_mod.name, 'to', btrim(p_name)));
    END IF;

    SELECT key INTO v_current FROM module_keys WHERE module_id = p_module AND retired_at IS NULL;
    IF v_current <> v_key THEN
      SELECT module_id INTO v_owner FROM module_keys WHERE workspace_id = v_mod.workspace_id AND key = v_key;
      IF v_owner IS NOT NULL AND v_owner <> p_module THEN
        RAISE EXCEPTION 'this key is in use or was used before' USING ERRCODE = '55000';
      END IF;
      UPDATE module_keys SET retired_at = now() WHERE module_id = p_module AND retired_at IS NULL;
      IF v_owner = p_module THEN
        -- 같은 모듈의 옛 키를 되찾는다.
        UPDATE module_keys SET retired_at = NULL WHERE workspace_id = v_mod.workspace_id AND key = v_key;
      ELSE
        INSERT INTO module_keys (workspace_id, key, module_id) VALUES (v_mod.workspace_id, v_key, p_module);
      END IF;
      PERFORM app_audit(v_mod.workspace_id, 'change_key', 'module', p_module,
                        jsonb_build_object('from', v_current, 'to', v_key));
    END IF;
  END
  $$;

CREATE FUNCTION app_archive_module(p_module uuid, p_archived boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_mod modules%ROWTYPE;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR UPDATE;
    IF v_mod.id IS NULL OR app_member_role(v_mod.workspace_id) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_mod.workspace_id);
    IF (v_mod.archived_at IS NOT NULL) <> p_archived THEN
      UPDATE modules SET archived_at = CASE WHEN p_archived THEN now() END WHERE id = p_module;
      PERFORM app_audit(v_mod.workspace_id, CASE WHEN p_archived THEN 'archive' ELSE 'unarchive' END,
                        'module', p_module, '{}'::jsonb);
    END IF;
  END
  $$;

------------------------------------------------------------------------------
-- 항목 관리
------------------------------------------------------------------------------

CREATE FUNCTION app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_mod  modules%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module;
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

CREATE FUNCTION app_update_item(p_item uuid, p_title text, p_description text, p_visibility text)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(v_item.id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF p_visibility <> v_item.visibility AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can change visibility' USING ERRCODE = '42501';
    END IF;
    UPDATE items SET title = btrim(p_title), description = coalesce(p_description, ''),
                     visibility = p_visibility, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.title <> btrim(p_title) THEN
      PERFORM app_audit(v_item.workspace_id, 'rename', 'item', p_item,
                        jsonb_build_object('from', v_item.title, 'to', btrim(p_title)));
    END IF;
    IF v_item.visibility <> p_visibility THEN
      PERFORM app_audit(v_item.workspace_id, 'change_visibility', 'item', p_item,
                        jsonb_build_object('from', v_item.visibility, 'to', p_visibility));
    END IF;
  END
  $$;

CREATE FUNCTION app_archive_item(p_item uuid, p_archived boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item items%ROWTYPE;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_access(p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can archive' USING ERRCODE = '42501';
    END IF;
    IF (v_item.archived_at IS NOT NULL) <> p_archived THEN
      UPDATE items SET archived_at = CASE WHEN p_archived THEN now() END WHERE id = p_item;
      PERFORM app_audit(v_item.workspace_id, CASE WHEN p_archived THEN 'archive' ELSE 'unarchive' END,
                        'item', p_item, '{}'::jsonb);
    END IF;
  END
  $$;

-- 항목 아래 문서 생성(항목 쓰기 권한).
CREATE FUNCTION app_create_module_document(
  p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb
) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_item   items%ROWTYPE;
    v_access text := app_item_access(p_item);
    v_id     uuid;
    v_number bigint;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    IF app_item_archived(v_item.id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    SELECT id INTO v_id FROM documents
    WHERE workspace_id = v_item.workspace_id AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
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

------------------------------------------------------------------------------
-- 이동 일반화: 대상 = 워크스페이스 + (항목 또는 위키). 같은 워크스페이스 안 위키↔항목 이동 (D-59).
------------------------------------------------------------------------------

DROP FUNCTION app_move_document(uuid, uuid);
DROP FUNCTION app_move_preview(uuid, uuid);
DROP FUNCTION app_check_move(uuid, uuid);
DROP FUNCTION app_document_audience(uuid, uuid, uuid, text, uuid);

CREATE FUNCTION app_document_audience(
  p_workspace uuid, p_item uuid, p_owner uuid, p_creator uuid, p_visibility text, p_doc uuid
) RETURNS TABLE (user_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT p_owner WHERE p_owner IS NOT NULL
    UNION
    SELECT m.user_id FROM workspace_members m
    LEFT JOIN items i ON i.id = p_item
    WHERE p_workspace IS NOT NULL AND m.workspace_id = p_workspace
      AND (m.role = 'admin' OR p_visibility = 'workspace' OR m.user_id = p_creator)
      AND (p_item IS NULL OR m.role = 'admin' OR i.visibility = 'workspace' OR i.created_by = m.user_id)
    UNION
    SELECT u.id FROM document_shares s JOIN users u
      ON u.email_verified AND lower(u.email) = s.grantee_email
    WHERE s.document_id = p_doc
  $$;

CREATE FUNCTION app_check_move(p_doc uuid, p_target uuid, p_item uuid) RETURNS documents
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE;
    v_item items%ROWTYPE;
  BEGIN
    PERFORM app_require_document(p_doc, 'owner');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF p_target IS NULL THEN
      RAISE EXCEPTION 'team documents stay in the workspace; make a copy instead' USING ERRCODE = '55000';
    END IF;
    IF v_doc.workspace_id IS NOT DISTINCT FROM p_target AND v_doc.item_id IS NOT DISTINCT FROM p_item THEN
      RAISE EXCEPTION 'the document is already there' USING ERRCODE = '55000';
    END IF;
    IF app_member_role(p_target) IS NULL THEN
      RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_target) NOT IN ('admin', 'writer') THEN
      RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
    END IF;
    IF p_item IS NOT NULL THEN
      SELECT * INTO v_item FROM items WHERE id = p_item;
      IF v_item.id IS NULL OR v_item.workspace_id <> p_target OR app_item_access(p_item) IS NULL THEN
        RAISE EXCEPTION 'target item not found' USING ERRCODE = 'P0002';
      END IF;
      IF app_item_access(p_item) NOT IN ('owner', 'write') THEN
        RAISE EXCEPTION 'write access to the target item is required' USING ERRCODE = '42501';
      END IF;
      IF app_item_archived(v_item.id) THEN
        RAISE EXCEPTION 'the target item is archived' USING ERRCODE = '55000';
      END IF;
    END IF;
    RETURN v_doc;
  END
  $$;

CREATE FUNCTION app_move_preview(p_doc uuid, p_target uuid, p_item uuid)
  RETURNS TABLE (change text, display_name text)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc documents%ROWTYPE := app_check_move(p_doc, p_target, p_item);
  BEGIN
    RETURN QUERY
    WITH old_set AS (
      SELECT a.user_id AS uid FROM app_document_audience(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id,
                                                         v_doc.created_by, v_doc.visibility, p_doc) a
    ), new_set AS (
      SELECT a.user_id AS uid FROM app_document_audience(p_target, p_item, NULL, v_doc.created_by,
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

CREATE FUNCTION app_move_document(p_doc uuid, p_target uuid, p_item uuid)
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
    v_doc := app_check_move(p_doc, p_target, p_item);
    v_ref := app_document_reference(p_doc);
    PERFORM app_audit_document(p_doc, 'move_out', jsonb_build_object('reference', v_ref));
    v_scope := CASE WHEN p_item IS NULL THEN p_target ELSE (SELECT module_id FROM items WHERE id = p_item) END;
    v_number := app_allocate_number(p_target, v_scope, 'document', p_doc);
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
-- RLS·권한
------------------------------------------------------------------------------

ALTER TABLE modules ENABLE ROW LEVEL SECURITY;
CREATE POLICY modules_select ON modules FOR SELECT USING (app_member_role(workspace_id) IS NOT NULL);
ALTER TABLE module_keys ENABLE ROW LEVEL SECURITY;
CREATE POLICY module_keys_select ON module_keys FOR SELECT USING (app_member_role(workspace_id) IS NOT NULL);
ALTER TABLE items ENABLE ROW LEVEL SECURITY;
CREATE POLICY items_select ON items FOR SELECT USING (app_item_access(id) IS NOT NULL);
ALTER TABLE numbered_targets ENABLE ROW LEVEL SECURITY;
-- 번호 대장은 대상을 볼 수 있는 사람에게만 보인다(비공개 항목의 존재를 번호로 드러내지 않는다).
CREATE POLICY numbered_targets_select ON numbered_targets FOR SELECT
  USING (CASE target_kind
           WHEN 'item' THEN app_item_access(target_id) IS NOT NULL
           WHEN 'document' THEN app_document_access(target_id) IS NOT NULL
           ELSE false
         END);
GRANT SELECT ON modules, module_keys, items, numbered_targets TO fvoci_app;

DO $$
DECLARE
  f text;
BEGIN
  FOREACH f IN ARRAY ARRAY[
    'app_allocate_number(uuid, uuid, text, uuid)',
    'app_item_access(uuid)',
    'app_item_archived(uuid)',
    'app_document_reference(uuid)',
    'app_item_reference(uuid)',
    'app_create_module(uuid, uuid, text, text, text)',
    'app_update_module(uuid, text, text)',
    'app_archive_module(uuid, boolean)',
    'app_create_item(uuid, uuid, text, text)',
    'app_update_item(uuid, text, text, text)',
    'app_archive_item(uuid, boolean)',
    'app_create_module_document(uuid, uuid, text, integer, jsonb)',
    'app_document_audience(uuid, uuid, uuid, uuid, text, uuid)',
    'app_check_move(uuid, uuid, uuid)',
    'app_move_preview(uuid, uuid, uuid)',
    'app_move_document(uuid, uuid, uuid)'
  ] LOOP
    EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', f);
  END LOOP;
END
$$;

GRANT EXECUTE ON FUNCTION
  app_item_access(uuid),
  app_document_reference(uuid),
  app_item_reference(uuid),
  app_create_module(uuid, uuid, text, text, text),
  app_update_module(uuid, text, text),
  app_archive_module(uuid, boolean),
  app_create_item(uuid, uuid, text, text),
  app_update_item(uuid, text, text, text),
  app_archive_item(uuid, boolean),
  app_create_module_document(uuid, uuid, text, integer, jsonb),
  app_move_preview(uuid, uuid, uuid),
  app_move_document(uuid, uuid, uuid)
TO fvoci_app;
