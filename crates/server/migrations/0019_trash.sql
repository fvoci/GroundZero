-- 휴지통·복원·영구 삭제 (D-39, D-41, D-57).
--
-- 지우면 휴지통 묶음(trash_batches) 하나가 생기고 함께 사라질 것들이 그 묶음에 든다: 문서는 하위 문서까지,
-- 업무는 하위 업무까지, 항목은 항목 하나(그 안의 업무·문서·일정은 항목이 안 보이면 함께 안 보인다), 일정은 하나.
-- 복원·영구 삭제는 묶음 단위다(부모 없이 자식만 돌아오는 상태가 생기지 않게).
-- 지운 것은 접근 함수가 없는 것으로 보므로 목록·참조·역방향 링크·달력·홈·검색 후보에 나오지 않는다.
-- 보존 기간은 워크스페이스마다 관리자가 정한다(기본 30일, D-41). 기간이 지난 묶음은 되살릴 수 없고 "만료"로
-- 남는다. 실제로 지우는 일(영구 삭제)은 관리자(개인 노트는 소유자)가 직접 한다. 기간이 지나면 저절로 지우는
-- 일은 백그라운드 작업 구조(O-19)가 정해진 뒤에 한다. 영구 삭제해도 번호는 다시 쓰지 않는다(D-17).
-- 볼 수 있는 사람: 워크스페이스 휴지통은 관리자는 전부, 그 밖에는 자기가 지운 것만(아직 작성자 이상일 때).
-- 개인 노트는 소유자만. 되살리기도 같은 사람, 영구 삭제는 관리자(개인 노트는 소유자).

ALTER TABLE workspaces ADD COLUMN trash_retention_days integer NOT NULL DEFAULT 30
  CHECK (trash_retention_days BETWEEN 1 AND 3650);

CREATE TABLE trash_batches (
  id            uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id  uuid REFERENCES workspaces (id),
  owner_user_id uuid REFERENCES users (id),
  root_kind     text NOT NULL CHECK (root_kind IN ('item', 'document', 'task', 'event')),
  root_id       uuid NOT NULL,
  title         text NOT NULL,
  member_count  integer NOT NULL,
  deleted_by    uuid NOT NULL REFERENCES users (id),
  deleted_at    timestamptz NOT NULL DEFAULT now(),
  restored_at   timestamptz,
  purged_at     timestamptz,
  CONSTRAINT trash_batches_place CHECK ((workspace_id IS NULL) <> (owner_user_id IS NULL))
);
CREATE INDEX trash_batches_workspace ON trash_batches (workspace_id, deleted_at) WHERE restored_at IS NULL AND purged_at IS NULL;
CREATE INDEX trash_batches_owner ON trash_batches (owner_user_id, deleted_at) WHERE restored_at IS NULL AND purged_at IS NULL;

ALTER TABLE documents ADD COLUMN deleted_at timestamptz;
ALTER TABLE documents ADD COLUMN trash_batch_id uuid REFERENCES trash_batches (id);
ALTER TABLE items ADD COLUMN deleted_at timestamptz;
ALTER TABLE items ADD COLUMN trash_batch_id uuid REFERENCES trash_batches (id);
ALTER TABLE tasks ADD COLUMN deleted_at timestamptz;
ALTER TABLE tasks ADD COLUMN trash_batch_id uuid REFERENCES trash_batches (id);
ALTER TABLE events ADD COLUMN trash_batch_id uuid REFERENCES trash_batches (id);
CREATE INDEX documents_trash ON documents (trash_batch_id) WHERE trash_batch_id IS NOT NULL;
CREATE INDEX items_trash ON items (trash_batch_id) WHERE trash_batch_id IS NOT NULL;
CREATE INDEX tasks_trash ON tasks (trash_batch_id) WHERE trash_batch_id IS NOT NULL;
CREATE INDEX events_trash ON events (trash_batch_id) WHERE trash_batch_id IS NOT NULL;
-- 홈의 "최근 작업"(내가 마지막으로 고친 것) 조회용.
CREATE INDEX tasks_updated_by ON tasks (updated_by, updated_at DESC);
CREATE INDEX documents_updated_by ON documents (updated_by, updated_at DESC);

------------------------------------------------------------------------------
-- 접근 함수: 지운 것은 없는 것과 같다.
------------------------------------------------------------------------------

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
    IF v_item.visibility <> 'workspace' THEN
      RETURN NULL;
    END IF;
    RETURN CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
  END
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

CREATE OR REPLACE FUNCTION app_task_access(p_task uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   uuid;
    v_access text;
  BEGIN
    SELECT item_id INTO v_item FROM tasks WHERE id = p_task AND deleted_at IS NULL;
    IF v_item IS NULL THEN
      RETURN NULL;
    END IF;
    v_access := app_item_access(v_item);
    IF v_access IS NOT NULL AND v_access <> 'read' AND app_item_archived(v_item) THEN
      RETURN 'read';
    END IF;
    RETURN v_access;
  END
  $$;

------------------------------------------------------------------------------
-- 지우기(휴지통으로)
------------------------------------------------------------------------------

-- 문서와 하위 문서. 문서 소유 권한(관리자·개인 노트 소유자)이나, 쓰기 권한이 있고 내가 만든 문서만.
CREATE FUNCTION app_trash_document(p_doc uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_doc    documents%ROWTYPE;
    v_access text := app_document_access(p_doc);
    v_batch  uuid;
    v_count  integer;
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
    WITH RECURSIVE tree(id) AS (
      SELECT p_doc
      UNION
      SELECT d.id FROM documents d JOIN tree t ON d.parent_id = t.id WHERE d.deleted_at IS NULL
    )
    UPDATE documents SET deleted_at = now(), trash_batch_id = v_batch WHERE id IN (SELECT id FROM tree);
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE trash_batches SET member_count = v_count WHERE id = v_batch;
    PERFORM app_audit_document(p_doc, 'delete', jsonb_build_object('batch', v_batch, 'count', v_count));
    RETURN v_batch;
  END
  $$;

-- 업무와 하위 업무(업무 쓰기 권한).
CREATE FUNCTION app_trash_task(p_task uuid) RETURNS uuid
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

-- 항목(항목 소유 권한). 그 안의 업무·문서·일정은 항목과 함께 보이지 않는다.
CREATE FUNCTION app_trash_item(p_item uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item  items%ROWTYPE;
    v_batch uuid;
  BEGIN
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_access(p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can delete it' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF v_item.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_item.workspace_id, 'item', p_item, v_item.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE items SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_item;
    PERFORM app_audit(v_item.workspace_id, 'delete', 'item', p_item, jsonb_build_object('batch', v_batch));
    RETURN v_batch;
  END
  $$;

-- 일정 지우기는 휴지통 묶음을 만든다(되살릴 수 있게).
CREATE OR REPLACE FUNCTION app_delete_event(p_event uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
    v_batch uuid;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    INSERT INTO trash_batches (workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_event.workspace_id, 'event', p_event, v_event.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE events SET deleted_at = now(), deleted_by = app_user_id(), trash_batch_id = v_batch WHERE id = p_event;
    PERFORM app_audit(v_event.workspace_id, 'delete', 'event', p_event,
                      jsonb_build_object('title', v_event.title, 'batch', v_batch));
  END
  $$;

------------------------------------------------------------------------------
-- 휴지통 보기·되살리기·영구 삭제
------------------------------------------------------------------------------

-- 묶음을 다룰 수 있는지: 'admin'(전부), 'deleter'(자기가 지운 것), NULL(볼 수 없음).
CREATE FUNCTION app_trash_role(p_batch trash_batches) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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
    v_role := app_member_role(p_batch.workspace_id);
    IF v_role = 'admin' THEN
      RETURN 'admin';
    END IF;
    IF v_role = 'writer' AND p_batch.deleted_by = v_user THEN
      RETURN 'deleter';
    END IF;
    RETURN NULL;
  END
  $$;

-- 묶음이 만료됐는지(워크스페이스의 지금 보존 기간 기준, 개인 노트는 기본 30일).
CREATE FUNCTION app_trash_expires_at(p_batch trash_batches) RETURNS timestamptz
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT p_batch.deleted_at + make_interval(days => coalesce(
      (SELECT trash_retention_days FROM workspaces WHERE id = p_batch.workspace_id), 30))
  $$;

-- 내가 볼 수 있는 휴지통 묶음(워크스페이스, 또는 NULL이면 내 개인 노트).
CREATE FUNCTION app_trash_list(p_workspace uuid)
  RETURNS TABLE (id uuid, root_kind text, root_id uuid, title text, member_count integer, deleted_by uuid,
                 deleted_by_name text, deleted_at timestamptz, expires_at timestamptz, can_purge boolean)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT b.id, b.root_kind, b.root_id, b.title, b.member_count, b.deleted_by, u.display_name, b.deleted_at,
           app_trash_expires_at(b), app_trash_role(b) = 'admin'
    FROM trash_batches b LEFT JOIN users u ON u.id = b.deleted_by
    WHERE b.restored_at IS NULL AND b.purged_at IS NULL
      AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
      AND app_trash_role(b) IS NOT NULL
    ORDER BY b.deleted_at DESC, b.id
  $$;

-- 되살리기. 만료됐으면 409. 부모가 지워져 있는 문서·업무는 최상위로 돌아온다.
CREATE FUNCTION app_restore_batch(p_batch uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_trash_expires_at(v_batch) <= now() THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
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

-- 문서 행들을 실제로 지운다(연결된 리비전·링크·언급·알림·공유·공개 링크도). 남는 하위 문서는 최상위로.
CREATE FUNCTION app_purge_documents(p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

-- 업무 행들을 실제로 지운다(담당·체크리스트·선행 관계·반복 정보도). 시간 기록은 남고 대상 이름만 사라진다.
CREATE FUNCTION app_purge_tasks(p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE tasks SET parent_id = NULL WHERE parent_id = ANY (p_ids) AND NOT (id = ANY (p_ids));
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

-- 영구 삭제(관리자, 개인 노트는 소유자). 만료 전에도 할 수 있다. 번호는 다시 쓰지 않는다.
CREATE FUNCTION app_purge_batch(p_batch uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_item  uuid;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_trash_role(v_batch) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can delete permanently' USING ERRCODE = '42501';
    END IF;
    IF v_batch.root_kind = 'document' THEN
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'event' THEN
      DELETE FROM events WHERE trash_batch_id = p_batch;
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
        AND root_kind <> 'item';
      DELETE FROM items WHERE id = v_item;
    END IF;
    UPDATE trash_batches SET purged_at = now() WHERE id = p_batch;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_batch.workspace_id, v_batch.owner_user_id, app_user_id(), 'purge', v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count));
  END
  $$;

-- 만료된 묶음 비우기(관리자). 비운 수.
CREATE FUNCTION app_purge_expired(p_workspace uuid) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      PERFORM app_purge_batch(v_id);
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 휴지통 보존 기간(관리자, 1~3650일).
CREATE FUNCTION app_set_trash_retention(p_workspace uuid, p_days integer) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old integer;
  BEGIN
    IF app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_workspace) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can change the retention period' USING ERRCODE = '42501';
    END IF;
    IF p_days NOT BETWEEN 1 AND 3650 THEN
      RAISE EXCEPTION 'retention must be 1 to 3650 days' USING ERRCODE = '22023';
    END IF;
    SELECT trash_retention_days INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET trash_retention_days = p_days WHERE id = p_workspace;
    PERFORM app_audit(p_workspace, 'set_trash_retention', 'workspace', p_workspace,
                      jsonb_build_object('from', v_old, 'to', p_days));
  END
  $$;

-- 반복 동기화는 첫 회차가 지워진 반복을 건너뛴다(항목 전체 동기화가 그 하나 때문에 실패하지 않게).
CREATE OR REPLACE FUNCTION app_sync_item_series(p_item uuid) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_item_access(p_item);
    v_id     uuid;
    v_made   integer := 0;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_lane(p_item, 'done');
    PERFORM app_lock_lane(p_item, 'in_progress');
    PERFORM app_lock_lane(p_item, 'todo');
    FOR v_id IN
      SELECT s.id FROM task_series s JOIN tasks r ON r.id = s.root_task_id
      WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      v_made := v_made + app_sync_task_series(v_id);
    END LOOP;
    RETURN v_made;
  END
  $$;

------------------------------------------------------------------------------
-- 홈 리뷰 반영
------------------------------------------------------------------------------

CREATE INDEX tasks_item_updated ON tasks (item_id, updated_at DESC);
CREATE INDEX tasks_due_open ON tasks (due_date) WHERE status <> 'done' AND due_date IS NOT NULL;

-- 고정·즐겨찾기 상한은 지금 볼 수 있는 대상만 센다(볼 수 없게 된 표시가 자리를 막지 않게). 이미 있으면 그대로.
CREATE OR REPLACE FUNCTION app_set_mark(p_mark text, p_kind text, p_id uuid, p_on boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    IF p_mark NOT IN ('pin', 'favorite') OR p_kind NOT IN ('item', 'document', 'task') THEN
      RAISE EXCEPTION 'invalid mark' USING ERRCODE = '22023';
    END IF;
    IF NOT p_on THEN
      DELETE FROM user_marks WHERE user_id = v_user AND mark = p_mark AND target_kind = p_kind AND target_id = p_id;
      RETURN;
    END IF;
    IF NOT app_target_visible(p_kind, p_id) THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-marks:' || v_user::text, 0));
    IF EXISTS (SELECT 1 FROM user_marks WHERE user_id = v_user AND mark = p_mark AND target_kind = p_kind
               AND target_id = p_id) THEN
      RETURN;
    END IF;
    IF (SELECT count(*) FROM user_marks m WHERE m.user_id = v_user AND m.mark = p_mark
        AND app_target_visible(m.target_kind, m.target_id)) >= 100 THEN
      RAISE EXCEPTION 'too many marks' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_marks (user_id, mark, target_kind, target_id) VALUES (v_user, p_mark, p_kind, p_id)
    ON CONFLICT DO NOTHING;
  END
  $$;

CREATE OR REPLACE FUNCTION app_shift_excluded_dates() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old date := coalesce(OLD.start_date, (OLD.starts_at AT TIME ZONE OLD.timezone)::date);
    v_new date := coalesce(NEW.start_date, (NEW.starts_at AT TIME ZONE NEW.timezone)::date);
  BEGIN
    IF cardinality(NEW.excluded_dates) > 0 AND v_old IS DISTINCT FROM v_new AND NEW.recurrence IS NOT NULL
       AND (NEW.recurrence ->> 'freq' = 'daily'
            OR (NEW.recurrence ->> 'freq' = 'weekly' AND coalesce(jsonb_array_length(NEW.recurrence -> 'weekdays'), 0) = 0)) THEN
      NEW.excluded_dates := ARRAY(SELECT d + (v_new - v_old) FROM unnest(NEW.excluded_dates) d);
    END IF;
    RETURN NEW;
  END
  $$;

ALTER TABLE trash_batches ENABLE ROW LEVEL SECURITY;
-- 목록은 함수(app_trash_list)로만 본다.
CREATE POLICY trash_batches_none ON trash_batches FOR SELECT USING (false);

REVOKE ALL ON FUNCTION app_trash_document(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_task(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_item(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_role(trash_batches) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_expires_at(trash_batches) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_list(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_restore_batch(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_documents(uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_tasks(uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_batch(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_expired(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_trash_retention(uuid, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_trash_document(uuid),
  app_trash_task(uuid),
  app_trash_item(uuid),
  app_trash_list(uuid),
  app_restore_batch(uuid),
  app_purge_batch(uuid),
  app_purge_expired(uuid),
  app_set_trash_retention(uuid, integer)
TO fvoci_app;
