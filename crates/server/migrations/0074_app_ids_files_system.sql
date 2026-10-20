-- 앱이 만든 id로 행을 만든다(D-88, 0070 머리말). 이 파일: 업로드·파일·파일 버전, 휴지통(되살리기·영구 삭제·
-- 비우기), 작업 프로세스의 정리·알림. 함수 본문은 마지막 정의와 같고 id 부분만 다르다.

------------------------------------------------------------------------------------------------------------------
-- 업로드·파일
------------------------------------------------------------------------------------------------------------------

-- id 1개(새 업로드일 때). 저장 키에도 이 id를 쓴다.
CREATE FUNCTION app_create_upload(p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text,
                                  p_type text, p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint,
                                  p_max_storage bigint, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_id    uuid;
    v_file  files%ROWTYPE;
    v_place record;
    v_kind  text := p_kind;
    v_tgt   uuid := p_target;
    v_name  text := btrim(coalesce(p_name, ''));
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'not signed in' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM uploads WHERE user_id = v_user AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_size < 1 THEN
      RAISE EXCEPTION 'empty files cannot be uploaded' USING ERRCODE = '22023';
    END IF;
    IF p_size > p_max_file THEN
      RAISE EXCEPTION 'the file is larger than the limit (% bytes)', p_max_file USING ERRCODE = '22023';
    END IF;
    IF p_file IS NOT NULL THEN
      SELECT * INTO v_file FROM files WHERE id = p_file;
      IF app_file_access(p_file) IS NULL THEN
        RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
      END IF;
      v_kind := CASE WHEN v_file.document_id IS NOT NULL THEN 'document' ELSE 'item' END;
      v_tgt := coalesce(v_file.document_id, v_file.item_id);
      IF v_name = '' THEN
        v_name := v_file.name;
      END IF;
    END IF;
    SELECT * INTO v_place FROM app_file_place(v_kind, v_tgt);
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023';
    END IF;
    -- 같은 영역의 업로드 시작을 줄 세워 저장량 확인이 겹치지 않게 한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage:' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text, 0));
    IF app_storage_used(v_place.workspace_id, v_place.owner_user_id, NULL) + p_size > p_max_storage THEN
      RAISE EXCEPTION 'not enough storage left (limit % bytes)', p_max_storage USING ERRCODE = '55000';
    END IF;
    v_id := app_id(p_ids, 1);
    INSERT INTO uploads (id, user_id, request_id, file_id, target_kind, target_id, workspace_id, owner_user_id,
                         name, note, content_type, size, chunk_bytes, storage_key, expires_at)
    VALUES (v_id, v_user, p_request, p_file,
            CASE WHEN p_file IS NULL THEN v_kind END, CASE WHEN p_file IS NULL THEN v_tgt END,
            v_place.workspace_id, v_place.owner_user_id, v_name, coalesce(p_note, ''),
            coalesce(nullif(btrim(p_type), ''), 'application/octet-stream'), p_size, p_chunk,
            'v1/' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text || '/' || v_id::text,
            now() + make_interval(secs => p_ttl_secs))
    ON CONFLICT (user_id, request_id) DO NOTHING;
    -- 같은 요청이 동시에 오면 먼저 들어간 쪽을 돌려준다.
    SELECT id INTO v_id FROM uploads WHERE user_id = v_user AND request_id = p_request;
    RETURN v_id;
  END
  $$;

-- id 3개: 파일(새 파일일 때), 파일 버전, 감사 기록.
CREATE FUNCTION app_finish_upload(p_upload uuid, p_sha256 text, p_max_storage bigint, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_up      uploads%ROWTYPE;
    v_file    uuid;
    v_version integer;
    v_vid     uuid;
    v_place   record;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_up.state = 'done' THEN
      RETURN v_up.version_id;
    END IF;
    IF v_up.state <> 'completing' THEN
      RAISE EXCEPTION 'the upload is no longer open' USING ERRCODE = '55000';
    END IF;
    IF v_up.file_id IS NOT NULL THEN
      IF app_file_access(v_up.file_id) IS NULL THEN
        RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
      END IF;
      SELECT * INTO v_place FROM app_file_place(
        (SELECT CASE WHEN document_id IS NOT NULL THEN 'document' ELSE 'item' END FROM files WHERE id = v_up.file_id),
        (SELECT coalesce(document_id, item_id) FROM files WHERE id = v_up.file_id));
    ELSE
      SELECT * INTO v_place FROM app_file_place(v_up.target_kind, v_up.target_id);
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage:' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text, 0));
    IF app_storage_used(v_place.workspace_id, v_place.owner_user_id, p_upload) + v_up.size > p_max_storage THEN
      RAISE EXCEPTION 'not enough storage left (limit % bytes)', p_max_storage USING ERRCODE = '55000';
    END IF;
    IF v_up.file_id IS NULL THEN
      INSERT INTO files (id, workspace_id, owner_user_id, document_id, item_id, name, created_by)
      VALUES (app_id(p_ids, 1), v_place.workspace_id, v_place.owner_user_id,
              CASE WHEN v_up.target_kind = 'document' THEN v_up.target_id END,
              CASE WHEN v_up.target_kind = 'item' THEN v_up.target_id END,
              v_up.name, v_up.user_id)
      RETURNING id INTO v_file;
    ELSE
      v_file := v_up.file_id;
    END IF;
    UPDATE files SET current_version = current_version + 1, updated_at = now()
    WHERE id = v_file RETURNING current_version INTO v_version;
    INSERT INTO file_versions (id, file_id, version, size, sha256, content_type, storage_key, note, uploaded_by)
    VALUES (app_id(p_ids, 2), v_file, v_version, v_up.size, p_sha256, v_up.content_type, v_up.storage_key, v_up.note,
            v_up.user_id)
    RETURNING id INTO v_vid;
    UPDATE uploads SET state = 'done', state_changed_at = now(), version_id = v_vid WHERE id = p_upload;
    DELETE FROM upload_parts WHERE upload_id = p_upload;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 3), v_place.workspace_id, v_place.owner_user_id, v_up.user_id, 'upload', 'file', v_file,
            jsonb_build_object('name', v_up.name, 'version', v_version, 'size', v_up.size));
    RETURN v_vid;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_rename_file(p_file uuid, p_name text, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_file_access(p_file);
    v_file   files%ROWTYPE;
    v_name   text := btrim(coalesce(p_name, ''));
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF app_file_archived(p_file) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    UPDATE files SET name = v_name, updated_at = now() WHERE id = p_file;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'rename', 'file', p_file,
            jsonb_build_object('from', v_file.name, 'to', v_name));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_delete_file_version(p_file uuid, p_version integer, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_file_access(p_file);
    v_file   files%ROWTYPE;
    v_id     uuid;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF app_file_archived(p_file) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_version = v_file.current_version THEN
      RAISE EXCEPTION 'the current version cannot be deleted; upload a new one or delete the file'
        USING ERRCODE = '55000';
    END IF;
    SELECT id INTO v_id FROM file_versions WHERE file_id = p_file AND version = p_version;
    IF v_id IS NULL THEN
      RAISE EXCEPTION 'version not found' USING ERRCODE = 'P0002';
    END IF;
    UPDATE uploads SET version_id = NULL WHERE version_id = v_id;
    DELETE FROM file_versions WHERE id = v_id;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete_version', 'file',
            p_file, jsonb_build_object('name', v_file.name, 'version', p_version));
  END
  $$;

-- id 2개: 휴지통 묶음, 감사 기록.
CREATE FUNCTION app_trash_file(p_file uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_file_access(p_file);
    v_file   files%ROWTYPE;
    v_batch  uuid;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF app_file_archived(p_file) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF v_file.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_file.workspace_id, v_file.owner_user_id, 'file', p_file, v_file.name, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE files SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_file;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete', 'file', p_file,
            jsonb_build_object('name', v_file.name, 'batch', v_batch));
    RETURN v_batch;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 휴지통
------------------------------------------------------------------------------------------------------------------

-- id 1개.
CREATE FUNCTION app_restore_batch(p_batch uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_doc   documents%ROWTYPE;
    v_item  uuid;
    v_ids   uuid[];
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_trash_expires_at(v_batch) <= now() AND NOT app_batch_keeps_referenced(v_batch) THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
    END IF;
    IF v_batch.root_kind = 'module' THEN
      -- 모듈 관리는 관리자 일이다. 지운 뒤 관리자가 아니게 된 사람은 되살리지 못한다.
      IF app_trash_role(v_batch) <> 'admin' THEN
        RAISE EXCEPTION 'only an admin can restore a module' USING ERRCODE = '42501';
      END IF;
      PERFORM 1 FROM modules WHERE id = v_batch.root_id FOR UPDATE;
      UPDATE modules SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'module', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
      UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
      RETURN;
    END IF;
    -- 지우기와 같은 잠금 순서(문서: 트리 → 문서 행 → 항목, 업무: 열 → 업무 행 → 항목).
    IF v_batch.root_kind = 'document' THEN
      SELECT * INTO v_doc FROM documents WHERE id = v_batch.root_id;
      PERFORM app_lock_tree(CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END,
                            v_doc.item_id, v_doc.owner_user_id);
      PERFORM 1 FROM documents WHERE trash_batch_id = p_batch ORDER BY id FOR NO KEY UPDATE;
      v_item := v_doc.item_id;
    ELSIF v_batch.root_kind = 'task' THEN
      v_ids := ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch);
      PERFORM app_lock_task_rows(v_ids, NULL);
      SELECT item_id INTO v_item FROM tasks WHERE id = v_batch.root_id;
    ELSIF v_batch.root_kind = 'event' THEN
      SELECT item_id INTO v_item FROM events WHERE id = v_batch.root_id FOR NO KEY UPDATE;
    ELSIF v_batch.root_kind = 'file' THEN
      -- 항목 문서에 붙은 파일은 그 항목의 권한·보관을 본다.
      SELECT coalesce(f.item_id, d.item_id) INTO v_item
      FROM files f LEFT JOIN documents d ON d.id = f.document_id
      WHERE f.id = v_batch.root_id FOR NO KEY UPDATE OF f;
      -- 붙은 문서가 휴지통에 있으면 먼저 문서를 되살려야 한다.
      IF EXISTS (SELECT 1 FROM files f JOIN documents d ON d.id = f.document_id
                 WHERE f.id = v_batch.root_id AND d.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the document first' USING ERRCODE = '55000';
      END IF;
    ELSE
      PERFORM 1 FROM items WHERE id = v_batch.root_id FOR NO KEY UPDATE;
      IF EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                 WHERE i.id = v_batch.root_id AND m.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the module first' USING ERRCODE = '55000';
      END IF;
    END IF;
    IF v_item IS NOT NULL THEN
      PERFORM app_lock_item(v_item);
      IF EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                 WHERE i.id = v_item AND m.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the module first' USING ERRCODE = '55000';
      END IF;
      -- 항목이 휴지통에 있으면 먼저 항목을 되살려야 한다. 보관된 항목에는 되살리지 않는다(지우기와 같은 규칙).
      IF EXISTS (SELECT 1 FROM items WHERE id = v_item AND deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the item first' USING ERRCODE = '55000';
      END IF;
      IF app_item_archived(v_item) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      -- 지운 사람은 지금도 그 항목에 쓸 수 있어야 한다(볼 수 없게 됐으면 없는 것과 같다).
      IF app_trash_role(v_batch) = 'deleter' AND app_item_access(v_item) IS DISTINCT FROM 'owner'
         AND app_item_access(v_item) IS DISTINCT FROM 'write' THEN
        RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
      END IF;
    END IF;
    IF v_batch.root_kind = 'document' THEN
      UPDATE documents SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE documents d SET parent_id = NULL
      WHERE d.id = v_batch.root_id AND d.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM documents p WHERE p.id = d.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit_document(app_id(p_ids, 1), v_batch.root_id, 'restore', jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      UPDATE tasks SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE tasks t SET parent_id = NULL
      WHERE t.id = v_batch.root_id AND t.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM tasks p WHERE p.id = t.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'task', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'item' THEN
      UPDATE items SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'item', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'file' THEN
      UPDATE files SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_batch.workspace_id, v_batch.owner_user_id, app_user_id(), 'restore', 'file',
              v_batch.root_id, jsonb_build_object('batch', p_batch));
    ELSE
      UPDATE events SET deleted_at = NULL, deleted_by = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'event', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    END IF;
    UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_purge_batch_as(p_batch uuid, p_system boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch  trash_batches%ROWTYPE;
    v_item   uuid;
    v_total  bigint;
    v_locked bigint;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR v_batch.restored_at IS NOT NULL OR v_batch.purged_at IS NOT NULL
       OR (NOT p_system AND app_trash_role(v_batch) IS NULL) THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system AND app_trash_role(v_batch) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can delete permanently' USING ERRCODE = '42501';
    END IF;
    -- 작업 프로세스는 만료된 묶음만 지운다.
    IF p_system AND app_trash_expires_at(v_batch) > now() THEN
      RETURN;
    END IF;
    -- 자동 정리는 살아 있는 기록·문서가 참조하는 버전을 지우지 않는다(D-56). 잠금 없이 먼저 보고, 지울 때만 잠근 뒤 다시 본다.
    IF p_system THEN
      IF app_batch_keeps_referenced(v_batch) THEN
        RETURN;
      END IF;
      SELECT count(*) INTO v_total FROM file_versions
      WHERE file_id IN (SELECT file_id FROM app_batch_files(v_batch));
      SELECT count(*) INTO v_locked FROM (
        SELECT 1 FROM file_versions
        WHERE file_id IN (SELECT file_id FROM app_batch_files(v_batch))
        ORDER BY id
        FOR UPDATE SKIP LOCKED) l;
      IF v_locked < v_total OR app_batch_keeps_referenced(v_batch) THEN
        RETURN;
      END IF;
    END IF;
    IF v_batch.root_kind = 'document' THEN
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'event' THEN
      DELETE FROM events WHERE trash_batch_id = p_batch;
    ELSIF v_batch.root_kind = 'file' THEN
      PERFORM app_delete_file_rows(ARRAY(SELECT id FROM files WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'module' THEN
      -- 모듈의 모든 항목(따로 휴지통에 있던 것 포함)을 지운 뒤 모듈을 지운다. 그 항목들의 묶음도 끝난다.
      FOR v_item IN SELECT id FROM items WHERE module_id = v_batch.root_id ORDER BY id LOOP
        PERFORM app_purge_item_rows(v_item);
      END LOOP;
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND root_kind = 'item' AND NOT EXISTS (SELECT 1 FROM items WHERE id = trash_batches.root_id);
      DELETE FROM numbered_targets WHERE scope_id = v_batch.root_id;
      -- 키 행을 지우지 않고 예약으로 남긴다(주인을 비우고 과거 키로).
      UPDATE module_keys SET module_id = NULL, retired_at = coalesce(retired_at, now())
      WHERE module_id = v_batch.root_id;
      DELETE FROM modules WHERE id = v_batch.root_id;
    ELSE
      PERFORM app_purge_item_rows(v_batch.root_id);
    END IF;
    IF v_batch.root_kind IN ('item', 'module') THEN
      -- 지운 항목 안의 것이 다른 휴지통 묶음에 있었으면 그 묶음도 함께 끝난다.
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND NOT EXISTS (SELECT 1 FROM documents WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM tasks WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM events WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM files WHERE trash_batch_id = trash_batches.id)
        AND root_kind NOT IN ('item', 'module');
    END IF;
    UPDATE trash_batches SET purged_at = now() WHERE id = p_batch;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_batch.workspace_id, v_batch.owner_user_id,
            CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
  END
  $$;

-- id 1개.
CREATE FUNCTION app_purge_batch(p_batch uuid, p_ids uuid[]) RETURNS void
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT app_purge_batch_as(p_batch, false, p_ids) $$;

-- id: 비울 묶음마다 1개(개수는 `_ids`).
CREATE FUNCTION app_purge_expired(p_workspace uuid, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
  BEGIN
    IF p_workspace IS NOT NULL AND app_member_role(p_workspace) IS DISTINCT FROM 'admin' THEN
      RAISE EXCEPTION 'only an admin can empty the trash' USING ERRCODE = '42501';
    END IF;
    FOR v_id IN
      SELECT b.id FROM trash_batches b
      WHERE b.restored_at IS NULL AND b.purged_at IS NULL
        AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
        AND app_trash_expires_at(b) <= now()
      ORDER BY b.deleted_at
    LOOP
      v_made := v_made + 1;
      PERFORM app_purge_batch(v_id, ARRAY[app_id(p_ids, v_made)]);
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE FUNCTION app_purge_expired_ids(p_workspace uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN p_workspace IS NOT NULL AND app_member_role(p_workspace) IS DISTINCT FROM 'admin' THEN 0 ELSE
      (SELECT count(*) FROM trash_batches b
       WHERE b.restored_at IS NULL AND b.purged_at IS NULL
         AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
         AND app_trash_expires_at(b) <= now())::integer
    END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 작업 프로세스
------------------------------------------------------------------------------------------------------------------

-- id: 지울 워크스페이스마다 1개, 그다음 만료된 묶음마다 1개(개수는 `_ids`).
CREATE FUNCTION app_system_purge_expired(p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_ws    uuid;
    v_made  integer := 0;
    v_at    integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_ws IN
      SELECT id FROM workspaces
      WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now()
      ORDER BY deleted_at
    LOOP
      v_at := v_at + 1;
      BEGIN
        IF app_purge_workspace_as(v_ws, true, ARRAY[app_id(p_ids, v_at)]) THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of deleted workspace % failed with SQLSTATE %', v_ws, SQLSTATE;
      END;
    END LOOP;
    FOR v_batch IN
      SELECT * FROM trash_batches WHERE restored_at IS NULL AND purged_at IS NULL ORDER BY deleted_at
    LOOP
      CONTINUE WHEN app_trash_expires_at(v_batch) > now();
      v_at := v_at + 1;
      BEGIN
        PERFORM app_purge_batch_as(v_batch.id, true, ARRAY[app_id(p_ids, v_at)]);
        IF (SELECT purged_at FROM trash_batches WHERE id = v_batch.id) IS NOT NULL THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of trash batch % failed with SQLSTATE %', v_batch.id, SQLSTATE;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE FUNCTION app_system_purge_expired_ids() RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      (SELECT count(*) FROM workspaces
       WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now())::integer
      + (SELECT count(*) FROM trash_batches b
         WHERE b.restored_at IS NULL AND b.purged_at IS NULL AND app_trash_expires_at(b) <= now())::integer
    END
  $$;

-- id: 알릴 업무·사람 쌍마다 1개(개수는 `_ids`).
CREATE FUNCTION app_system_due_notifications(p_today date, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task   tasks%ROWTYPE;
    v_uid    uuid;
    v_count  integer := 0;
    v_before integer;
    v_at     integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_task IN
      SELECT t.* FROM tasks t JOIN items i ON i.id = t.item_id JOIN workspaces w ON w.id = t.workspace_id
      WHERE t.deleted_at IS NULL AND i.deleted_at IS NULL AND i.archived_at IS NULL AND w.deleted_at IS NULL
        AND t.status <> 'done' AND t.due_date IN (p_today, p_today + 1)
    LOOP
      FOR v_uid IN
        SELECT user_id FROM task_assignees WHERE task_id = v_task.id
        UNION SELECT v_task.created_by WHERE NOT EXISTS (SELECT 1 FROM task_assignees WHERE task_id = v_task.id)
      LOOP
        SELECT count(*) INTO v_before FROM notifications WHERE user_id = v_uid AND task_id = v_task.id AND kind = 'due';
        v_at := v_at + 1;
        PERFORM app_notify(app_id(p_ids, v_at), v_uid, 'due', NULL, v_task.id, NULL, NULL,
                           jsonb_build_object('date', p_today, 'due_date', v_task.due_date,
                                              'title', v_task.title));
        IF (SELECT count(*) FROM notifications WHERE user_id = v_uid AND task_id = v_task.id AND kind = 'due') > v_before THEN
          v_count := v_count + 1;
        END IF;
      END LOOP;
    END LOOP;
    RETURN v_count;
  END
  $$;

CREATE FUNCTION app_system_due_notifications_ids(p_today date) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      (SELECT count(*)
       FROM tasks t JOIN items i ON i.id = t.item_id JOIN workspaces w ON w.id = t.workspace_id
       CROSS JOIN LATERAL (
         SELECT user_id FROM task_assignees WHERE task_id = t.id
         UNION SELECT t.created_by WHERE NOT EXISTS (SELECT 1 FROM task_assignees WHERE task_id = t.id)) r
       WHERE t.deleted_at IS NULL AND i.deleted_at IS NULL AND i.archived_at IS NULL AND w.deleted_at IS NULL
         AND t.status <> 'done' AND t.due_date IN (p_today, p_today + 1))::integer
    END
  $$;

-- id: 알릴 항목·사람 쌍마다 1개(개수는 `_ids`).
CREATE FUNCTION app_system_item_reminders(p_today date, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row   record;
    v_uid   uuid;
    v_count integer := 0;
    v_at    integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    -- 강좌 수강권 만료(7일 전·당일)와 목표 완료일(하루 전·당일).
    FOR v_row IN
      SELECT c.item_id, i.title, i.created_by, x.key, x.on_date
      FROM course_details c
      JOIN items i ON i.id = c.item_id
      JOIN modules m ON m.id = i.module_id
      JOIN workspaces w ON w.id = i.workspace_id
      CROSS JOIN LATERAL (VALUES ('course_expiry', c.expires_on, ARRAY[p_today, p_today + 7]),
                                 ('course_target', c.target_on, ARRAY[p_today, p_today + 1])) AS x(key, on_date, days)
      WHERE x.on_date = ANY (x.days) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND w.deleted_at IS NULL
    LOOP
      FOR v_uid IN
        SELECT DISTINCT p.user_id FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
        WHERE e.item_id = v_row.item_id
        UNION SELECT v_row.created_by
        WHERE NOT EXISTS (SELECT 1 FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
                          WHERE e.item_id = v_row.item_id)
      LOOP
        v_at := v_at + 1;
        IF app_notify_item(app_id(p_ids, v_at), v_uid, 'expiry', v_row.item_id, NULL,
                           jsonb_build_object('key', v_row.key, 'date', p_today, 'due_date', v_row.on_date,
                                              'subject', v_row.title)) THEN
          v_count := v_count + 1;
        END IF;
      END LOOP;
    END LOOP;

    -- 기록 날짜: 복습일(review), 투고 제출 예정일·수정 마감일과 산출물 마감일(deadline).
    FOR v_row IN
      SELECT r.id, r.item_id, r.kind, r.title, r.created_by, r.fields, x.key, x.on_date
      FROM item_records r
      JOIN items i ON i.id = r.item_id
      JOIN modules m ON m.id = i.module_id
      JOIN workspaces w ON w.id = i.workspace_id
      CROSS JOIN LATERAL (VALUES ('date', r.occurred_on),
                                 ('revision_due', CASE WHEN r.kind = 'submission'
                                                       THEN (r.fields->>'revision_due')::date END)) AS x(key, on_date)
      WHERE r.kind IN ('review', 'submission', 'deliverable')
        AND x.on_date IN (p_today, p_today + 1)
        AND coalesce(r.fields->>'status', '') NOT IN ('accepted', 'published', 'rejected', 'done')
        AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL AND w.deleted_at IS NULL
    LOOP
      FOR v_uid IN
        SELECT v_row.created_by
        UNION SELECT (jsonb_array_elements_text(coalesce(v_row.fields->'coauthors', '[]')))::uuid
        WHERE v_row.kind = 'submission'
        UNION SELECT (v_row.fields->>'owner')::uuid WHERE v_row.kind = 'deliverable' AND v_row.fields ? 'owner'
      LOOP
        v_at := v_at + 1;
        IF app_notify_item(app_id(p_ids, v_at), v_uid, CASE WHEN v_row.kind = 'review' THEN 'review' ELSE 'deadline' END,
                           v_row.item_id, v_row.id,
                           jsonb_build_object('key', v_row.id::text || ':' || v_row.key, 'date', p_today,
                                              'due_date', v_row.on_date, 'subject', v_row.title,
                                              'record_kind', v_row.kind, 'field', v_row.key)) THEN
          v_count := v_count + 1;
        END IF;
      END LOOP;
    END LOOP;
    RETURN v_count;
  END
  $$;

-- 위 두 고르기의 (대상, 받는 사람) 쌍 수.
CREATE FUNCTION app_system_item_reminders_ids(p_today date) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      (SELECT count(*)
       FROM course_details c
       JOIN items i ON i.id = c.item_id
       JOIN modules m ON m.id = i.module_id
       JOIN workspaces w ON w.id = i.workspace_id
       CROSS JOIN LATERAL (VALUES ('course_expiry', c.expires_on, ARRAY[p_today, p_today + 7]),
                                  ('course_target', c.target_on, ARRAY[p_today, p_today + 1])) AS x(key, on_date, days)
       CROSS JOIN LATERAL (
         SELECT DISTINCT p.user_id FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
         WHERE e.item_id = c.item_id
         UNION SELECT i.created_by
         WHERE NOT EXISTS (SELECT 1 FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
                           WHERE e.item_id = c.item_id)) u
       WHERE x.on_date = ANY (x.days) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
         AND w.deleted_at IS NULL)::integer
      + (SELECT count(*)
         FROM item_records r
         JOIN items i ON i.id = r.item_id
         JOIN modules m ON m.id = i.module_id
         JOIN workspaces w ON w.id = i.workspace_id
         CROSS JOIN LATERAL (VALUES ('date', r.occurred_on),
                                    ('revision_due', CASE WHEN r.kind = 'submission'
                                                          THEN (r.fields->>'revision_due')::date END)) AS x(key, on_date)
         CROSS JOIN LATERAL (
           SELECT r.created_by
           UNION SELECT (jsonb_array_elements_text(coalesce(r.fields->'coauthors', '[]')))::uuid
           WHERE r.kind = 'submission'
           UNION SELECT (r.fields->>'owner')::uuid WHERE r.kind = 'deliverable' AND r.fields ? 'owner') u
         WHERE r.kind IN ('review', 'submission', 'deliverable')
           AND x.on_date IN (p_today, p_today + 1)
           AND coalesce(r.fields->>'status', '') NOT IN ('accepted', 'published', 'rejected', 'done')
           AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
           AND w.deleted_at IS NULL)::integer
    END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_create_upload(uuid, uuid, text, uuid, text, text, text, bigint, bigint, integer, bigint,
                                         bigint, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_finish_upload(uuid, text, bigint, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_rename_file(uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_file_version(uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_file(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_restore_batch(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_batch_as(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_batch(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_expired(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_expired_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_purge_expired(uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_purge_expired_ids() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_due_notifications(date, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_due_notifications_ids(date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_item_reminders(date, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_item_reminders_ids(date) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
  app_create_upload(uuid, uuid, text, uuid, text, text, text, bigint, bigint, integer, bigint, bigint, uuid[]),
  app_finish_upload(uuid, text, bigint, uuid[]),
  app_rename_file(uuid, text, uuid[]),
  app_delete_file_version(uuid, integer, uuid[]),
  app_trash_file(uuid, uuid[]),
  app_restore_batch(uuid, uuid[]),
  app_purge_batch(uuid, uuid[]),
  app_purge_expired(uuid, uuid[]),
  app_purge_expired_ids(uuid),
  app_system_purge_expired(uuid[]),
  app_system_purge_expired_ids(),
  app_system_due_notifications(date, uuid[]),
  app_system_due_notifications_ids(date),
  app_system_item_reminders(date, uuid[]),
  app_system_item_reminders_ids(date)
TO fvoci_app;
