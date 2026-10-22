-- 대신 저장한 리비전의 실제 편집자(D-95, #71 후속, docs/development.md "#71" 항목).
--
-- 실시간 방(또는 방 없이 남은 저장 대기 상태의 저장)이 리비전을 남길 때 작성자는 그 트랜잭션을 연 사람이다(작성자 =
-- `app_user_id()`, `document_revisions_insert` 정책). 마지막 리비전 뒤로 고친 사람이 쓸 수 없으면 옮기는 사람·실시간을
-- 끄는 관리자·연결된 다른 작성자가 대신 남기는데, 그때 기록은 그 사람이 내용을 쓴 것처럼 보였다.
--
-- 1. `document_revisions.edited_by`: 작성자가 마지막 리비전 뒤로 고친 사람에 들지 않을 때만 그 사람들을 적는다(서버의
--    방 코드만 적는다). NULL은 보통 리비전(작성자가 고친 사람이거나 저장 API·복원 등 방 밖의 길), 빈 배열은 대신
--    저장했으나 고친 사람 기록이 없음(이 변경 전에 저장된 대기 상태), 그 밖에는 고친 사람(마지막이 앞)이다. 작성자의
--    의미는 바꾸지 않는다.
-- 2. `document_collab_states.editors`: 저장 대기 상태에 담긴 편집을 한 사람(마지막 리비전 뒤, 마지막이 앞). 방이 닫힌 뒤
--    남은 상태를 리비전으로 남길 때(옮기기 전·실시간 끄기 전) 작성자 순서와 1번에 쓴다. Yjs 문서에 두지 않는다(클라이언트가
--    바꿀 수 있다). 리비전으로 남은 상태(`pending`이 거짓)는 늘 빈 배열이다.
-- 3. `app_system_collab_store`는 편집자를 받는 서명으로 바꾼다(옛 서명은 지운다, D-92). 본문은 0065와 같고 편집자만 더했다.
-- 4. `app_collab_load`는 상태와 함께 그 편집자를 돌려준다(반환 모양이 바뀌어 지우고 다시 만든다). 권한 확인과 상태를 쓰는
--    조건은 0049와 같다.

ALTER TABLE document_revisions ADD COLUMN edited_by uuid[];
ALTER TABLE document_collab_states ADD COLUMN editors uuid[] NOT NULL DEFAULT '{}';

DROP FUNCTION app_system_collab_store(uuid, integer, bytea, boolean, jsonb);
CREATE FUNCTION app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean,
                                        p_body jsonb, p_editors uuid[])
  RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RAISE EXCEPTION 'system function' USING ERRCODE = '42501';
    END IF;
    -- 지운(휴지통) 문서는 저장만 하지 않고 참조는 그대로 둔다. 낡은 상태만 참조를 지운다.
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND deleted_at IS NULL) THEN
      RETURN false;
    END IF;
    IF NOT app_collab_state_current(p_doc, p_revision) THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      RETURN false;
    END IF;
    -- 바뀐 곳: 편집자를 함께 적는다. 리비전으로 남은 상태에는 남길 편집이 없으므로 비운다.
    INSERT INTO document_collab_states (document_id, revision, state, pending, editors, updated_at)
    VALUES (p_doc, p_revision, p_state, p_pending,
            CASE WHEN p_pending THEN coalesce(p_editors, '{}') ELSE '{}' END, now())
    ON CONFLICT (document_id) DO UPDATE
      SET revision = excluded.revision, state = excluded.state, pending = excluded.pending,
          editors = excluded.editors, updated_at = now();
    IF NOT p_pending THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
    ELSIF p_body IS NOT NULL THEN
      PERFORM 1 FROM file_versions v
      JOIN (SELECT DISTINCT r.file_id, r.version FROM app_file_refs(p_body) r) r
        ON r.file_id = v.file_id AND r.version = v.version
      ORDER BY v.id
      FOR KEY SHARE OF v;
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      INSERT INTO document_collab_file_refs (document_id, file_id, version)
      SELECT DISTINCT p_doc, r.file_id, r.version FROM app_file_refs(p_body) r
      ON CONFLICT DO NOTHING;
    END IF;
    RETURN true;
  END
  $$;

-- 방을 열 때 읽는 것: 저장된 Yjs 상태(그 뒤로 본문이 바뀌지 않았을 때만)와 그 편집자, 문서 본문·리비전. 볼 수 있는
-- 사람만.
DROP FUNCTION app_collab_load(uuid);
CREATE FUNCTION app_collab_load(p_doc uuid)
  RETURNS TABLE (doc_revision integer, title text, body jsonb, state bytea, editors uuid[])
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_collab_access(p_doc);
    RETURN QUERY
    SELECT d.revision, d.title, d.body, cs.state, cs.editors
    FROM documents d
    -- 상태가 이어받은 리비전 뒤로 본문이 바뀌지 않았으면(제목만 바꾼 저장 등) 그 상태와 편집자를 그대로 쓴다.
    LEFT JOIN LATERAL (
      SELECT s.state, s.editors FROM document_collab_states s
      WHERE s.document_id = d.id
        AND NOT EXISTS (
          SELECT 1 FROM document_revisions r
          WHERE r.document_id = d.id AND r.revision > s.revision
            AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                         WHERE b.document_id = d.id AND b.revision = s.revision))
    ) cs ON true
    WHERE d.id = p_doc;
  END
  $$;

REVOKE ALL ON FUNCTION app_system_collab_store(uuid, integer, bytea, boolean, jsonb, uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_collab_load(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_system_collab_store(uuid, integer, bytea, boolean, jsonb, uuid[]),
  app_collab_load(uuid)
TO fvoci_app;
