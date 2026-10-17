-- 홈: 중요 항목 고정·즐겨찾기 (D-19).
--
-- 고정(pin)과 즐겨찾기(favorite)는 사용자 소유의 표시이며 본인만 본다. 대상은 항목·문서·업무다. 표시할 때 그
-- 대상을 볼 수 있어야 하고, 나중에 볼 수 없게 되면 홈에 나오지 않는다(표시는 남지만 없는 것과 같다).

CREATE TABLE user_marks (
  user_id     uuid NOT NULL REFERENCES users (id),
  mark        text NOT NULL CHECK (mark IN ('pin', 'favorite')),
  target_kind text NOT NULL CHECK (target_kind IN ('item', 'document', 'task')),
  target_id   uuid NOT NULL,
  created_at  timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (user_id, mark, target_kind, target_id)
);

-- 대상을 지금 볼 수 있는지.
CREATE FUNCTION app_target_visible(p_kind text, p_id uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE p_kind
      WHEN 'item' THEN app_item_access(p_id) IS NOT NULL
      WHEN 'document' THEN app_document_access(p_id) IS NOT NULL
      WHEN 'task' THEN app_task_access(p_id) IS NOT NULL
      ELSE false
    END
  $$;

-- 표시 켜기·끄기(본인). 켤 때 대상을 볼 수 있어야 한다(없는 것과 같으면 404). 한 사람당 종류마다 100개까지.
CREATE FUNCTION app_set_mark(p_mark text, p_kind text, p_id uuid, p_on boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    IF p_mark NOT IN ('pin', 'favorite') OR p_kind NOT IN ('item', 'document', 'task') THEN
      RAISE EXCEPTION 'invalid mark' USING ERRCODE = '22023';
    END IF;
    IF NOT p_on THEN
      DELETE FROM user_marks WHERE user_id = v_user AND mark = p_mark AND target_kind = p_kind AND target_id = p_id;
      RETURN;
    END IF;
    IF NOT app_target_visible(p_kind, p_id) THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-marks:' || v_user::text, 0));
    IF (SELECT count(*) FROM user_marks WHERE user_id = v_user AND mark = p_mark) >= 100 THEN
      RAISE EXCEPTION 'too many marks' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_marks (user_id, mark, target_kind, target_id) VALUES (v_user, p_mark, p_kind, p_id)
    ON CONFLICT DO NOTHING;
  END
  $$;

ALTER TABLE user_marks ENABLE ROW LEVEL SECURITY;
CREATE POLICY user_marks_select ON user_marks FOR SELECT USING (user_id = app_user_id());
GRANT SELECT ON user_marks TO fvoci_app;

REVOKE ALL ON FUNCTION app_target_visible(text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_mark(text, text, uuid, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_target_visible(text, uuid), app_set_mark(text, text, uuid, boolean) TO fvoci_app;
