-- 스터디 단위 시간 기록과 사용자 시간대 (D-32 "공부에 연결", D-50 "개인 시간 집계는 사용자 시간대").
--
-- 시간 기록의 대상에 스터디 단위를 더한다(강의 회차와 같은 방식, 그 스터디를 볼 수 있어야 함).
-- 사용자 시간대(IANA 이름)를 본인 설정으로 둔다. 다른 사람에게 보이지 않는다(users 표가 아니라 따로 둔 표, RLS 본인).
-- 비우면 화면이 브라우저 시간대를 쓴다.

ALTER TABLE time_entries DROP CONSTRAINT time_entries_target_kind_check;
ALTER TABLE time_entries ADD CONSTRAINT time_entries_target_kind_check
  CHECK (target_kind IN ('task', 'document', 'item', 'episode', 'study_unit'));

CREATE OR REPLACE FUNCTION app_time_target_workspace(p_kind text, p_id uuid) RETURNS uuid
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_kind IS NULL THEN
      RETURN NULL;
    END IF;
    IF p_kind = 'task' AND app_task_access(p_id) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM tasks WHERE id = p_id);
    ELSIF p_kind = 'document' AND app_document_access(p_id) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM documents WHERE id = p_id);
    ELSIF p_kind = 'item' AND app_item_access(p_id) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM items WHERE id = p_id);
    ELSIF p_kind = 'episode'
          AND app_item_access((SELECT item_id FROM course_episodes WHERE id = p_id)) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM course_episodes WHERE id = p_id);
    ELSIF p_kind = 'study_unit'
          AND app_item_access((SELECT item_id FROM study_units WHERE id = p_id)) IS NOT NULL THEN
      RETURN (SELECT workspace_id FROM study_units WHERE id = p_id);
    END IF;
    RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
  END
  $$;

CREATE OR REPLACE FUNCTION app_time_target_label(p_kind text, p_id uuid) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE
      WHEN p_kind = 'task' AND app_task_access(p_id) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_task_reference(id), title) FROM tasks WHERE id = p_id)
      WHEN p_kind = 'document' AND app_document_access(p_id) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_document_reference(id), title) FROM documents WHERE id = p_id)
      WHEN p_kind = 'item' AND app_item_access(p_id) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_item_reference(id), title) FROM items WHERE id = p_id)
      WHEN p_kind = 'episode'
           AND app_item_access((SELECT item_id FROM course_episodes WHERE id = p_id)) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_item_reference(e.item_id), e.number || '회', e.title)
         FROM course_episodes e WHERE e.id = p_id)
      WHEN p_kind = 'study_unit'
           AND app_item_access((SELECT item_id FROM study_units WHERE id = p_id)) IS NOT NULL THEN
        (SELECT concat_ws(' ', app_item_reference(s.item_id), s.number || '.', s.title)
         FROM study_units s WHERE s.id = p_id)
    END
  $$;

CREATE TABLE user_settings (
  user_id   uuid PRIMARY KEY REFERENCES users (id),
  time_zone text NOT NULL DEFAULT '' CHECK (length(time_zone) <= 64)
);

CREATE FUNCTION app_set_time_zone(p_zone text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_zone text := btrim(coalesce(p_zone, ''));
  BEGIN
    IF app_user_id() IS NULL THEN
      RAISE EXCEPTION 'sign in first' USING ERRCODE = '42501';
    END IF;
    IF v_zone <> '' AND NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = v_zone) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_settings (user_id, time_zone) VALUES (app_user_id(), v_zone)
    ON CONFLICT (user_id) DO UPDATE SET time_zone = EXCLUDED.time_zone;
  END
  $$;

ALTER TABLE user_settings ENABLE ROW LEVEL SECURITY;
CREATE POLICY user_settings_select ON user_settings FOR SELECT USING (user_id = app_user_id());
GRANT SELECT ON user_settings TO fvoci_app;
REVOKE ALL ON FUNCTION app_set_time_zone(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_time_zone(text) TO fvoci_app;
