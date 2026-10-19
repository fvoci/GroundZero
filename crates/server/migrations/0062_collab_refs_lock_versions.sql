-- 마무리 ③(docs/development.md 80절): 실시간 상태의 파일 참조를 적을 때도 리비전 참조(0046 `app_record_file_refs`)와
-- 같은 규칙으로 그 버전 행을 공유 잠금한다. 전에는 상태 저장 트랜잭션이 끝나기 전에 자동 정리(이전 버전 정리)가
-- 같은 버전을 지울 수 있었고(아직 보이지 않는 참조), 저장은 그 뒤 없는 버전을 가리켰다. 잠그면 정리는 그 버전을
-- 건너뛰고(`FOR UPDATE SKIP LOCKED`), 저장이 끝난 뒤에는 참조를 보고 남긴다.
-- 함수 본문은 0061과 같고 표시한 곳만 다르다.

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
    -- 지운(휴지통) 문서는 저장만 하지 않고 참조는 그대로 둔다. 낡은 상태만 참조를 지운다.
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
      -- 바뀐 곳: 리비전 참조처럼 가리키는 버전 행을 공유 잠금한다(동시에 도는 자동 정리가 이 저장을 기다린 뒤 참조를 본다).
      PERFORM 1 FROM file_versions v
      JOIN document_collab_file_refs r ON r.file_id = v.file_id AND r.version = v.version
      WHERE r.document_id = p_doc
      ORDER BY v.id
      FOR KEY SHARE OF v;
    END IF;
    RETURN true;
  END
  $$;
