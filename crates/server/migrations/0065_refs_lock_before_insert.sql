-- 검토 9번(docs/development.md 81절): 파일 참조를 적는 두 곳(실시간 상태 저장 0062, 리비전 저장 0046)은 참조 행을 넣은
-- 뒤에 가리키는 버전 행을 공유 잠금했다. 넣은 뒤 잠그기 전의 틈에 자동 정리(이전 버전 정리)가 먼저 그 버전을 잠그면
-- 아직 보이지 않는 참조를 못 보고 지웠다. 이제 버전 행을 먼저 잠그고 참조를 넣는다. 먼저 잠근 쪽이 이긴다: 적는 쪽이
-- 먼저면 정리는 그 버전을 건너뛰고(`FOR UPDATE SKIP LOCKED`), 정리가 먼저면 적는 쪽은 정리가 끝난 뒤 진행한다.
-- 함수 본문은 앞선 정의와 같고 "바뀐 곳"만 다르다.

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
      -- 바뀐 곳: 가리키는 버전 행을 참조를 넣기 전에 공유 잠금한다.
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

CREATE OR REPLACE FUNCTION app_record_file_refs() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    -- 바뀐 곳: 가리키는 버전 행을 참조를 넣기 전에 공유 잠금한다.
    PERFORM 1 FROM file_versions v
    JOIN (SELECT DISTINCT r.file_id, r.version FROM app_file_refs(NEW.body) r) r
      ON r.file_id = v.file_id AND r.version = v.version
    ORDER BY v.id
    FOR KEY SHARE OF v;
    INSERT INTO document_file_refs (document_id, revision, file_id, version)
    SELECT NEW.document_id, NEW.revision, r.file_id, r.version FROM app_file_refs(NEW.body) r
    ON CONFLICT DO NOTHING;
    RETURN NULL;
  END
  $$;
