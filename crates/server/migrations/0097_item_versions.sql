-- 항목 정보의 동시 수정 보호 (D-104, 감사 P-30).
--
-- 항목 정보는 두 경로로 고친다. 둘 다 마지막 저장이 이겼다(항목에 판이 없었다, 0084 머리말).
--   - `PUT /api/items/{id}`(항목 화면의 정보 폼): 제목·설명·공개 범위.
--   - `PUT /api/items/{id}/profile`(항목 정보 패널): 리더·목표·범위·시작일·끝날·상태·종류별 정보.
-- 이제 두 경로 모두 읽은 판을 받아 행을 잠근 같은 트랜잭션에서 비교한다.
--
-- 1. 판을 두 개 둔다. `version`은 제목·설명·공개 범위, `profile_version`은 항목 정보 패널의 칸. 한 폼의 저장이 다른
--    폼을 409로 만들지 않게 따로 센다. 어느 함수가 바꿨든 그 칸 묶음이 실제로 바뀌면 표의 트리거가 1 올린다(0076의
--    `move_version`과 같은 방식). 예를 들어 멤버에서 빠진 리더를 지우는 정리(`app_set_item_members`,
--    `app_drop_unseen_assignees`)도 `profile_version`을 올려, 그 전에 연 패널이 지워진 리더를 되살리지 못한다. 보관·
--    휴지통·템플릿 표시처럼 두 폼이 보내지 않는 칸은 판을 올리지 않는다. 직접 다른 값을 넣어도 트리거가 맞춘다.
-- 2. `app_update_item`·`app_set_item_profile`에 `p_base_version`을 더한다. 보관 검사 뒤에 비교하고, 다르면 아무것도
--    바꾸지 않고 'conflict'(서버가 409와 서버 최신본을 돌려준다). 다만 보낸 값이 이미 지금 값과 모두 같으면(응답을 잃은
--    같은 저장의 재시도) 바꿀 것이 없어 'updated'로 끝낸다. 판이 같으면 지금처럼 고친다. 판을 보내지 않으면(NULL)
--    지난 판과 같이 다룬다(`IS DISTINCT FROM`, 0073·0082의 판 검사와 같다). 서버 API는 판을 늘 보내지만 이 함수가
--    마지막 보장이다.
-- 함수 본문은 마지막 정의(0084 `app_update_item`, 0081 `app_set_item_profile`)와 같고 판 부분만 다르다.

ALTER TABLE items
  ADD COLUMN version integer NOT NULL DEFAULT 1,
  ADD COLUMN profile_version integer NOT NULL DEFAULT 1;

CREATE FUNCTION app_items_versions() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF (NEW.title, NEW.description, NEW.visibility) IS DISTINCT FROM (OLD.title, OLD.description, OLD.visibility) THEN
      NEW.version := OLD.version + 1;
    ELSE
      NEW.version := OLD.version;
    END IF;
    IF (NEW.leader_id, NEW.goal, NEW.scope, NEW.starts_on, NEW.ends_on, NEW.status, NEW.details)
       IS DISTINCT FROM (OLD.leader_id, OLD.goal, OLD.scope, OLD.starts_on, OLD.ends_on, OLD.status, OLD.details) THEN
      NEW.profile_version := OLD.profile_version + 1;
    ELSE
      NEW.profile_version := OLD.profile_version;
    END IF;
    RETURN NEW;
  END
  $$;
REVOKE ALL ON FUNCTION app_items_versions() FROM PUBLIC;

CREATE TRIGGER items_versions BEFORE UPDATE ON items
  FOR EACH ROW EXECUTE FUNCTION app_items_versions();

DROP FUNCTION app_update_item(uuid, text, text, text, uuid[]);

-- id: 이름 바꾸기·공개 범위 감사(2), 공개 범위를 좁혀 담당에서 빠지는 사람마다(개수는 `_ids`).
CREATE FUNCTION app_update_item(p_item uuid, p_base_version integer, p_title text, p_description text,
                                p_visibility text, p_ids uuid[])
  RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item       items%ROWTYPE;
    v_access     text;
    v_visibility text;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_item_access(p_item);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(v_item.id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 0097: 읽은 판이 지났으면 바꾸지 않는다(바꿀 것이 없는 재시도는 그대로 성공).
    IF v_item.version IS DISTINCT FROM p_base_version THEN
      IF v_item.title = btrim(p_title) AND v_item.description = coalesce(p_description, '')
         AND (p_visibility IS NULL OR p_visibility = v_item.visibility) THEN
        RETURN 'updated';
      END IF;
      RETURN 'conflict';
    END IF;
    -- 보내지 않았으면 지금 값 그대로(잠근 뒤의 값).
    v_visibility := coalesce(p_visibility, v_item.visibility);
    IF v_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF v_visibility <> v_item.visibility AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can change visibility' USING ERRCODE = '42501';
    END IF;
    UPDATE items SET title = btrim(p_title), description = coalesce(p_description, ''),
                     visibility = v_visibility, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'rename', 'item', p_item,
                        jsonb_build_object('from', v_item.title, 'to', btrim(p_title)));
    END IF;
    IF v_item.visibility <> v_visibility THEN
      PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'change_visibility', 'item', p_item,
                        jsonb_build_object('from', v_item.visibility, 'to', v_visibility));
      -- 비공개가 되어 볼 수 없게 된 사람은 그 항목 업무의 담당에서 뺀다.
      PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_private', app_ids_from(p_ids, 3));
    END IF;
    RETURN 'updated';
  END
  $$;

DROP FUNCTION app_set_item_profile(uuid, uuid, text, text, date, date, text, jsonb, uuid[]);

CREATE FUNCTION app_set_item_profile(p_item uuid, p_base_version integer, p_leader uuid, p_goal text, p_scope text,
                                     p_starts date, p_ends date, p_status text, p_details jsonb, p_ids uuid[])
  RETURNS text
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
    -- 0097: 읽은 판이 지났으면 바꾸지 않는다(바꿀 것이 없는 재시도는 그대로 성공).
    IF v_item.profile_version IS DISTINCT FROM p_base_version THEN
      IF v_item.leader_id IS NOT DISTINCT FROM p_leader AND v_item.goal = coalesce(p_goal, '')
         AND v_item.scope = coalesce(p_scope, '') AND v_item.starts_on IS NOT DISTINCT FROM p_starts
         AND v_item.ends_on IS NOT DISTINCT FROM p_ends AND v_item.status = p_status
         AND v_item.details = coalesce(p_details, '{}'::jsonb) THEN
        RETURN 'updated';
      END IF;
      RETURN 'conflict';
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
    RETURN 'updated';
  END
  $$;

REVOKE ALL ON FUNCTION app_update_item(uuid, integer, text, text, text, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_item_profile(uuid, integer, uuid, text, text, date, date, text, jsonb, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_update_item(uuid, integer, text, text, text, uuid[]),
  app_set_item_profile(uuid, integer, uuid, text, text, date, date, text, jsonb, uuid[])
TO fvoci_app;
