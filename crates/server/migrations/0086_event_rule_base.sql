-- 일정 반복 규칙 저장의 기준 규칙 (코드 전반 감사 W21, D-92).
--
-- 일정 반복 정하기(`PUT /api/events/{id}/recurrence`)는 아무 기준 없이 저장해, 화면을 연 뒤 다른 사람이 바꾼 규칙(예:
-- 수요일 추가)을 옛 규칙으로 말없이 덮었다(감사 기록만 남음). 화면이 편집을 시작한 규칙(`p_base`, 반복이 없었으면
-- NULL)을 보내고, 잠근 뒤의 규칙이 그것과 다르면 아무것도 바꾸지 않고 'conflict'(409, 최신 일정 포함)를 돌려준다.
-- 지금 규칙이 이미 보낸 규칙과 같으면 충돌이 아니고(같은 요청의 재시도) 아무것도 쓰지 않는다(판·감사 기록을 다시 올리면
-- 응답을 잃고 다시 보낸 화면이 판이 두 번 오른 것으로 보고 일정 입력의 기준을 놓쳐, 제 변경과 부딪히는 409를 받았다).
-- 비교는 NULL 칸을 빼고 한다(저장 경로에 따라
-- `count: null`이 있거나 없다). 일정 판(`version`)이 아니라 규칙만 보므로 그사이 제목만 바뀌었으면 충돌이 아니다.
-- 기준 없는 옛 서명은 없앤다(D-92).
-- 함수 본문은 0073의 마지막 정의와 같고 기준 비교와 돌려주는 값만 다르다.

DROP FUNCTION app_set_event_recurrence(uuid, jsonb, uuid[]);

-- id 1개.
CREATE FUNCTION app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    IF p_rule IS NOT NULL THEN
      PERFORM app_check_recurrence(p_rule);
    END IF;
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF jsonb_strip_nulls(v_event.recurrence) IS DISTINCT FROM jsonb_strip_nulls(p_rule)
       AND jsonb_strip_nulls(v_event.recurrence) IS DISTINCT FROM jsonb_strip_nulls(p_base) THEN
      RETURN 'conflict';
    END IF;
    IF jsonb_strip_nulls(v_event.recurrence) IS NOT DISTINCT FROM jsonb_strip_nulls(p_rule) THEN
      RETURN 'updated';
    END IF;
    IF (p_rule ->> 'until')::date < coalesce(v_event.start_date, (v_event.starts_at AT TIME ZONE v_event.timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    UPDATE events SET recurrence = p_rule, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'set_recurrence', 'event', p_event,
                      jsonb_build_object('from', v_event.recurrence, 'to', p_rule));
    RETURN 'updated';
  END
  $$;

REVOKE ALL ON FUNCTION app_set_event_recurrence(uuid, jsonb, jsonb, uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_event_recurrence(uuid, jsonb, jsonb, uuid[]) TO fvoci_app;
