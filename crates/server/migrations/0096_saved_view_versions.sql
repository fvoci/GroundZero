-- 저장한 보기의 동시 수정 보호 (D-103, 감사 P-26·SD1).
--
-- 보기 저장은 이름이 같으면 조건 없이 덮어썼다(`ON CONFLICT … DO UPDATE`). 덮어쓰기는 요청 id를 남기지 않아, 응답을 잃은
-- 저장을 같은 요청 id로 다시 보내면 그사이 다른 탭이 저장한 설정을 덮었다(`docs/development.md` 94절, 95절 SD1). 이제 새로
-- 만들기와 기존 보기 고치기를 나눈다(옛 호출을 위한 덮어쓰기 경로는 남기지 않는다, D-92).
--
-- 1. 보기마다 판(`version`)을 둔다. 고칠 때마다 1 오른다.
-- 2. `app_create_view`: 새 보기만 만든다. 같은 이름이 이미 있으면 아무것도 바꾸지 않고 'exists'(서버가 409와 그 보기를
--    돌려준다). 같은 요청 id의 재시도는 처음 만든 보기를 돌려준다(지금과 같다).
-- 3. `app_update_view`: 행을 잠근 같은 트랜잭션에서 요청 id와 판을 본다. 마지막 고치기의 요청 id(`updated_request_id`)와
--    같으면 이미 반영한 재시도라 쓰지 않고 'replayed', 읽은 판이 지금 판과 다르면 쓰지 않고 'conflict'(409와 최신 보기),
--    아니면 고치고 판을 올린다. 요청 id는 마지막 고치기의 것만 남으므로, 다른 탭이 그 뒤 또 저장했으면 옛 재시도는
--    알아보지 못하고 판 검사에서 409가 된다(덮지 않는다).
-- 4. 이미 쓴 요청 id(다른 보기의 것이거나 만들기의 것)를 다른 저장에 쓰면 22023(400).

ALTER TABLE saved_views
  ADD COLUMN version integer NOT NULL DEFAULT 1,
  ADD COLUMN updated_request_id uuid,
  ADD CONSTRAINT saved_views_updated_request UNIQUE (user_id, updated_request_id);

DROP FUNCTION app_save_view(uuid, uuid, text, jsonb, uuid[]);

-- id 1개: 새 보기.
CREATE FUNCTION app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[])
  RETURNS TABLE (view_id uuid, outcome text)
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_view saved_views%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_view FROM saved_views
    WHERE user_id = app_user_id() AND (request_id = p_request OR updated_request_id = p_request);
    IF v_view.id IS NOT NULL THEN
      IF v_view.request_id <> p_request OR v_view.item_id <> p_item OR v_view.name <> btrim(p_name) THEN
        RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
      END IF;
      RETURN QUERY SELECT v_view.id, 'created'::text;
      RETURN;
    END IF;
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO saved_views (id, user_id, item_id, name, config, request_id)
    VALUES (app_id(p_ids, 1), app_user_id(), p_item, btrim(p_name), p_config, p_request)
    ON CONFLICT (user_id, item_id, name) DO NOTHING
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      RETURN QUERY SELECT id, 'exists'::text FROM saved_views
      WHERE user_id = app_user_id() AND item_id = p_item AND name = btrim(p_name);
      RETURN;
    END IF;
    RETURN QUERY SELECT v_id, 'created'::text;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE FUNCTION app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb)
  RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_view saved_views%ROWTYPE;
  BEGIN
    SELECT * INTO v_view FROM saved_views WHERE id = p_view AND user_id = app_user_id() FOR UPDATE;
    IF v_view.id IS NULL OR app_item_access(v_view.item_id) IS NULL THEN
      RAISE EXCEPTION 'view not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_view.updated_request_id = p_request THEN
      RETURN 'replayed';
    END IF;
    IF EXISTS (SELECT 1 FROM saved_views
               WHERE user_id = app_user_id() AND (request_id = p_request OR updated_request_id = p_request)) THEN
      RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
    END IF;
    IF v_view.version <> p_base_version THEN
      RETURN 'conflict';
    END IF;
    UPDATE saved_views SET config = p_config, version = version + 1, updated_request_id = p_request
    WHERE id = p_view;
    RETURN 'updated';
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

REVOKE ALL ON FUNCTION app_create_view(uuid, uuid, text, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_view(uuid, uuid, integer, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_create_view(uuid, uuid, text, jsonb, uuid[]),
  app_update_view(uuid, uuid, integer, jsonb)
TO fvoci_app;
