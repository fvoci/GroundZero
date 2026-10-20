-- 함께 고치는 행의 덮어쓰기를 서버에서 막는다.
--
-- 강의 회차·스터디 공통 단위: 고치기가 id만으로 행을 통째로 바꿔, 같은 내용을 보고 고친 두 사람 가운데 뒤에 저장한
-- 사람이 앞사람의 저장을 조용히 덮었다(화면이 저장 전에 다시 읽어 비교했지만 그 사이의 저장은 막지 못한다). 기록과 같이
-- 판 번호(version)를 두고, 읽은 판일 때만 바꾸는 조건을 UPDATE에 넣어 확인과 바꾸기를 한 문장으로 한다. 판이 다르면
-- 40001(409)이고 아무것도 바뀌지 않는다. 돌려주는 값은 새 판.
--
-- 체크리스트 항목: 고치기가 문구와 완료를 함께 받아, 문구를 고치면 화면에 남은 옛 완료 상태로, 완료를 바꾸면 옛 문구로
-- 다른 사람의 변경을 되돌렸다. NULL로 보낸 칸은 그대로 둔다(바꾸려는 칸만 보낸다). 서명은 같다.
ALTER TABLE course_episodes ADD COLUMN version integer NOT NULL DEFAULT 1 CHECK (version > 0);
ALTER TABLE study_units ADD COLUMN version integer NOT NULL DEFAULT 1 CHECK (version > 0);

DROP FUNCTION app_update_episode(uuid, integer, text, text, integer, text, text, date);
CREATE FUNCTION app_update_episode(
  p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer,
  p_url text, p_materials text, p_planned date) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_version integer;
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_item);
    UPDATE course_episodes SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      duration_seconds = p_duration, url = btrim(coalesce(p_url, '')), materials = coalesce(p_materials, ''),
      planned_on = p_planned, version = version + 1, updated_at = now()
    WHERE id = p_episode AND version = p_version
    RETURNING version INTO v_version;
    IF v_version IS NULL THEN
      RAISE EXCEPTION 'the episode changed; reload it' USING ERRCODE = '40001';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

DROP FUNCTION app_update_study_unit(uuid, integer, text, text, text, integer, date);
CREATE FUNCTION app_update_study_unit(
  p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer,
  p_planned date) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
    v_version integer;
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_item);
    UPDATE study_units SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      unit = p_kind, amount = p_amount, planned_on = p_planned, version = version + 1, updated_at = now()
    WHERE id = p_unit AND version = p_version
    RETURNING version INTO v_version;
    IF v_version IS NULL THEN
      RAISE EXCEPTION 'the unit changed; reload it' USING ERRCODE = '40001';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE OR REPLACE FUNCTION app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_task_access(v_task) IS NULL THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    PERFORM app_lock_item((SELECT item_id FROM tasks WHERE id = v_task));
    PERFORM app_require_task_write(v_task);
    UPDATE checklist_items SET text = coalesce(btrim(p_text), text), checked = coalesce(p_checked, checked),
      updated_at = now()
    WHERE id = p_id;
  END
  $$;

REVOKE ALL ON FUNCTION app_update_episode(uuid, integer, integer, text, text, integer, text, text, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_study_unit(uuid, integer, integer, text, text, text, integer, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_update_episode(uuid, integer, integer, text, text, integer, text, text, date),
  app_update_study_unit(uuid, integer, integer, text, text, text, integer, date)
TO fvoci_app;
