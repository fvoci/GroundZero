-- 파일 리뷰 반영 (0027 보완).
--
-- 1. 내려받기 권한(D-20 "복제·다운로드 허용 여부는 소유자가 별도 설정", D-12): 문서 첨부는 문서를 고칠 수 있는
--    사람이나, 소유자가 복제·다운로드를 허용한 문서의 독자만 내려받는다. 독자도 안전한 형식은 브라우저 안에서
--    볼 수 있다(보기는 읽기 권한). 항목 첨부는 항목에 그런 설정이 없어 읽을 수 있는 사람이 내려받는다.
-- 2. 마치기가 끊겨도 이어서: 같은 요청을 다시 만들면 먼저 들어간 업로드를 돌려주고(동시 생성 500 방지), 마치는
--    동안에는 기한을 늘리며, 'completing'은 오래 멈췄을 때만 기한 만료로 닫는다. 저장소에서 합치지 못하면
--    (조각 표시가 어긋남 등) 업로드를 다시 'uploading'으로 돌리고 받은 조각을 비워 처음부터 다시 받는다.
-- 3. 보관된 항목(항목 문서 포함)의 파일은 이름 바꾸기·지우기를 막고, 되살리기는 그 항목의 권한·보관을 본다.
-- 4. 파일 행을 지우면 파일만 담은 휴지통 묶음도 끝낸다(되살릴 것이 없는 묶음이 남지 않게).
-- 5. 참조되지 않는 이전 버전 지우기(D-56): 쓸 수 있는 사람이 현재 버전이 아닌 버전을 영구 삭제한다. 분석 기록이
--    생기면 참조 중인 버전은 여기서 막는다.

-- 파일이 보관된 항목(직접 또는 항목 문서를 통해)에 있는지.
CREATE FUNCTION app_file_archived(p_file uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce(app_item_archived(coalesce(f.item_id, d.item_id)), false)
    FROM files f LEFT JOIN documents d ON d.id = f.document_id
    WHERE f.id = p_file
  $$;

-- 내려받을 수 있는지(보기와 구분).
CREATE FUNCTION app_file_can_download(p_file uuid) RETURNS boolean
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_file_access(p_file);
    v_doc    uuid;
  BEGIN
    IF v_access IS NULL THEN
      RETURN false;
    END IF;
    IF v_access IN ('owner', 'write') THEN
      RETURN true;
    END IF;
    SELECT document_id INTO v_doc FROM files WHERE id = p_file;
    IF v_doc IS NULL THEN
      RETURN true;
    END IF;
    RETURN coalesce((SELECT allow_copy FROM documents WHERE id = v_doc), false);
  END
  $$;

-- 저장소에서 합치지 못한 업로드를 처음부터 다시 받게 한다(올린 사람, 'completing'일 때).
CREATE FUNCTION app_reset_upload(p_upload uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    UPDATE uploads SET state = 'uploading', state_changed_at = now()
    WHERE id = p_upload AND user_id = app_user_id() AND state = 'completing';
    IF FOUND THEN
      DELETE FROM upload_parts WHERE upload_id = p_upload;
    END IF;
  END
  $$;

-- 현재가 아닌 버전 하나를 영구 삭제(쓸 수 있는 사람). 저장소의 내용은 작업 프로세스가 지운다.
CREATE FUNCTION app_delete_file_version(p_file uuid, p_version integer) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete_version', 'file', p_file,
            jsonb_build_object('name', v_file.name, 'version', p_version));
  END
  $$;

CREATE OR REPLACE FUNCTION app_create_upload(
  p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text, p_type text,
  p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint, p_max_storage bigint)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    v_id := uuidv7();
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

CREATE OR REPLACE FUNCTION app_begin_complete(p_upload uuid) RETURNS uploads
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_up    uploads%ROWTYPE;
    v_parts integer;
    v_bytes bigint;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_up.state = 'done' THEN
      RETURN v_up;
    END IF;
    IF v_up.state = 'completing' AND v_up.state_changed_at > now() - interval '5 minutes' THEN
      RAISE EXCEPTION 'the upload is being completed; try again shortly' USING ERRCODE = '55000';
    END IF;
    IF v_up.state NOT IN ('uploading', 'completing') OR v_up.expires_at <= now() THEN
      RAISE EXCEPTION 'the upload is no longer open' USING ERRCODE = '55000';
    END IF;
    SELECT count(*), coalesce(sum(size), 0) INTO v_parts, v_bytes FROM upload_parts WHERE upload_id = p_upload;
    IF v_bytes <> v_up.size OR v_parts <> greatest(1, ceil(v_up.size::numeric / v_up.chunk_bytes))::integer
       OR EXISTS (SELECT 1 FROM upload_parts WHERE upload_id = p_upload
                  AND number > greatest(1, ceil(v_up.size::numeric / v_up.chunk_bytes))::integer) THEN
      RAISE EXCEPTION 'some parts are missing' USING ERRCODE = '55000';
    END IF;
    -- 마치는 동안 기한이 지나 닫히지 않게 늘린다.
    UPDATE uploads SET state = 'completing', state_changed_at = now(),
                       expires_at = greatest(expires_at, now() + interval '1 hour')
    WHERE id = p_upload RETURNING * INTO v_up;
    RETURN v_up;
  END
  $$;

CREATE OR REPLACE FUNCTION app_system_expire_uploads() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    WITH gone AS (
      UPDATE uploads SET state = 'expired', state_changed_at = now()
      WHERE expires_at <= now()
        AND (state = 'uploading'
             OR (state = 'completing' AND state_changed_at < now() - interval '10 minutes'))
      RETURNING id, storage_key, storage_upload_id
    ), parts AS (
      DELETE FROM upload_parts WHERE upload_id IN (SELECT id FROM gone)
    )
    INSERT INTO storage_deletions (storage_key, storage_upload_id)
    SELECT storage_key, coalesce(storage_upload_id, '') FROM gone
    UNION SELECT storage_key, '' FROM gone
    ON CONFLICT DO NOTHING;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;

CREATE OR REPLACE FUNCTION app_rename_file(p_file uuid, p_name text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'rename', 'file', p_file,
            jsonb_build_object('from', v_file.name, 'to', v_name));
  END
  $$;

CREATE OR REPLACE FUNCTION app_trash_file(p_file uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    INSERT INTO trash_batches (workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_file.workspace_id, v_file.owner_user_id, 'file', p_file, v_file.name, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE files SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_file;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete', 'file', p_file,
            jsonb_build_object('name', v_file.name, 'batch', v_batch));
    RETURN v_batch;
  END
  $$;

CREATE OR REPLACE FUNCTION app_delete_file_rows(p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE uploads SET version_id = NULL
    WHERE version_id IN (SELECT id FROM file_versions WHERE file_id = ANY (p_ids));
    DELETE FROM file_versions WHERE file_id = ANY (p_ids);
    -- 파일만 담은 휴지통 묶음은 함께 끝난다(되살릴 것이 없으므로).
    UPDATE trash_batches SET purged_at = now()
    WHERE root_kind = 'file' AND root_id = ANY (p_ids) AND purged_at IS NULL AND restored_at IS NULL;
    DELETE FROM files WHERE id = ANY (p_ids);
  END
  $$;

CREATE OR REPLACE FUNCTION app_restore_batch(p_batch uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    IF app_trash_expires_at(v_batch) <= now() THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
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
    END IF;
    IF v_item IS NOT NULL THEN
      PERFORM app_lock_item(v_item);
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
      PERFORM app_audit_document(v_batch.root_id, 'restore', jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      UPDATE tasks SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE tasks t SET parent_id = NULL
      WHERE t.id = v_batch.root_id AND t.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM tasks p WHERE p.id = t.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'task', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'item' THEN
      UPDATE items SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'item', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'file' THEN
      UPDATE files SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (v_batch.workspace_id, v_batch.owner_user_id, app_user_id(), 'restore', 'file', v_batch.root_id,
              jsonb_build_object('batch', p_batch));
    ELSE
      UPDATE events SET deleted_at = NULL, deleted_by = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'event', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    END IF;
    UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
  END
  $$;

-- 저장소 정리 재시도는 횟수만큼 미룬다(같은 주기에 바로 다시 꺼내지 않게).
CREATE OR REPLACE FUNCTION app_system_storage_claim(p_limit integer)
  RETURNS TABLE (storage_key text, storage_upload_id text, attempts integer)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    DELETE FROM storage_deletions d
    WHERE (d.storage_key, d.storage_upload_id) IN (
      SELECT s.storage_key, s.storage_upload_id FROM storage_deletions s
      WHERE app_user_id() IS NULL AND s.attempts < 10 AND s.queued_at <= now()
      ORDER BY s.queued_at LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING d.storage_key, d.storage_upload_id, d.attempts
  $$;

CREATE OR REPLACE FUNCTION app_system_storage_retry(p_key text, p_upload text, p_attempts integer) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO storage_deletions (storage_key, storage_upload_id, attempts, queued_at)
    SELECT p_key, p_upload, p_attempts + 1, now() + make_interval(mins => p_attempts + 1)
    WHERE app_user_id() IS NULL
    ON CONFLICT DO NOTHING
  $$;

REVOKE ALL ON FUNCTION app_file_archived(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_file_can_download(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_reset_upload(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_file_version(uuid, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_file_can_download(uuid),
  app_reset_upload(uuid),
  app_delete_file_version(uuid, integer)
TO fvoci_app;
