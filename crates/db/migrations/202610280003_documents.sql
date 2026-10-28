-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

CREATE TABLE fvoci.document_collab_file_refs (
    document_id uuid NOT NULL,
    file_id uuid NOT NULL,
    version integer NOT NULL
);


ALTER TABLE fvoci.document_collab_file_refs OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_collab_states (
    document_id uuid NOT NULL,
    revision integer NOT NULL,
    state bytea NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    pending boolean DEFAULT false NOT NULL,
    editors uuid[] DEFAULT '{}'::uuid[] NOT NULL
);


ALTER TABLE fvoci.document_collab_states OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_file_refs (
    document_id uuid NOT NULL,
    revision integer NOT NULL,
    file_id uuid NOT NULL,
    version integer NOT NULL
);


ALTER TABLE fvoci.document_file_refs OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_links (
    document_id uuid NOT NULL,
    target_kind text NOT NULL,
    target_id uuid NOT NULL,
    CONSTRAINT document_links_target_kind_check CHECK ((target_kind = ANY (ARRAY['item'::text, 'document'::text, 'task'::text])))
);


ALTER TABLE fvoci.document_links OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_mentions (
    document_id uuid NOT NULL,
    user_id uuid NOT NULL,
    mentioned_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.document_mentions OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_revisions (
    document_id uuid NOT NULL,
    revision integer NOT NULL,
    title text NOT NULL,
    schema_version integer NOT NULL,
    body jsonb NOT NULL,
    author uuid NOT NULL,
    request_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    restored_from integer,
    edited_by uuid[],
    CONSTRAINT document_revisions_restored_from_check CHECK ((restored_from > 0)),
    CONSTRAINT document_revisions_revision_check CHECK ((revision > 0))
);


ALTER TABLE fvoci.document_revisions OWNER TO fvoci_migrator;

-- 개별 공유(D-23·D-49·D-114). 받는 사람은 조직 멤버이고 공유 행에 조직을 기록한다. 둘 중 하나가 그 조직에서 빠지면 철회한다.
CREATE TABLE fvoci.document_shares (
    id uuid NOT NULL,
    document_id uuid NOT NULL,
    organization_id uuid NOT NULL,
    grantee_user_id uuid NOT NULL,
    permission text NOT NULL,
    granted_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT document_shares_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT document_shares_permission_check CHECK ((permission = ANY (ARRAY['read'::text, 'write'::text])))
);


ALTER TABLE fvoci.document_shares OWNER TO fvoci_migrator;

CREATE TABLE fvoci.documents (
    id uuid NOT NULL,
    workspace_id uuid,
    placement text NOT NULL,
    scope text NOT NULL,
    number bigint,
    title text NOT NULL,
    schema_version integer NOT NULL,
    body jsonb NOT NULL,
    revision integer NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    owner_user_id uuid,
    visibility text DEFAULT 'workspace'::text NOT NULL,
    allow_copy boolean DEFAULT false NOT NULL,
    item_id uuid,
    parent_id uuid,
    "position" numeric DEFAULT 0 NOT NULL,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    is_template boolean DEFAULT false NOT NULL,
    CONSTRAINT documents_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT documents_module_shape CHECK (((placement = 'module'::text) = (item_id IS NOT NULL))),
    CONSTRAINT documents_one_owner CHECK (((workspace_id IS NULL) <> (owner_user_id IS NULL))),
    CONSTRAINT documents_personal_shape CHECK ((((placement = 'personal'::text) = (owner_user_id IS NOT NULL)) AND ((placement = 'personal'::text) = (number IS NULL)))),
    CONSTRAINT documents_placement_check CHECK ((placement = ANY (ARRAY['wiki'::text, 'module'::text, 'personal'::text]))),
    CONSTRAINT documents_revision_check CHECK ((revision > 0)),
    CONSTRAINT documents_schema_version_check CHECK ((schema_version > 0)),
    CONSTRAINT documents_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT documents_visibility_check CHECK ((visibility = ANY (ARRAY['workspace'::text, 'private'::text])))
);


ALTER TABLE fvoci.documents OWNER TO fvoci_migrator;

CREATE TABLE fvoci.public_links (
    id uuid NOT NULL,
    document_id uuid NOT NULL,
    token_hash bytea NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone,
    revoked_at timestamp with time zone,
    CONSTRAINT public_links_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.public_links OWNER TO fvoci_migrator;

-- 공유·공개 링크·설정 관리는 지금 등급이 owner인 사람만 한다(D-113: 등급 하나). 보관된 항목의 문서는 read로 낮아지지만
-- 워크스페이스 관리자는 보관 중에도 관리할 수 있다(개정 2026-10-26 ⑯, "보관 중 관리자 예외" 복원; 등급 위에 얹는 추가 조건).
CREATE FUNCTION fvoci.app_document_can_manage(p_doc uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
    v_doc    documents%ROWTYPE;
  BEGIN
    -- 문서 관리(공유·링크·설정)는 owner 등급 하나다(D-113·D-115). 404/보관 409/403의 구분은 app_require_document와 같다.
    IF v_access = 'owner' THEN
      RETURN true;
    END IF;
    -- 개정 2026-10-26(⑯, D-113·D-106): 보관된 항목의 문서는 등급이 read로 낮아지지만, 워크스페이스 관리자(보관 전 owner 등급)는
    -- 보관 해제 없이 공유·공개 링크·설정을 바꿀 수 있다. 내용 편집·댓글·상태 변경은 보관 중 409 그대로(app_require_document).
    IF v_access <> 'none' THEN
      SELECT * INTO v_doc FROM documents WHERE id = p_doc;
      IF v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id)
         AND app_workspace_role(v_doc.workspace_id, app_user_id()) = 'admin' THEN
        RETURN true;
      END IF;
    END IF;
    RETURN false;
  END
  $$;

ALTER FUNCTION fvoci.app_document_can_manage(p_doc uuid) OWNER TO fvoci_migrator;

-- 개별 공유(D-23·D-49·D-114). 받는 사람은 이메일이 검증된 사용자이자 같은 조직 멤버다. 공간 문서는 그 공간의 조직,
-- 개인 노트는 둘이 함께 속한 조직(둘 이상이면 p_organization으로 고른다)을 공유 행에 적는다. 역할이 상한이다(D-20).
CREATE FUNCTION fvoci.app_share_document(p_doc uuid, p_email text, p_permission text, p_organization uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc     documents%ROWTYPE;
    v_email   text := lower(btrim(p_email));
    v_grantee uuid;
    v_org     uuid;
    v_common  uuid[];
    v_role    text;
    v_id      uuid;
  BEGIN
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    IF p_permission NOT IN ('read', 'write') THEN
      RAISE EXCEPTION 'invalid permission' USING ERRCODE = '22023';
    END IF;
    IF position('@' in v_email) < 2 THEN
      RAISE EXCEPTION 'invalid email' USING ERRCODE = '22023';
    END IF;
    SELECT u.id INTO v_grantee FROM users u WHERE u.email_verified AND lower(u.email) = v_email
    ORDER BY u.created_at LIMIT 1;
    IF v_grantee IS NULL THEN
      RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
    END IF;
    IF v_grantee = app_user_id() THEN
      RAISE EXCEPTION 'you already own this document' USING ERRCODE = '22023';
    END IF;
    IF v_doc.owner_user_id IS NOT NULL THEN
      v_common := ARRAY(SELECT o1.organization_id FROM organization_members o1
                        JOIN organization_members o2 ON o2.organization_id = o1.organization_id AND o2.user_id = v_grantee
                        WHERE o1.user_id = v_doc.owner_user_id
                          AND (p_organization IS NULL OR o1.organization_id = p_organization)
                        ORDER BY o1.organization_id);
      IF cardinality(v_common) = 0 THEN
        RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
      END IF;
      IF cardinality(v_common) > 1 THEN
        RAISE EXCEPTION 'choose the organization for this share' USING ERRCODE = '22023';
      END IF;
      v_org := v_common[1];
    ELSE
      SELECT organization_id INTO v_org FROM workspaces WHERE id = v_doc.workspace_id;
      IF p_organization IS NOT NULL AND p_organization <> v_org THEN
        RAISE EXCEPTION 'the document belongs to another organization' USING ERRCODE = '22023';
      END IF;
      IF NOT EXISTS (SELECT 1 FROM organization_members WHERE organization_id = v_org AND user_id = v_grantee) THEN
        RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
      END IF;
      v_role := app_workspace_role(v_doc.workspace_id, v_grantee);
      IF v_role = 'readonly' AND p_permission = 'write' THEN
        RAISE EXCEPTION 'a read-only member cannot receive write access' USING ERRCODE = '22023';
      END IF;
    END IF;
    -- 조직 제거·탈퇴(멤버 행 갱신 잠금 뒤 공유 삭제)와 줄 세운다(D-114).
    PERFORM 1 FROM organization_members WHERE organization_id = v_org AND user_id = v_grantee FOR SHARE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
    END IF;
    INSERT INTO document_shares (id, document_id, organization_id, grantee_user_id, permission, granted_by)
    VALUES (app_id(p_ids, 1), p_doc, v_org, v_grantee, p_permission, app_user_id())
    ON CONFLICT ON CONSTRAINT document_shares_target
      DO UPDATE SET permission = EXCLUDED.permission, granted_by = EXCLUDED.granted_by,
                    organization_id = EXCLUDED.organization_id
    RETURNING id INTO v_id;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'share',
                               jsonb_build_object('user', v_grantee, 'email', v_email, 'permission', p_permission,
                                                  'organization', v_org));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_share_document(p_doc uuid, p_email text, p_permission text, p_organization uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_unshare_document(p_share uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_share document_shares%ROWTYPE;
  BEGIN
    SELECT * INTO v_share FROM document_shares WHERE id = p_share;
    PERFORM 1 FROM documents WHERE id = v_share.document_id FOR UPDATE;
    IF v_share.id IS NULL OR app_access('document', v_share.document_id) = 'none' THEN
      RAISE EXCEPTION 'share not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_document_manager(v_share.document_id);
    DELETE FROM document_shares WHERE id = p_share;
    PERFORM app_audit_document(app_id(p_ids, 1), v_share.document_id, 'unshare',
                               jsonb_build_object('user', v_share.grantee_user_id));
  END
  $$;


ALTER FUNCTION fvoci.app_unshare_document(p_share uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_shared_with_me(p_organization uuid DEFAULT NULL::uuid) RETURNS TABLE(id uuid, title text, permission text, shared_by text, updated_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT d.id, d.title, s.permission, granter.display_name, d.updated_at
    FROM document_shares s
    JOIN documents d ON d.id = s.document_id
    JOIN users granter ON granter.id = s.granted_by
    -- 공유받은 문서는 현재 조직 범위(D-111): p_organization이 NULL이면 전체. 공유 행의 조직은 공유를 준 조직이다(D-114).
    WHERE s.grantee_user_id = app_user_id() AND app_access('document', d.id) <> 'none'
      AND (p_organization IS NULL OR s.organization_id = p_organization)
    ORDER BY d.updated_at DESC
  $$;


ALTER FUNCTION fvoci.app_shared_with_me(p_organization uuid) OWNER TO fvoci_migrator;

-- 문서가 그 자리에 있을 때 볼 수 있는 사람과 등급. 후보는 소유자·공간 멤버·공유받은 사람이다.
CREATE FUNCTION fvoci.app_document_audience(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid) RETURNS TABLE(user_id uuid, grade text)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT c.id, g.grade
    FROM (SELECT p_owner AS id WHERE p_owner IS NOT NULL
          UNION SELECT m.user_id FROM workspace_members m WHERE p_workspace IS NOT NULL AND m.workspace_id = p_workspace
          UNION SELECT s.grantee_user_id FROM document_shares s WHERE s.document_id = p_doc
          UNION SELECT d.owner_user_id FROM documents d WHERE d.id = p_doc AND d.owner_user_id IS NOT NULL) c
    CROSS JOIN LATERAL (SELECT app_grade_document_at(p_doc, p_workspace, p_item, p_owner, c.id) AS grade) g
    WHERE g.grade <> 'none'
  $$;


ALTER FUNCTION fvoci.app_document_audience(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid) OWNER TO fvoci_migrator;

-- 이동 전 확인(D-24·D-114): 사용자 id·등급 단위의 변화. 공간 사이 이동은 같은 조직 안에서만 한다(app_check_move).
-- 개인 노트는 소유자가 속한 어느 공간으로든 옮길 수 있고, 그 공간의 조직 밖 개별 공유는 app_move_document가 철회한다.
CREATE FUNCTION fvoci.app_move_preview(p_doc uuid, p_target uuid, p_item uuid) RETURNS TABLE(change text, user_id uuid, display_name text, from_grade text, to_grade text)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE := app_check_move(p_doc, p_target, p_item);
  BEGIN
    RETURN QUERY
    WITH old_set AS (
      SELECT a.user_id AS uid, a.grade FROM app_document_audience(p_doc, v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id) a
    ), new_set AS (
      SELECT a.user_id AS uid, a.grade FROM app_document_audience(p_doc, p_target, p_item, NULL) a
    )
    SELECT CASE WHEN o.uid IS NULL THEN 'gain' WHEN n.uid IS NULL THEN 'lose' ELSE 'change' END::text,
           coalesce(o.uid, n.uid), u.display_name, o.grade, n.grade
    FROM old_set o FULL JOIN new_set n ON n.uid = o.uid
    JOIN users u ON u.id = coalesce(o.uid, n.uid)
    WHERE o.grade IS DISTINCT FROM n.grade
    ORDER BY 1, 3, 2;
  END
  $$;


ALTER FUNCTION fvoci.app_move_preview(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_move(p_doc uuid, p_target uuid, p_item uuid) RETURNS fvoci.documents
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    IF app_access('workspace', p_target) = 'none' THEN
      RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('workspace', p_target) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
    END IF;
    -- 공간 간 이동은 같은 조직 안에서만 한다(D-24·D-114). 개인 노트는 소유자가 속한 어느 공간으로든 옮길 수 있다.
    IF v_doc.workspace_id IS NOT NULL
       AND app_target_organization('workspace', v_doc.workspace_id) IS DISTINCT FROM app_target_organization('workspace', p_target) THEN
      RAISE EXCEPTION 'documents move within the organization' USING ERRCODE = '55000';
    END IF;
    IF p_item IS NOT NULL THEN
      SELECT * INTO v_item FROM items WHERE id = p_item;
      IF v_item.id IS NULL OR v_item.workspace_id <> p_target OR app_access('item', p_item) = 'none' THEN
        RAISE EXCEPTION 'target item not found' USING ERRCODE = 'P0002';
      END IF;
      IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
        RAISE EXCEPTION 'write access to the target item is required' USING ERRCODE = '42501';
      END IF;
      IF app_item_archived(v_item.id) THEN
        RAISE EXCEPTION 'the target item is archived' USING ERRCODE = '55000';
      END IF;
    END IF;
    RETURN v_doc;
  END
  $$;


ALTER FUNCTION fvoci.app_check_move(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

-- 복제(D-23·D-115). 소유 권한이 없으면 allow_copy가 켜진 문서만 복제한다. 템플릿에서 새 문서 만들기는 예외다.
-- 공간 간 복제는 같은 조직 안에서만 한다(D-114). 새 id 4: 문서, 원본 감사, 사본 감사, 사본 활동(개인 노트 사본은 3).
CREATE FUNCTION fvoci.app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    IF v_access <> 'owner' AND NOT v_src.allow_copy AND NOT v_src.is_template THEN
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
      IF app_access('workspace', p_target_workspace) = 'none' THEN
        RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
      END IF;
      IF app_access('workspace', p_target_workspace) NOT IN ('owner', 'write') THEN
        RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
      END IF;
      IF v_src.workspace_id IS NOT NULL
         AND app_target_organization('workspace', v_src.workspace_id) IS DISTINCT FROM app_target_organization('workspace', p_target_workspace) THEN
        RAISE EXCEPTION 'copies stay within the organization' USING ERRCODE = '55000';
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
    IF p_target_workspace IS NOT NULL THEN
      PERFORM app_activity(app_id(p_ids, 4), p_target_workspace, NULL, 'document', v_id, 'create',
                           jsonb_build_object('title', btrim(p_title), 'copied_from', p_doc));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 공간(위키 p_workspace 또는 항목 p_item)에 템플릿으로 새 문서를 만들 때의 본문(D-27·D-114·D-115). 읽을 수 있는 템플릿이어야
-- 하고, 템플릿 예외는 allow_copy만 넘는다: 공간 문서 템플릿은 복제처럼 같은 조직 공간에서만 쓰고 원본에 복제 감사를 남긴다.
-- 개인 노트 대상은 이 함수를 거치지 않는다. 새 id 1: 원본 감사.
CREATE FUNCTION fvoci.app_template_body(p_template uuid, p_workspace uuid, p_item uuid, p_request_id uuid, p_ids uuid[]) RETURNS jsonb
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_kind   text := CASE WHEN p_item IS NULL THEN 'workspace' ELSE 'item' END;
    v_target uuid := coalesce(p_item, p_workspace);
    v_ws     uuid := coalesce(p_workspace, (SELECT i.workspace_id FROM items i WHERE i.id = p_item));
    v_src    documents%ROWTYPE;
  BEGIN
    SELECT * INTO v_src FROM documents WHERE id = p_template AND is_template AND deleted_at IS NULL;
    IF v_src.id IS NULL OR app_access('document', p_template) = 'none' THEN
      RAISE EXCEPTION 'template not found' USING ERRCODE = 'P0002';
    END IF;
    IF (p_workspace IS NULL) = (p_item IS NULL) THEN
      RAISE EXCEPTION 'choose a workspace or an item' USING ERRCODE = '22023';
    END IF;
    IF app_access(v_kind, v_target) = 'none' THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_src.workspace_id IS NOT NULL
       AND app_target_organization('document', p_template) IS DISTINCT FROM app_target_organization(v_kind, v_target) THEN
      RAISE EXCEPTION 'copies stay within the organization' USING ERRCODE = '55000';
    END IF;
    -- 같은 request_id로 다시 보낸 생성은 이미 만든 문서를 돌려받으므로 감사를 다시 남기지 않는다(app_copy_document와 같다).
    IF NOT EXISTS (SELECT 1 FROM documents
                   WHERE workspace_id = v_ws AND created_by = app_user_id() AND request_id = p_request_id) THEN
      PERFORM app_audit_document(app_id(p_ids, 1), p_template, 'copy',
                                 jsonb_build_object('target', 'workspace', 'template', true));
    END IF;
    RETURN v_src.body;
  END
  $$;


ALTER FUNCTION fvoci.app_template_body(p_template uuid, p_workspace uuid, p_item uuid, p_request_id uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-115: allow_copy가 꺼진 문서는 소유 권한 외 누구도(수정 권한 포함) 내보내거나 첨부를 내려받지 못한다.
CREATE FUNCTION fvoci.app_document_can_export(p_doc uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
  BEGIN
    IF v_access = 'none' THEN
      RETURN false;
    END IF;
    IF v_access = 'owner' THEN
      RETURN true;
    END IF;
    RETURN coalesce((SELECT allow_copy FROM documents WHERE id = p_doc AND deleted_at IS NULL), false);
  END
  $$;


ALTER FUNCTION fvoci.app_document_can_export(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc        documents%ROWTYPE;
    v_visibility text;
    v_allow_copy boolean;
  BEGIN
    -- 행을 먼저 잠그고 권한을 본다. 동시에 이동된 문서에 이전 소유 권한으로 설정을 바꾸지 못하게 한다.
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    -- 보내지 않은 칸은 잠근 뒤의 지금 값 그대로.
    v_visibility := coalesce(p_visibility, v_doc.visibility);
    v_allow_copy := coalesce(p_allow_copy, v_doc.allow_copy);
    IF v_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF v_doc.owner_user_id IS NOT NULL AND v_visibility <> 'private' THEN
      RAISE EXCEPTION 'personal notes are always private' USING ERRCODE = '22023';
    END IF;
    UPDATE documents SET visibility = v_visibility, allow_copy = v_allow_copy WHERE id = p_doc;
    IF v_doc.visibility IS DISTINCT FROM v_visibility THEN
      PERFORM app_audit_document(app_id(p_ids, 1), p_doc, 'change_visibility',
                                 jsonb_build_object('from', v_doc.visibility, 'to', v_visibility));
    END IF;
    IF v_doc.allow_copy IS DISTINCT FROM v_allow_copy THEN
      PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'change_copy_policy',
                                 jsonb_build_object('allow_copy', v_allow_copy));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 비로그인 공개 링크(D-20·D-114): 유일한 조직 밖 출구. 조직 관리자가 끄면 새 링크를 만들 수 없고 기존 링크도 응답하지 않는다.
CREATE FUNCTION fvoci.app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamp with time zone, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id uuid;
    v_ws uuid;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    IF p_expires_at IS NOT NULL AND p_expires_at <= now() THEN
      RAISE EXCEPTION 'expiry must be in the future' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM documents WHERE id = p_doc;
    IF v_ws IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM workspaces w JOIN organizations o ON o.id = w.organization_id
      WHERE w.id = v_ws AND o.public_links_enabled
    ) THEN
      RAISE EXCEPTION 'public links are turned off for this organization' USING ERRCODE = '55000';
    END IF;
    INSERT INTO public_links (id, document_id, token_hash, created_by, expires_at)
    VALUES (app_id(p_ids, 1), p_doc, p_token_hash, app_user_id(), p_expires_at)
    RETURNING id INTO v_id;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'create_public_link',
                               jsonb_build_object('link', v_id, 'expires_at', p_expires_at));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamp with time zone, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_revoke_public_link(p_link uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc uuid;
  BEGIN
    SELECT document_id INTO v_doc FROM public_links WHERE id = p_link AND revoked_at IS NULL;
    IF v_doc IS NULL OR app_access('document', v_doc) = 'none' THEN
      RAISE EXCEPTION 'link not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM 1 FROM documents WHERE id = v_doc FOR UPDATE;
    PERFORM app_require_document_manager(v_doc);
    UPDATE public_links SET revoked_at = now() WHERE id = p_link;
    PERFORM app_audit_document(app_id(p_ids, 1), v_doc, 'revoke_public_link', jsonb_build_object('link', p_link));
  END
  $$;


ALTER FUNCTION fvoci.app_revoke_public_link(p_link uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_public_document(p_token_hash bytea) RETURNS TABLE(title text, body jsonb, schema_version integer, updated_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT d.title, d.body, d.schema_version, d.updated_at
    FROM public_links l JOIN documents d ON d.id = l.document_id
    WHERE l.token_hash = p_token_hash
      AND l.revoked_at IS NULL
      AND (l.expires_at IS NULL OR l.expires_at > now())
      AND d.deleted_at IS NULL
      -- 항목이나 그 모듈이 휴지통에 있으면 열리지 않는다.
      AND NOT EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                      WHERE i.id = d.item_id AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL))
      AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = d.workspace_id AND w.deleted_at IS NOT NULL)
      -- 조직이 공개 링크를 껐으면 기존 링크도 응답하지 않는다(D-114).
      AND (d.workspace_id IS NULL OR EXISTS (
            SELECT 1 FROM workspaces w JOIN organizations o ON o.id = w.organization_id
            WHERE w.id = d.workspace_id AND o.public_links_enabled))
  $$;


ALTER FUNCTION fvoci.app_public_document(p_token_hash bytea) OWNER TO fvoci_migrator;

-- 본문의 @참조와 언급(D-18·D-114). 연결은 작성자가 볼 수 있고 같은 조직 안에 있는 대상만 남긴다(개인 노트는 조직과 무관하므로
-- 볼 수 있는 대상이면 된다). 언급 알림은 그 사람이 문서를 볼 수 있을 때만 간다.
CREATE FUNCTION fvoci.app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_doc      documents%ROWTYPE;
    v_org      uuid;
    v_previous jsonb;
    v_uid      uuid;
    v_new      integer;
    v_at       integer := 0;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    v_org := app_target_organization('document', p_doc);

    DELETE FROM document_links WHERE document_id = p_doc;
    INSERT INTO document_links (document_id, target_kind, target_id)
    SELECT DISTINCT p_doc, r.kind, r.id FROM app_reference_nodes(v_doc.body) r
    WHERE r.kind IN ('item', 'document', 'task') AND NOT (r.kind = 'document' AND r.id = p_doc)
      AND app_access(r.kind, r.id) <> 'none'
      -- D-114: 같은 조직 안의 대상만. 개인 대상(조직 없음)은 볼 수 있으면 되고, 개인 노트에서는 쓴 사람이 속한 조직의
      -- 대상만 참조한다.
      AND (app_target_organization(r.kind, r.id) IS NULL
           OR app_target_organization(r.kind, r.id) = v_org
           OR (v_org IS NULL AND EXISTS (SELECT 1 FROM organization_members m
                                         WHERE m.organization_id = app_target_organization(r.kind, r.id)
                                           AND m.user_id = v_user)));

    IF NOT p_notify THEN
      RETURN;
    END IF;
    SELECT body INTO v_previous FROM document_revisions
    WHERE document_id = p_doc AND revision = v_doc.revision - 1;
    FOR v_uid IN
      SELECT r.id FROM app_reference_nodes(v_doc.body) r JOIN users u ON u.id = r.id
      WHERE r.kind = 'user' AND r.id <> v_user
        AND NOT EXISTS (SELECT 1 FROM app_reference_nodes(coalesce(v_previous, '{}'::jsonb)) p
                        WHERE p.kind = 'user' AND p.id = r.id)
    LOOP
      IF app_access_as('document', p_doc, v_uid) <> 'none' THEN
        INSERT INTO document_mentions (document_id, user_id, mentioned_by)
        VALUES (p_doc, v_uid, v_user)
        ON CONFLICT DO NOTHING;
        GET DIAGNOSTICS v_new = ROW_COUNT;
        IF v_new > 0 THEN
          v_at := v_at + 1;
          INSERT INTO notifications (id, user_id, organization_id, kind, document_id, actor)
          VALUES (app_id(p_ids, v_at), v_uid, v_org, 'mention', p_doc, v_user);
        END IF;
      END IF;
    END LOOP;
  END
  $$;


ALTER FUNCTION fvoci.app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 리비전 본문이 가리키는 첨부 버전을 고정한다(D-56). 같은 조직 밖의 첨부는 참조로 기록하지 않는다(D-114).
CREATE FUNCTION fvoci.app_record_file_refs() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE;
  BEGIN
    SELECT * INTO v_doc FROM documents WHERE id = NEW.document_id;
    -- 가리키는 버전 행을 참조를 넣기 전에 공유 잠금한다. 범위 밖 참조(다른 조직의 파일)는 기록하지 않는다(D-114).
    PERFORM 1 FROM file_versions v
    JOIN (SELECT DISTINCT r.file_id, r.version FROM app_file_refs(NEW.body) r) r
      ON r.file_id = v.file_id AND r.version = v.version
    JOIN files f ON f.id = v.file_id
    WHERE app_file_ref_in_scope(v_doc.workspace_id, v_doc.owner_user_id, f.workspace_id, f.owner_user_id)
    ORDER BY v.id
    FOR KEY SHARE OF v;
    INSERT INTO document_file_refs (document_id, revision, file_id, version)
    SELECT NEW.document_id, NEW.revision, r.file_id, r.version
    FROM app_file_refs(NEW.body) r JOIN files f ON f.id = r.file_id
    WHERE app_file_ref_in_scope(v_doc.workspace_id, v_doc.owner_user_id, f.workspace_id, f.owner_user_id)
    ON CONFLICT DO NOTHING;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_record_file_refs() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_ref_in_scope(p_doc_ws uuid, p_doc_owner uuid, p_file_ws uuid, p_file_owner uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 파일 참조의 유효 범위(D-114): 개인 파일은 같은 소유자의 문서만, 공간 파일은 같은 조직의 공간 문서 또는 그 조직 멤버가
    -- 소유한 개인 노트만. 참조 기록(app_record_file_refs)·버전 보존(app_version_referenced)·휴지통 보존
    -- (app_batch_keeps_referenced)이 같은 규칙을 쓴다.
    SELECT CASE
      WHEN p_file_owner IS NOT NULL THEN p_doc_owner IS NOT DISTINCT FROM p_file_owner
      WHEN p_doc_ws IS NOT NULL THEN
        (SELECT w.organization_id FROM workspaces w WHERE w.id = p_doc_ws)
          = (SELECT w.organization_id FROM workspaces w WHERE w.id = p_file_ws)
      WHEN p_doc_owner IS NOT NULL THEN EXISTS (
        SELECT 1 FROM organization_members m JOIN workspaces w ON w.organization_id = m.organization_id
        WHERE w.id = p_file_ws AND m.user_id = p_doc_owner)
      ELSE false
    END
  $$;


ALTER FUNCTION fvoci.app_file_ref_in_scope(p_doc_ws uuid, p_doc_owner uuid, p_file_ws uuid, p_file_owner uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_access(p_doc uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
  BEGIN
    IF app_user_id() IS NULL OR v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT app_document_realtime(p_doc) THEN
      RAISE EXCEPTION 'realtime editing is off for this document' USING ERRCODE = '55000';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_collab_access(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_load(p_doc uuid) RETURNS TABLE(doc_revision integer, title text, body jsonb, state bytea, editors uuid[])
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_collab_access(p_doc);
    RETURN QUERY
    SELECT d.revision, d.title, d.body, cs.state, cs.editors
    FROM documents d
    -- 상태가 이어받은 리비전 뒤로 본문이 바뀌지 않았으면(제목만 바꾼 저장 등) 그 상태와 편집자를 그대로 쓴다.
    LEFT JOIN LATERAL (
      SELECT s.state, s.editors FROM document_collab_states s
      WHERE s.document_id = d.id
        AND NOT EXISTS (
          SELECT 1 FROM document_revisions r
          WHERE r.document_id = d.id AND r.revision > s.revision
            AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                         WHERE b.document_id = d.id AND b.revision = s.revision))
    ) cs ON true
    WHERE d.id = p_doc;
  END
  $$;


ALTER FUNCTION fvoci.app_collab_load(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_pending(p_workspace uuid) RETURNS SETOF uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can change realtime editing' USING ERRCODE = '42501';
    END IF;
    RETURN QUERY
      SELECT s.document_id
      FROM document_collab_states s
      JOIN documents d ON d.id = s.document_id
      WHERE s.pending AND d.workspace_id = p_workspace AND d.deleted_at IS NULL
        AND NOT EXISTS (SELECT 1 FROM document_revisions r
                        WHERE r.document_id = s.document_id AND r.revision > s.revision
                          AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                       WHERE b.document_id = s.document_id
                                                         AND b.revision = s.revision))
      ORDER BY s.document_id;
  END
  $$;


ALTER FUNCTION fvoci.app_collab_pending(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_state_current(p_doc uuid, p_revision integer) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT NOT EXISTS (SELECT 1 FROM document_revisions r
                       WHERE r.document_id = p_doc AND r.revision > p_revision
                         AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                      WHERE b.document_id = p_doc AND b.revision = p_revision))
  $$;


ALTER FUNCTION fvoci.app_collab_state_current(p_doc uuid, p_revision integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_item     items%ROWTYPE;
    v_access   text;
    v_id       uuid;
    v_existing uuid;
    v_number   bigint;
  BEGIN
    v_access := app_access('item', p_item);
    IF v_access = 'none' THEN
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
    IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
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
    PERFORM app_activity(app_id(p_ids, 3), v_item.workspace_id, p_item, 'document', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 4));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user      uuid := app_user_id();
    v_id        uuid;
    v_placement text;
    v_number    bigint;
  BEGIN
    IF v_user IS NULL OR app_access('workspace', p_workspace) = 'none'
       OR app_access('workspace', p_workspace) NOT IN ('owner', 'write') THEN
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
    PERFORM app_activity(app_id(p_ids, 3), p_workspace, NULL, 'document', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 4));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_depth(p_doc uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH RECURSIVE up(id, parent_id, depth) AS (
      SELECT id, parent_id, 1 FROM documents WHERE id = p_doc
      UNION ALL
      SELECT d.id, d.parent_id, up.depth + 1 FROM documents d JOIN up ON d.id = up.parent_id
      WHERE up.depth < 1000
    )
    SELECT max(depth) FROM up
  $$;


ALTER FUNCTION fvoci.app_document_depth(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_height(p_doc uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH RECURSIVE down(id, height) AS (
      SELECT id, 1 FROM documents WHERE id = p_doc
      UNION ALL
      SELECT d.id, down.height + 1 FROM documents d JOIN down ON d.parent_id = down.id
      WHERE down.height < 1000
    )
    SELECT max(height) FROM down
  $$;


ALTER FUNCTION fvoci.app_document_height(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_realtime(p_doc uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT w.realtime_enabled FROM documents d JOIN workspaces w ON w.id = d.workspace_id
                     WHERE d.id = p_doc), false)
  $$;


ALTER FUNCTION fvoci.app_document_realtime(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_realtime_locked(p_doc uuid) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_enabled boolean;
  BEGIN
    IF app_access('document', p_doc) = 'none' THEN
      RETURN false;
    END IF;
    SELECT w.realtime_enabled INTO v_enabled
    FROM documents d JOIN workspaces w ON w.id = d.workspace_id
    WHERE d.id = p_doc
    FOR SHARE OF w;
    RETURN coalesce(v_enabled, false);
  END
  $$;


ALTER FUNCTION fvoci.app_document_realtime_locked(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_reference(p_doc uuid) RETURNS text
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_document_reference(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_refs(p_body jsonb) RETURNS TABLE(file_id uuid, version integer)
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $_$
    SELECT DISTINCT lower(replace(m[1], '-', ''))::uuid, (substring(m[2] FROM '(?:^|&)version=0*([0-9]{1,9})(?:&|$)'))::integer
    FROM regexp_matches(p_body::text, '/api/files/([0-9A-Fa-f-]{32,36})/content\?([^"]*)', 'g') AS m
    WHERE lower(replace(m[1], '-', '')) ~ '^[0-9a-f]{32}$'
      AND substring(m[2] FROM '(?:^|&)version=0*([0-9]{1,9})(?:&|$)') IS NOT NULL
  $_$;


ALTER FUNCTION fvoci.app_file_refs(p_body jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_move(p_doc uuid, p_target uuid, p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc  documents%ROWTYPE;
    v_from text;
    v_to   text;
  BEGIN
    IF app_access('document', p_doc) = 'none' THEN
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


ALTER FUNCTION fvoci.app_lock_move(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_tree(p_workspace uuid, p_item uuid, p_owner uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT pg_advisory_xact_lock(hashtextextended(
      'fvoci-tree:' || coalesce(p_workspace::text, '') || ':' || coalesce(p_item::text, '') || ':'
        || coalesce(p_owner::text, ''), 0))
  $$;


ALTER FUNCTION fvoci.app_lock_tree(p_workspace uuid, p_item uuid, p_owner uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[]) RETURNS TABLE(workspace_id uuid, reference text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    -- D-114: 옮긴 자리의 조직 밖 공유는 철회한다(개인 노트를 공간으로 옮길 때 다른 조직에서 준 공유).
    -- 활동은 공간 멤버가 읽으므로 받는 사람(다른 조직 사람)의 id는 남기지 않는다(SR1-7, D-49).
    FOR v_child IN
      SELECT s.id FROM document_shares s
      WHERE s.document_id = p_doc
        AND s.organization_id <> (SELECT w.organization_id FROM workspaces w WHERE w.id = p_target)
      ORDER BY s.id
    LOOP
      DELETE FROM document_shares WHERE id = v_child.id;
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), p_target, p_item, 'document', p_doc, 'unshare',
                           jsonb_build_object('reason', 'moved'));
    END LOOP;
    -- D-114: 이 문서의 참조 가운데 옮긴 자리의 조직 밖 대상은 지운다(개인 노트는 쓴 사람이 속한 다른 조직의 대상도 참조한다).
    -- 조직이 없는 개인 대상은 참조 기록 규칙대로 남긴다.
    DELETE FROM document_links l
    WHERE l.document_id = p_doc
      AND app_target_organization(l.target_kind, l.target_id)
          <> (SELECT w.organization_id FROM workspaces w WHERE w.id = p_target);
    v_at := v_at + 1;
    PERFORM app_audit_document(app_id(p_ids, v_at), p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    v_at := v_at + 1;
    PERFORM app_activity(app_id(p_ids, v_at), p_target, p_item, 'document', p_doc, 'move',
      jsonb_build_object('from_workspace', v_doc.workspace_id, 'from_item', v_doc.item_id,
                         'from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END,
                         'reference', v_ref));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;


ALTER FUNCTION fvoci.app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_move_document_ids(p_doc uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- move_out·move_in 감사 2 + 활동 1 + 원래 자리에 남기는 자식마다 1 + 철회할 수 있는 공유마다 1(상한).
    SELECT CASE WHEN app_access('document', p_doc) = 'none' THEN 0
                ELSE 3 + (SELECT count(*) FROM documents c WHERE c.parent_id = p_doc)::integer
                       + (SELECT count(*) FROM document_shares s WHERE s.document_id = p_doc)::integer END
  $$;


ALTER FUNCTION fvoci.app_move_document_ids(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
      IF app_access('document', p_parent) = 'none' THEN
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
      IF v_before.id IS NULL OR v_before.id = p_doc OR app_access('document', p_before) = 'none'
         -- 형제 판단은 호출한 사람이 보는 부모 기준(볼 수 없는 부모 아래 문서는 최상위로 보인다).
         OR (CASE WHEN v_before.parent_id IS NOT NULL AND app_access('document', v_before.parent_id) <> 'none'
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


ALTER FUNCTION fvoci.app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_record_document_references_ids(p_doc uuid, p_notify boolean) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN NOT p_notify OR app_access('document', p_doc) = 'none' THEN 0
                ELSE (SELECT count(*) FROM documents d, app_reference_nodes(d.body) r
                      WHERE d.id = p_doc AND r.kind = 'user')::integer END
  $$;


ALTER FUNCTION fvoci.app_record_document_references_ids(p_doc uuid, p_notify boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_reference_nodes(p_body jsonb) RETURNS TABLE(kind text, id uuid)
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $_$
    SELECT DISTINCT r.kind, r.raw_id::uuid
    FROM (
      SELECT n -> 'attrs' ->> 'kind' AS kind, n -> 'attrs' ->> 'id' AS raw_id
      FROM jsonb_path_query(p_body, 'lax $.**') AS n
      WHERE jsonb_typeof(n) = 'object' AND n ->> 'type' = 'reference'
    ) r
    WHERE r.kind IN ('item', 'document', 'task', 'user')
      AND r.raw_id ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  $_$;


ALTER FUNCTION fvoci.app_reference_nodes(p_body jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    -- 앱이 공급한 UUID로 제목 변경 감사를 리비전 기록 뒤에 남긴다(D-88).
    PERFORM app_audit_document_rename(app_id(p_ids, 1), p_doc, v_next);
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'restore_revision',
                               jsonb_build_object('from', p_revision, 'to', v_next));
    PERFORM app_record_document_references(p_doc, false, '{}'::uuid[]);
    RETURN 'restored';
  END
  $$;


ALTER FUNCTION fvoci.app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR NO KEY UPDATE;
    v_access := app_access('document', p_doc);
    IF v_access = 'none' THEN
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


ALTER FUNCTION fvoci.app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[]) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RAISE EXCEPTION 'system function' USING ERRCODE = '42501';
    END IF;
    -- 지운(휴지통) 문서는 저장만 하지 않고 참조는 그대로 둔다. 낡은 상태만 참조를 지운다.
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND deleted_at IS NULL) THEN
      RETURN false;
    END IF;
    IF NOT app_collab_state_current(p_doc, p_revision) THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      RETURN false;
    END IF;
    -- 편집자를 함께 적는다. 리비전으로 남은 상태에는 남길 편집이 없으므로 비운다.
    INSERT INTO document_collab_states (document_id, revision, state, pending, editors, updated_at)
    VALUES (p_doc, p_revision, p_state, p_pending,
            CASE WHEN p_pending THEN coalesce(p_editors, '{}') ELSE '{}' END, now())
    ON CONFLICT (document_id) DO UPDATE
      SET revision = excluded.revision, state = excluded.state, pending = excluded.pending,
          editors = excluded.editors, updated_at = now();
    IF NOT p_pending THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
    ELSIF p_body IS NOT NULL THEN
      PERFORM 1 FROM file_versions v
      JOIN (SELECT DISTINCT r.file_id, r.version FROM app_file_refs(p_body) r) r
        ON r.file_id = v.file_id AND r.version = v.version
      ORDER BY v.id
      FOR KEY SHARE OF v;
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      INSERT INTO document_collab_file_refs (document_id, file_id, version)
      SELECT DISTINCT p_doc, r.file_id, r.version FROM app_file_refs(p_body) r
      ON CONFLICT DO NOTHING;
    END IF;
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_tree_siblings(p_workspace uuid, p_item uuid, p_owner uuid, p_parent uuid) RETURNS SETOF fvoci.documents
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_tree_siblings(p_workspace uuid, p_item uuid, p_owner uuid, p_parent uuid) OWNER TO fvoci_migrator;

-- 호출자 권한으로 문서를 읽는다. 소유자 권한으로 RLS를 건너뛰면 다른 공간 문서의 존재가 오류로 드러날 수 있다.
CREATE FUNCTION fvoci.document_revisions_match() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF NEW.revision <> (SELECT revision FROM documents WHERE id = NEW.document_id) THEN
      RAISE EXCEPTION 'revision row must match the document revision' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.document_revisions_match() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.documents_initial_position() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    SELECT coalesce(max(s.position), 0) + 1 INTO NEW.position
    FROM app_tree_siblings(NEW.workspace_id, NEW.item_id, NEW.owner_user_id, NEW.parent_id) s;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.documents_initial_position() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.documents_revision_step() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.documents_revision_step() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.documents_tree_check() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.documents_tree_check() OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.document_collab_file_refs
    ADD CONSTRAINT document_collab_file_refs_pkey PRIMARY KEY (document_id, file_id, version);

ALTER TABLE ONLY fvoci.document_collab_states
    ADD CONSTRAINT document_collab_states_pkey PRIMARY KEY (document_id);

ALTER TABLE ONLY fvoci.document_file_refs
    ADD CONSTRAINT document_file_refs_pkey PRIMARY KEY (document_id, revision, file_id, version);

ALTER TABLE ONLY fvoci.document_links
    ADD CONSTRAINT document_links_pkey PRIMARY KEY (document_id, target_kind, target_id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_pkey PRIMARY KEY (document_id, user_id);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_pkey PRIMARY KEY (document_id, revision);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_request UNIQUE (document_id, author, request_id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_target UNIQUE (document_id, grantee_user_id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_number UNIQUE (workspace_id, scope, number);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_request UNIQUE (workspace_id, created_by, request_id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_token_hash_key UNIQUE (token_hash);

CREATE INDEX document_collab_file_refs_version ON fvoci.document_collab_file_refs USING btree (file_id, version);

CREATE INDEX document_file_refs_file ON fvoci.document_file_refs USING btree (file_id, version);

CREATE INDEX document_links_target ON fvoci.document_links USING btree (target_kind, target_id);

CREATE INDEX document_shares_grantee ON fvoci.document_shares USING btree (grantee_user_id);

CREATE INDEX document_shares_organization ON fvoci.document_shares USING btree (organization_id);

CREATE INDEX documents_item ON fvoci.documents USING btree (item_id);

CREATE INDEX documents_item_roots ON fvoci.documents USING btree (item_id, "position") WHERE (parent_id IS NULL);

CREATE INDEX documents_note_roots ON fvoci.documents USING btree (owner_user_id, "position") WHERE (parent_id IS NULL);

CREATE INDEX documents_owner_updated ON fvoci.documents USING btree (owner_user_id, updated_at DESC) WHERE (owner_user_id IS NOT NULL);

CREATE INDEX documents_parent ON fvoci.documents USING btree (parent_id, "position");

CREATE UNIQUE INDEX documents_personal_request ON fvoci.documents USING btree (owner_user_id, request_id) WHERE (owner_user_id IS NOT NULL);

CREATE INDEX documents_templates ON fvoci.documents USING btree (workspace_id) WHERE (is_template AND (deleted_at IS NULL));

CREATE INDEX documents_trash ON fvoci.documents USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX documents_updated_by ON fvoci.documents USING btree (updated_by, updated_at DESC);

CREATE INDEX documents_wiki_roots ON fvoci.documents USING btree (workspace_id, "position") WHERE ((item_id IS NULL) AND (owner_user_id IS NULL) AND (parent_id IS NULL));

CREATE INDEX documents_workspace_updated ON fvoci.documents USING btree (workspace_id, updated_at DESC);

CREATE INDEX public_links_document ON fvoci.public_links USING btree (document_id);

CREATE TRIGGER document_revisions_file_refs AFTER INSERT ON fvoci.document_revisions FOR EACH ROW EXECUTE FUNCTION fvoci.app_record_file_refs();

CREATE TRIGGER document_revisions_match BEFORE INSERT ON fvoci.document_revisions FOR EACH ROW EXECUTE FUNCTION fvoci.document_revisions_match();

CREATE TRIGGER documents_files BEFORE DELETE ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.app_delete_attached_files('document');

CREATE TRIGGER documents_initial_position BEFORE INSERT ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.documents_initial_position();

CREATE TRIGGER documents_revision_step BEFORE UPDATE ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.documents_revision_step();

CREATE TRIGGER documents_search AFTER INSERT OR DELETE OR UPDATE OF title, body, deleted_at, visibility, item_id, workspace_id, owner_user_id ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('document');

CREATE TRIGGER documents_search_files AFTER UPDATE OF deleted_at ON fvoci.documents FOR EACH ROW WHEN ((old.deleted_at IS DISTINCT FROM new.deleted_at)) EXECUTE FUNCTION fvoci.app_search_enqueue_document_files();

CREATE CONSTRAINT TRIGGER documents_tree_check AFTER INSERT OR UPDATE OF parent_id, workspace_id, item_id, owner_user_id ON fvoci.documents NOT DEFERRABLE INITIALLY IMMEDIATE FOR EACH ROW EXECUTE FUNCTION fvoci.documents_tree_check();

ALTER TABLE ONLY fvoci.document_collab_file_refs
    ADD CONSTRAINT document_collab_file_refs_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.document_collab_states(document_id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.document_collab_states
    ADD CONSTRAINT document_collab_states_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.document_file_refs
    ADD CONSTRAINT document_file_refs_document_id_revision_fkey FOREIGN KEY (document_id, revision) REFERENCES fvoci.document_revisions(document_id, revision) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.document_links
    ADD CONSTRAINT document_links_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_mentioned_by_fkey FOREIGN KEY (mentioned_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_author_fkey FOREIGN KEY (author) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_granted_by_fkey FOREIGN KEY (granted_by) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_grantee_user_id_fkey FOREIGN KEY (grantee_user_id) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
