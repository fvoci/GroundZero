-- 0057 검토 반영: 문서 옮기기가 문서를 먼저 잠그고 첨부의 열린 업로드를 나중에 고쳐, 업로드 마치기(업로드 행 →
-- 문서 순서로 잠금)와 엇갈리면 교착(40P01, 409 "다시 시도")이 날 수 있었다. 열린 업로드를 먼저 잠가 순서를 맞춘다.
-- 함수 본문은 0057과 같고 표시한 곳만 다르다.

CREATE OR REPLACE FUNCTION app_move_document(p_doc uuid, p_target uuid, p_item uuid)
  RETURNS TABLE (workspace_id uuid, reference text)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_seen   documents%ROWTYPE;
    v_doc    documents%ROWTYPE;
    v_ref    text;
    v_scope  uuid;
    v_number bigint;
    v_next   numeric;
    v_child  record;
  BEGIN
    SELECT * INTO v_seen FROM documents d WHERE d.id = p_doc;
    -- 바뀐 곳: 업로드 마치기와 같은 순서(업로드 행 → 문서)로 먼저 잠근다.
    PERFORM 1 FROM uploads u
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc))
    ORDER BY u.id
    FOR NO KEY UPDATE;
    PERFORM app_lock_move(p_doc, p_target, p_item);
    PERFORM 1 FROM documents d WHERE d.id = p_doc FOR UPDATE;
    PERFORM app_lock_item((SELECT d.item_id FROM documents d WHERE d.id = p_doc));
    PERFORM app_lock_item(p_item);
    v_doc := app_check_move(p_doc, p_target, p_item);
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    v_ref := app_document_reference(p_doc);
    PERFORM app_audit_document(p_doc, 'move_out', jsonb_build_object('reference', v_ref));
    v_scope := CASE WHEN p_item IS NULL THEN p_target ELSE (SELECT i.module_id FROM items i WHERE i.id = p_item) END;
    IF app_move_keeps_number(p_doc, p_target, p_item) THEN
      v_number := v_doc.number;
    ELSE
      v_number := app_allocate_number(p_target, v_scope, 'document', p_doc);
    END IF;

    -- 자식: 원래 부모 아래, 이동한 문서가 있던 위치와 그다음 형제 사이에 원래 순서대로.
    SELECT min(s.position) INTO v_next
    FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, v_doc.parent_id) s
    WHERE s.id <> p_doc AND s.position > v_doc.position;
    v_next := coalesce(v_next, v_doc.position + 1);
    FOR v_child IN
      SELECT c.id, row_number() OVER (ORDER BY c.position, c.number NULLS LAST, c.created_at, c.id) AS k,
             count(*) OVER () AS n
      FROM documents c WHERE c.parent_id = p_doc
    LOOP
      UPDATE documents d
      SET parent_id = v_doc.parent_id,
          position = v_doc.position + (v_next - v_doc.position) * v_child.k / (v_child.n + 1)
      WHERE d.id = v_child.id;
      PERFORM app_audit_document(v_child.id, 'reparent',
                                 jsonb_build_object('parent', v_doc.parent_id, 'reason', 'parent_moved'));
    END LOOP;

    UPDATE documents d
    SET workspace_id = p_target, owner_user_id = NULL, item_id = p_item,
        placement = CASE WHEN p_item IS NULL THEN 'wiki' ELSE 'module' END,
        scope = CASE WHEN p_item IS NULL THEN 'WIKI' ELSE v_scope::text END,
        number = v_number, parent_id = NULL,
        position = (SELECT coalesce(max(s.position), 0) + 1 FROM app_tree_siblings(p_target, p_item, NULL, NULL) s)
    WHERE d.id = p_doc;
    -- 바뀐 곳: 첨부(휴지통에 있는 것 포함)·그 휴지통 묶음·열린 업로드도 새 자리로. 권한·휴지통·감사·저장 용량·
    -- 내보내기가 모두 이 자리를 본다.
    UPDATE files f SET workspace_id = p_target, owner_user_id = NULL WHERE f.document_id = p_doc;
    UPDATE trash_batches b SET workspace_id = p_target, owner_user_id = NULL
    WHERE b.root_kind = 'file' AND b.restored_at IS NULL AND b.purged_at IS NULL
      AND b.root_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc);
    UPDATE uploads u SET workspace_id = p_target, owner_user_id = NULL
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc));
    PERFORM app_audit_document(p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;
