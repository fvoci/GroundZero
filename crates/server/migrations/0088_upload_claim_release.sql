-- 업로드 마무리의 일시적 실패 뒤 점유 풀기 (코드 전반 감사 S11).
--
-- 업로드 마치기(`POST /api/uploads/{id}/complete`)는 `app_begin_complete`로 상태를 'completing'으로 바꿔 커밋한 뒤 저장소
-- 합치기·내용 확인·기록을 한다. 그 사이 일시적 오류(저장소 읽기 오류, 연결 풀 대기 초과, 교착 등)가 나면 상태는
-- 'completing' 그대로였고, 다시 요청할 때마다 "마치는 중"(55000, 409)을 5분 동안 받았다. 웹은 약 60초 다시 묻고 실패로
-- 알렸다. 마무리 작업이 멈춘 뒤 서버가 이 함수로 점유를 바로 오래된 것으로 돌려, 다음 요청이 기존의 넘겨받기 경로(저장소에
-- 이미 합친 것은 그대로 확인)로 이어 가게 한다. 닫거나(실패) 되돌린(조각 다시 받기) 업로드는 'completing'이 아니라 바꾸지
-- 않는다.
--
-- 점유는 마치기 시도마다 `state_changed_at`(`app_begin_complete`가 그 시도에 매긴 시각)으로 구별한다. 오래 걸린 앞 시도가
-- 5분 뒤 다른 요청에 넘겨진 다음 늦게 돌아와도, 새 점유를 풀거나(세 번째 마치기가 동시에 시작됨) 업로드를 되돌리거나
-- (받은 조각 삭제) 기록·닫지 않게 한다. 풀기는 그 시도의 점유일 때만 하고, 서버는 되돌리기·기록·닫기 바로 앞에
-- `app_upload_claim_held`로 같은 트랜잭션에서 행을 잠근 채 점유를 확인한다.

CREATE FUNCTION app_release_upload_claim(p_upload uuid, p_claim timestamptz) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE uploads SET state_changed_at = now() - interval '5 minutes' - interval '1 second'
    WHERE id = p_upload AND user_id = app_user_id() AND state = 'completing' AND state_changed_at = p_claim
  $$;

-- 이 시도가 아직 마무리를 맡고 있는지(행을 잠근다). 이미 마쳤으면('done') 참이다(기록은 같은 결과를 돌려주고, 되돌리기와
-- 닫기는 아무것도 바꾸지 않는다).
CREATE FUNCTION app_upload_claim_held(p_upload uuid, p_claim timestamptz) RETURNS boolean
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce((SELECT state = 'done' OR (state = 'completing' AND state_changed_at = p_claim)
                     FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE), false)
  $$;

REVOKE ALL ON FUNCTION app_release_upload_claim(uuid, timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_release_upload_claim(uuid, timestamptz) TO fvoci_app;
REVOKE ALL ON FUNCTION app_upload_claim_held(uuid, timestamptz) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_upload_claim_held(uuid, timestamptz) TO fvoci_app;
