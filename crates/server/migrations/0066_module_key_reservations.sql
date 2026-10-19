-- 남은 항목 ③(D-18, docs/development.md 82절): 모듈을 휴지통에서 영구 삭제하면 그 모듈의 키 행(지금 키·과거 키)을 지워,
-- 다른 모듈이 그 키를 다시 쓸 수 있었다(남은 글자 참조 `KEY-12`가 새 모듈을 가리킴). 키 행을 남기고 주인만 비운다(예약).
-- 주인 없는 행은 늘 과거 키다. 워크스페이스 영구 삭제는 예약까지 지운다(그대로). 이 마이그레이션 전에 영구 삭제된 모듈의
-- 키는 이미 지워져 되살릴 수 없다. 함수 본문은 앞선 정의와 같고 "바뀐 곳"만 다르다.

ALTER TABLE module_keys ALTER COLUMN module_id DROP NOT NULL;
ALTER TABLE module_keys ADD CONSTRAINT module_keys_reserved_retired CHECK (module_id IS NOT NULL OR retired_at IS NOT NULL);

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
      -- 바뀐 곳: 키 행을 지우지 않고 예약으로 남긴다(주인을 비우고 과거 키로).
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
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_batch.workspace_id, v_batch.owner_user_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
  END
  $$;

CREATE OR REPLACE FUNCTION app_update_module(p_module uuid, p_name text, p_key text) RETURNS void
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
      -- 바뀐 곳: 주인 없는 예약 키(영구 삭제한 모듈의 것)도 거절한다.
      IF FOUND AND v_owner IS DISTINCT FROM p_module THEN
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
