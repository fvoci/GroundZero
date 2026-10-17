-- 휴지통 리뷰 반영 (D-39, D-41, D-57).
--
-- 1. 공개 링크는 지운 문서(또는 휴지통에 있는 항목의 문서)를 보여 주지 않는다.
-- 2. 항목이 휴지통에 있으면 그 안의 문서는 공유로도 볼 수 없다. 받은 공유 목록에도 볼 수 없는 문서는 빠진다.
-- 3. 문서를 지울 때 지우는 사람이 지울 수 없는 하위 문서는 함께 넣지 않고 부모 아래로 옮겨 남긴다.
-- 4. 업무를 지울 때 잠그는 사이 생긴 하위 업무도 함께 넣는다.
-- 5. 영구 삭제한 반복 회차는 다시 만들지 않는다(skipped_dates).
-- 6. 되살리기도 지우기와 같은 잠금 순서를 따르고, 항목이 휴지통·보관 중이면 409, 지운 사람이 지금 그 항목에
--    쓸 수 없으면 404.
-- 7. 예전 방식으로 지운 일정(묶음 없음)을 휴지통 묶음으로 옮긴다.

ALTER TABLE task_series ADD COLUMN skipped_dates date[] NOT NULL DEFAULT '{}';

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
      AND NOT EXISTS (SELECT 1 FROM items i WHERE i.id = d.item_id AND i.deleted_at IS NOT NULL)
  $$;

CREATE OR REPLACE FUNCTION app_shared_with_me()
  RETURNS TABLE (id uuid, title text, permission text, shared_by text, updated_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT d.id, d.title, s.permission, granter.display_name, d.updated_at
    FROM document_shares s
    JOIN users me ON me.id = app_user_id() AND me.email_verified AND lower(me.email) = s.grantee_email
    JOIN documents d ON d.id = s.document_id
    JOIN users granter ON granter.id = s.granted_by
    WHERE app_document_access(d.id) IS NOT NULL
    ORDER BY d.updated_at DESC
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
    -- 항목이 휴지통에 있으면 그 안의 문서는 공유로도 볼 수 없다.
    IF v_doc.item_id IS NOT NULL AND EXISTS (SELECT 1 FROM items WHERE id = v_doc.item_id AND deleted_at IS NOT NULL) THEN
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

CREATE OR REPLACE FUNCTION app_trash_document(p_doc uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_doc    documents%ROWTYPE;
    v_access text := app_document_access(p_doc);
    v_batch  uuid;
    v_count  integer;
    v_tree   uuid[];
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF NOT (v_access = 'owner' OR (v_access = 'write' AND v_doc.created_by = v_user)) THEN
      IF v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id)
         AND app_document_base_access(p_doc) IS NOT NULL THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'only the owner or the author can delete this document' USING ERRCODE = '42501';
    END IF;
    -- 트리 잠금 → 문서 행 → 항목 공유(문서 잠금 순서).
    PERFORM app_lock_tree(CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END,
                          v_doc.item_id, v_doc.owner_user_id);
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_lock_item(v_doc.item_id);
    IF app_document_access(p_doc) IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END, v_doc.owner_user_id,
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
        AND (app_document_access(d.id) = 'owner'
             OR (app_document_access(d.id) = 'write' AND d.created_by = v_user))
    )
    SELECT id FROM tree);
    UPDATE documents SET parent_id = v_doc.parent_id
    WHERE parent_id = ANY (v_tree) AND NOT (id = ANY (v_tree)) AND deleted_at IS NULL;
    UPDATE documents SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_tree);
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE trash_batches SET member_count = v_count WHERE id = v_batch;
    PERFORM app_audit_document(p_doc, 'delete', jsonb_build_object('batch', v_batch, 'count', v_count));
    RETURN v_batch;
  END
  $$;

CREATE OR REPLACE FUNCTION app_trash_task(p_task uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task  tasks%ROWTYPE;
    v_batch uuid;
    v_ids   uuid[];
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    PERFORM app_lock_task_rows(v_ids, NULL);
    -- 잠그는 사이 새로 생긴 하위 업무도 함께(드문 경우, 다시 잠근다).
    IF EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task AND deleted_at IS NULL AND NOT (id = ANY (v_ids))) THEN
      SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
      PERFORM app_lock_task_rows(v_ids, NULL);
    END IF;
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    INSERT INTO trash_batches (workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_task.workspace_id, 'task', p_task, v_task.title, cardinality(v_ids), app_user_id())
    RETURNING id INTO v_batch;
    UPDATE tasks SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_ids);
    PERFORM app_audit(v_task.workspace_id, 'delete', 'task', p_task,
                      jsonb_build_object('batch', v_batch, 'count', cardinality(v_ids)));
    RETURN v_batch;
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
    ELSE
      UPDATE events SET deleted_at = NULL, deleted_by = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'event', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    END IF;
    UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
  END
  $$;

CREATE OR REPLACE FUNCTION app_purge_tasks(p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

CREATE OR REPLACE FUNCTION app_sync_task_series_as(p_series uuid, p_system boolean) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_series task_series%ROWTYPE;
    v_root   tasks%ROWTYPE;
    v_today  date;
    v_date   date;
    v_id     uuid;
    v_made   integer := 0;
    v_user   uuid := CASE WHEN p_system THEN NULL ELSE app_user_id() END;
    v_module uuid;
    v_uid    uuid;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    PERFORM app_lock_task_rows(ARRAY[v_series.root_task_id], 'todo');
    SELECT * INTO v_root FROM tasks WHERE id = v_series.root_task_id;
    PERFORM app_lock_item(v_root.item_id);
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    -- 작업 프로세스는 만든 사람 이름으로 회차를 만들고 감사 기록에는 system으로 남긴다.
    v_user := coalesce(v_user, v_series.created_by);
    IF v_series.rule IS NULL OR (v_root.due_date IS NULL AND v_root.start_date IS NULL)
       OR v_root.deleted_at IS NOT NULL OR app_item_archived(v_root.item_id) THEN
      RETURN 0;
    END IF;
    v_today := (now() AT TIME ZONE v_series.timezone)::date;
    SELECT module_id INTO v_module FROM items WHERE id = v_root.item_id;
    FOR v_date IN
      WITH wanted AS (
        SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_series.effective_from, v_today)
        UNION
        (SELECT occurrence
         FROM app_recurrence_dates(v_series.rule, v_series.anchor, greatest(v_today + 1, v_series.effective_from),
                                   greatest(v_today + 1, v_series.effective_from) + 3660)
         ORDER BY occurrence LIMIT 1)
      )
      SELECT w.occurrence FROM wanted w
      WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = p_series AND t.occurrence_date = w.occurrence)
        AND NOT (w.occurrence = ANY (v_series.skipped_dates))
      ORDER BY w.occurrence
      LIMIT 50
    LOOP
      v_id := uuidv7();
      INSERT INTO tasks (id, workspace_id, item_id, number, title, description, priority, start_date, due_date,
                         estimate_minutes, tags, milestone, position, created_by, updated_by, request_id,
                         series_id, occurrence_date)
      VALUES (v_id, v_root.workspace_id, v_root.item_id,
              app_allocate_number(v_root.workspace_id, v_module, 'task', v_id),
              v_root.title, v_root.description, v_root.priority,
              v_root.start_date + (v_date - v_series.anchor), v_root.due_date + (v_date - v_series.anchor),
              v_root.estimate_minutes, v_root.tags, v_root.milestone, app_task_lane_end(v_root.item_id, 'todo'),
              v_user, v_user, uuidv7(), p_series, v_date)
      ON CONFLICT (series_id, occurrence_date) DO NOTHING;
      IF NOT FOUND THEN
        CONTINUE;
      END IF;
      INSERT INTO checklist_items (task_id, text, checked, position, request_id, created_by)
      SELECT v_id, c.text, false, c.position, uuidv7(), v_user FROM checklist_items c WHERE c.task_id = v_root.id;
      FOR v_uid IN SELECT a.user_id FROM task_assignees a WHERE a.task_id = v_root.id ORDER BY a.user_id LOOP
        IF EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_root.workspace_id AND m.user_id = v_uid)
           AND app_item_access_as(v_root.item_id, v_uid) IS NOT NULL THEN
          INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
        END IF;
      END LOOP;
      INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
      VALUES (v_root.workspace_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'create', 'task', v_id,
              jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'series', p_series,
                                 'occurrence', v_date)
                || CASE WHEN p_system THEN jsonb_build_object('system', true) ELSE '{}'::jsonb END);
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE OR REPLACE FUNCTION app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date)
  RETURNS TABLE (series_id uuid, root_task_id uuid, item_id uuid, title text, milestone boolean,
                 occurrence date, start_date date, due_date date)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT s.id, r.id, r.item_id, r.title, r.milestone, o.occurrence,
           r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)
    FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    CROSS JOIN LATERAL app_recurrence_dates(s.rule, s.anchor, greatest(s.effective_from, p_from - 400), p_to) o
    WHERE s.rule IS NOT NULL
      AND (p_workspace IS NULL OR s.workspace_id = p_workspace)
      AND (p_item IS NULL OR s.item_id = p_item)
      AND app_task_access(r.id) IS NOT NULL
      AND NOT app_item_archived(r.item_id)
      AND coalesce(r.due_date + (o.occurrence - s.anchor), r.start_date + (o.occurrence - s.anchor)) >= p_from
      AND coalesce(r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)) <= p_to
      AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = s.id AND t.occurrence_date = o.occurrence)
      AND NOT (o.occurrence = ANY (s.skipped_dates))
  $$;

-- 예전 방식으로 지운 일정을 휴지통 묶음으로.
DO $$
DECLARE
  v_event events%ROWTYPE;
  v_batch uuid;
BEGIN
  FOR v_event IN SELECT * FROM events WHERE deleted_at IS NOT NULL AND trash_batch_id IS NULL LOOP
    INSERT INTO trash_batches (workspace_id, root_kind, root_id, title, member_count, deleted_by, deleted_at)
    VALUES (v_event.workspace_id, 'event', v_event.id, v_event.title, 1,
            coalesce(v_event.deleted_by, v_event.updated_by), v_event.deleted_at)
    RETURNING id INTO v_batch;
    UPDATE events SET trash_batch_id = v_batch WHERE id = v_event.id;
  END LOOP;
END
$$;
