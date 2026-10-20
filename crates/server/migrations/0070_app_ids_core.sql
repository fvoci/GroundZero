-- 새 행의 id를 앱이 만든다(D-88). 지금까지는 표의 `DEFAULT uuidv7()`과 함수 안의 `uuidv7()`이 id를 만들었다.
-- PostgreSQL 17(셀프호스트 Supabase의 공식 DB 이미지)에는 내장 `uuidv7()`이 없고, 같은 함수를 DB에 따로 두는
-- 대신 id 생성을 앱(Rust `Uuid::now_v7()`)으로 옮긴다. 행을 만드는 함수는 마지막 인자 `p_ids uuid[]`로 앱이 만든
-- id를 받아 차례로 꺼내 쓴다(`app_id`). 모자라거나 UUIDv7이 아니면 멈춘다: 빈 id를 DB가 다른 값으로 채우지 않는다.
--
-- 만드는 행 수가 미리 정해지지 않는 함수(알림 대상, 권한을 잃은 담당자 정리, 복사 등)는 같은 트랜잭션에서 먼저
-- 부르는 `<함수>_ids` 함수가 필요한 개수의 상한을 알려 준다. 권한이 없으면 0을 돌려준다(본 함수가 권한 오류를 낸다).
--
-- 0070~0074는 도메인별로 새 서명의 함수를 옛 서명 옆에 만든다. 함수 본문은 마지막 정의와 같고 id 부분만 다르다.
-- 옛 서명·표의 기본값·제목 변경 트리거는 모든 호출을 옮긴 뒤 0075에서 지운다.
-- 이 파일: 공통 도우미, 계정·세션, 워크스페이스·멤버·그룹·초대·소유권, 워크스페이스 설정·지우기.

------------------------------------------------------------------------------------------------------------------
-- 공통 도우미
------------------------------------------------------------------------------------------------------------------

-- 앱이 넘긴 id 목록의 p_at번째. 두 가지 실패를 다른 코드로 낸다(서버가 다르게 다룬다):
-- - FV001 목록이 짧다: `<함수>_ids` 상한을 본 뒤 다른 트랜잭션이 대상을 늘렸을 수 있다(다시 하면 된다).
-- - FV002 목록이 잘못됐다: 빈 값, UUIDv7이 아닌 값, 1보다 작은 위치(서버 버그).
CREATE FUNCTION app_id(p_ids uuid[], p_at integer) RETURNS uuid
  LANGUAGE plpgsql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_at IS NULL OR p_at < 1 THEN
      RAISE EXCEPTION 'invalid new id position %', p_at USING ERRCODE = 'FV002';
    END IF;
    IF p_at > coalesce(cardinality(p_ids), 0) THEN
      RAISE EXCEPTION 'new id #% was not supplied by the application', p_at USING ERRCODE = 'FV001';
    END IF;
    IF p_ids[p_at] IS NULL OR uuid_extract_version(p_ids[p_at]) IS DISTINCT FROM 7 THEN
      RAISE EXCEPTION 'new ids must be UUIDv7' USING ERRCODE = 'FV002';
    END IF;
    RETURN p_ids[p_at];
  END
  $$;

-- id 목록의 p_from번째부터(다음 함수에 넘길 몫). 비어 있으면 빈 목록.
CREATE FUNCTION app_ids_from(p_ids uuid[], p_from integer) RETURNS uuid[]
  LANGUAGE sql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$ SELECT coalesce(p_ids[p_from:], '{}'::uuid[]) $$;

CREATE FUNCTION app_audit(p_id uuid, p_workspace uuid, p_action text, p_target_type text, p_target uuid,
                          p_detail jsonb) RETURNS void
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
    VALUES (p_id, p_workspace, app_user_id(), p_action, p_target_type, p_target, coalesce(p_detail, '{}'::jsonb))
  $$;

CREATE FUNCTION app_audit_document(p_id uuid, p_doc uuid, p_action text, p_detail jsonb) RETURNS void
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    SELECT p_id, d.workspace_id, d.owner_user_id, app_user_id(), p_action, 'document', d.id,
           coalesce(p_detail, '{}'::jsonb)
    FROM documents d WHERE d.id = p_doc
  $$;

CREATE FUNCTION app_notify(p_id uuid, p_user uuid, p_kind text, p_document uuid, p_task uuid, p_comment uuid,
                           p_actor uuid, p_detail jsonb) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_user IS NULL OR p_user = p_actor THEN
      RETURN;
    END IF;
    IF app_comment_target_access_as(p_document, p_task, p_user) IS NULL THEN
      RETURN;
    END IF;
    INSERT INTO notifications (id, user_id, kind, document_id, task_id, comment_id, actor, detail)
    VALUES (p_id, p_user, p_kind, p_document, p_task, p_comment, p_actor, coalesce(p_detail, '{}'::jsonb))
    ON CONFLICT DO NOTHING;
  END
  $$;

CREATE FUNCTION app_notify_item(p_id uuid, p_user uuid, p_kind text, p_item uuid, p_record uuid, p_detail jsonb)
  RETURNS boolean
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF p_user IS NULL OR app_item_access_as(p_item, p_user) IS NULL THEN
      RETURN false;
    END IF;
    INSERT INTO notifications (id, user_id, kind, item_id, record_id, actor, detail)
    VALUES (p_id, p_user, p_kind, p_item, p_record, NULL, p_detail)
    ON CONFLICT DO NOTHING;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count > 0;
  END
  $$;

-- 권한을 잃은 담당자·리더 정리가 남길 수 있는 감사 기록 수의 상한(아래 함수와 같은 대상, 권한 확인 전).
CREATE FUNCTION app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT (SELECT count(*) FROM task_assignees a JOIN tasks t ON t.id = a.task_id
            WHERE (p_item IS NULL OR t.item_id = p_item)
              AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
              AND (p_user IS NULL OR a.user_id = p_user))::integer
         + (SELECT count(*) FROM items i
            WHERE i.leader_id IS NOT NULL AND i.deleted_at IS NULL
              AND i.workspace_id = coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item))
              AND (p_item IS NULL OR i.id = p_item)
              AND (p_user IS NULL OR i.leader_id = p_user))::integer
  $$;

-- 바뀐 곳: 감사 기록 id를 p_ids에서 차례로 쓰고, 쓴 개수를 돌려준다(호출한 쪽이 다음 id를 이어 쓴다).
CREATE FUNCTION app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[])
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
      IF app_item_access_as(v_row.item_id, v_row.user_id) IS NULL THEN
        DELETE FROM task_assignees WHERE task_id = v_row.task_id AND user_id = v_row.user_id;
        IF FOUND THEN
          v_at := v_at + 1;
          PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unassign', 'task', v_row.task_id,
                            jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
        END IF;
      END IF;
    END LOOP;
    -- 리더도 그 항목을 볼 수 있는 사람이어야 한다. 휴지통의 항목은 되살릴 때를 위해 둔다.
    FOR v_row IN
      SELECT i.id, i.workspace_id, i.leader_id FROM items i
      WHERE i.leader_id IS NOT NULL AND i.deleted_at IS NULL AND i.workspace_id = v_ws
        AND (p_item IS NULL OR i.id = p_item)
        AND (p_user IS NULL OR i.leader_id = p_user)
      FOR UPDATE
    LOOP
      IF app_item_access_as(v_row.id, v_row.leader_id) IS NULL THEN
        UPDATE items SET leader_id = NULL WHERE id = v_row.id;
        v_at := v_at + 1;
        PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unset_leader', 'item', v_row.id,
                          jsonb_build_object('user', v_row.leader_id, 'reason', p_reason));
      END IF;
    END LOOP;
    RETURN v_at;
  END
  $$;

REVOKE ALL ON FUNCTION app_id(uuid[], integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_ids_from(uuid[], integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit(uuid, uuid, text, text, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit_document(uuid, uuid, text, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_notify(uuid, uuid, text, uuid, uuid, uuid, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_notify_item(uuid, uuid, text, uuid, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_unseen_assignees_bound(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_drop_unseen_assignees(uuid, uuid, uuid, text, uuid[]) FROM PUBLIC;

------------------------------------------------------------------------------------------------------------------
-- 계정·세션
------------------------------------------------------------------------------------------------------------------

-- id 1개(새 사용자일 때만 쓴다. 이미 있으면 그 id를 돌려준다).
CREATE FUNCTION app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean,
                                p_display_name text, p_ids uuid[]) RETURNS uuid
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO users (id, issuer, subject, email, email_verified, display_name)
    VALUES (app_id(p_ids, 1), p_issuer, p_subject, p_email, coalesce(p_email_verified, false), p_display_name)
    ON CONFLICT ON CONSTRAINT users_identity
      DO UPDATE SET email = EXCLUDED.email,
                    email_verified = EXCLUDED.email_verified,
                    display_name = EXCLUDED.display_name,
                    last_login_at = now()
    RETURNING id
  $$;

-- id 1개.
CREATE FUNCTION app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint,
                                   p_replaces bytea, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    -- 로그인 중이던 세션이 있으면 회전한다(이전 토큰 무효).
    IF p_replaces IS NOT NULL THEN
      DELETE FROM sessions WHERE token_hash = p_replaces;
    END IF;
    DELETE FROM sessions
    WHERE id IN (SELECT id FROM sessions WHERE expires_at < now() LIMIT 100);
    INSERT INTO sessions (id, token_hash, user_id, id_token, expires_at)
    VALUES (app_id(p_ids, 1), p_token_hash, p_user, p_id_token, now() + make_interval(secs => p_ttl_secs));
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 워크스페이스
------------------------------------------------------------------------------------------------------------------

-- id 2개: 워크스페이스, 감사 기록.
CREATE FUNCTION app_create_workspace(p_request_id uuid, p_name text, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'no current user' USING ERRCODE = '42501';
    END IF;

    INSERT INTO workspaces (id, name, created_by, owner_id, request_id)
    VALUES (app_id(p_ids, 1), btrim(p_name), v_user, v_user, p_request_id)
    ON CONFLICT ON CONSTRAINT workspaces_request DO NOTHING
    RETURNING id INTO v_id;

    IF v_id IS NULL THEN
      SELECT id INTO v_id FROM workspaces WHERE created_by = v_user AND request_id = p_request_id;
      RETURN v_id;
    END IF;

    INSERT INTO workspace_members (workspace_id, user_id, role) VALUES (v_id, v_user, 'admin');
    INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), v_id, v_user, 'create', 'workspace', v_id, jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  END
  $$;

-- id 3개: 요청 id, 워크스페이스, 감사 기록.
CREATE FUNCTION app_system_claim_initial_workspace(p_user uuid, p_name text, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_me text := current_setting('app.user_id', true);
    v_ws uuid;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN NULL;
    END IF;
    INSERT INTO instance_state (key, value) VALUES ('initial_admin', jsonb_build_object('user', p_user))
    ON CONFLICT (key) DO NOTHING;
    IF NOT FOUND THEN
      RETURN NULL;
    END IF;
    PERFORM set_config('app.user_id', p_user::text, true);
    v_ws := app_create_workspace(app_id(p_ids, 1), coalesce(nullif(btrim(p_name), ''), 'fvoci'),
                                 app_ids_from(p_ids, 2));
    PERFORM set_config('app.user_id', coalesce(v_me, ''), true);
    UPDATE instance_state SET value = value || jsonb_build_object('workspace', v_ws) WHERE key = 'initial_admin';
    RETURN v_ws;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_delete_workspace(p_ws uuid, p_name text, p_ids uuid[]) RETURNS timestamptz
  LANGUAGE plpgsql SECURITY DEFINER
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
    UPDATE invitations SET closed_at = now(), close_reason = 'workspace_deleted'
    WHERE workspace_id = p_ws AND closed_at IS NULL;
    GET DIAGNOSTICS v_closed = ROW_COUNT;
    UPDATE workspaces SET deleted_at = now(), deleted_by = v_user WHERE id = p_ws;
    PERFORM app_audit(app_id(p_ids, 1), p_ws, 'delete', 'workspace', p_ws,
                      jsonb_build_object('name', v_ws.name, 'invitations_closed', v_closed));
    RETURN now() + make_interval(days => v_ws.trash_retention_days);
  END
  $$;

-- id 1개.
CREATE FUNCTION app_restore_workspace(p_ws uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
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
    UPDATE invitations SET closed_at = NULL, close_reason = NULL
    WHERE workspace_id = p_ws AND close_reason = 'workspace_deleted' AND expires_at > now();
    PERFORM app_audit(app_id(p_ids, 1), p_ws, 'restore', 'workspace', p_ws, jsonb_build_object('name', v_ws.name));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_purge_workspace_as(p_ws uuid, p_system boolean, p_ids uuid[]) RETURNS boolean
  LANGUAGE plpgsql SECURITY DEFINER
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
    DELETE FROM module_keys WHERE workspace_id = p_ws;
    DELETE FROM modules WHERE workspace_id = p_ws;
    -- 묶음을 가리키는 모듈을 지운 뒤에 묶음을 지운다.
    DELETE FROM trash_batches WHERE workspace_id = p_ws;
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
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), NULL, v_ws.owner_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
    RETURN true;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_purge_workspace(p_ws uuid, p_ids uuid[]) RETURNS void
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$ SELECT app_purge_workspace_as(p_ws, false, p_ids) $$;

-- id 1개.
CREATE FUNCTION app_set_trash_retention(p_workspace uuid, p_days integer, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
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
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'set_trash_retention', 'workspace', p_workspace,
                      jsonb_build_object('from', v_old, 'to', p_days));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_set_version_retention(p_workspace uuid, p_days integer, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old integer;
  BEGIN
    IF app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_workspace) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can change version retention' USING ERRCODE = '42501';
    END IF;
    IF p_days IS NOT NULL AND p_days NOT BETWEEN 1 AND 3650 THEN
      RAISE EXCEPTION 'retention must be 1 to 3650 days' USING ERRCODE = '22023';
    END IF;
    SELECT version_retention_days INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET version_retention_days = p_days WHERE id = p_workspace;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'set_version_retention', 'workspace', p_workspace,
                      jsonb_build_object('from', v_old, 'to', p_days));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_set_workspace_realtime(p_workspace uuid, p_enabled boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old boolean;
  BEGIN
    IF app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_member_role(p_workspace) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can change realtime editing' USING ERRCODE = '42501';
    END IF;
    SELECT realtime_enabled INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET realtime_enabled = p_enabled WHERE id = p_workspace;
    IF v_old IS DISTINCT FROM p_enabled THEN
      PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'set_realtime', 'workspace', p_workspace,
                        jsonb_build_object('enabled', p_enabled));
    END IF;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_audit_workspace_export(p_workspace uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_member_role(p_workspace) IS DISTINCT FROM 'admin' THEN
      RAISE EXCEPTION 'only an admin can export the workspace' USING ERRCODE = '42501';
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'export', 'workspace', p_workspace, '{}'::jsonb);
  END
  $$;

-- id 1개.
CREATE FUNCTION app_audit_personal_export(p_ids uuid[]) RETURNS void
  LANGUAGE sql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    SELECT app_id(p_ids, 1), NULL, app_user_id(), app_user_id(), 'export', 'user', app_user_id(), '{}'::jsonb
    WHERE app_user_id() IS NOT NULL
  $$;

------------------------------------------------------------------------------------------------------------------
-- 초대·소유권
------------------------------------------------------------------------------------------------------------------

-- id 2개: 초대(새 초대일 때), 감사 기록.
CREATE FUNCTION app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[])
  RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_require_admin(p_workspace);
    v_email text := lower(btrim(p_email));
    v_id    uuid;
  BEGIN
    PERFORM app_lock_live_workspace(p_workspace);
    IF p_role NOT IN ('admin', 'writer', 'reader') THEN
      RAISE EXCEPTION 'invalid role' USING ERRCODE = '22023';
    END IF;
    IF position('@' in v_email) < 2 THEN
      RAISE EXCEPTION 'invalid email' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (
      SELECT 1 FROM workspace_members m JOIN users u ON u.id = m.user_id
      WHERE m.workspace_id = p_workspace AND u.email_verified AND lower(u.email) = v_email
    ) THEN
      RAISE EXCEPTION 'already a member' USING ERRCODE = '55000';
    END IF;
    -- 열린 초대가 있으면 역할·만료를 갱신한다(재초대).
    UPDATE invitations SET role = p_role, invited_by = v_user,
                           expires_at = now() + make_interval(secs => p_ttl_secs)
    WHERE workspace_id = p_workspace AND email = v_email AND closed_at IS NULL
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      INSERT INTO invitations (id, workspace_id, email, role, invited_by, expires_at)
      VALUES (app_id(p_ids, 1), p_workspace, v_email, p_role, v_user, now() + make_interval(secs => p_ttl_secs))
      RETURNING id INTO v_id;
    END IF;
    PERFORM app_audit(app_id(p_ids, 2), p_workspace, 'invite', 'invitation', v_id,
                      jsonb_build_object('email', v_email, 'role', p_role));
    RETURN v_id;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_respond_invitation(p_invitation uuid, p_accept boolean, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_inv  invitations%ROWTYPE;
    v_ok   boolean;
  BEGIN
    -- 지우기와 같은 잠금 순서(워크스페이스 → 초대). 지운 워크스페이스의 초대는 없는 것과 같다.
    PERFORM app_lock_live_workspace((SELECT workspace_id FROM invitations WHERE id = p_invitation));
    SELECT * INTO v_inv FROM invitations WHERE id = p_invitation FOR UPDATE;
    SELECT u.email_verified AND lower(u.email) = v_inv.email INTO v_ok FROM users u WHERE u.id = v_user;
    -- 대상이 아니면 초대의 존재를 알리지 않는다.
    IF v_user IS NULL OR v_inv.id IS NULL OR NOT coalesce(v_ok, false) THEN
      RAISE EXCEPTION 'invitation not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_inv.closed_at IS NOT NULL OR v_inv.expires_at <= now() THEN
      RAISE EXCEPTION 'invitation is no longer open' USING ERRCODE = '55000';
    END IF;
    IF p_accept THEN
      INSERT INTO workspace_members (workspace_id, user_id, role)
      VALUES (v_inv.workspace_id, v_user, v_inv.role)
      ON CONFLICT (workspace_id, user_id) DO NOTHING;
      UPDATE invitations SET closed_at = now(), close_reason = 'accepted',
                             accepted_by = v_user, accepted_at = now()
      WHERE id = p_invitation;
      INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_inv.workspace_id, v_user, 'join', 'member', v_user,
              jsonb_build_object('role', v_inv.role));
    ELSE
      UPDATE invitations SET closed_at = now(), close_reason = 'declined' WHERE id = p_invitation;
      INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_inv.workspace_id, v_user, 'decline', 'invitation', p_invitation, '{}'::jsonb);
    END IF;
    RETURN v_inv.workspace_id;
  END
  $$;

-- id 1개.
CREATE FUNCTION app_revoke_invitation(p_invitation uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM invitations WHERE id = p_invitation AND closed_at IS NULL;
    IF v_ws IS NULL THEN
      RAISE EXCEPTION 'invitation not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    UPDATE invitations SET closed_at = now(), close_reason = 'revoked' WHERE id = p_invitation;
    PERFORM app_audit(app_id(p_ids, 1), v_ws, 'revoke', 'invitation', p_invitation, '{}'::jsonb);
  END
  $$;

-- id 1개.
CREATE FUNCTION app_offer_ownership(p_workspace uuid, p_to uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
  BEGIN
    PERFORM app_lock_live_workspace(p_workspace);
    IF v_user <> (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'only the owner can transfer ownership' USING ERRCODE = '42501';
    END IF;
    IF p_to = v_user OR NOT EXISTS (
      SELECT 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_to
    ) THEN
      RAISE EXCEPTION 'the recipient must be another member' USING ERRCODE = '22023';
    END IF;
    INSERT INTO ownership_offers (workspace_id, from_user, to_user) VALUES (p_workspace, v_user, p_to)
    ON CONFLICT (workspace_id) DO UPDATE SET to_user = EXCLUDED.to_user, created_at = now();
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'offer_ownership', 'member', p_to, '{}'::jsonb);
  END
  $$;

-- id 1개.
CREATE FUNCTION app_accept_ownership(p_workspace uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_offer ownership_offers%ROWTYPE;
  BEGIN
    PERFORM app_lock_live_workspace(p_workspace);
    SELECT * INTO v_offer FROM ownership_offers WHERE workspace_id = p_workspace FOR UPDATE;
    IF v_user IS NULL OR v_offer.workspace_id IS NULL OR v_offer.to_user <> v_user
       OR app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'no offer for you' USING ERRCODE = 'P0002';
    END IF;
    UPDATE workspaces SET owner_id = v_user WHERE id = p_workspace;
    UPDATE workspace_members SET role = 'admin' WHERE workspace_id = p_workspace AND user_id = v_user;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'accept_ownership', 'member', v_user,
                      jsonb_build_object('from', v_offer.from_user));
  END
  $$;

-- id 1개.
CREATE FUNCTION app_cancel_ownership_offer(p_workspace uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
  BEGIN
    IF v_user <> (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'only the owner can cancel' USING ERRCODE = '42501';
    END IF;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'no pending offer' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'cancel_ownership_offer', 'workspace', p_workspace,
                      '{}'::jsonb);
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 멤버·그룹 (권한을 잃은 담당자 정리가 따라온다: 개수는 `_ids` 함수로)
------------------------------------------------------------------------------------------------------------------

-- id: 담당자 정리 몫(앞), 감사 기록 1개(뒤).
CREATE FUNCTION app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_used integer;
  BEGIN
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'the owner cannot leave or be removed; transfer ownership first' USING ERRCODE = '55000';
    END IF;
    DELETE FROM group_members gm USING groups g
    WHERE gm.group_id = g.id AND g.workspace_id = p_workspace AND gm.user_id = p_member;
    -- 항목 멤버도 끝난다. 다시 초대되면 비공개 항목은 다시 멤버로 넣어야 열린다.
    DELETE FROM item_members im USING items i
    WHERE im.item_id = i.id AND i.workspace_id = p_workspace AND im.user_id = p_member;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace AND to_user = p_member;
    -- 멤버 행을 먼저 잠근다: 동시에 담당자로 지정하는 쪽(멤버 행 공유 잠금)이 이 제거가 끝날 때까지 기다리게 한다.
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member FOR UPDATE;
    DELETE FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    v_used := app_drop_unseen_assignees(NULL, p_workspace, p_member, p_action, p_ids);
    PERFORM app_audit(app_id(p_ids, v_used + 1), p_workspace, p_action, 'member', p_member, '{}'::jsonb);
  END
  $$;

CREATE FUNCTION app_remove_member(p_workspace uuid, p_member uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_require_admin(p_workspace);
    PERFORM app_remove_member_internal(p_workspace, p_member, 'remove', p_ids);
  END
  $$;

CREATE FUNCTION app_remove_member_ids(p_workspace uuid, p_member uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_member_role(p_workspace) = 'admin'
                THEN 1 + app_unseen_assignees_bound(NULL, p_workspace, p_member) ELSE 0 END
  $$;

CREATE FUNCTION app_leave_workspace(p_workspace uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_remove_member_internal(p_workspace, app_user_id(), 'leave', p_ids);
  END
  $$;

CREATE FUNCTION app_leave_workspace_ids(p_workspace uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL AND app_member_role(p_workspace) IS NOT NULL
                THEN 1 + app_unseen_assignees_bound(NULL, p_workspace, app_user_id()) ELSE 0 END
  $$;

-- id: 감사 기록 1개(앞), 담당자 정리 몫(뒤).
CREATE FUNCTION app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old text;
  BEGIN
    PERFORM app_require_admin(p_workspace);
    IF p_role NOT IN ('admin', 'writer', 'reader') THEN
      RAISE EXCEPTION 'invalid role' USING ERRCODE = '22023';
    END IF;
    SELECT role INTO v_old FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member FOR UPDATE;
    IF v_old IS NULL THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) AND p_role <> 'admin' THEN
      RAISE EXCEPTION 'the owner must stay an admin' USING ERRCODE = '55000';
    END IF;
    UPDATE workspace_members SET role = p_role WHERE workspace_id = p_workspace AND user_id = p_member;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'change_role', 'member', p_member,
                      jsonb_build_object('from', v_old, 'to', p_role));
    PERFORM app_drop_unseen_assignees(NULL, p_workspace, p_member, 'role_change', app_ids_from(p_ids, 2));
  END
  $$;

CREATE FUNCTION app_set_member_role_ids(p_workspace uuid, p_member uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_member_role(p_workspace) = 'admin'
                THEN 1 + app_unseen_assignees_bound(NULL, p_workspace, p_member) ELSE 0 END
  $$;

-- id 2개: 그룹, 감사 기록.
CREATE FUNCTION app_create_group(p_workspace uuid, p_name text, p_ids uuid[]) RETURNS uuid
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
  BEGIN
    PERFORM app_require_admin(p_workspace);
    INSERT INTO groups (id, workspace_id, name) VALUES (app_id(p_ids, 1), p_workspace, btrim(p_name))
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), p_workspace, 'create', 'group', v_id, jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  EXCEPTION WHEN unique_violation THEN
    RAISE EXCEPTION 'a group with this name exists' USING ERRCODE = '55000';
  END
  $$;

-- id: 담당자 정리 몫(앞), 감사 기록 1개(뒤).
CREATE FUNCTION app_delete_group(p_group uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws   uuid;
    v_used integer;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    -- 멤버 행을 먼저 잠근다: 동시에 담당자로 지정하는 쪽(멤버 행 공유 잠금)이 정리가 끝날 때까지 기다린다.
    PERFORM 1 FROM workspace_members m JOIN group_members gm ON gm.user_id = m.user_id
    WHERE m.workspace_id = v_ws AND gm.group_id = p_group
    ORDER BY m.user_id
    FOR UPDATE OF m;
    DELETE FROM groups WHERE id = p_group;
    v_used := app_drop_unseen_assignees(NULL, v_ws, NULL, 'group_deleted', p_ids);
    PERFORM app_audit(app_id(p_ids, v_used + 1), v_ws, 'delete', 'group', p_group, '{}'::jsonb);
  END
  $$;

CREATE FUNCTION app_delete_group_ids(p_group uuid) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_member_role(g.workspace_id) = 'admin'
                THEN 1 + app_unseen_assignees_bound(NULL, g.workspace_id, NULL) ELSE 0 END
    FROM groups g WHERE g.id = p_group
  $$;

-- id: 넣을 때 감사 기록 1개. 뺄 때 담당자 정리 몫(앞), 감사 기록 1개(뒤).
CREATE FUNCTION app_set_group_member(p_group uuid, p_member uuid, p_present boolean, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws   uuid;
    v_used integer;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    IF p_present THEN
      IF NOT EXISTS (SELECT 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id = p_member) THEN
        RAISE EXCEPTION 'only workspace members can join a group' USING ERRCODE = '22023';
      END IF;
      INSERT INTO group_members (group_id, user_id) VALUES (p_group, p_member) ON CONFLICT DO NOTHING;
      PERFORM app_audit(app_id(p_ids, 1), v_ws, 'add_group_member', 'group', p_group,
                        jsonb_build_object('user', p_member));
    ELSE
      PERFORM 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id = p_member FOR UPDATE;
      DELETE FROM group_members WHERE group_id = p_group AND user_id = p_member;
      v_used := app_drop_unseen_assignees(NULL, v_ws, p_member, 'group_member_removed', p_ids);
      PERFORM app_audit(app_id(p_ids, v_used + 1), v_ws, 'remove_group_member', 'group', p_group,
                        jsonb_build_object('user', p_member));
    END IF;
  END
  $$;

CREATE FUNCTION app_set_group_member_ids(p_group uuid, p_member uuid, p_present boolean) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_member_role(g.workspace_id) IS DISTINCT FROM 'admin' THEN 0
                WHEN p_present THEN 1
                ELSE 1 + app_unseen_assignees_bound(NULL, g.workspace_id, p_member) END
    FROM groups g WHERE g.id = p_group
  $$;

------------------------------------------------------------------------------------------------------------------
-- 권한: 내부 도우미는 앱에 주지 않는다(옛 서명과 같음).
------------------------------------------------------------------------------------------------------------------

REVOKE ALL ON FUNCTION app_upsert_user(text, text, text, boolean, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_session(bytea, uuid, text, bigint, bytea, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_workspace(uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_claim_initial_workspace(uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_workspace(uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_restore_workspace(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_workspace_as(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_purge_workspace(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_trash_retention(uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_version_retention(uuid, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_workspace_realtime(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit_workspace_export(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit_personal_export(uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_invite(uuid, text, text, integer, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_respond_invitation(uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_revoke_invitation(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_offer_ownership(uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_accept_ownership(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_cancel_ownership_offer(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_member_internal(uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_member(uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_member_ids(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_leave_workspace(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_leave_workspace_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_member_role(uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_member_role_ids(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_group(uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_group(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_group_ids(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_group_member(uuid, uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_group_member_ids(uuid, uuid, boolean) FROM PUBLIC;

GRANT EXECUTE ON FUNCTION
  app_upsert_user(text, text, text, boolean, text, uuid[]),
  app_create_session(bytea, uuid, text, bigint, bytea, uuid[]),
  app_create_workspace(uuid, text, uuid[]),
  app_system_claim_initial_workspace(uuid, text, uuid[]),
  app_delete_workspace(uuid, text, uuid[]),
  app_restore_workspace(uuid, uuid[]),
  app_purge_workspace(uuid, uuid[]),
  app_set_trash_retention(uuid, integer, uuid[]),
  app_set_version_retention(uuid, integer, uuid[]),
  app_set_workspace_realtime(uuid, boolean, uuid[]),
  app_audit_workspace_export(uuid, uuid[]),
  app_audit_personal_export(uuid[]),
  app_invite(uuid, text, text, integer, uuid[]),
  app_respond_invitation(uuid, boolean, uuid[]),
  app_revoke_invitation(uuid, uuid[]),
  app_offer_ownership(uuid, uuid, uuid[]),
  app_accept_ownership(uuid, uuid[]),
  app_cancel_ownership_offer(uuid, uuid[]),
  app_remove_member(uuid, uuid, uuid[]),
  app_remove_member_ids(uuid, uuid),
  app_leave_workspace(uuid, uuid[]),
  app_leave_workspace_ids(uuid),
  app_set_member_role(uuid, uuid, text, uuid[]),
  app_set_member_role_ids(uuid, uuid),
  app_create_group(uuid, text, uuid[]),
  app_delete_group(uuid, uuid[]),
  app_delete_group_ids(uuid),
  app_set_group_member(uuid, uuid, boolean, uuid[]),
  app_set_group_member_ids(uuid, uuid, boolean)
TO fvoci_app;
