-- D-56 일관성 수정(사용자 지시 2026-10-17, O-34): 휴지통 만료로 작업 프로세스가 묶음을 자동 영구 삭제할 때, 그 묶음과
-- 함께 사라지지 않는 기록·문서가 참조하는 파일 버전이 들어 있으면 지우지 않고 "만료" 상태로 둔다. 관리자의 직접
-- 영구 삭제(기존 승인 정책)는 바꾸지 않는다.
-- 동시성: 문서 저장이 새 참조를 적을 때 그 버전 행을 공유 잠금(FOR KEY SHARE)하고, 자동 정리는 지울 버전 행을
-- 먼저 배타 잠금(FOR UPDATE)한 뒤 참조를 다시 본다. 정리가 뒤에 잠그면 저장이 끝난 뒤의 참조를 보고 건너뛴다.

-- 묶음을 영구 삭제하면 사라지는 문서.
CREATE FUNCTION app_batch_documents(p_batch trash_batches) RETURNS TABLE (document_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT d.id FROM documents d
    WHERE (p_batch.root_kind = 'document' AND d.trash_batch_id = p_batch.id)
       OR (p_batch.root_kind = 'item' AND d.item_id = p_batch.root_id)
  $$;

-- 묶음을 영구 삭제하면 사라지는 파일(파일 묶음, 지워지는 문서의 첨부, 지워지는 항목의 첨부).
CREATE FUNCTION app_batch_files(p_batch trash_batches) RETURNS TABLE (file_id uuid)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT f.id FROM files f
    WHERE (p_batch.root_kind = 'file' AND f.trash_batch_id = p_batch.id)
       OR (p_batch.root_kind = 'item' AND f.item_id = p_batch.root_id)
       OR f.document_id IN (SELECT document_id FROM app_batch_documents(p_batch))
  $$;

-- 묶음이 지울 파일 버전 중, 묶음과 함께 사라지지 않는 기록 또는 같은 곳의 문서(app_version_referenced와 같은 범위)가
-- 참조하는 것이 있는지.
CREATE FUNCTION app_batch_keeps_referenced(p_batch trash_batches) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (
             SELECT 1 FROM app_batch_files(p_batch) bf
             JOIN record_files rf ON rf.file_id = bf.file_id AND rf.version_id IS NOT NULL
             JOIN item_records r ON r.id = rf.record_id
             WHERE NOT (p_batch.root_kind = 'item' AND r.item_id = p_batch.root_id))
        OR EXISTS (
             SELECT 1 FROM app_batch_files(p_batch) bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_file_refs ref ON ref.file_id = f.id
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             WHERE d.id NOT IN (SELECT document_id FROM app_batch_documents(p_batch))
               AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                 OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

-- 화면 안내용: 내가 볼 수 있는 묶음 중 참조 때문에 자동 삭제를 미루는 것.
CREATE FUNCTION app_trash_kept_for_references(p_ids uuid[]) RETURNS SETOF uuid
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT b.id FROM trash_batches b
    WHERE b.id = ANY (p_ids) AND b.purged_at IS NULL AND b.restored_at IS NULL
      AND app_trash_role(b) IS NOT NULL AND app_batch_keeps_referenced(b)
  $$;

-- 문서 리비전의 참조를 적을 때 그 버전 행을 공유 잠금한다(동시에 도는 자동 정리가 이 저장을 기다린 뒤 참조를 본다).
CREATE OR REPLACE FUNCTION app_record_file_refs() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO document_file_refs (document_id, revision, file_id, version)
    SELECT NEW.document_id, NEW.revision, r.file_id, r.version FROM app_file_refs(NEW.body) r
    ON CONFLICT DO NOTHING;
    PERFORM 1 FROM file_versions v
    JOIN document_file_refs r ON r.file_id = v.file_id AND r.version = v.version
    WHERE r.document_id = NEW.document_id AND r.revision = NEW.revision
    ORDER BY v.id
    FOR KEY SHARE OF v;
    RETURN NULL;
  END
  $$;

CREATE OR REPLACE FUNCTION app_purge_batch_as(p_batch uuid, p_system boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_item  uuid;
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
    -- 자동 정리는 살아 있는 기록·문서가 참조하는 버전을 지우지 않는다(D-56). 지울 버전 행을 먼저 잠근 뒤 본다.
    IF p_system THEN
      PERFORM 1 FROM file_versions
      WHERE file_id IN (SELECT file_id FROM app_batch_files(v_batch))
      ORDER BY id
      FOR UPDATE;
      IF app_batch_keeps_referenced(v_batch) THEN
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

-- 참조 때문에 건너뛴 묶음은 지운 수에 넣지 않는다(다음 실행에서 다시 본다).
CREATE OR REPLACE FUNCTION app_system_purge_expired() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_ws    uuid;
    v_made  integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_ws IN
      SELECT id FROM workspaces
      WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now()
      ORDER BY deleted_at
    LOOP
      BEGIN
        IF app_purge_workspace_as(v_ws, true) THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of deleted workspace % failed: %', v_ws, SQLERRM;
      END;
    END LOOP;
    FOR v_batch IN
      SELECT * FROM trash_batches WHERE restored_at IS NULL AND purged_at IS NULL ORDER BY deleted_at
    LOOP
      CONTINUE WHEN app_trash_expires_at(v_batch) > now();
      BEGIN
        PERFORM app_purge_batch_as(v_batch.id, true);
        IF (SELECT purged_at FROM trash_batches WHERE id = v_batch.id) IS NOT NULL THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of trash batch % failed: %', v_batch.id, SQLERRM;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 이전 버전 정리도 같은 순서: 후보 행을 잠근 뒤 참조를 다시 보고, 그사이 생긴 참조면 건너뛴다(오류로 작업 전체를
-- 멈추지 않는다). 지우기 트리거의 확인은 마지막 방어선으로 남는다.
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
      PERFORM 1 FROM file_versions WHERE id = v_row.id FOR UPDATE;
      CONTINUE WHEN NOT FOUND OR app_version_referenced(v_row.file_id, v_row.version, v_row.id);
      UPDATE uploads SET version_id = NULL WHERE version_id = v_row.id;
      DELETE FROM file_versions WHERE id = v_row.id;
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  END
  $$;

REVOKE ALL ON FUNCTION app_batch_documents(trash_batches) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_batch_files(trash_batches) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_batch_keeps_referenced(trash_batches) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_kept_for_references(uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_trash_kept_for_references(uuid[]) TO fvoci_app;
