-- 문서 설정은 바꾸는 칸만 받는다 (코드 전반 감사 W6).
--
-- 문서 설정(`PUT /api/documents/{id}/settings`)은 공개 범위와 복제 허용을 늘 함께 받았다. 화면은 한 칸만 바꿔도 다른
-- 칸을 연 뒤 다시 읽지 않은 값으로 보냈고, 그사이 다른 소유 권한자가 켠 비공개·복제 금지를 말없이 되돌렸다. 이제 두
-- 칸 모두 보낼 때만 바꾸고, NULL이면 행을 잠근 뒤의 지금 값 그대로다. 화면은 사용자가 바꾼 칸만 보낸다.
-- 함수 본문은 0071의 마지막 정의와 같고 두 칸을 고르는 부분만 다르다.

-- id 2개: 공개 범위·복제 허용 감사.
CREATE OR REPLACE FUNCTION app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean,
                                                     p_ids uuid[])
  RETURNS void
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc        documents%ROWTYPE;
    v_visibility text;
    v_allow_copy boolean;
  BEGIN
    -- 행을 먼저 잠그고 권한을 본다. 동시에 이동된 문서에 이전 소유 권한으로 설정을 바꾸지 못하게 한다.
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    -- 보내지 않은 칸은 잠근 뒤의 지금 값 그대로.
    v_visibility := coalesce(p_visibility, v_doc.visibility);
    v_allow_copy := coalesce(p_allow_copy, v_doc.allow_copy);
    IF v_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF v_doc.owner_user_id IS NOT NULL AND v_visibility <> 'private' THEN
      RAISE EXCEPTION 'personal notes are always private' USING ERRCODE = '22023';
    END IF;
    IF app_document_access(p_doc) <> 'owner'
       AND ((v_visibility = 'workspace' AND v_doc.visibility <> 'workspace')
            OR (v_allow_copy AND NOT v_doc.allow_copy)) THEN
      RAISE EXCEPTION 'archived documents can only be restricted' USING ERRCODE = '55000';
    END IF;
    UPDATE documents SET visibility = v_visibility, allow_copy = v_allow_copy WHERE id = p_doc;
    IF v_doc.visibility IS DISTINCT FROM v_visibility THEN
      PERFORM app_audit_document(app_id(p_ids, 1), p_doc, 'change_visibility',
                                 jsonb_build_object('from', v_doc.visibility, 'to', v_visibility));
    END IF;
    IF v_doc.allow_copy IS DISTINCT FROM v_allow_copy THEN
      PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'change_copy_policy',
                                 jsonb_build_object('allow_copy', v_allow_copy));
    END IF;
  END
  $$;
