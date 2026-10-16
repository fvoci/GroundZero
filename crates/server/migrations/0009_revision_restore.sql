-- 리비전 기록 보기와 복원 (D-27, D-29, D-57, D-58).
-- 복원은 옛 내용을 덮어쓰는 것이 아니라 그 내용으로 새 리비전을 하나 더 만든다(이력은 지워지지 않는다).
-- 연결된 업무 완료 상태·강의 진도는 되돌리지 않는다(D-58). 지금은 문서 내용(제목·본문)만 복원한다.

ALTER TABLE document_revisions ADD COLUMN restored_from integer CHECK (restored_from > 0);

-- 결과: 'restored'(새 리비전), 'unchanged'(내용이 이미 같음), 'conflict'(기준 리비전이 지났거나 재시도가 남의 저장 뒤).
-- 볼 수 없으면 P0002, 쓰기 권한이 없으면 42501, 없는 리비전이면 P0002, 저장 형식이 다른 리비전이면 55000,
-- request_id를 저장이나 다른 리비전의 복원에 이미 썼으면 22023.
-- 권한을 먼저 확인한 뒤에만 잠근다(볼 수 없는 문서를 잠가 남의 저장을 막지 못하게). 잠금 순서는 이동과 같다
-- (문서 → 항목·모듈). 잠근 뒤 권한을 다시 확인한다(그사이 이동·보관 반영).
-- 언급 알림은 만들지 않는다: 복원은 새 언급을 쓰는 것이 아니며, 사본·옛 리비전의 언급을 복원한 사람 이름으로
-- 알리지 않는다. 링크는 다시 기록한다.
CREATE FUNCTION app_restore_document_revision(
  p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer
) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_doc      documents%ROWTYPE;
    v_source   document_revisions%ROWTYPE;
    v_replay   document_revisions%ROWTYPE;
    v_next     integer;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_lock_item(v_doc.item_id);
    PERFORM app_require_document(p_doc, 'write');

    -- 같은 사람의 같은 요청 재시도: 같은 리비전의 복원이고 그 결과가 아직 최신일 때만 성공.
    SELECT * INTO v_replay FROM document_revisions
    WHERE document_id = p_doc AND author = v_user AND request_id = p_request_id;
    IF v_replay.document_id IS NOT NULL THEN
      IF v_replay.restored_from IS DISTINCT FROM p_revision THEN
        RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
      END IF;
      RETURN CASE WHEN v_replay.revision = v_doc.revision THEN 'restored' ELSE 'conflict' END;
    END IF;
    IF v_doc.revision <> p_base_revision THEN
      RETURN 'conflict';
    END IF;

    SELECT * INTO v_source FROM document_revisions WHERE document_id = p_doc AND revision = p_revision;
    IF v_source.document_id IS NULL THEN
      RAISE EXCEPTION 'revision not found' USING ERRCODE = 'P0002';
    END IF;
    -- 옛 저장 형식으로 문서를 되돌리지 않는다(형식 변환이 생기면 변환 뒤 복원).
    IF v_source.schema_version <> p_schema_version THEN
      RAISE EXCEPTION 'this revision uses an older document format' USING ERRCODE = '55000';
    END IF;
    IF v_source.title = v_doc.title AND v_source.body = v_doc.body
       AND v_source.schema_version = v_doc.schema_version THEN
      RETURN 'unchanged';
    END IF;

    v_next := v_doc.revision + 1;
    UPDATE documents
    SET title = v_source.title, body = v_source.body, schema_version = v_source.schema_version,
        revision = v_next, updated_by = v_user, updated_at = now()
    WHERE id = p_doc;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id,
                                    restored_from)
    VALUES (p_doc, v_next, v_source.title, v_source.schema_version, v_source.body, v_user, p_request_id,
            p_revision);
    PERFORM app_audit_document(p_doc, 'restore_revision',
                               jsonb_build_object('from', p_revision, 'to', v_next));
    PERFORM app_record_document_references(p_doc, false);
    RETURN 'restored';
  END
  $$;

REVOKE ALL ON FUNCTION app_restore_document_revision(uuid, integer, uuid, integer, integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_restore_document_revision(uuid, integer, uuid, integer, integer) TO fvoci_app;
