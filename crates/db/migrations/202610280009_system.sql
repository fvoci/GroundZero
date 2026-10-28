-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

CREATE TABLE fvoci.scheduled_runs (
    name text NOT NULL,
    last_run_at timestamp with time zone NOT NULL
);


ALTER TABLE fvoci.scheduled_runs OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_batch_documents(p_batch fvoci.trash_batches) RETURNS TABLE(document_id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT d.id FROM documents d
    WHERE (p_batch.root_kind = 'document' AND d.trash_batch_id = p_batch.id)
       OR (p_batch.root_kind = 'item' AND d.item_id = p_batch.root_id)
       OR (p_batch.root_kind = 'module'
           AND d.item_id IN (SELECT i.id FROM items i WHERE i.module_id = p_batch.root_id))
  $$;


ALTER FUNCTION fvoci.app_batch_documents(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_batch_expired(p_batch uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT b.restored_at IS NULL AND b.purged_at IS NULL AND app_trash_expires_at(b) <= now()
                     FROM trash_batches b WHERE b.id = p_batch), false)
  $$;


ALTER FUNCTION fvoci.app_batch_expired(p_batch uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_batch_files(p_batch fvoci.trash_batches) RETURNS TABLE(file_id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_batch_files(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_batch_keeps_referenced(p_batch fvoci.trash_batches) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
               AND app_file_ref_in_scope(d.workspace_id, d.owner_user_id, f.workspace_id, f.owner_user_id))
        -- 리비전 줄과 같은 규칙(휴지통 만료), 낡은 상태는 빼고.
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_collab_file_refs ref ON ref.file_id = f.id
             JOIN document_collab_states s ON s.document_id = ref.document_id AND s.pending
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE app_collab_state_current(s.document_id, s.revision)
               AND NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND app_file_ref_in_scope(d.workspace_id, d.owner_user_id, f.workspace_id, f.owner_user_id))
  $$;


ALTER FUNCTION fvoci.app_batch_keeps_referenced(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_claim_schedule(p_name text, p_every interval) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_last timestamptz;
  BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-schedule:' || p_name, 0));
    SELECT last_run_at INTO v_last FROM scheduled_runs WHERE name = p_name;
    IF v_last IS NOT NULL AND v_last + p_every > now() THEN
      RETURN false;
    END IF;
    INSERT INTO scheduled_runs (name, last_run_at) VALUES (p_name, now())
    ON CONFLICT (name) DO UPDATE SET last_run_at = now();
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_claim_schedule(p_name text, p_every interval) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_batch(p_batch uuid, p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_purge_batch_as(p_batch, false, p_ids) $$;


ALTER FUNCTION fvoci.app_purge_batch(p_batch uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 자동 정리는 참조를 먼저 확인하고 실제로 지울 버전만 잠근다. 다른 저장이 잡은 버전은 SKIP LOCKED로 미룬다.
-- 참조 기록은 버전 행을 먼저 공유 잠금하므로, 잠금 뒤의 재검사가 저장 중인 참조를 잃지 않게 한다(D-56).
CREATE FUNCTION fvoci.app_purge_batch_as(p_batch uuid, p_system boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_purge_batch_as(p_batch uuid, p_system boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_documents(p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE documents SET parent_id = NULL WHERE parent_id = ANY (p_ids) AND NOT (id = ANY (p_ids));
    UPDATE documents SET parent_id = NULL WHERE id = ANY (p_ids);
    DELETE FROM notifications WHERE document_id = ANY (p_ids);
    DELETE FROM document_mentions WHERE document_id = ANY (p_ids);
    DELETE FROM document_links WHERE document_id = ANY (p_ids);
    DELETE FROM document_shares WHERE document_id = ANY (p_ids);
    DELETE FROM public_links WHERE document_id = ANY (p_ids);
    DELETE FROM document_revisions WHERE document_id = ANY (p_ids);
    DELETE FROM user_marks WHERE target_kind = 'document' AND target_id = ANY (p_ids);
    DELETE FROM documents WHERE id = ANY (p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_purge_documents(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_expired(p_workspace uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
  BEGIN
    IF p_workspace IS NOT NULL AND app_access('workspace', p_workspace) <> 'owner' THEN
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


ALTER FUNCTION fvoci.app_purge_expired(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_expired_ids(p_workspace uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN p_workspace IS NOT NULL AND app_access('workspace', p_workspace) <> 'owner' THEN 0 ELSE
      (SELECT count(*) FROM trash_batches b
       WHERE b.restored_at IS NULL AND b.purged_at IS NULL
         AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
         AND app_trash_expires_at(b) <= now())::integer
    END
  $$;


ALTER FUNCTION fvoci.app_purge_expired_ids(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_item_rows(p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_purge_item_rows(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_tasks(p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE tasks SET parent_id = NULL WHERE parent_id = ANY (p_ids) AND NOT (id = ANY (p_ids));
    -- 영구 삭제한 반복 회차는 다시 만들지 않는다(반복에 건너뛸 날짜로 남긴다).
    UPDATE task_series s SET skipped_dates = (
      SELECT ARRAY(SELECT DISTINCT d FROM unnest(s.skipped_dates || ARRAY(
        SELECT t.occurrence_date FROM tasks t
        WHERE t.id = ANY (p_ids) AND t.series_id = s.id AND t.id <> s.root_task_id)) d ORDER BY d))
    WHERE s.id IN (SELECT series_id FROM tasks WHERE id = ANY (p_ids) AND series_id IS NOT NULL);
    UPDATE tasks SET series_id = NULL, occurrence_date = NULL
    WHERE series_id IN (SELECT id FROM task_series WHERE root_task_id = ANY (p_ids));
    DELETE FROM task_series WHERE root_task_id = ANY (p_ids);
    UPDATE tasks SET series_id = NULL, occurrence_date = NULL WHERE id = ANY (p_ids);
    DELETE FROM task_assignees WHERE task_id = ANY (p_ids);
    DELETE FROM checklist_items WHERE task_id = ANY (p_ids);
    DELETE FROM user_marks WHERE target_kind = 'task' AND target_id = ANY (p_ids);
    DELETE FROM tasks WHERE id = ANY (p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_purge_tasks(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_workspace(p_ws uuid, p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_purge_workspace_as(p_ws, false, p_ids) $$;


ALTER FUNCTION fvoci.app_purge_workspace(p_ws uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_workspace_as(p_ws uuid, p_system boolean, p_ids uuid[]) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws    workspaces%ROWTYPE;
    v_items uuid[];
  BEGIN
    -- 새 내용이 이 워크스페이스를 가리키며 들어오지 못하게 행을 잠근다(외래 키 공유 잠금과 충돌).
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NULL
       OR (NOT p_system AND v_ws.owner_id IS DISTINCT FROM app_user_id()
           AND app_access('organization', v_ws.organization_id) <> 'owner') THEN
      RAISE EXCEPTION 'deleted workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_system AND v_ws.deleted_at + make_interval(days => v_ws.trash_retention_days) > now() THEN
      RETURN false;
    END IF;
    v_items := ARRAY(SELECT id FROM items WHERE workspace_id = p_ws);
    PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE workspace_id = p_ws));
    DELETE FROM task_dependencies WHERE item_id = ANY (v_items);
    DELETE FROM task_series WHERE workspace_id = p_ws;
    PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE workspace_id = p_ws));
    DELETE FROM events WHERE workspace_id = p_ws;
    DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = ANY (v_items);
    DELETE FROM items WHERE workspace_id = p_ws;
    DELETE FROM module_keys WHERE workspace_id = p_ws;
    DELETE FROM modules WHERE workspace_id = p_ws;
    -- 묶음을 가리키는 모듈을 지운 뒤에 묶음을 지운다.
    DELETE FROM trash_batches WHERE workspace_id = p_ws;
    DELETE FROM numbered_targets WHERE workspace_id = p_ws;
    DELETE FROM reference_counters WHERE workspace_id = p_ws;
    DELETE FROM groups WHERE workspace_id = p_ws;
    DELETE FROM invitations WHERE workspace_id = p_ws;
    DELETE FROM ownership_offers WHERE workspace_id = p_ws;
    -- 개인 시간 기록은 남는다(대상 이름만 사라진다).
    UPDATE time_entries SET workspace_id = NULL WHERE workspace_id = p_ws;
    -- 공간 감사 기록은 소유자 영역으로 옮겨 남기고(조직 로그는 그대로), 활동 피드는 워크스페이스와 함께 지운다(FK).
    UPDATE audit_log SET workspace_id = NULL, organization_id = NULL,
                         owner_user_id = coalesce(owner_user_id, v_ws.owner_id)
    WHERE workspace_id = p_ws;
    DELETE FROM workspace_members WHERE workspace_id = p_ws;
    DELETE FROM workspaces WHERE id = p_ws;
    INSERT INTO audit_log (id, workspace_id, organization_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), NULL, NULL, v_ws.owner_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END),
           (app_id(p_ids, 2), NULL, v_ws.organization_id, NULL, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_purge_workspace_as(p_ws uuid, p_system boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_restore_batch(p_batch uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_doc   documents%ROWTYPE;
    v_item  uuid;
    v_ids   uuid[];
    v_file_doc uuid;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch;
    IF v_batch.id IS NULL OR app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    -- 첨부 묶음은 붙은 문서 행을 묶음보다 먼저 잠근다. 문서 이동은 문서 행 → 첨부 → 묶음 순서로 잡으므로 묶음을 먼저 잡으면
    -- 서로 기다릴 수 있다(교착 방지).
    IF v_batch.root_kind = 'file' THEN
      PERFORM 1 FROM documents WHERE id = (SELECT document_id FROM files WHERE id = v_batch.root_id) FOR SHARE;
    END IF;
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF app_trash_role(v_batch) IS NULL THEN
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
      -- 붙은 문서 행을 먼저 공유 잠금한다(#87): 공유 철회·공개 링크 철회·문서 휴지통 이동은 문서 행을 FOR UPDATE로 잡으므로
      -- 복원은 그 뒤에 지금 권한·휴지통 상태를 본다. 문서 → 파일 순서는 지우기(트리 → 문서 행 → 항목)와 같은 방향이다.
      PERFORM 1 FROM documents WHERE id = (SELECT document_id FROM files WHERE id = v_batch.root_id) FOR SHARE;
      -- 항목 문서에 붙은 파일은 그 항목의 권한·보관을 본다.
      SELECT coalesce(f.item_id, d.item_id), f.document_id INTO v_item, v_file_doc
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
      -- 보관된 모듈 아래에서는 항목도 복구하지 못한다. 모듈 보관을 푼 뒤 복구한다(D-113·D-125).
      IF EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                 WHERE i.id = v_batch.root_id AND m.archived_at IS NOT NULL) THEN
        RAISE EXCEPTION 'the module is archived' USING ERRCODE = '55000';
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
      IF app_trash_role(v_batch) = 'deleter' AND app_access('item', v_item) <> 'owner'
         AND app_access('item', v_item) <> 'write' THEN
        RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
      END IF;
    END IF;
    -- 잠금 뒤 다시 본다: 기다리는 사이 역할을 잃었으면(강등·제거·공유 철회) 없는 것과 같다.
    IF app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    -- 문서에 붙은 첨부는 지운 사람이 지금도 그 문서에 쓸 수 있어야 되살린다(항목 없는 위키 문서·공유 철회 포함).
    IF v_batch.root_kind = 'file' AND v_file_doc IS NOT NULL AND app_trash_role(v_batch) = 'deleter'
       AND app_access('document', v_file_doc) <> 'owner'
       AND app_access('document', v_file_doc) <> 'write' THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
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


ALTER FUNCTION fvoci.app_restore_batch(p_batch uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_purge_expired(p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
      -- 워크스페이스마다 새 id 둘(소유자 로그·조직 로그).
      v_at := v_at + 2;
      BEGIN
        IF app_purge_workspace_as(v_ws, true, ARRAY[app_id(p_ids, v_at - 1), app_id(p_ids, v_at)]) THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION
        WHEN SQLSTATE 'FV001' OR SQLSTATE 'FV002' THEN
          -- 새 id 계약 위반(D-88)은 묶음 실패가 아니라 호출 전체를 되돌려 다시 하게 한다.
          RAISE;
        WHEN OTHERS THEN
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
      EXCEPTION
        WHEN SQLSTATE 'FV001' OR SQLSTATE 'FV002' THEN
          RAISE;
        WHEN OTHERS THEN
          RAISE WARNING 'purge of trash batch % failed with SQLSTATE %', v_batch.id, SQLSTATE;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_system_purge_expired(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_purge_expired_ids() RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      2 * (SELECT count(*) FROM workspaces
       WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now())::integer
      + (SELECT count(*) FROM trash_batches b
         WHERE b.restored_at IS NULL AND b.purged_at IS NULL AND app_trash_expires_at(b) <= now())::integer
    END
  $$;


ALTER FUNCTION fvoci.app_system_purge_expired_ids() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_document(p_doc uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_doc    documents%ROWTYPE;
    v_seen   documents%ROWTYPE;
    v_access text := app_access('document', p_doc);
    v_batch  uuid;
    v_count  integer;
    v_tree   uuid[];
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF NOT (v_access = 'owner' OR (v_access = 'write' AND v_doc.created_by = v_user)) THEN
      IF v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id)
         AND app_access('document', p_doc) <> 'none' THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'only the owner or the author can delete this document' USING ERRCODE = '42501';
    END IF;
    -- 트리 잠금 → 문서 행 → 항목 공유(문서 잠금 순서).
    PERFORM app_lock_tree(CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END,
                          v_doc.item_id, v_doc.owner_user_id);
    -- 잠근 뒤 자리를 다시 본다. 기다리는 사이 다른 워크스페이스·항목·개인 노트로 옮겨졌으면 옛 자리 기준으로
    -- 묶음을 만들지 않고 다시 하게 한다(자리 바꾸기와 같은 규칙). 권한·작성자 규칙도 잠근 뒤의 값으로 다시 본다.
    v_seen := v_doc;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_item(v_doc.item_id);
    v_access := app_access('document', p_doc);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT (v_access = 'owner' OR (v_access = 'write' AND v_doc.created_by = v_user)) THEN
      RAISE EXCEPTION 'only the owner or the author can delete this document' USING ERRCODE = '42501';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END, v_doc.owner_user_id,
            'document', p_doc, v_doc.title, 0, v_user)
    RETURNING id INTO v_batch;
    -- 지우는 사람이 지울 수 있는 하위 문서만 함께 지운다. 볼 수 없거나 지울 수 없는 하위 문서는 이 문서의
    -- 부모 아래로 옮겨 남긴다(남의 문서를 몰래 휴지통에 넣지 않게).
    v_tree := ARRAY(
    WITH RECURSIVE tree(id) AS (
      SELECT p_doc
      UNION
      SELECT d.id FROM documents d JOIN tree t ON d.parent_id = t.id
      WHERE d.deleted_at IS NULL
        AND (app_access('document', d.id) = 'owner'
             OR (app_access('document', d.id) = 'write' AND d.created_by = v_user))
    )
    SELECT id FROM tree);
    UPDATE documents SET parent_id = v_doc.parent_id
    WHERE parent_id = ANY (v_tree) AND NOT (id = ANY (v_tree)) AND deleted_at IS NULL;
    UPDATE documents SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_tree);
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE trash_batches SET member_count = v_count WHERE id = v_batch;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'delete', jsonb_build_object('batch', v_batch, 'count', v_count));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_document(p_doc uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_expires_at(p_batch fvoci.trash_batches) RETURNS timestamp with time zone
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT p_batch.deleted_at + make_interval(days => coalesce(
      (SELECT trash_retention_days FROM workspaces WHERE id = p_batch.workspace_id), 30))
  $$;


ALTER FUNCTION fvoci.app_trash_expires_at(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_file(p_file uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_file   files%ROWTYPE;
    v_batch  uuid;
  BEGIN
    PERFORM app_require_file_write(p_file);
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


ALTER FUNCTION fvoci.app_trash_file(p_file uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_item(p_item uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item  items%ROWTYPE;
    v_batch uuid;
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('item', p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can delete it' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF v_item.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_item.workspace_id, 'item', p_item, v_item.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE items SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_item;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'delete', 'item', p_item,
                      jsonb_build_object('batch', v_batch));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_item(p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_kept_for_references(p_ids uuid[]) RETURNS SETOF uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT b.id FROM trash_batches b
    WHERE b.id = ANY (p_ids) AND b.purged_at IS NULL AND b.restored_at IS NULL
      AND app_trash_role(b) IS NOT NULL AND app_batch_keeps_referenced(b)
  $$;


ALTER FUNCTION fvoci.app_trash_kept_for_references(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_list(p_workspace uuid) RETURNS TABLE(id uuid, root_kind text, root_id uuid, title text, member_count integer, deleted_by uuid, deleted_by_name text, deleted_at timestamp with time zone, expires_at timestamp with time zone, can_purge boolean)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT b.id, b.root_kind, b.root_id, b.title, b.member_count, b.deleted_by, u.display_name, b.deleted_at,
           app_trash_expires_at(b), app_trash_role(b) = 'admin'
    FROM trash_batches b LEFT JOIN users u ON u.id = b.deleted_by
    WHERE b.restored_at IS NULL AND b.purged_at IS NULL
      AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
      AND app_trash_role(b) IS NOT NULL
    ORDER BY b.deleted_at DESC, b.id
  $$;


ALTER FUNCTION fvoci.app_trash_list(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_module(p_module uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_module modules%ROWTYPE;
    v_batch  uuid;
    v_count  integer;
  BEGIN
    SELECT * INTO v_module FROM modules WHERE id = p_module FOR UPDATE;
    IF v_module.id IS NULL OR v_module.deleted_at IS NOT NULL OR app_access('workspace', v_module.workspace_id) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_module.workspace_id);
    -- 휴지통에 없는 항목 수(목록 표시용). 따로 지운 항목은 자기 묶음에 있다.
    SELECT count(*) INTO v_count FROM items WHERE module_id = p_module AND deleted_at IS NULL;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_module.workspace_id, 'module', p_module, v_module.name, v_count, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE modules SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_module;
    PERFORM app_audit(app_id(p_ids, 2), v_module.workspace_id, 'delete', 'module', p_module,
                      jsonb_build_object('batch', v_batch, 'items', v_count));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_module(p_module uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_role(p_batch fvoci.trash_batches) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_role text;
  BEGIN
    IF v_user IS NULL OR p_batch.restored_at IS NOT NULL OR p_batch.purged_at IS NOT NULL THEN
      RETURN NULL;
    END IF;
    IF p_batch.owner_user_id IS NOT NULL THEN
      RETURN CASE WHEN p_batch.owner_user_id = v_user THEN 'admin' END;
    END IF;
    v_role := app_access('workspace', p_batch.workspace_id);
    IF v_role = 'owner' THEN
      RETURN 'admin';
    END IF;
    IF v_role = 'write' AND p_batch.deleted_by = v_user THEN
      RETURN 'deleter';
    END IF;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_role(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_task(p_task uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task  tasks%ROWTYPE;
    v_batch uuid;
    v_ids   uuid[];
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    PERFORM app_lock_task_rows(v_ids, NULL);
    -- 잠그는 사이 새로 생긴 하위 업무가 있으면 다시 잠그지 않는다(이미 쥔 열 뒤에 더 낮은 열을 잡으면 순서가 뒤집혀 교착할
    -- 수 있다). 드문 경우이므로 다시 하게 한다(409).
    IF EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task AND deleted_at IS NULL AND NOT (id = ANY (v_ids))) THEN
      RAISE EXCEPTION 'subtasks changed while deleting; try again' USING ERRCODE = '55000';
    END IF;
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- 잠근 뒤 다시 고른다. 기다리는 사이 다른 지우기가 끝낸 하위 업무는 이미 자기 묶음에 있고, 다른 트랜잭션이 떼어 낸
    -- 하위 업무(최상위로 올리거나 다른 상위로 옮김)는 더는 이 업무의 하위가 아니다.
    SELECT array_agg(id) INTO v_ids FROM tasks
    WHERE id = ANY (v_ids) AND (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_task.workspace_id, 'task', p_task, v_task.title, cardinality(v_ids), app_user_id())
    RETURNING id INTO v_batch;
    UPDATE tasks SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_ids);
    PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'delete', 'task', p_task,
                      jsonb_build_object('batch', v_batch, 'count', cardinality(v_ids)));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_task(p_task uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.scheduled_runs
    ADD CONSTRAINT scheduled_runs_pkey PRIMARY KEY (name);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
