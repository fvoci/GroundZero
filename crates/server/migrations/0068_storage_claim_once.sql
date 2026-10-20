-- 저장소 정리 꺼내기가 정한 수(p_limit)보다 많이 지울 목록에서 빼지 않게 한다.
--
-- `DELETE ... WHERE (키) IN (SELECT ... LIMIT n FOR UPDATE SKIP LOCKED)`는 계획에 따라 하위 질의를 바깥 행마다 다시
-- 돌린다(지울 목록을 비워진 표로 분석한 뒤의 Nested Loop Semi Join). 그러면 같은 queued_at으로 함께 오른 행(버전 여럿인
-- 파일·파일 여럿을 한 번에 영구 삭제)이 한 번에 모두 목록에서 빠지는데, 작업은 돌려받은 첫 행만 저장소에서 지워
-- 나머지 객체가 저장소에 영구히 남았다. 고를 행을 MATERIALIZED CTE로 한 번만 정하고 그 행만 지운다.
-- 이미 남은 객체는 여기서 찾거나 지우지 않는다(별도 점검·정리 대상).
CREATE OR REPLACE FUNCTION app_system_storage_claim(p_limit integer)
  RETURNS TABLE (storage_key text, storage_upload_id text, attempts integer)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH picked AS MATERIALIZED (
      SELECT s.storage_key, s.storage_upload_id FROM storage_deletions s
      WHERE app_user_id() IS NULL AND s.attempts < 10 AND s.queued_at <= now()
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    DELETE FROM storage_deletions d USING picked p
    WHERE d.storage_key = p.storage_key AND d.storage_upload_id = p.storage_upload_id
    RETURNING d.storage_key, d.storage_upload_id, d.attempts
  $$;
