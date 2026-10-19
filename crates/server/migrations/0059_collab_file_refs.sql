-- 제품 로직 점검(2026-10-19, docs/development.md 80절): 아직 리비전이 되지 않은 실시간 본문이 가리키는 파일 버전도
-- 붙잡는다(D-56). 전에는 리비전·기록의 참조만 보아, 실시간 문서에 넣은 이미지의 버전을 그사이 지우거나(버전 삭제,
-- 이전 버전 정리, 휴지통 비우기) 리비전이 된 뒤 그 이미지가 404가 될 수 있었다.
--
-- 방이 저장되지 않은 편집을 가진 채 상태를 저장할 때(`pending`) 본문에서 파일 주소를 뽑아(`app_file_refs`, 리비전과
-- 같은 규칙) 남기고, 리비전이 되거나(pending 아님) 문서가 사라지면 지운다. 함수 본문은 마지막 정의(0050, 0045,
-- 0053)와 같고 표시한 곳만 다르다.

CREATE TABLE document_collab_file_refs (
  document_id uuid NOT NULL REFERENCES document_collab_states (document_id) ON DELETE CASCADE,
  file_id     uuid NOT NULL,
  version     integer NOT NULL,
  PRIMARY KEY (document_id, file_id, version)
);
CREATE INDEX document_collab_file_refs_version ON document_collab_file_refs (file_id, version);

DROP FUNCTION app_system_collab_store(uuid, integer, bytea, boolean);
CREATE FUNCTION app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean,
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
    -- 바뀐 곳: 리비전이 되지 않은 본문의 파일 참조. 본문을 받지 못했으면(NULL) 지난 참조를 그대로 둔다.
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

-- 붙잡는 것: 기록 연결(D-35), 같은 곳 문서의 리비전(D-56), 그리고 바뀐 곳: 같은 곳 문서의 아직 리비전이 되지 않은
-- 실시간 본문.
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
        OR EXISTS (SELECT 1 FROM document_collab_file_refs r
                   JOIN document_collab_states s ON s.document_id = r.document_id AND s.pending
                   JOIN documents d ON d.id = r.document_id
                   JOIN files f ON f.id = r.file_id
                   WHERE r.file_id = p_file AND r.version = p_version AND d.deleted_at IS NULL
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
        -- 바뀐 곳: 아직 리비전이 되지 않은 실시간 본문의 참조도 같은 규칙으로 본다.
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_collab_file_refs ref ON ref.file_id = f.id
             JOIN document_collab_states s ON s.document_id = ref.document_id AND s.pending
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE d.deleted_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                 OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

REVOKE ALL ON FUNCTION app_system_collab_store(uuid, integer, bytea, boolean, jsonb) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_system_collab_store(uuid, integer, bytea, boolean, jsonb) TO fvoci_app;
