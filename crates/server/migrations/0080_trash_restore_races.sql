-- 휴지통·복원의 경합과 권한 재확인 (D-39·D-41, 코드 전반 감사 S1·S5·S7·S13).
--
-- 1. 상위 업무 지우기(`app_trash_task`): 잠그기 전에 고른 업무 목록을 잠근 뒤에도 그대로 써서, 기다리는 사이 다른
--    트랜잭션이 떼어 낸(최상위로 올리거나 다른 상위로 옮긴) 하위 업무까지 상위 업무의 묶음으로 휴지통에 넣었다. 잠근 뒤
--    "지금도 그 업무이거나 그 업무의 하위 업무인 것"만 남긴다(통합 시험으로 재현).
-- 2. 문서 지우기(`app_trash_document`): 트리 잠금을 기다리는 사이 문서가 다른 자리(워크스페이스·항목·개인 노트)로 옮겨지면
--    옛 자리 기준으로 묶음을 만들었다(옛 워크스페이스 휴지통, 옛 부모 아래로 남은 하위 문서 옮기기). 자리 바꾸기
--    (`app_set_document_parent`)와 같이 잠근 뒤 자리를 다시 보고, 바뀌었으면 55000(409)으로 다시 하게 한다. 소유·작성자
--    규칙도 잠근 뒤 다시 본다.
-- 3. 담당자·리더 정리(`app_drop_unseen_assignees`): 휴지통에 든 항목·모듈은 "지금 볼 수 없음"이 되어, 역할 변경(올림 포함)·
--    그룹 삭제·그룹에서 빼기만으로도 그 항목 업무의 담당자가 지워졌고 되살린 뒤에도 돌아오지 않았다. 휴지통 여부를 빼고
--    접근 규칙만 보는 `app_item_access_ignoring_trash`로 "정말 권한을 잃었는지"를 본다. 워크스페이스를 떠난 사람은
--    휴지통 항목에서도 빠지고, 리더도 같은 규칙으로 정리한다(전에는 휴지통 항목의 리더를 남겨 두어 떠난 사람이 되살린
--    항목의 리더로 남았다). `app_item_access`는 휴지통 확인 뒤 같은 규칙 함수를 불러 규칙을 한 곳에 둔다.
-- 4. 복원(`app_restore_batch`): 첨부 파일을 지운 사람이 되살릴 때 항목 권한만 다시 봐서, 항목이 없는 위키 문서(또는 공유가
--    철회된 문서)의 첨부를 볼 수 없는 문서에 되살렸다. 문서에 붙은 첨부는 그 문서에 쓸 수 있어야 한다(아니면 없는 것과 같다).
-- 함수 본문은 0053·0070·0071·0074의 마지막 정의와 같고 위 부분만 다르다.

-- 휴지통 여부를 보지 않는 항목 접근 규칙(관리자 → owner, 만든 사람 → 쓰기 역할이면 owner, 비공개는 항목 멤버만).
CREATE FUNCTION app_item_access_ignoring_trash(p_item uuid, p_user uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item items%ROWTYPE;
    v_role text;
  BEGIN
    IF p_user IS NULL THEN
      RETURN NULL;
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    IF v_item.id IS NULL THEN
      RETURN NULL;
    END IF;
    SELECT m.role INTO v_role
    FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
    WHERE m.workspace_id = v_item.workspace_id AND m.user_id = p_user;
    IF v_role IS NULL THEN
      RETURN NULL;
    END IF;
    IF v_role = 'admin' THEN
      RETURN 'owner';
    END IF;
    -- 만든 사람도 독자로 내려가면 읽기만 한다.
    IF v_item.created_by = p_user THEN
      RETURN CASE v_role WHEN 'writer' THEN 'owner' ELSE 'read' END;
    END IF;
    -- 비공개 항목은 항목 멤버(직접·그룹)만, 워크스페이스 역할 안에서.
    IF v_item.visibility <> 'workspace' AND NOT app_is_item_member(p_item, p_user) THEN
      RETURN NULL;
    END IF;
    RETURN CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
  END
  $$;

REVOKE ALL ON FUNCTION app_item_access_ignoring_trash(uuid, uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION app_item_access(p_item uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_item items%ROWTYPE;
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
    RETURN app_item_access_ignoring_trash(p_item, v_user);
  END
  $$;

CREATE OR REPLACE FUNCTION app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT (SELECT count(*) FROM task_assignees a JOIN tasks t ON t.id = a.task_id
            WHERE (p_item IS NULL OR t.item_id = p_item)
              AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
              AND (p_user IS NULL OR a.user_id = p_user))::integer
         + (SELECT count(*) FROM items i
            WHERE i.leader_id IS NOT NULL
              AND i.workspace_id = coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item))
              AND (p_item IS NULL OR i.id = p_item)
              AND (p_user IS NULL OR i.leader_id = p_user))::integer
  $$;

-- 바뀐 곳: 감사 기록 id를 p_ids에서 차례로 쓰고, 쓴 개수를 돌려준다(호출한 쪽이 다음 id를 이어 쓴다).
CREATE OR REPLACE FUNCTION app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[])
  RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row record;
    v_ws  uuid := coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item));
    v_at  integer := 0;
  BEGIN
    -- 같은 워크스페이스의 정리를 직렬화한다. 비공개 전환과 역할 강등이 동시에 일어나도 나중 쪽이 앞의 변경을
    -- 보고 정리한다(이 잠금 뒤의 조회는 앞 트랜잭션이 끝난 뒤의 상태를 본다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-assignees:' || v_ws::text, 0));
    FOR v_row IN
      SELECT a.task_id, a.user_id, t.workspace_id, t.item_id
      FROM task_assignees a JOIN tasks t ON t.id = a.task_id
      WHERE (p_item IS NULL OR t.item_id = p_item)
        AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
        AND (p_user IS NULL OR a.user_id = p_user)
    LOOP
      -- 0080: 휴지통이라 지금 볼 수 없는 것이 아니라 정말 권한을 잃었을 때만 뺀다.
      IF app_item_access_ignoring_trash(v_row.item_id, v_row.user_id) IS NULL THEN
        DELETE FROM task_assignees WHERE task_id = v_row.task_id AND user_id = v_row.user_id;
        IF FOUND THEN
          v_at := v_at + 1;
          PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unassign', 'task', v_row.task_id,
                            jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
        END IF;
      END IF;
    END LOOP;
    -- 리더도 그 항목을 볼 수 있는 사람이어야 한다(0080: 휴지통 항목도 같은 규칙, 휴지통이라는 이유로는 빼지 않는다).
    FOR v_row IN
      SELECT i.id, i.workspace_id, i.leader_id FROM items i
      WHERE i.leader_id IS NOT NULL AND i.workspace_id = v_ws
        AND (p_item IS NULL OR i.id = p_item)
        AND (p_user IS NULL OR i.leader_id = p_user)
      FOR UPDATE
    LOOP
      IF app_item_access_ignoring_trash(v_row.id, v_row.leader_id) IS NULL THEN
        UPDATE items SET leader_id = NULL WHERE id = v_row.id;
        v_at := v_at + 1;
        PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unset_leader', 'item', v_row.id,
                          jsonb_build_object('user', v_row.leader_id, 'reason', p_reason));
      END IF;
    END LOOP;
    RETURN v_at;
  END
  $$;


CREATE OR REPLACE FUNCTION app_trash_task(p_task uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
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
    -- 잠근 뒤 다시 고른다. 기다리는 사이 다른 지우기가 끝낸 하위 업무는 이미 자기 묶음에 있고, 다른 트랜잭션이 떼어 낸
    -- 하위 업무(최상위로 올리거나 다른 상위로 옮김)는 더는 이 업무의 하위가 아니다(0080).
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

CREATE OR REPLACE FUNCTION app_trash_document(p_doc uuid, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_doc    documents%ROWTYPE;
    v_seen   documents%ROWTYPE;
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
    -- 0080: 잠근 뒤 자리를 다시 본다. 기다리는 사이 다른 워크스페이스·항목·개인 노트로 옮겨졌으면 옛 자리 기준으로
    -- 묶음을 만들지 않고 다시 하게 한다(자리 바꾸기와 같은 규칙). 권한·작성자 규칙도 잠근 뒤의 값으로 다시 본다.
    v_seen := v_doc;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_item(v_doc.item_id);
    v_access := app_document_access(p_doc);
    IF v_access IS NULL THEN
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
        AND (app_document_access(d.id) = 'owner'
             OR (app_document_access(d.id) = 'write' AND d.created_by = v_user))
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

CREATE OR REPLACE FUNCTION app_restore_batch(p_batch uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_doc   documents%ROWTYPE;
    v_item  uuid;
    v_ids   uuid[];
    v_file_doc uuid;
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
    -- 0080: 문서에 붙은 첨부는 지운 사람이 지금도 그 문서에 쓸 수 있어야 되살린다(항목 없는 위키 문서·공유 철회 포함).
    IF v_batch.root_kind = 'file' AND v_file_doc IS NOT NULL AND app_trash_role(v_batch) = 'deleter'
       AND app_document_access(v_file_doc) IS DISTINCT FROM 'owner'
       AND app_document_access(v_file_doc) IS DISTINCT FROM 'write' THEN
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

REVOKE ALL ON FUNCTION app_unseen_assignees_bound(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_drop_unseen_assignees(uuid, uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_task(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_document(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_restore_batch(uuid, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_trash_task(uuid, uuid[]), app_trash_document(uuid, uuid[]), app_restore_batch(uuid, uuid[])
  TO fvoci_app;
