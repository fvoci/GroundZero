-- 판이 지난 수정은 "다시 읽기"로 알린다(코드 전반 감사 C32, fvoci/GroundZero#77).
--
-- 강의 회차·스터디 단위·항목 기록 고치기는 읽은 판(version)이 지금 판과 다르면 40001로 끝냈다. 서버는 40001을 동시 변경
-- 재시도(교착·잠금 경합)로 보고 "the change collided with another one; try again"으로 바꿔 409를 돌려주므로, 다른 API
-- 클라이언트는 같은 판으로 다시 보내라는 뜻으로 읽었다(다시 보내도 또 실패한다). 웹은 409에서 다시 읽어 자체 문구를 보이므로
-- 화면 영향은 없었다. 판 충돌은 지금 상태에서 할 수 없는 요청이므로 55000(409 `invalid_state`, 메시지 보존)으로 바꾼다.
-- 상태 코드·오류 코드(409 `invalid_state`)는 그대로이고, 메시지만 "…changed; reload it"이 그대로 전달된다(D-83).
-- 본문은 0069(회차·단위)·0036(기록)과 같고 오류 코드만 다르다. 진짜 동시 변경 재시도(업무 열 잠금의 40001)는 그대로 둔다.

CREATE OR REPLACE FUNCTION app_update_episode(
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
      RAISE EXCEPTION 'the episode changed; reload it' USING ERRCODE = '55000';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE OR REPLACE FUNCTION app_update_study_unit(
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
      RAISE EXCEPTION 'the unit changed; reload it' USING ERRCODE = '55000';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

CREATE OR REPLACE FUNCTION app_update_record(
  p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb)
  RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_rec item_records%ROWTYPE;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    IF v_rec.id IS NULL OR app_item_access(v_rec.item_id) IS NULL THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.version <> p_version THEN
      RAISE EXCEPTION 'the record changed; reload it' USING ERRCODE = '55000';
    END IF;
    UPDATE item_records SET title = btrim(p_title), occurred_on = p_occurred, body = coalesce(p_body, ''),
                            fields = coalesce(p_fields, '{}'), updated_by = app_user_id(), updated_at = now(),
                            version = version + 1
    WHERE id = p_record;
    PERFORM app_set_record_files(p_record, v_rec.item_id, p_files);
    RETURN v_rec.version + 1;
  EXCEPTION
    WHEN check_violation THEN
      RAISE EXCEPTION 'invalid record: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

REVOKE ALL ON FUNCTION app_update_episode(uuid, integer, integer, text, text, integer, text, text, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_study_unit(uuid, integer, integer, text, text, text, integer, date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_record(uuid, integer, text, date, text, jsonb, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_update_episode(uuid, integer, integer, text, text, integer, text, text, date),
  app_update_study_unit(uuid, integer, integer, text, text, text, integer, date),
  app_update_record(uuid, integer, text, date, text, jsonb, jsonb)
TO fvoci_app;
