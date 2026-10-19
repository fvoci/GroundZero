-- 0060 검토 반영(docs/development.md 80절): 상태 저장이 거절될 때 참조를 지우는 것은 낡은 상태일 때만이다. 문서를
-- 휴지통에 넣은 뒤의 마지막 저장(방이 닫히며)도 거절되는데, 그때 참조를 지우면 휴지통 문서의 아직 리비전이 되지 않은
-- 본문이 가리키는 파일 버전이 풀려(0060의 2번과 반대) 되살린 뒤 이미지가 404가 될 수 있었다.
-- 함수 본문은 0060과 같고 표시한 곳만 다르다.

CREATE OR REPLACE FUNCTION app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean,
                                                   p_body jsonb)
  RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RAISE EXCEPTION 'system function' USING ERRCODE = '42501';
    END IF;
    -- 바뀐 곳: 지운(휴지통) 문서는 저장만 하지 않고 참조는 그대로 둔다. 낡은 상태만 참조를 지운다.
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND deleted_at IS NULL) THEN
      RETURN false;
    END IF;
    IF NOT app_collab_state_current(p_doc, p_revision) THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      RETURN false;
    END IF;
    INSERT INTO document_collab_states (document_id, revision, state, pending, updated_at)
    VALUES (p_doc, p_revision, p_state, p_pending, now())
    ON CONFLICT (document_id) DO UPDATE
      SET revision = excluded.revision, state = excluded.state, pending = excluded.pending, updated_at = now();
    IF NOT p_pending THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
    ELSIF p_body IS NOT NULL THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      INSERT INTO document_collab_file_refs (document_id, file_id, version)
      SELECT DISTINCT p_doc, r.file_id, r.version FROM app_file_refs(p_body) r
      ON CONFLICT DO NOTHING;
    END IF;
    RETURN true;
  END
  $$;
