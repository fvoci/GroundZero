-- 워크스페이스 삭제 (D-51, 보존 기간 D-41).
--
-- 소유자만, 이름을 다시 입력해 확인한 뒤 지운다. 지우면 곧바로 모든 멤버에게 없는 것과 같다(멤버 판정
-- `app_member_role`·`app_my_workspace_ids`·`app_shares_workspace`가 지운 워크스페이스를 빼므로 그 위의 모든 접근
-- 함수·RLS·검색 거르기가 함께 닫힌다). 받은 공유와 공개 링크도 닫는다. 열린 초대는 취소하고, 소유권 인계가
-- 진행 중이면 먼저 취소해야 한다(409). 소유자는 보존 기간(그 워크스페이스의 휴지통 보존 일수) 안에 되살리거나
-- 바로 영구 삭제할 수 있고, 기간이 지나면 백그라운드 작업이 영구 삭제한다.
-- 영구 삭제는 워크스페이스의 모든 내용을 지운다. 개인 시간 기록은 남고(워크스페이스 연결만 끊김) 감사 기록은
-- 워크스페이스 연결을 끊고 소유자 영역에 남긴다.

ALTER TABLE workspaces ADD COLUMN deleted_at timestamptz;
ALTER TABLE workspaces ADD COLUMN deleted_by uuid REFERENCES users (id);
CREATE INDEX workspaces_deleted ON workspaces (deleted_at) WHERE deleted_at IS NOT NULL;

CREATE OR REPLACE FUNCTION app_member_role(ws uuid) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT m.role FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
    WHERE m.workspace_id = ws AND m.user_id = app_user_id()
  $$;

CREATE OR REPLACE FUNCTION app_my_workspace_ids() RETURNS SETOF uuid
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT m.workspace_id FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
    WHERE m.user_id = app_user_id()
  $$;

CREATE OR REPLACE FUNCTION app_shares_workspace(other uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (
      SELECT 1
      FROM workspace_members mine
      JOIN workspace_members theirs ON theirs.workspace_id = mine.workspace_id
      JOIN workspaces w ON w.id = mine.workspace_id AND w.deleted_at IS NULL
      WHERE mine.user_id = app_user_id() AND theirs.user_id = other
    )
  $$;

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
      AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = d.workspace_id AND w.deleted_at IS NOT NULL)
  $$;

-- 받은 공유도 닫는다.
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
    -- 워크스페이스를 지웠으면 그 안의 문서는 공유로도 볼 수 없다.
    IF v_doc.workspace_id IS NOT NULL AND EXISTS (SELECT 1 FROM workspaces WHERE id = v_doc.workspace_id AND deleted_at IS NOT NULL) THEN
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

-- 지우기(소유자, 이름 확인). 보존 기간이 끝나는 때를 돌려준다.
CREATE FUNCTION app_delete_workspace(p_ws uuid, p_name text) RETURNS timestamptz
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_ws     workspaces%ROWTYPE;
    v_closed integer;
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR NO KEY UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NOT NULL OR app_member_role(p_ws) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_ws.owner_id IS DISTINCT FROM v_user THEN
      RAISE EXCEPTION 'only the owner can delete the workspace' USING ERRCODE = '42501';
    END IF;
    IF btrim(coalesce(p_name, '')) IS DISTINCT FROM btrim(v_ws.name) THEN
      RAISE EXCEPTION 'type the workspace name to confirm' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM ownership_offers WHERE workspace_id = p_ws) THEN
      RAISE EXCEPTION 'cancel the ownership transfer first' USING ERRCODE = '55000';
    END IF;
    UPDATE invitations SET closed_at = now(), close_reason = 'revoked'
    WHERE workspace_id = p_ws AND closed_at IS NULL;
    GET DIAGNOSTICS v_closed = ROW_COUNT;
    UPDATE workspaces SET deleted_at = now(), deleted_by = v_user WHERE id = p_ws;
    PERFORM app_audit(p_ws, 'delete', 'workspace', p_ws,
                      jsonb_build_object('name', v_ws.name, 'invitations_closed', v_closed));
    RETURN now() + make_interval(days => v_ws.trash_retention_days);
  END
  $$;

-- 내가 소유한 지운 워크스페이스.
CREATE FUNCTION app_my_deleted_workspaces()
  RETURNS TABLE (id uuid, name text, deleted_at timestamptz, expires_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT w.id, w.name, w.deleted_at, w.deleted_at + make_interval(days => w.trash_retention_days)
    FROM workspaces w
    WHERE w.owner_id = app_user_id() AND w.deleted_at IS NOT NULL
    ORDER BY w.deleted_at DESC, w.id
  $$;

-- 되살리기(소유자, 보존 기간 안).
CREATE FUNCTION app_restore_workspace(p_ws uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws workspaces%ROWTYPE;
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR NO KEY UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NULL OR v_ws.owner_id IS DISTINCT FROM app_user_id() THEN
      RAISE EXCEPTION 'deleted workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_ws.deleted_at + make_interval(days => v_ws.trash_retention_days) <= now() THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
    END IF;
    UPDATE workspaces SET deleted_at = NULL, deleted_by = NULL WHERE id = p_ws;
    PERFORM app_audit(p_ws, 'restore', 'workspace', p_ws, jsonb_build_object('name', v_ws.name));
  END
  $$;

-- 영구 삭제. 사용자(소유자)는 언제든, 작업 프로세스는 보존 기간이 지난 것만. 지웠으면 true.
CREATE FUNCTION app_purge_workspace_as(p_ws uuid, p_system boolean) RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws    workspaces%ROWTYPE;
    v_items uuid[];
  BEGIN
    -- 새 내용이 이 워크스페이스를 가리키며 들어오지 못하게 행을 잠근다(외래 키 공유 잠금과 충돌).
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NULL
       OR (NOT p_system AND v_ws.owner_id IS DISTINCT FROM app_user_id()) THEN
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
    DELETE FROM trash_batches WHERE workspace_id = p_ws;
    DELETE FROM module_keys WHERE workspace_id = p_ws;
    DELETE FROM modules WHERE workspace_id = p_ws;
    DELETE FROM numbered_targets WHERE workspace_id = p_ws;
    DELETE FROM reference_counters WHERE workspace_id = p_ws;
    DELETE FROM groups WHERE workspace_id = p_ws;
    DELETE FROM invitations WHERE workspace_id = p_ws;
    DELETE FROM ownership_offers WHERE workspace_id = p_ws;
    DELETE FROM time_sharing WHERE workspace_id = p_ws;
    -- 개인 시간 기록은 남는다(대상 이름만 사라진다).
    UPDATE time_entries SET workspace_id = NULL WHERE workspace_id = p_ws;
    -- 감사 기록은 소유자 영역으로 옮겨 남긴다.
    UPDATE audit_log SET workspace_id = NULL, owner_user_id = coalesce(owner_user_id, v_ws.owner_id)
    WHERE workspace_id = p_ws;
    DELETE FROM workspace_members WHERE workspace_id = p_ws;
    DELETE FROM workspaces WHERE id = p_ws;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (NULL, v_ws.owner_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge', 'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
    RETURN true;
  END
  $$;

CREATE FUNCTION app_purge_workspace(p_ws uuid) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT app_purge_workspace_as(p_ws, false) $$;

-- 작업: 만료된 휴지통 묶음과 만료된 지운 워크스페이스를 영구 삭제한다. 하나가 실패해도 계속한다.
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
        v_made := v_made + 1;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of trash batch % failed: %', v_batch.id, SQLERRM;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

-- 작업: 지운 워크스페이스의 반복은 건너뛴다.
CREATE OR REPLACE FUNCTION app_system_recurrence_tick() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_id IN
      SELECT s.id FROM task_series s
      JOIN tasks r ON r.id = s.root_task_id
      JOIN items i ON i.id = s.item_id
      JOIN workspaces w ON w.id = s.workspace_id
      WHERE s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL AND w.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      BEGIN
        v_made := v_made + app_sync_task_series_as(v_id, true);
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'recurrence sync of series % failed: %', v_id, SQLERRM;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

REVOKE ALL ON FUNCTION app_delete_workspace(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_my_deleted_workspaces() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_restore_workspace(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_workspace_as(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_workspace(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_delete_workspace(uuid, text),
  app_my_deleted_workspaces(),
  app_restore_workspace(uuid),
  app_purge_workspace(uuid)
TO fvoci_app;
