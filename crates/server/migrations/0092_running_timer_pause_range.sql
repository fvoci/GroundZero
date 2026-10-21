-- 진행 중 타이머의 시작 변경이 누적 멈춤보다 짧은 구간을 만들지 않게 한다(P-27, 코드 전반 감사 델타 SD3).
--
-- 진행 중 분기는 시작이 지금(멈춰 있으면 멈춘 때)보다 늦지 않은지만 봐서, 한 시간을 멈췄던 타이머의 시작을 지금으로
-- 옮기는 수정을 받았다. 그 상태로 멈추면 멈춤이 구간보다 긴 끝난 기록이 저장되고, 그 뒤에는 메모·종류·대상만 고쳐도
-- 끝난 기록 분기의 범위 검사(멈춤 > 구간)에 걸려 400이 났다(멈춘 시간을 직접 줄이기 전에는 고칠 수 없음).
-- - 진행 중 분기: 끝난 기록과 같은 검사를 지금까지의 구간(멈춰 있으면 멈춘 때까지)에 한다. 멈춰 있는 동안의 멈춤은
--   아직 누적에 없으므로 멈춘 때까지의 구간과 비교한다. 이 검사는 P-27의 두 안(멈춤이 구간을 넘지 않을 때만 허용, 멈춘
--   동안 시작 변경 자체를 막음)에 모두 필요하다. 멈춘 동안 아예 막을지는 그대로 사용자 결정이다.
-- - 끝난 기록 분기: 시작·끝·멈춤을 바꿀 때만 범위를 검사한다. 이미 저장된 어긋난 기록도 메모·종류·대상은 고칠 수 있고,
--   데이터는 바꾸지 않는다. 끝을 지우는 수정(`p_ended IS NULL`)은 지금처럼 거절한다.
-- 본문은 0073과 같고 두 검사만 다르다.

CREATE OR REPLACE FUNCTION app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamptz,
                                                 p_ended timestamptz, p_paused integer, p_kind text, p_memo text,
                                                 p_target_kind text, p_target uuid, p_ids uuid[]) RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
    v_ws    uuid;
  BEGIN
    IF v_entry.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    IF v_entry.ended_at IS NULL THEN
      IF p_ended IS NOT NULL OR p_paused IS DISTINCT FROM v_entry.paused_seconds
         OR p_started > coalesce(v_entry.paused_at, now()) THEN
        RAISE EXCEPTION 'a running timer only changes its start, kind, memo and target' USING ERRCODE = '22023';
      END IF;
      IF v_entry.paused_seconds > extract(epoch FROM coalesce(v_entry.paused_at, now()) - p_started) THEN
        RAISE EXCEPTION 'the start leaves less time than the timer was paused' USING ERRCODE = '22023';
      END IF;
    ELSIF p_ended IS NULL THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    ELSIF (p_started, p_ended, p_paused) IS DISTINCT FROM (v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds)
          AND (p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started)) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    -- 대상을 바꿀 때만 지금 볼 수 있는지 확인한다(이미 연결된 대상은 볼 수 없게 되어도 그대로 둘 수 있다).
    IF (p_target_kind, p_target) IS DISTINCT FROM (v_entry.target_kind, v_entry.target_id) THEN
      v_ws := app_time_target_workspace(p_target_kind, p_target);
    ELSE
      v_ws := v_entry.workspace_id;
    END IF;
    INSERT INTO time_entry_revisions (id, entry_id, user_id, started_at, ended_at, paused_seconds, kind, memo,
                                      target_kind, target_id)
    VALUES (app_id(p_ids, 1), p_entry, v_entry.user_id, v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds,
            v_entry.kind, v_entry.memo, v_entry.target_kind, v_entry.target_id);
    UPDATE time_entries SET
      started_at = p_started, ended_at = p_ended, paused_seconds = p_paused, kind = p_kind,
      memo = coalesce(p_memo, ''), target_kind = p_target_kind, target_id = p_target, workspace_id = v_ws,
      version = version + 1, updated_at = now()
    WHERE id = p_entry;
    RETURN 'updated';
  END
  $$;
REVOKE ALL ON FUNCTION app_update_time_entry(uuid, integer, timestamptz, timestamptz, integer, text, text, text, uuid,
                                             uuid[]) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_update_time_entry(uuid, integer, timestamptz, timestamptz, integer, text, text, text,
                                                uuid, uuid[]) TO fvoci_app;
