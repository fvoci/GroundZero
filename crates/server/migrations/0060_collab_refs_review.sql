-- 0059 검토 반영(docs/development.md 80절).
--
-- 1. 낡은 실시간 상태(그 뒤 리비전 본문이 바뀜: 복원, 실시간을 끈 뒤 저장 등)는 다시 쓰이지 않으므로 그 참조로 파일
--    버전을 붙잡지 않는다(`app_collab_pending`과 같은 판단). 전에는 그런 상태의 참조가 영영 남아 버전 삭제·휴지통
--    비우기를 막았다. 상태 저장이 낡음으로 거절될 때도 그 참조를 지운다.
-- 2. 휴지통에 넣은 문서의 아직 리비전이 되지 않은 실시간 본문도 리비전과 같은 규칙으로 붙잡는다(문서를 되살리면
--    그 상태를 다시 연다). 전에는 문서를 휴지통에 넣는 순간 그 참조를 무시했다.
-- 함수 본문은 0059와 같고 표시한 곳만 다르다.

-- 실시간 상태가 아직 쓸모 있는지: 그 상태의 리비전 뒤로 본문이 바뀌지 않았다(0050 `app_collab_pending`과 같다).
CREATE FUNCTION app_collab_state_current(p_doc uuid, p_revision integer) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT NOT EXISTS (SELECT 1 FROM document_revisions r
                       WHERE r.document_id = p_doc AND r.revision > p_revision
                         AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                      WHERE b.document_id = p_doc AND b.revision = p_revision))
  $$;

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
    -- 그 리비전 뒤로 본문이 바뀌었으면(복원 등) 이 상태는 낡았다. 제목만 바뀐 것은 괜찮다.
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND deleted_at IS NULL)
       OR NOT app_collab_state_current(p_doc, p_revision) THEN
      -- 바뀐 곳: 낡은 상태의 참조는 남기지 않는다.
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

CREATE OR REPLACE FUNCTION app_version_referenced(p_file uuid, p_version integer, p_version_id uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (SELECT 1 FROM record_files WHERE version_id = p_version_id)
        OR EXISTS (SELECT 1 FROM document_file_refs r
                   JOIN documents d ON d.id = r.document_id
                   JOIN files f ON f.id = r.file_id
                   WHERE r.file_id = p_file AND r.version = p_version
                     AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                       OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
        -- 바뀐 곳: 휴지통의 문서도 리비전과 같게 보고, 낡은 상태는 빼고.
        OR EXISTS (SELECT 1 FROM document_collab_file_refs r
                   JOIN document_collab_states s ON s.document_id = r.document_id AND s.pending
                   JOIN documents d ON d.id = r.document_id
                   JOIN files f ON f.id = r.file_id
                   WHERE r.file_id = p_file AND r.version = p_version
                     AND app_collab_state_current(s.document_id, s.revision)
                     AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                       OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

CREATE OR REPLACE FUNCTION app_batch_keeps_referenced(p_batch trash_batches) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    WITH bf AS (SELECT file_id FROM app_batch_files(p_batch)),
         gone AS (SELECT document_id FROM app_batch_documents(p_batch))
    SELECT EXISTS (
             SELECT 1 FROM bf
             JOIN file_versions v ON v.file_id = bf.file_id
             JOIN record_files rf ON rf.version_id = v.id
             JOIN item_records r ON r.id = rf.record_id
             JOIN items i ON i.id = r.item_id
             JOIN modules m ON m.id = i.module_id
             WHERE NOT (p_batch.root_kind = 'item' AND r.item_id = p_batch.root_id)
               AND NOT (p_batch.root_kind = 'module' AND i.module_id = p_batch.root_id)
               AND NOT app_batch_expired(i.trash_batch_id)
               AND NOT app_batch_expired(m.trash_batch_id))
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_file_refs ref ON ref.file_id = f.id
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                 OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
        -- 바뀐 곳: 리비전 줄과 같은 규칙(휴지통 만료), 낡은 상태는 빼고.
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_collab_file_refs ref ON ref.file_id = f.id
             JOIN document_collab_states s ON s.document_id = ref.document_id AND s.pending
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE app_collab_state_current(s.document_id, s.revision)
               AND NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                 OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

REVOKE ALL ON FUNCTION app_collab_state_current(uuid, integer) FROM PUBLIC;
