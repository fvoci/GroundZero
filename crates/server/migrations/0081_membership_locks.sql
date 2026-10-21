-- 멤버·소유권·초대의 잠금 순서 (D-13·D-20·D-47, 코드 전반 감사 S8·S14·S15·S16·S17).
--
-- 모두 격리 PostgreSQL에서 트랜잭션 하나를 열어 둔 채 다른 요청을 보내 재현했다(통합 시험).
-- 1. 소유권 수락과 멤버 내보내기(S14): 내보내기가 소유자를 잠그지 않고 읽어, 수락이 커밋되기 전의 옛 소유자를 보고
--    통과했다. 그 결과 소유자가 멤버가 아닌 워크스페이스가 남았다. 내보내기·떠나기는 수락·제안과 같이 워크스페이스 행을
--    먼저 잠근 뒤 소유자를 본다(순서: 워크스페이스 → 멤버 행).
-- 2. 내보내기와 동시 추가(S15·S8): 내보내기가 항목 멤버·그룹 멤버를 지운 뒤에야 멤버 행을 잠갔고, 넣는 쪽(항목 멤버·그룹
--    멤버·리더)은 멤버 행을 잠그지 않고 확인했다. 그래서 동시에 넣은 행이 내보내기 뒤에도 남아 다시 초대되면 비공개 항목이
--    저절로 열렸다. 내보내기는 멤버 행을 먼저 잠그고, 넣는 쪽은 대상의 멤버 행을 공유 잠금한 뒤 확인한다(담당자 지정
--    `app_check_assignee`와 같은 방식). 항목 멤버·프로필은 멤버 행 → 항목 행 순서로 잠근다(내보내기의 리더 정리와 같은 순서).
-- 3. 초대 철회와 수락(S16): 철회가 초대를 잠그지 않고 열린 것을 확인한 뒤 갱신해, 그사이 수락된 초대를 '철회됨'으로
--    덮었다. 수락과 같은 순서(워크스페이스 → 초대)로 잠근 뒤 아직 열려 있는지 다시 본다(아니면 55000, 409).
-- 4. 관리자끼리 서로 내보내기·강등(S17): 호출한 사람의 관리자 확인을 잠그지 않고 해서 둘 다 통과했다. 두 사람의 멤버 행을
--    id 순서로 잠근 뒤 확인한다(먼저 끝난 쪽만 통과, 다른 쪽은 이미 관리자가 아니거나 멤버가 아님).
-- 함수 본문은 0070·0072의 마지막 정의와 같고 잠금과 다시 확인만 다르다.

CREATE OR REPLACE FUNCTION app_revoke_invitation(p_invitation uuid, p_ids uuid[]) RETURNS void
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
    -- 0081: 수락과 같은 순서(워크스페이스 → 초대)로 잠근 뒤 아직 열려 있는지 다시 본다.
    PERFORM app_lock_live_workspace(v_ws);
    PERFORM 1 FROM invitations WHERE id = p_invitation AND closed_at IS NULL FOR UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'invitation is no longer open' USING ERRCODE = '55000';
    END IF;
    UPDATE invitations SET closed_at = now(), close_reason = 'revoked' WHERE id = p_invitation;
    PERFORM app_audit(app_id(p_ids, 1), v_ws, 'revoke', 'invitation', p_invitation, '{}'::jsonb);
  END
  $$;

CREATE OR REPLACE FUNCTION app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text, p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_used integer;
  BEGIN
    -- 0081: 소유권 수락·제안과 같은 순서로 워크스페이스 행 → 멤버 행을 먼저 잠근다. 그 뒤에 소유자를 보고(수락이 끝난
    -- 뒤의 값), 항목·그룹 멤버를 지운다(동시에 넣는 쪽은 멤버 행 공유 잠금을 기다린다).
    PERFORM app_lock_live_workspace(p_workspace);
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member FOR UPDATE;
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'the owner cannot leave or be removed; transfer ownership first' USING ERRCODE = '55000';
    END IF;
    DELETE FROM group_members gm USING groups g
    WHERE gm.group_id = g.id AND g.workspace_id = p_workspace AND gm.user_id = p_member;
    -- 항목 멤버도 끝난다. 다시 초대되면 비공개 항목은 다시 멤버로 넣어야 열린다.
    DELETE FROM item_members im USING items i
    WHERE im.item_id = i.id AND i.workspace_id = p_workspace AND im.user_id = p_member;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace AND to_user = p_member;
    DELETE FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    v_used := app_drop_unseen_assignees(NULL, p_workspace, p_member, p_action, p_ids);
    PERFORM app_audit(app_id(p_ids, v_used + 1), p_workspace, p_action, 'member', p_member, '{}'::jsonb);
  END
  $$;

CREATE OR REPLACE FUNCTION app_remove_member(p_workspace uuid, p_member uuid, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    -- 0081: 워크스페이스 행 → 두 사람의 멤버 행(id 순서)을 잠근 뒤 관리자인지 본다(관리자끼리 동시에 서로 내보내기).
    PERFORM app_lock_live_workspace(p_workspace);
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
    PERFORM app_require_admin(p_workspace);
    PERFORM app_remove_member_internal(p_workspace, p_member, 'remove', p_ids);
  END
  $$;

CREATE OR REPLACE FUNCTION app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_old text;
  BEGIN
    -- 0081: 두 사람의 멤버 행을 id 순서로 잠근 뒤 관리자인지 본다(관리자끼리 동시에 서로 강등하기).
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
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

CREATE OR REPLACE FUNCTION app_set_group_member(p_group uuid, p_member uuid, p_present boolean, p_ids uuid[]) RETURNS void
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
      -- 0081: 대상의 멤버 행을 공유 잠금한 뒤 확인한다(동시에 내보내는 중이면 끝날 때까지 기다린 뒤 다시 본다).
      PERFORM 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id = p_member FOR SHARE;
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

CREATE OR REPLACE FUNCTION app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[], p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
    v_users  uuid[] := ARRAY(SELECT DISTINCT unnest(coalesce(p_users, '{}')));
    v_groups uuid[] := ARRAY(SELECT DISTINCT unnest(coalesce(p_groups, '{}')));
    v_before jsonb;
    v_after  jsonb;
  BEGIN
    -- 0081: 넣을 사람들의 멤버 행을 id 순서로 공유 잠금한 뒤(멤버 행 → 항목 행) 확인한다. 동시에 내보내는 중이면 끝날
    -- 때까지 기다리고, 끝난 뒤의 멤버 목록으로 본다.
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = (SELECT workspace_id FROM items WHERE id = p_item) AND user_id = ANY (v_users)
    ORDER BY user_id FOR SHARE;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can choose members' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF cardinality(v_users) + cardinality(v_groups) > 500 THEN
      RAISE EXCEPTION 'too many members' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM unnest(v_users) u
               WHERE NOT EXISTS (SELECT 1 FROM workspace_members m
                                 WHERE m.workspace_id = v_item.workspace_id AND m.user_id = u)) THEN
      RAISE EXCEPTION 'members must belong to the workspace' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM unnest(v_groups) g
               WHERE NOT EXISTS (SELECT 1 FROM groups gr WHERE gr.id = g AND gr.workspace_id = v_item.workspace_id)) THEN
      RAISE EXCEPTION 'groups must belong to the workspace' USING ERRCODE = '22023';
    END IF;
    SELECT jsonb_build_object('users', coalesce(jsonb_agg(user_id) FILTER (WHERE user_id IS NOT NULL), '[]'),
                              'groups', coalesce(jsonb_agg(group_id) FILTER (WHERE group_id IS NOT NULL), '[]'))
      INTO v_before FROM item_members WHERE item_id = p_item;
    DELETE FROM item_members WHERE item_id = p_item
      AND ((user_id IS NOT NULL AND user_id <> ALL (v_users)) OR (group_id IS NOT NULL AND group_id <> ALL (v_groups)));
    INSERT INTO item_members (item_id, user_id, added_by)
      SELECT p_item, u, app_user_id() FROM unnest(v_users) u
      ON CONFLICT (item_id, user_id) WHERE user_id IS NOT NULL DO NOTHING;
    INSERT INTO item_members (item_id, group_id, added_by)
      SELECT p_item, g, app_user_id() FROM unnest(v_groups) g
      ON CONFLICT (item_id, group_id) WHERE group_id IS NOT NULL DO NOTHING;
    v_after := jsonb_build_object('users', to_jsonb(v_users), 'groups', to_jsonb(v_groups));
    IF v_before IS DISTINCT FROM v_after THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'set_members', 'item', p_item,
                        jsonb_build_object('users', cardinality(v_users), 'groups', cardinality(v_groups)));
    END IF;
    PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_member_removed', app_ids_from(p_ids, 2));
    IF v_item.leader_id IS NOT NULL AND app_item_access_as(p_item, v_item.leader_id) IS NULL THEN
      UPDATE items SET leader_id = NULL WHERE id = p_item;
    END IF;
  END
  $$;

CREATE OR REPLACE FUNCTION app_set_item_profile(p_item uuid, p_leader uuid, p_goal text, p_scope text, p_starts date,
                                     p_ends date, p_status text, p_details jsonb, p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
  BEGIN
    -- 0081: 새 리더의 멤버 행을 먼저 공유 잠금한다(멤버 행 → 항목 행). 동시에 내보내는 중이면 끝난 뒤의 상태로 본다.
    IF p_leader IS NOT NULL THEN
      PERFORM 1 FROM workspace_members
      WHERE workspace_id = (SELECT workspace_id FROM items WHERE id = p_item) AND user_id = p_leader FOR SHARE;
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_status NOT IN ('planned', 'active', 'on_hold', 'done', 'cancelled') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF p_starts IS NOT NULL AND p_ends IS NOT NULL AND p_starts > p_ends THEN
      RAISE EXCEPTION 'the period ends before it starts' USING ERRCODE = '22023';
    END IF;
    IF length(coalesce(p_goal, '')) > 4000 OR length(coalesce(p_scope, '')) > 4000
       OR length(coalesce(p_details, '{}'::jsonb)::text) > 20000 THEN
      RAISE EXCEPTION 'item details are too long' USING ERRCODE = '22023';
    END IF;
    IF p_leader IS NOT NULL AND p_leader IS DISTINCT FROM v_item.leader_id
       AND app_item_access_as(p_item, p_leader) IS NULL THEN
      RAISE EXCEPTION 'the leader must be able to see the item' USING ERRCODE = '22023';
    END IF;
    UPDATE items SET leader_id = p_leader, goal = coalesce(p_goal, ''), scope = coalesce(p_scope, ''),
                     starts_on = p_starts, ends_on = p_ends, status = p_status,
                     details = coalesce(p_details, '{}'::jsonb), updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.status <> p_status THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'change_status', 'item', p_item,
                        jsonb_build_object('from', v_item.status, 'to', p_status));
    END IF;
    IF v_item.leader_id IS DISTINCT FROM p_leader THEN
      PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'change_leader', 'item', p_item,
                        jsonb_build_object('from', v_item.leader_id, 'to', p_leader));
    END IF;
  END
  $$;

REVOKE ALL ON FUNCTION app_revoke_invitation(uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_member_internal(uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_remove_member(uuid, uuid, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_member_role(uuid, uuid, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_group_member(uuid, uuid, boolean, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_members(uuid, uuid[], uuid[], uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_revoke_invitation(uuid, uuid[]),
  app_remove_member(uuid, uuid, uuid[]),
  app_set_member_role(uuid, uuid, text, uuid[]),
  app_set_group_member(uuid, uuid, boolean, uuid[]),
  app_set_item_members(uuid, uuid[], uuid[], uuid[]),
  app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb, uuid[])
TO fvoci_app;
