-- 실시간 끄기·켜기 때 편집 보존(사용자 지시 2026-10-17, docs/development.md 59절, 리뷰 반영).
--
-- 1. 저장된 Yjs 상태가 아직 리비전으로 남지 않은 편집을 담고 있는지(`pending`). 방이 리비전을 남기지 못하고
--    닫혔을 때(예: 마지막 사람이 나갈 때 쓸 수 있는 사람이 없음) 실시간을 끄기 전에 이것을 리비전으로 남긴다.
-- 2. 저장 API가 실시간 여부를 읽을 때 워크스페이스 행을 공유 잠금한다. 켜기(행 갱신)와 순서가 정해져, 꺼진 줄
--    알고 쓴 본문이 켜진 뒤에 들어와 방의 편집을 밀어내지 못한다.

ALTER TABLE document_collab_states ADD COLUMN pending boolean NOT NULL DEFAULT false;

DROP FUNCTION app_system_collab_store(uuid, integer, bytea);
CREATE FUNCTION app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean)
  RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RAISE EXCEPTION 'system function' USING ERRCODE = '42501';
    END IF;
    -- 그 리비전 뒤로 본문이 바뀌었으면(복원 등) 이 상태는 낡았다. 제목만 바뀐 것은 괜찮다.
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND deleted_at IS NULL)
       OR EXISTS (SELECT 1 FROM document_revisions r
                  WHERE r.document_id = p_doc AND r.revision > p_revision
                    AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                 WHERE b.document_id = p_doc AND b.revision = p_revision)) THEN
      RETURN false;
    END IF;
    INSERT INTO document_collab_states (document_id, revision, state, pending, updated_at)
    VALUES (p_doc, p_revision, p_state, p_pending, now())
    ON CONFLICT (document_id) DO UPDATE
      SET revision = excluded.revision, state = excluded.state, pending = excluded.pending, updated_at = now();
    RETURN true;
  END
  $$;

-- 실시간을 끄기 전에 리비전으로 남겨야 할 문서(관리자). 지운 문서와, 그 뒤로 본문이 바뀐(쓸모없어진) 상태는 뺀다.
CREATE FUNCTION app_collab_pending(p_workspace uuid) RETURNS SETOF uuid
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_member_role(p_workspace) IS DISTINCT FROM 'admin' THEN
      RAISE EXCEPTION 'only an admin can change realtime editing' USING ERRCODE = '42501';
    END IF;
    RETURN QUERY
      SELECT s.document_id
      FROM document_collab_states s
      JOIN documents d ON d.id = s.document_id
      WHERE s.pending AND d.workspace_id = p_workspace AND d.deleted_at IS NULL
        AND NOT EXISTS (SELECT 1 FROM document_revisions r
                        WHERE r.document_id = s.document_id AND r.revision > s.revision
                          AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                       WHERE b.document_id = s.document_id
                                                         AND b.revision = s.revision))
      ORDER BY s.document_id;
  END
  $$;

-- 저장 API용: 실시간 여부를 워크스페이스 행 공유 잠금과 함께 읽는다(켜기·끄기의 행 잠금과 순서가 정해진다).
CREATE FUNCTION app_document_realtime_locked(p_doc uuid) RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_enabled boolean;
  BEGIN
    IF app_document_access(p_doc) IS NULL THEN
      RETURN false;
    END IF;
    SELECT w.realtime_enabled INTO v_enabled
    FROM documents d JOIN workspaces w ON w.id = d.workspace_id
    WHERE d.id = p_doc
    FOR SHARE OF w;
    RETURN coalesce(v_enabled, false);
  END
  $$;

REVOKE ALL ON FUNCTION app_system_collab_store(uuid, integer, bytea, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_collab_pending(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_document_realtime_locked(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_system_collab_store(uuid, integer, bytea, boolean),
  app_collab_pending(uuid),
  app_document_realtime_locked(uuid)
TO fvoci_app;
