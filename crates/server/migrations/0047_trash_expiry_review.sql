-- 0046 리뷰 반영:
-- 1. 자동 정리는 참조를 먼저 잠금 없이 보고, 지울 묶음일 때만 버전 행을 잠근다. 잠금은 SKIP LOCKED로 잡아, 저장 중인
--    트랜잭션이 행을 잡고 있으면 기다리지 않고 이번 실행에서는 건너뛴다(교착이 생기지 않는다). 남겨 두는 묶음은
--    잠그지 않는다. 이전 버전 정리도 같다.
-- 2. 묶음이 지울 파일을 종류별로 색인이 있는 조건으로 고르고, 기록은 버전 id로 잇는다.
-- 3. 참조하는 쪽이 이미 만료된 휴지통에 있으면(되살릴 수 없음) 살아 있는 참조로 보지 않는다. 서로를 가리키는
--    두 묶음이 영원히 남지 않는다.

-- 만료돼 되살릴 수 없는 휴지통 묶음인지.
CREATE FUNCTION app_batch_expired(p_batch uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce((SELECT b.restored_at IS NULL AND b.purged_at IS NULL AND app_trash_expires_at(b) <= now()
                     FROM trash_batches b WHERE b.id = p_batch), false)
  $$;

CREATE OR REPLACE FUNCTION app_batch_files(p_batch trash_batches) RETURNS TABLE (file_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT f.id FROM files f WHERE p_batch.root_kind = 'file' AND f.trash_batch_id = p_batch.id
    UNION ALL
    SELECT f.id FROM files f WHERE p_batch.root_kind = 'item' AND f.item_id = p_batch.root_id
    UNION ALL
    SELECT f.id FROM app_batch_documents(p_batch) d JOIN files f ON f.document_id = d.document_id
  $$;

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
             WHERE NOT (p_batch.root_kind = 'item' AND r.item_id = p_batch.root_id)
               AND NOT app_batch_expired(i.trash_batch_id))
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_file_refs ref ON ref.file_id = f.id
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             WHERE NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                 OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

CREATE OR REPLACE FUNCTION app_purge_batch_as(p_batch uuid, p_system boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
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
      -- 남길 묶음은 잠그지 않는다.
      IF app_batch_keeps_referenced(v_batch) THEN
        RETURN;
      END IF;
      -- 지울 버전 행을 잠근다. 저장 중인 트랜잭션이 잡고 있는 행이 있으면 이번에는 건너뛴다(기다리지 않음).
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
    ELSE
      v_item := v_batch.root_id;
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE item_id = v_item));
      DELETE FROM task_dependencies WHERE item_id = v_item;
      DELETE FROM task_series WHERE item_id = v_item;
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE item_id = v_item));
      DELETE FROM events WHERE item_id = v_item;
      DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = v_item;
      -- 이 항목 안의 것이 다른 휴지통 묶음에 있었으면 그 묶음도 함께 끝난다.
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND NOT EXISTS (SELECT 1 FROM documents WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM tasks WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM events WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM files WHERE trash_batch_id = trash_batches.id)
        AND root_kind <> 'item';
      DELETE FROM items WHERE id = v_item;
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

-- 후보 행을 다른 트랜잭션이 잡고 있으면(새 참조를 적는 중일 수 있음) 기다리지 않고 다음 실행으로 미룬다.
CREATE OR REPLACE FUNCTION app_system_purge_old_versions() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row   record;
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_row IN
      SELECT v.id, v.file_id, v.version
      FROM file_versions v
      JOIN files f ON f.id = v.file_id
      JOIN workspaces w ON w.id = f.workspace_id
      WHERE w.version_retention_days IS NOT NULL
        AND v.version <> f.current_version
        AND (SELECT min(n.created_at) FROM file_versions n WHERE n.file_id = v.file_id AND n.version > v.version)
            < now() - make_interval(days => w.version_retention_days)
        AND NOT app_version_referenced(v.file_id, v.version, v.id)
      ORDER BY v.created_at
      LIMIT 500
    LOOP
      PERFORM 1 FROM file_versions WHERE id = v_row.id FOR UPDATE SKIP LOCKED;
      CONTINUE WHEN NOT FOUND OR app_version_referenced(v_row.file_id, v_row.version, v_row.id);
      UPDATE uploads SET version_id = NULL WHERE version_id = v_row.id;
      DELETE FROM file_versions WHERE id = v_row.id;
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  END
  $$;

REVOKE ALL ON FUNCTION app_batch_expired(uuid) FROM PUBLIC;
