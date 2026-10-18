-- 운영 로그(D-83): 주기 작업이 실패를 삼키고 남기는 경고에 PostgreSQL 오류 원문(SQLERRM, 입력 값이 섞일 수 있음) 대신
-- 오류 코드(SQLSTATE)만 남긴다. 함수 본문은 마지막 정의(0046, 0025)와 같고 경고 문구만 다르다.

CREATE OR REPLACE FUNCTION app_system_purge_expired() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_ws    uuid;
    v_made  integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_ws IN
      SELECT id FROM workspaces
      WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now()
      ORDER BY deleted_at
    LOOP
      BEGIN
        IF app_purge_workspace_as(v_ws, true) THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of deleted workspace % failed with SQLSTATE %', v_ws, SQLSTATE;
      END;
    END LOOP;
    FOR v_batch IN
      SELECT * FROM trash_batches WHERE restored_at IS NULL AND purged_at IS NULL ORDER BY deleted_at
    LOOP
      CONTINUE WHEN app_trash_expires_at(v_batch) > now();
      BEGIN
        PERFORM app_purge_batch_as(v_batch.id, true);
        IF (SELECT purged_at FROM trash_batches WHERE id = v_batch.id) IS NOT NULL THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'purge of trash batch % failed with SQLSTATE %', v_batch.id, SQLSTATE;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;

CREATE OR REPLACE FUNCTION app_system_recurrence_tick() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_id IN
      SELECT s.id FROM task_series s
      JOIN tasks r ON r.id = s.root_task_id
      JOIN items i ON i.id = s.item_id
      JOIN workspaces w ON w.id = s.workspace_id
      WHERE s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL AND w.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      BEGIN
        v_made := v_made + app_sync_task_series_as(v_id, true);
      EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'recurrence sync of series % failed with SQLSTATE %', v_id, SQLSTATE;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;
