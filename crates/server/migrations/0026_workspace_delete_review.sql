-- 워크스페이스 삭제 리뷰 반영 (D-51, 0025 보완).
--
-- 1. 초대·초대 수락·소유권 제안·수락은 먼저 워크스페이스 행을 잠그고 지워지지 않았는지 다시 본다. 지우기와 같은
--    잠금 순서(워크스페이스 → 초대·제안)라, 지우는 동안 새 초대가 생기거나 인계가 끝나 지운 워크스페이스의
--    소유자·멤버가 바뀌지 않는다.
-- 2. 받은 초대 목록에서 지운 워크스페이스를 뺀다.
-- 3. 지울 때 닫은 초대는 따로 표시(workspace_deleted)하고, 되살리면 기한이 남은 것을 다시 연다.
--
-- 참고: 영구 삭제는 워크스페이스 행을 먼저 잠근다. 지우기 전에 시작해 아직 끝나지 않은 쓰기가 있으면 교착으로
-- 한쪽이 취소될 수 있다(사용자는 409 "다시 시도", 작업 프로세스는 경고 후 다음 주기에 다시). 데이터는 그대로다.

ALTER TABLE invitations DROP CONSTRAINT invitations_close_reason_check;
ALTER TABLE invitations ADD CONSTRAINT invitations_close_reason_check
  CHECK (close_reason IN ('accepted', 'declined', 'revoked', 'workspace_deleted'));

-- 살아 있는 워크스페이스 행을 잠근다. 없거나 지웠으면 404.
CREATE FUNCTION app_lock_live_workspace(p_ws uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM 1 FROM workspaces WHERE id = p_ws AND deleted_at IS NULL FOR NO KEY UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
  END
  $$;
REVOKE ALL ON FUNCTION app_lock_live_workspace(uuid) FROM PUBLIC;

CREATE OR REPLACE FUNCTION app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      INSERT INTO invitations (workspace_id, email, role, invited_by, expires_at)
      VALUES (p_workspace, v_email, p_role, v_user, now() + make_interval(secs => p_ttl_secs))
      RETURNING id INTO v_id;
    END IF;
    PERFORM app_audit(p_workspace, 'invite', 'invitation', v_id,
                      jsonb_build_object('email', v_email, 'role', p_role));
    RETURN v_id;
  END
  $$;

CREATE OR REPLACE FUNCTION app_my_invitations()
  RETURNS TABLE (id uuid, workspace_id uuid, workspace_name text, role text, invited_by_name text,
                 expires_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT i.id, i.workspace_id, w.name, i.role, inviter.display_name, i.expires_at
    FROM invitations i
    JOIN workspaces w ON w.id = i.workspace_id AND w.deleted_at IS NULL
    JOIN users inviter ON inviter.id = i.invited_by
    JOIN users me ON me.id = app_user_id()
    WHERE i.closed_at IS NULL AND i.expires_at > now()
      AND me.email_verified AND lower(me.email) = i.email
    ORDER BY i.created_at
  $$;

CREATE OR REPLACE FUNCTION app_respond_invitation(p_invitation uuid, p_accept boolean) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
      VALUES (v_inv.workspace_id, v_user, 'join', 'member', v_user, jsonb_build_object('role', v_inv.role));
    ELSE
      UPDATE invitations SET closed_at = now(), close_reason = 'declined' WHERE id = p_invitation;
      INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
      VALUES (v_inv.workspace_id, v_user, 'decline', 'invitation', p_invitation, '{}'::jsonb);
    END IF;
    RETURN v_inv.workspace_id;
  END
  $$;

CREATE OR REPLACE FUNCTION app_offer_ownership(p_workspace uuid, p_to uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(p_workspace, 'offer_ownership', 'member', p_to, '{}'::jsonb);
  END
  $$;

CREATE OR REPLACE FUNCTION app_accept_ownership(p_workspace uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(p_workspace, 'accept_ownership', 'member', v_user,
                      jsonb_build_object('from', v_offer.from_user));
  END
  $$;

CREATE OR REPLACE FUNCTION app_delete_workspace(p_ws uuid, p_name text) RETURNS timestamptz
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
    UPDATE invitations SET closed_at = now(), close_reason = 'workspace_deleted'
    WHERE workspace_id = p_ws AND closed_at IS NULL;
    GET DIAGNOSTICS v_closed = ROW_COUNT;
    UPDATE workspaces SET deleted_at = now(), deleted_by = v_user WHERE id = p_ws;
    PERFORM app_audit(p_ws, 'delete', 'workspace', p_ws,
                      jsonb_build_object('name', v_ws.name, 'invitations_closed', v_closed));
    RETURN now() + make_interval(days => v_ws.trash_retention_days);
  END
  $$;

CREATE OR REPLACE FUNCTION app_restore_workspace(p_ws uuid) RETURNS void
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
    UPDATE invitations SET closed_at = NULL, close_reason = NULL
    WHERE workspace_id = p_ws AND close_reason = 'workspace_deleted' AND expires_at > now();
    PERFORM app_audit(p_ws, 'restore', 'workspace', p_ws, jsonb_build_object('name', v_ws.name));
  END
  $$;
