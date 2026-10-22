-- 항목 정보 저장은 공개 범위를 바꿀 때만 받는다 (코드 전반 감사 W13).
--
-- 항목 정보 저장(`PUT /api/items/{id}`)은 공개 범위를 늘 함께 받았고 화면은 연 때의 값을 그대로 보냈다. 그사이 다른
-- 소유 권한자가 공개 범위를 바꿨으면, 설명만 고친 저장이 그 변경을 말없이 되돌렸다. 비공개로 바꾼 항목이 다시
-- 워크스페이스 전체에 열리거나(접근 노출), 공개로 바꾼 뒤 배정한 담당과 리더가 비공개로 되돌리며 지워졌다. 쓰기
-- 권한자는 공개 범위 체크를 보지도 않는데 옛 값 때문에 403을 받았다.
-- 이제 `p_visibility`가 NULL이면 공개 범위는 그대로다. 화면은 소유 권한자가 체크를 바꿨을 때만 보낸다. 제목·설명은
-- 여전히 마지막 저장이 이긴다(항목에 판이 없어 따로 결정이 필요하다).
-- 함수 본문은 0072의 마지막 정의와 같고 공개 범위 부분만 다르다.

-- id: 이름 바꾸기·공개 범위 감사(2), 공개 범위를 좁혀 담당에서 빠지는 사람마다(개수는 `_ids`).
CREATE OR REPLACE FUNCTION app_update_item(p_item uuid, p_title text, p_description text, p_visibility text,
                                           p_ids uuid[])
  RETURNS void
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
  END
  $$;
