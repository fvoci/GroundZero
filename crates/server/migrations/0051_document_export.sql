-- 문서 한 건 내보내기(D-75). 권한은 첨부 내려받기(0028 `app_file_can_download`)와 같다: 고칠 수 있는 사람, 또는
-- 볼 수 있고 소유자가 복제·내려받기를 허용한(`allow_copy`) 문서. 개인 노트·공유 문서도 같은 규칙이다.

CREATE FUNCTION app_document_can_export(p_doc uuid) RETURNS boolean
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_document_access(p_doc);
  BEGIN
    IF v_access IS NULL THEN
      RETURN false;
    END IF;
    IF v_access IN ('owner', 'write') THEN
      RETURN true;
    END IF;
    RETURN coalesce((SELECT allow_copy FROM documents WHERE id = p_doc AND deleted_at IS NULL), false);
  END
  $$;

-- 내보내기 기록. 워크스페이스 문서는 그 워크스페이스 감사 기록, 개인 노트는 노트 소유자의 개인 영역 기록에 남긴다
-- (앱 역할은 감사 기록을 직접 쓰지 못한다, 0002·0030).
CREATE FUNCTION app_audit_document_export(p_doc uuid, p_revision integer, p_format text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_workspace uuid;
    v_owner     uuid;
  BEGIN
    IF app_document_access(p_doc) IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT app_document_can_export(p_doc) THEN
      RAISE EXCEPTION 'exporting this document is not allowed' USING ERRCODE = '42501';
    END IF;
    IF p_format NOT IN ('markdown', 'zip', 'pdf') THEN
      RAISE EXCEPTION 'unknown export format' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id, owner_user_id INTO v_workspace, v_owner FROM documents WHERE id = p_doc;
    IF v_owner IS NULL THEN
      PERFORM app_audit(v_workspace, 'export', 'document', p_doc,
                        jsonb_build_object('revision', p_revision, 'format', p_format));
    ELSE
      INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (NULL, v_owner, app_user_id(), 'export', 'document', p_doc,
              jsonb_build_object('revision', p_revision, 'format', p_format));
    END IF;
  END
  $$;

REVOKE ALL ON FUNCTION app_document_can_export(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_audit_document_export(uuid, integer, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_document_can_export(uuid),
  app_audit_document_export(uuid, integer, text)
TO fvoci_app;
