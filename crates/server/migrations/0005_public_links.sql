-- 비로그인 공개 링크 (D-20): 소유 권한자가 명시적으로 만들고, 만료·철회할 수 있다.
-- 링크 토큰 원문은 링크에만 있고 DB에는 SHA-256 해시만 있다. 공개 보기는 읽기 전용이다.

CREATE TABLE public_links (
  id          uuid PRIMARY KEY,
  document_id uuid NOT NULL REFERENCES documents (id),
  token_hash  bytea NOT NULL UNIQUE,
  created_by  uuid NOT NULL REFERENCES users (id),
  created_at  timestamptz NOT NULL DEFAULT now(),
  expires_at  timestamptz,
  revoked_at  timestamptz
);
CREATE INDEX public_links_document ON public_links (document_id);

ALTER TABLE public_links ENABLE ROW LEVEL SECURITY;
-- 링크 목록은 소유 권한자만 본다. 토큰 해시는 응답에 내보내지 않는다(서버 코드).
CREATE POLICY public_links_select ON public_links FOR SELECT
  USING (app_document_access(document_id) = 'owner');
GRANT SELECT ON public_links TO fvoci_app;

CREATE FUNCTION app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamptz)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_id uuid;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document(p_doc, 'owner');
    IF p_expires_at IS NOT NULL AND p_expires_at <= now() THEN
      RAISE EXCEPTION 'expiry must be in the future' USING ERRCODE = '22023';
    END IF;
    INSERT INTO public_links (document_id, token_hash, created_by, expires_at)
    VALUES (p_doc, p_token_hash, app_user_id(), p_expires_at)
    RETURNING id INTO v_id;
    PERFORM app_audit_document(p_doc, 'create_public_link',
                               jsonb_build_object('link', v_id, 'expires_at', p_expires_at));
    RETURN v_id;
  END
  $$;

CREATE FUNCTION app_revoke_public_link(p_link uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_doc uuid;
  BEGIN
    SELECT document_id INTO v_doc FROM public_links WHERE id = p_link AND revoked_at IS NULL;
    IF v_doc IS NULL OR app_document_access(v_doc) IS NULL THEN
      RAISE EXCEPTION 'link not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM 1 FROM documents WHERE id = v_doc FOR UPDATE;
    PERFORM app_require_document(v_doc, 'owner');
    UPDATE public_links SET revoked_at = now() WHERE id = p_link;
    PERFORM app_audit_document(v_doc, 'revoke_public_link', jsonb_build_object('link', p_link));
  END
  $$;

-- 비로그인 조회. 철회·만료된 링크는 없는 것과 같다.
CREATE FUNCTION app_public_document(p_token_hash bytea)
  RETURNS TABLE (title text, body jsonb, schema_version integer, updated_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT d.title, d.body, d.schema_version, d.updated_at
    FROM public_links l JOIN documents d ON d.id = l.document_id
    WHERE l.token_hash = p_token_hash
      AND l.revoked_at IS NULL
      AND (l.expires_at IS NULL OR l.expires_at > now())
  $$;

REVOKE ALL ON FUNCTION app_create_public_link(uuid, bytea, timestamptz) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_revoke_public_link(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_public_document(bytea) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_create_public_link(uuid, bytea, timestamptz),
  app_revoke_public_link(uuid),
  app_public_document(bytea)
TO fvoci_app;
