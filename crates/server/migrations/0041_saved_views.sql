-- 저장한 보기 (D-31 "검색·필터·정렬·저장한 보기"). 항목 업무 화면의 보기 방식(칸반·목록·간트·달력)·필터·정렬을
-- 이름을 붙여 저장한다. 사람마다 따로이며 본인만 읽는다(개인 설정). 그 항목을 볼 수 있어야 만들고 쓴다. 보기 내용은
-- 화면이 해석하는 JSON이고 서버는 크기와 형식만 확인한다.

CREATE TABLE saved_views (
  id         uuid PRIMARY KEY DEFAULT uuidv7(),
  user_id    uuid NOT NULL REFERENCES users (id),
  item_id    uuid NOT NULL REFERENCES items (id) ON DELETE CASCADE,
  name       text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 100),
  config     jsonb NOT NULL CHECK (jsonb_typeof(config) = 'object' AND length(config::text) <= 4000),
  created_at timestamptz NOT NULL DEFAULT now(),
  request_id uuid NOT NULL,
  CONSTRAINT saved_views_request UNIQUE (user_id, request_id),
  CONSTRAINT saved_views_name UNIQUE (user_id, item_id, name)
);
CREATE INDEX saved_views_item ON saved_views (user_id, item_id);

CREATE FUNCTION app_save_view(p_request uuid, p_item uuid, p_name text, p_config jsonb) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
  BEGIN
    SELECT id INTO v_id FROM saved_views WHERE user_id = app_user_id() AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO saved_views (user_id, item_id, name, config, request_id)
    VALUES (app_user_id(), p_item, btrim(p_name), p_config, p_request)
    ON CONFLICT (user_id, item_id, name) DO UPDATE SET config = EXCLUDED.config
    RETURNING id INTO v_id;
    RETURN v_id;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE FUNCTION app_delete_view(p_view uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    DELETE FROM saved_views WHERE id = p_view AND user_id = app_user_id();
    IF NOT FOUND THEN
      RAISE EXCEPTION 'view not found' USING ERRCODE = 'P0002';
    END IF;
  END
  $$;

ALTER TABLE saved_views ENABLE ROW LEVEL SECURITY;
CREATE POLICY saved_views_select ON saved_views FOR SELECT
  USING (user_id = app_user_id() AND app_item_access(item_id) IS NOT NULL);
GRANT SELECT ON saved_views TO fvoci_app;

REVOKE ALL ON FUNCTION app_save_view(uuid, uuid, text, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_view(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_save_view(uuid, uuid, text, jsonb), app_delete_view(uuid) TO fvoci_app;
