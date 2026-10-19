-- 전체 점검(2026-10-19)에서 확인한 데이터 경계 결함 세 가지. 함수 본문은 마지막 정의와 같고 표시한 곳만 다르다.
--
-- 1. 모듈 휴지통(0053)은 항목의 `deleted_at`을 건드리지 않고 `app_item_access`로만 숨긴다. 그런데 문서 공유
--    받은 사람·공개 링크는 `app_item_access`를 거치지 않아, 휴지통에 넣은 모듈의 문서를 계속 읽고 고칠 수 있었다
--    (`app_document_base_access`, `app_public_document`, 마지막 정의 0025). 항목 휴지통과 같게 막는다.
-- 2. 문서를 다른 워크스페이스·개인 노트↔워크스페이스로 옮겨도(0010) 첨부 파일(0027보다 먼저 만든 함수라 빠짐)은
--    옛 자리에 남아, 옛 워크스페이스 관리자가 그 파일의 휴지통 묶음을 보고 되살리거나 영구 삭제하고 감사 기록을
--    읽었다. 첨부·열린 업로드·열린 파일 휴지통 묶음을 문서와 함께 옮기고, 파일 검색 색인도 다시 만든다.
-- 3. 기록을 고치면 연결 파일을 모두 지우고 다시 넣어(0036), 파일이 영구 삭제되어 이름·버전·SHA-256만 남은
--    연결(D-35 "당시 버전 유지")이 사라지고, 휴지통에 있는(되살릴 수 있는) 파일을 연결한 기록은 고칠 수 없었다.
--    남은 연결은 그대로 두고, 이미 연결된 파일·버전을 다시 보내면 그 연결을 그대로 쓴다.

-- 1 ------------------------------------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_public_document(p_token_hash bytea)
  RETURNS TABLE (title text, body jsonb, schema_version integer, updated_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT d.title, d.body, d.schema_version, d.updated_at
    FROM public_links l JOIN documents d ON d.id = l.document_id
    WHERE l.token_hash = p_token_hash
      AND l.revoked_at IS NULL
      AND (l.expires_at IS NULL OR l.expires_at > now())
      AND d.deleted_at IS NULL
      -- 바뀐 곳: 항목이나 그 모듈이 휴지통에 있으면 열리지 않는다.
      AND NOT EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                      WHERE i.id = d.item_id AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL))
      AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = d.workspace_id AND w.deleted_at IS NOT NULL)
  $$;

CREATE OR REPLACE FUNCTION app_document_base_access(p_doc uuid) RETURNS text
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
    IF v_doc.id IS NULL OR v_doc.deleted_at IS NOT NULL THEN
      RETURN NULL;
    END IF;
    -- 워크스페이스를 지웠으면 그 안의 문서는 공유로도 볼 수 없다.
    IF v_doc.workspace_id IS NOT NULL AND EXISTS (SELECT 1 FROM workspaces WHERE id = v_doc.workspace_id AND deleted_at IS NOT NULL) THEN
      RETURN NULL;
    END IF;
    -- 항목이나 그 모듈이 휴지통에 있으면 그 안의 문서는 공유로도 볼 수 없다(바뀐 곳: 모듈).
    IF v_doc.item_id IS NOT NULL AND EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
      WHERE i.id = v_doc.item_id AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL)
    ) THEN
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

-- 2 ------------------------------------------------------------------------------------------------------------

-- 자리가 바뀐 파일도 검색 색인을 다시 만든다(색인의 범위가 워크스페이스·소유자다).
DROP TRIGGER files_search ON files;
CREATE TRIGGER files_search AFTER INSERT OR DELETE OR UPDATE OF name, deleted_at, current_version, workspace_id, owner_user_id
  ON files FOR EACH ROW EXECUTE FUNCTION app_search_enqueue('file');

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
    -- 바뀐 곳: 첨부(휴지통에 있는 것 포함)·그 휴지통 묶음·열린 업로드도 새 자리로. 권한·휴지통·감사·저장 용량·
    -- 내보내기가 모두 이 자리를 본다.
    UPDATE files f SET workspace_id = p_target, owner_user_id = NULL WHERE f.document_id = p_doc;
    UPDATE trash_batches b SET workspace_id = p_target, owner_user_id = NULL
    WHERE b.root_kind = 'file' AND b.restored_at IS NULL AND b.purged_at IS NULL
      AND b.root_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc);
    UPDATE uploads u SET workspace_id = p_target, owner_user_id = NULL
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc));
    PERFORM app_audit_document(p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;

-- 3 ------------------------------------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row  jsonb;
    v_pos  integer := 0;
    v_file files%ROWTYPE;
    v_ver  file_versions%ROWTYPE;
    v_had  record_files%ROWTYPE;
    v_before record_files[];
    v_used integer[] := '{}';
  BEGIN
    IF jsonb_typeof(coalesce(p_files, '[]'::jsonb)) <> 'array' OR jsonb_array_length(coalesce(p_files, '[]')) > 50 THEN
      RAISE EXCEPTION 'give at most 50 linked files' USING ERRCODE = '22023';
    END IF;
    -- 바뀐 곳: 지난 연결을 기억해 두고, 다시 보낸 것은 그대로 쓰고, 파일이 영구 삭제된 연결은 끝에 그대로 둔다.
    SELECT coalesce(array_agg(rf ORDER BY rf.position), '{}') INTO v_before
    FROM record_files rf WHERE rf.record_id = p_record;
    DELETE FROM record_files WHERE record_id = p_record;
    FOR v_row IN SELECT * FROM jsonb_array_elements(coalesce(p_files, '[]'::jsonb)) LOOP
      v_had := NULL;
      SELECT * INTO v_had FROM unnest(v_before) b
      WHERE b.file_id = (v_row->>'file_id')::uuid AND b.version = (v_row->>'version')::integer
        AND b.position <> ALL (v_used)
      ORDER BY b.position LIMIT 1;
      IF v_had.record_id IS NOT NULL THEN
        -- 이미 연결된 파일·버전: 파일이 휴지통에 있어도 연결을 그대로 쓴다(되살릴 수 있고, 연결이 지킨다, D-56).
        INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
        VALUES (p_record, v_pos, coalesce(v_row->>'role', v_had.role), v_had.file_id, v_had.version_id,
                v_had.file_name, v_had.version, v_had.sha256, v_had.size);
        v_used := v_used || v_had.position;
        v_pos := v_pos + 1;
        CONTINUE;
      END IF;
      SELECT * INTO v_file FROM files
      WHERE id = (v_row->>'file_id')::uuid AND item_id = p_item AND deleted_at IS NULL FOR KEY SHARE;
      IF v_file.id IS NULL THEN
        RAISE EXCEPTION 'linked files must be attachments of this item' USING ERRCODE = '22023';
      END IF;
      SELECT * INTO v_ver FROM file_versions
      WHERE file_id = v_file.id AND version = coalesce((v_row->>'version')::integer, v_file.current_version)
      FOR KEY SHARE;
      IF v_ver.id IS NULL THEN
        RAISE EXCEPTION 'file version not found' USING ERRCODE = '22023';
      END IF;
      INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
      VALUES (p_record, v_pos, coalesce(v_row->>'role', 'other'), v_file.id, v_ver.id, v_file.name, v_ver.version,
              v_ver.sha256, v_ver.size);
      v_pos := v_pos + 1;
    END LOOP;
    -- 파일이 영구 삭제되어 이름·버전·SHA-256만 남은 연결은 화면이 다시 보낼 수 없으므로 그대로 둔다(D-35).
    INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
    SELECT p_record, v_pos + row_number() OVER (ORDER BY b.position) - 1, b.role, NULL, NULL,
           b.file_name, b.version, b.sha256, b.size
    FROM unnest(v_before) b WHERE b.file_id IS NULL;
  EXCEPTION
    WHEN invalid_text_representation OR check_violation OR numeric_value_out_of_range THEN
      RAISE EXCEPTION 'invalid linked file: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;
