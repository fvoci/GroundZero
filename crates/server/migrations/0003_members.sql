-- 멤버·초대·역할·그룹·소유권 인계 (D-20, D-25, D-51, D-57).
-- 모든 변경은 정의자 함수로만 하고 같은 트랜잭션에서 감사 기록을 남긴다.
-- 오류 코드 규칙: P0002 대상 없음·비멤버(404), 42501 역할 부족(403), 22023 잘못된 입력(400), 55000 상태 위반(409).

------------------------------------------------------------------------------
-- 0002 재검토 보강
------------------------------------------------------------------------------

-- 저장 재시도 키는 작성자 단위다(재시도 판단과 같은 범위).
ALTER TABLE document_revisions DROP CONSTRAINT document_revisions_request;
ALTER TABLE document_revisions
  ADD CONSTRAINT document_revisions_request UNIQUE (document_id, author, request_id);

-- 위조된 콜백이 진행 중인 로그인 상태를 지우지 못하게, state가 맞을 때만 꺼낸다.
DROP FUNCTION app_take_login_state(bytea);
CREATE FUNCTION app_take_login_state(p_secret_hash bytea, p_state text)
  RETURNS TABLE (pkce_verifier text, nonce text, return_to text)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    DELETE FROM oidc_login_states
    WHERE secret_hash = p_secret_hash AND state = p_state AND expires_at > now()
    RETURNING pkce_verifier, nonce, return_to
  $$;
REVOKE ALL ON FUNCTION app_take_login_state(bytea, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_take_login_state(bytea, text) TO fvoci_app;

-- 리비전 일치 트리거는 호출자 권한으로 읽는다(정의자 권한이면 RLS를 건너뛰어 다른 워크스페이스 문서의
-- 존재를 오류 코드로 흘릴 수 있다). 저장 경로의 작성자는 RLS 아래에서 그 문서를 볼 수 있다.
ALTER FUNCTION document_revisions_match() SECURITY INVOKER;

------------------------------------------------------------------------------
-- 사용자: 검증된 이메일 여부 (초대 대상 확인용, 계정 병합에는 쓰지 않는다)
------------------------------------------------------------------------------

ALTER TABLE users ADD COLUMN email_verified boolean NOT NULL DEFAULT false;

DROP FUNCTION app_upsert_user(text, text, text, text);
CREATE FUNCTION app_upsert_user(
  p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text
) RETURNS uuid
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO users (issuer, subject, email, email_verified, display_name)
    VALUES (p_issuer, p_subject, p_email, coalesce(p_email_verified, false), p_display_name)
    ON CONFLICT ON CONSTRAINT users_identity
      DO UPDATE SET email = EXCLUDED.email,
                    email_verified = EXCLUDED.email_verified,
                    display_name = EXCLUDED.display_name,
                    last_login_at = now()
    RETURNING id
  $$;

------------------------------------------------------------------------------
-- 소유자
------------------------------------------------------------------------------

ALTER TABLE workspaces ADD COLUMN owner_id uuid REFERENCES users (id);
UPDATE workspaces SET owner_id = created_by;
ALTER TABLE workspaces ALTER COLUMN owner_id SET NOT NULL;

CREATE TABLE ownership_offers (
  workspace_id uuid PRIMARY KEY REFERENCES workspaces (id),
  from_user    uuid NOT NULL REFERENCES users (id),
  to_user      uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now()
);

-- app_create_workspace가 owner_id를 채우도록 다시 만든다.
CREATE OR REPLACE FUNCTION app_create_workspace(p_request_id uuid, p_name text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'no current user' USING ERRCODE = '42501';
    END IF;

    INSERT INTO workspaces (name, created_by, owner_id, request_id)
    VALUES (btrim(p_name), v_user, v_user, p_request_id)
    ON CONFLICT ON CONSTRAINT workspaces_request DO NOTHING
    RETURNING id INTO v_id;

    IF v_id IS NULL THEN
      SELECT id INTO v_id FROM workspaces WHERE created_by = v_user AND request_id = p_request_id;
      RETURN v_id;
    END IF;

    INSERT INTO workspace_members (workspace_id, user_id, role) VALUES (v_id, v_user, 'admin');
    INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
    VALUES (v_id, v_user, 'create', 'workspace', v_id, jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  END
  $$;

------------------------------------------------------------------------------
-- 공통 검사
------------------------------------------------------------------------------

-- 현재 사용자가 워크스페이스 관리자인지 확인한다. 비멤버면 P0002, 관리자 아니면 42501.
CREATE FUNCTION app_require_admin(p_workspace uuid) RETURNS uuid
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_role text := app_member_role(p_workspace);
  BEGIN
    IF v_user IS NULL OR v_role IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_role <> 'admin' THEN
      RAISE EXCEPTION 'admin role required' USING ERRCODE = '42501';
    END IF;
    RETURN v_user;
  END
  $$;

CREATE FUNCTION app_audit(p_workspace uuid, p_action text, p_target_type text, p_target uuid, p_detail jsonb)
  RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO audit_log (workspace_id, actor, action, target_type, target_id, detail)
    VALUES (p_workspace, app_user_id(), p_action, p_target_type, p_target, coalesce(p_detail, '{}'::jsonb))
  $$;

------------------------------------------------------------------------------
-- 초대
------------------------------------------------------------------------------

CREATE TABLE invitations (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  email        text NOT NULL CHECK (email = lower(btrim(email)) AND position('@' in email) > 1),
  role         text NOT NULL CHECK (role IN ('admin', 'writer', 'reader')),
  invited_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  expires_at   timestamptz NOT NULL,
  accepted_by  uuid REFERENCES users (id),
  accepted_at  timestamptz,
  closed_at    timestamptz,
  close_reason text CHECK (close_reason IN ('accepted', 'declined', 'revoked'))
);
-- 같은 워크스페이스·이메일에 열린 초대는 하나만.
CREATE UNIQUE INDEX invitations_open ON invitations (workspace_id, email) WHERE closed_at IS NULL;

CREATE FUNCTION app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_require_admin(p_workspace);
    v_email text := lower(btrim(p_email));
    v_id    uuid;
  BEGIN
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

CREATE FUNCTION app_revoke_invitation(p_invitation uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(v_ws, 'revoke', 'invitation', p_invitation, '{}'::jsonb);
  END
  $$;

-- 현재 사용자의 검증된 이메일로 받은 열린 초대. 워크스페이스 멤버가 아니어도 이름을 볼 수 있다.
CREATE FUNCTION app_my_invitations()
  RETURNS TABLE (id uuid, workspace_id uuid, workspace_name text, role text, invited_by_name text,
                 expires_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT i.id, i.workspace_id, w.name, i.role, inviter.display_name, i.expires_at
    FROM invitations i
    JOIN workspaces w ON w.id = i.workspace_id
    JOIN users inviter ON inviter.id = i.invited_by
    JOIN users me ON me.id = app_user_id()
    WHERE i.closed_at IS NULL AND i.expires_at > now()
      AND me.email_verified AND lower(me.email) = i.email
    ORDER BY i.created_at
  $$;

-- 초대 수락: 로그인한 사용자의 검증된 이메일이 초대 대상과 같아야 한다 (D-51).
CREATE FUNCTION app_respond_invitation(p_invitation uuid, p_accept boolean) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_inv  invitations%ROWTYPE;
    v_ok   boolean;
  BEGIN
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

------------------------------------------------------------------------------
-- 멤버 역할·제거·탈퇴
------------------------------------------------------------------------------

CREATE FUNCTION app_set_member_role(p_workspace uuid, p_member uuid, p_role text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(p_workspace, 'change_role', 'member', p_member,
                      jsonb_build_object('from', v_old, 'to', p_role));
  END
  $$;

CREATE FUNCTION app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'the owner cannot leave or be removed; transfer ownership first' USING ERRCODE = '55000';
    END IF;
    DELETE FROM group_members gm USING groups g
    WHERE gm.group_id = g.id AND g.workspace_id = p_workspace AND gm.user_id = p_member;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace AND to_user = p_member;
    DELETE FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_audit(p_workspace, p_action, 'member', p_member, '{}'::jsonb);
  END
  $$;

CREATE FUNCTION app_remove_member(p_workspace uuid, p_member uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_require_admin(p_workspace);
    PERFORM app_remove_member_internal(p_workspace, p_member, 'remove');
  END
  $$;

CREATE FUNCTION app_leave_workspace(p_workspace uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_member_role(p_workspace) IS NULL THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_remove_member_internal(p_workspace, app_user_id(), 'leave');
  END
  $$;

------------------------------------------------------------------------------
-- 소유권 인계: 소유자가 제안하고 수신자가 수락해야 완료된다 (D-51).
------------------------------------------------------------------------------

CREATE FUNCTION app_offer_ownership(p_workspace uuid, p_to uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
  BEGIN
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

CREATE FUNCTION app_cancel_ownership_offer(p_workspace uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
    PERFORM app_audit(p_workspace, 'cancel_ownership_offer', 'workspace', p_workspace, '{}'::jsonb);
  END
  $$;

CREATE FUNCTION app_accept_ownership(p_workspace uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_offer ownership_offers%ROWTYPE;
  BEGIN
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

------------------------------------------------------------------------------
-- 그룹
------------------------------------------------------------------------------

CREATE TABLE groups (
  id           uuid PRIMARY KEY DEFAULT uuidv7(),
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  name         text NOT NULL CHECK (length(btrim(name)) > 0),
  created_at   timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT groups_name UNIQUE (workspace_id, name)
);

CREATE TABLE group_members (
  group_id uuid NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
  user_id  uuid NOT NULL REFERENCES users (id),
  PRIMARY KEY (group_id, user_id)
);

CREATE FUNCTION app_create_group(p_workspace uuid, p_name text) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
  BEGIN
    PERFORM app_require_admin(p_workspace);
    INSERT INTO groups (workspace_id, name) VALUES (p_workspace, btrim(p_name)) RETURNING id INTO v_id;
    PERFORM app_audit(p_workspace, 'create', 'group', v_id, jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  EXCEPTION WHEN unique_violation THEN
    RAISE EXCEPTION 'a group with this name exists' USING ERRCODE = '55000';
  END
  $$;

CREATE FUNCTION app_delete_group(p_group uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    DELETE FROM groups WHERE id = p_group;
    PERFORM app_audit(v_ws, 'delete', 'group', p_group, '{}'::jsonb);
  END
  $$;

CREATE FUNCTION app_set_group_member(p_group uuid, p_member uuid, p_present boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
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
      PERFORM app_audit(v_ws, 'add_group_member', 'group', p_group, jsonb_build_object('user', p_member));
    ELSE
      DELETE FROM group_members WHERE group_id = p_group AND user_id = p_member;
      PERFORM app_audit(v_ws, 'remove_group_member', 'group', p_group, jsonb_build_object('user', p_member));
    END IF;
  END
  $$;

------------------------------------------------------------------------------
-- 읽기 권한 (RLS)
------------------------------------------------------------------------------

ALTER TABLE invitations ENABLE ROW LEVEL SECURITY;
CREATE POLICY invitations_select ON invitations FOR SELECT USING (app_member_role(workspace_id) = 'admin');

ALTER TABLE ownership_offers ENABLE ROW LEVEL SECURITY;
CREATE POLICY ownership_offers_select ON ownership_offers FOR SELECT
  USING (app_member_role(workspace_id) IS NOT NULL);

ALTER TABLE groups ENABLE ROW LEVEL SECURITY;
CREATE POLICY groups_select ON groups FOR SELECT USING (app_member_role(workspace_id) IS NOT NULL);

ALTER TABLE group_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY group_members_select ON group_members FOR SELECT
  USING (EXISTS (SELECT 1 FROM groups g WHERE g.id = group_id));

GRANT SELECT ON invitations, ownership_offers, groups, group_members TO fvoci_app;

DO $$
DECLARE
  f text;
BEGIN
  FOREACH f IN ARRAY ARRAY[
    'app_upsert_user(text, text, text, boolean, text)',
    'app_require_admin(uuid)',
    'app_audit(uuid, text, text, uuid, jsonb)',
    'app_invite(uuid, text, text, integer)',
    'app_revoke_invitation(uuid)',
    'app_my_invitations()',
    'app_respond_invitation(uuid, boolean)',
    'app_set_member_role(uuid, uuid, text)',
    'app_remove_member_internal(uuid, uuid, text)',
    'app_remove_member(uuid, uuid)',
    'app_leave_workspace(uuid)',
    'app_offer_ownership(uuid, uuid)',
    'app_cancel_ownership_offer(uuid)',
    'app_accept_ownership(uuid)',
    'app_create_group(uuid, text)',
    'app_delete_group(uuid)',
    'app_set_group_member(uuid, uuid, boolean)'
  ] LOOP
    EXECUTE format('REVOKE ALL ON FUNCTION %s FROM PUBLIC', f);
  END LOOP;
END
$$;

-- 내부용(app_require_admin·app_audit·app_remove_member_internal)은 앱 역할이 직접 부를 수 없다.
GRANT EXECUTE ON FUNCTION
  app_upsert_user(text, text, text, boolean, text),
  app_invite(uuid, text, text, integer),
  app_revoke_invitation(uuid),
  app_my_invitations(),
  app_respond_invitation(uuid, boolean),
  app_set_member_role(uuid, uuid, text),
  app_remove_member(uuid, uuid),
  app_leave_workspace(uuid),
  app_offer_ownership(uuid, uuid),
  app_cancel_ownership_offer(uuid),
  app_accept_ownership(uuid),
  app_create_group(uuid, text),
  app_delete_group(uuid),
  app_set_group_member(uuid, uuid, boolean)
TO fvoci_app;
