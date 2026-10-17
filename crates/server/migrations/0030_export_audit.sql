-- 반출 감사 기록 (D-57). 앱 역할은 감사 기록을 직접 쓰지 못하므로(0002) 반출 기록만 남기는 정의자 함수를 둔다.

-- 워크스페이스 반출(관리자만).
CREATE FUNCTION app_audit_workspace_export(p_workspace uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_member_role(p_workspace) IS DISTINCT FROM 'admin' THEN
      RAISE EXCEPTION 'only an admin can export the workspace' USING ERRCODE = '42501';
    END IF;
    PERFORM app_audit(p_workspace, 'export', 'workspace', p_workspace, '{}'::jsonb);
  END
  $$;

-- 내 개인 자료 반출(개인 영역 기록).
CREATE FUNCTION app_audit_personal_export() RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    SELECT NULL, app_user_id(), app_user_id(), 'export', 'user', app_user_id(), '{}'::jsonb
    WHERE app_user_id() IS NOT NULL
  $$;

REVOKE ALL ON FUNCTION app_audit_workspace_export(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit_personal_export() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_audit_workspace_export(uuid), app_audit_personal_export() TO fvoci_app;
