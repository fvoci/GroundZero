-- 문서 템플릿 (D-27 "문서 트리·템플릿"). 소유 권한자가 문서를 템플릿으로 표시하면, 그 문서를 읽을 수 있는 사람이
-- 새 문서를 만들 때 본문을 가져다 쓸 수 있다. 표시는 복제 허용(D-20)처럼 소유 권한자만 한다(재사용 동의).

ALTER TABLE documents ADD COLUMN is_template boolean NOT NULL DEFAULT false;
CREATE INDEX documents_templates ON documents (workspace_id) WHERE is_template AND deleted_at IS NULL;

CREATE FUNCTION app_set_document_template(p_doc uuid, p_template boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR NO KEY UPDATE;
    v_access := app_document_access(p_doc);
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the owner can mark a template' USING ERRCODE = '42501';
    END IF;
    UPDATE documents SET is_template = p_template WHERE id = p_doc;
    PERFORM app_audit_document(p_doc, CASE WHEN p_template THEN 'mark_template' ELSE 'unmark_template' END,
                               '{}'::jsonb);
  END
  $$;

REVOKE ALL ON FUNCTION app_set_document_template(uuid, boolean) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_document_template(uuid, boolean) TO fvoci_app;
