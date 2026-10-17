-- 항목 정보와 항목 멤버 (D-33 리더·멤버·그룹·목표·범위·기간·프로젝트 상태, D-34 연구 정보, D-36 스터디 정보,
-- D-38 취미 정보, D-20 공유).
--
-- 항목 정보: 리더(표시만, 권한을 더 주지 않음), 목표·범위·기간·상태(계획·진행·보류·완료·취소), 모듈 종류별 정보
-- (연구: 분야·키워드·질문·가설·모형·방법·표본, 스터디: 수준·목표 분량, 취미: 활동 유형). 종류별 정보의 형식은
-- 서버가 확인한다(D-47 구현 선택: 업무 상태는 D-31의 세 가지를 그대로 두고 "프로젝트별 상태"는 항목 상태로 둔다).
--
-- 항목 멤버: 비공개 항목을 함께 쓰는 사람·그룹. 문서의 명시적 공유(D-20)와 같은 뜻으로, 멤버는 워크스페이스 역할
-- 안에서 그 항목을 본다(작성자 → 쓰기, 독자 → 읽기). 멤버를 정하는 것은 항목 소유 권한자(만든 사람·관리자)만
-- 한다(재공유는 소유자만). 워크스페이스에서 나가면 멤버여도 보지 못한다. 공개 항목에서는 멤버가 권한을 바꾸지 않는다.

ALTER TABLE modules ADD COLUMN icon text NOT NULL DEFAULT '' CHECK (length(icon) <= 16);

ALTER TABLE items
  ADD COLUMN leader_id uuid REFERENCES users (id),
  ADD COLUMN goal      text NOT NULL DEFAULT '' CHECK (length(goal) <= 4000),
  ADD COLUMN scope     text NOT NULL DEFAULT '' CHECK (length(scope) <= 4000),
  ADD COLUMN starts_on date,
  ADD COLUMN ends_on   date,
  ADD COLUMN status    text NOT NULL DEFAULT 'active'
                       CHECK (status IN ('planned', 'active', 'on_hold', 'done', 'cancelled')),
  ADD COLUMN details   jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(details) = 'object'),
  ADD CONSTRAINT items_period CHECK (starts_on IS NULL OR ends_on IS NULL OR starts_on <= ends_on);

CREATE TABLE item_members (
  item_id  uuid NOT NULL REFERENCES items (id) ON DELETE CASCADE,
  user_id  uuid REFERENCES users (id),
  group_id uuid REFERENCES groups (id) ON DELETE CASCADE,
  added_by uuid NOT NULL REFERENCES users (id),
  added_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT item_members_one CHECK ((user_id IS NULL) <> (group_id IS NULL))
);
CREATE UNIQUE INDEX item_members_user ON item_members (item_id, user_id) WHERE user_id IS NOT NULL;
CREATE UNIQUE INDEX item_members_group ON item_members (item_id, group_id) WHERE group_id IS NOT NULL;
CREATE INDEX item_members_by_user ON item_members (user_id) WHERE user_id IS NOT NULL;

-- 이 사람이 항목 멤버인지(직접 또는 그룹).
CREATE FUNCTION app_is_item_member(p_item uuid, p_user uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (SELECT 1 FROM item_members WHERE item_id = p_item AND user_id = p_user)
        OR EXISTS (SELECT 1 FROM item_members im JOIN group_members gm ON gm.group_id = im.group_id
                   WHERE im.item_id = p_item AND gm.user_id = p_user)
  $$;

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
    -- 비공개 항목은 항목 멤버(직접·그룹)만, 워크스페이스 역할 안에서.
    IF v_item.visibility <> 'workspace' AND NOT app_is_item_member(p_item, v_user) THEN
      RETURN NULL;
    END IF;
    RETURN CASE v_role WHEN 'writer' THEN 'write' ELSE 'read' END;
  END
  $$;

-- 항목 정보 바꾸기(쓸 수 있는 사람). 리더는 그 항목을 볼 수 있는 사람이어야 한다.
CREATE FUNCTION app_set_item_profile(
  p_item uuid, p_leader uuid, p_goal text, p_scope text, p_starts date, p_ends date, p_status text,
  p_details jsonb) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
  BEGIN
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
      PERFORM app_audit(v_item.workspace_id, 'change_status', 'item', p_item,
                        jsonb_build_object('from', v_item.status, 'to', p_status));
    END IF;
    IF v_item.leader_id IS DISTINCT FROM p_leader THEN
      PERFORM app_audit(v_item.workspace_id, 'change_leader', 'item', p_item,
                        jsonb_build_object('from', v_item.leader_id, 'to', p_leader));
    END IF;
  END
  $$;

-- 항목 멤버 정하기(항목 소유 권한자). 사람은 이 워크스페이스 멤버, 그룹은 이 워크스페이스 그룹이어야 한다.
-- 빠져서 볼 수 없게 된 사람은 그 항목 업무의 담당에서 빠지고, 리더였으면 리더도 비운다.
CREATE FUNCTION app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
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
      PERFORM app_audit(v_item.workspace_id, 'set_members', 'item', p_item,
                        jsonb_build_object('users', cardinality(v_users), 'groups', cardinality(v_groups)));
    END IF;
    PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_member_removed');
    IF v_item.leader_id IS NOT NULL AND app_item_access_as(p_item, v_item.leader_id) IS NULL THEN
      UPDATE items SET leader_id = NULL WHERE id = p_item;
    END IF;
  END
  $$;

-- 모듈 아이콘(관리자). 그림 문자 하나 정도.
CREATE FUNCTION app_set_module_icon(p_module uuid, p_icon text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid := (SELECT workspace_id FROM modules WHERE id = p_module);
  BEGIN
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    IF length(btrim(coalesce(p_icon, ''))) > 16 THEN
      RAISE EXCEPTION 'the icon is too long' USING ERRCODE = '22023';
    END IF;
    UPDATE modules SET icon = btrim(coalesce(p_icon, '')) WHERE id = p_module;
  END
  $$;

-- 항목 멤버 읽기: 그 항목을 볼 수 있는 사람.
ALTER TABLE item_members ENABLE ROW LEVEL SECURITY;
CREATE POLICY item_members_select ON item_members FOR SELECT USING (app_item_access(item_id) IS NOT NULL);
GRANT SELECT ON item_members TO fvoci_app;

REVOKE ALL ON FUNCTION app_is_item_member(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_members(uuid, uuid[], uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_module_icon(uuid, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb),
  app_set_item_members(uuid, uuid[], uuid[]),
  app_set_module_icon(uuid, text)
TO fvoci_app;
