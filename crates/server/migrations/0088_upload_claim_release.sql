-- 업로드 마무리의 일시적 실패 뒤 점유 풀기 (코드 전반 감사 S11).
--
-- 업로드 마치기(`POST /api/uploads/{id}/complete`)는 `app_begin_complete`로 상태를 'completing'으로 바꿔 커밋한 뒤 저장소
-- 합치기·내용 확인·기록을 한다. 그 사이 일시적 오류(저장소 읽기 오류, 연결 풀 대기 초과, 교착 등)가 나면 상태는
-- 'completing' 그대로였고, 다시 요청할 때마다 "마치는 중"(55000, 409)을 5분 동안 받았다. 웹은 약 60초 다시 묻고 실패로
-- 알렸다. 마무리 작업이 멈춘 뒤 서버가 이 함수로 점유를 바로 오래된 것으로 돌려, 다음 요청이 기존의 넘겨받기 경로(저장소에
-- 이미 합친 것은 그대로 확인)로 이어 가게 한다. 닫거나(실패) 되돌린(조각 다시 받기) 업로드는 'completing'이 아니라 바꾸지
-- 않는다.

CREATE FUNCTION app_release_upload_claim(p_upload uuid) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE uploads SET state_changed_at = now() - interval '5 minutes' - interval '1 second'
    WHERE id = p_upload AND user_id = app_user_id() AND state = 'completing'
  $$;

REVOKE ALL ON FUNCTION app_release_upload_claim(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_release_upload_claim(uuid) TO fvoci_app;
