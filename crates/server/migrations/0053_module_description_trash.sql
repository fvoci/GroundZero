-- 모듈 설명과 모듈 휴지통·복구(사용자 결정, D-78).
--
-- 설명: 관리자가 쓰는 글(아이콘과 같은 방식). 모든 멤버가 본다.
-- 휴지통: 관리자가 모듈 하나를 휴지통 묶음(root_kind 'module')으로 보낸다. 표시는 모듈 행에만 하고 그 아래
-- 항목은 따로 표시하지 않는다. 항목 권한 함수(app_item_access)가 지운 모듈의 항목을 없는 것으로 보므로 항목
-- 아래 업무·문서·일정·첨부·기록·검색·알림·달력이 모두 함께 숨는다.
-- 복구: 그 묶음의 모듈 행만 되돌린다. 모듈을 지우기 전에 따로 휴지통에 간 항목·문서·업무는 자기 묶음에 그대로
-- 남고(되살리지 않음), 항목의 공개 범위·멤버는 바꾸지 않는다(권한을 넓히지 않음). 관리자만 되살린다.
-- 영구 삭제: 모듈의 모든 항목(따로 휴지통에 있던 것 포함)을 항목 영구 삭제와 같은 순서로 지우고, 비게 된 다른
-- 묶음을 끝낸다. 자동 정리는 살아 있는 기록·문서가 참조하는 버전을 지우지 않는다(D-56, 항목 묶음과 같은 규칙).

ALTER TABLE modules ADD COLUMN description text NOT NULL DEFAULT '' CHECK (length(description) <= 4000);
ALTER TABLE modules ADD COLUMN deleted_at timestamptz;
ALTER TABLE modules ADD COLUMN trash_batch_id uuid REFERENCES trash_batches (id);

ALTER TABLE trash_batches DROP CONSTRAINT trash_batches_root_kind_check;
ALTER TABLE trash_batches ADD CONSTRAINT trash_batches_root_kind_check
  CHECK (root_kind IN ('item', 'document', 'task', 'event', 'file', 'module'));

-- 지운 모듈은 목록·화면에서 사라진다(휴지통 목록은 묶음의 제목을 쓴다).
DROP POLICY modules_select ON modules;
CREATE POLICY modules_select ON modules FOR SELECT
  USING (app_member_role(workspace_id) IS NOT NULL AND deleted_at IS NULL);

CREATE FUNCTION app_set_module_description(p_module uuid, p_text text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid := (SELECT workspace_id FROM modules WHERE id = p_module AND deleted_at IS NULL);
  BEGIN
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    IF length(btrim(coalesce(p_text, ''))) > 4000 THEN
      RAISE EXCEPTION 'the description is too long' USING ERRCODE = '22023';
    END IF;
    UPDATE modules SET description = btrim(coalesce(p_text, '')) WHERE id = p_module;
  END
  $$;

-- 0035와 같고, 지운 모듈의 항목은 없는 것으로 본다.
CREATE OR REPLACE FUNCTION app_item_access(p_item uuid) RETURNS text
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
    IF v_item.id IS NULL OR v_item.deleted_at IS NOT NULL THEN
      RETURN NULL;
    END IF;
    IF EXISTS (SELECT 1 FROM modules WHERE id = v_item.module_id AND deleted_at IS NOT NULL) THEN
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
    -- 비공개 항목은 항목 멤버(직접·그룹)만, 워크스페이스 역할 안에서.
    IF v_item.visibility <> 'workspace' AND NOT app_is_item_member(p_item, v_user) THEN
      RETURN NULL;
    END IF;
    RETURN CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
  END
  $$;

CREATE FUNCTION app_trash_module(p_module uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    INSERT INTO trash_batches (workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_module.workspace_id, 'module', p_module, v_module.name, v_count, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE modules SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_module;
    PERFORM app_audit(v_module.workspace_id, 'delete', 'module', p_module,
                      jsonb_build_object('batch', v_batch, 'items', v_count));
    RETURN v_batch;
  END
  $$;

-- 묶음에 든 문서: 모듈 묶음이면 그 모듈 항목들의 문서.
CREATE OR REPLACE FUNCTION app_batch_documents(p_batch trash_batches) RETURNS TABLE (document_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT d.id FROM documents d
    WHERE (p_batch.root_kind = 'document' AND d.trash_batch_id = p_batch.id)
       OR (p_batch.root_kind = 'item' AND d.item_id = p_batch.root_id)
       OR (p_batch.root_kind = 'module'
           AND d.item_id IN (SELECT i.id FROM items i WHERE i.module_id = p_batch.root_id))
  $$;

CREATE OR REPLACE FUNCTION app_batch_files(p_batch trash_batches) RETURNS TABLE (file_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT f.id FROM files f WHERE p_batch.root_kind = 'file' AND f.trash_batch_id = p_batch.id
    UNION ALL
    SELECT f.id FROM files f WHERE p_batch.root_kind = 'item' AND f.item_id = p_batch.root_id
    UNION ALL
    SELECT f.id FROM files f JOIN items i ON i.id = f.item_id
    WHERE p_batch.root_kind = 'module' AND i.module_id = p_batch.root_id
    UNION ALL
    SELECT f.id FROM app_batch_documents(p_batch) d JOIN files f ON f.document_id = d.document_id
  $$;

-- 0047과 같고, 모듈 묶음은 그 모듈 항목의 기록·문서를 함께 사라질 것으로 본다. 지운 모듈이 만료되면 그 안의
-- 기록·문서도 만료된 것으로 본다.
CREATE OR REPLACE FUNCTION app_batch_keeps_referenced(p_batch trash_batches) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH bf AS (SELECT file_id FROM app_batch_files(p_batch)),
         gone AS (SELECT document_id FROM app_batch_documents(p_batch))
    SELECT EXISTS (
             SELECT 1 FROM bf
             JOIN file_versions v ON v.file_id = bf.file_id
             JOIN record_files rf ON rf.version_id = v.id
             JOIN item_records r ON r.id = rf.record_id
             JOIN items i ON i.id = r.item_id
             JOIN modules m ON m.id = i.module_id
             WHERE NOT (p_batch.root_kind = 'item' AND r.item_id = p_batch.root_id)
               AND NOT (p_batch.root_kind = 'module' AND i.module_id = p_batch.root_id)
               AND NOT app_batch_expired(i.trash_batch_id)
               AND NOT app_batch_expired(m.trash_batch_id))
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_file_refs ref ON ref.file_id = f.id
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                 OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

-- 0048과 같고 모듈 묶음을 더한다. 모듈 안의 것을 되살리려면 모듈을 먼저 되살려야 한다.
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
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'module', v_batch.root_id,
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

-- 항목 하나의 영구 삭제 본문(0047 항목 묶음과 같은 순서). 호출하는 쪽이 권한·잠금을 맡는다.
CREATE FUNCTION app_purge_item_rows(p_item uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE item_id = p_item));
    DELETE FROM task_dependencies WHERE item_id = p_item;
    DELETE FROM task_series WHERE item_id = p_item;
    PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE item_id = p_item));
    DELETE FROM events WHERE item_id = p_item;
    DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = p_item;
    DELETE FROM items WHERE id = p_item;
  END
  $$;

-- 0047과 같고 모듈 묶음을 더한다.
CREATE OR REPLACE FUNCTION app_purge_batch_as(p_batch uuid, p_system boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      DELETE FROM module_keys WHERE module_id = v_batch.root_id;
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
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_batch.workspace_id, v_batch.owner_user_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
  END
  $$;

REVOKE ALL ON FUNCTION app_set_module_description(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_module(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_item_rows(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_module_description(uuid, text), app_trash_module(uuid) TO fvoci_app;
