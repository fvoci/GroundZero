-- @ 링크·사용자 언급·역방향 링크·언급 알림 (D-18, D-27, D-39).
--
-- 본문에는 참조 노드 {"type":"reference","attrs":{"kind":"item|document|user","id":"<uuid>"}}만 둔다.
-- 표시 이름(번호·제목)은 저장하지 않고 볼 때 권한을 확인해 만든다. 대상이 나중에 비공개가 되어도
-- 본문을 읽는 사람에게 제목이 드러나지 않는다.
-- 링크·언급 목록은 저장할 때 서버가 저장된 본문에서 뽑는다(클라이언트가 보낸 목록을 믿지 않는다).
-- 언급은 접근 권한을 주지 않는다. 알림은 언급된 사람이 문서를 볼 수 있을 때만 한 번 만들고,
-- 접근이 사라지면 목록에서 보이지 않는다.

CREATE TABLE document_links (
  document_id uuid NOT NULL REFERENCES documents (id),
  target_kind text NOT NULL CHECK (target_kind IN ('item', 'document')),
  target_id   uuid NOT NULL,
  PRIMARY KEY (document_id, target_kind, target_id)
);
CREATE INDEX document_links_target ON document_links (target_kind, target_id);

-- 문서마다 사람마다 한 번만 알린다. 동시 자동 저장에서도 키가 중복을 막는다.
CREATE TABLE document_mentions (
  document_id  uuid NOT NULL REFERENCES documents (id),
  user_id      uuid NOT NULL REFERENCES users (id),
  mentioned_by uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  PRIMARY KEY (document_id, user_id)
);

CREATE TABLE notifications (
  id          uuid PRIMARY KEY DEFAULT uuidv7(),
  user_id     uuid NOT NULL REFERENCES users (id),
  kind        text NOT NULL CHECK (kind IN ('mention')),
  document_id uuid NOT NULL REFERENCES documents (id),
  actor       uuid NOT NULL REFERENCES users (id),
  created_at  timestamptz NOT NULL DEFAULT now(),
  read_at     timestamptz
);
CREATE INDEX notifications_user ON notifications (user_id, created_at DESC);

------------------------------------------------------------------------------
-- 본문에서 참조 노드 뽑기
------------------------------------------------------------------------------

CREATE FUNCTION app_reference_nodes(p_body jsonb) RETURNS TABLE (kind text, id uuid)
  LANGUAGE sql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT DISTINCT r.kind, r.raw_id::uuid
    FROM (
      SELECT n -> 'attrs' ->> 'kind' AS kind, n -> 'attrs' ->> 'id' AS raw_id
      FROM jsonb_path_query(p_body, 'lax $.**') AS n
      WHERE jsonb_typeof(n) = 'object' AND n ->> 'type' = 'reference'
    ) r
    WHERE r.kind IN ('item', 'document', 'user')
      AND r.raw_id ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  $$;

-- 다른 사용자의 문서 권한(보관 상한 없음). 같은 판정 함수를 쓰려고 트랜잭션 안에서 사용자를 잠시 바꾼다.
-- 앱 역할에는 주지 않는다(언급 알림 판정 전용).
CREATE FUNCTION app_document_access_as(p_doc uuid, p_user uuid) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_me     text := current_setting('app.user_id', true);
    v_access text;
  BEGIN
    PERFORM set_config('app.user_id', p_user::text, true);
    v_access := app_document_base_access(p_doc);
    PERFORM set_config('app.user_id', coalesce(v_me, ''), true);
    RETURN v_access;
  END
  $$;

-- 저장된 본문에서 링크·언급을 다시 기록한다. 저장한 트랜잭션 안에서 부른다(쓰기 권한 필요).
-- p_notify가 거짓이면(복제 등) 언급 알림을 만들지 않는다.
CREATE FUNCTION app_record_document_references(p_doc uuid, p_notify boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_body jsonb;
    v_uid  uuid;
    v_new  integer;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT body INTO v_body FROM documents WHERE id = p_doc;

    DELETE FROM document_links WHERE document_id = p_doc;
    INSERT INTO document_links (document_id, target_kind, target_id)
    SELECT p_doc, r.kind, r.id FROM app_reference_nodes(v_body) r
    WHERE r.kind IN ('item', 'document') AND NOT (r.kind = 'document' AND r.id = p_doc);

    IF NOT p_notify THEN
      RETURN;
    END IF;
    FOR v_uid IN
      SELECT r.id FROM app_reference_nodes(v_body) r JOIN users u ON u.id = r.id
      WHERE r.kind = 'user' AND r.id <> v_user
    LOOP
      -- 언급은 권한을 주지 않는다. 볼 수 없는 사람에게는 알리지 않는다.
      IF app_document_access_as(p_doc, v_uid) IS NOT NULL THEN
        INSERT INTO document_mentions (document_id, user_id, mentioned_by)
        VALUES (p_doc, v_uid, v_user)
        ON CONFLICT DO NOTHING;
        GET DIAGNOSTICS v_new = ROW_COUNT;
        IF v_new > 0 THEN
          INSERT INTO notifications (user_id, kind, document_id, actor)
          VALUES (v_uid, 'mention', p_doc, v_user);
        END IF;
      END IF;
    END LOOP;
  END
  $$;

------------------------------------------------------------------------------
-- 알림
------------------------------------------------------------------------------

-- 내 알림. 지금 문서를 볼 수 있는 것만 나온다(권한이 철회되면 사라진다).
CREATE FUNCTION app_my_notifications(p_limit integer)
  RETURNS TABLE (id uuid, kind text, document_id uuid, actor_name text, created_at timestamptz,
                 read_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT n.id, n.kind, n.document_id, u.display_name, n.created_at, n.read_at
    FROM notifications n JOIN users u ON u.id = n.actor
    WHERE n.user_id = app_user_id() AND app_document_access(n.document_id) IS NOT NULL
    ORDER BY n.created_at DESC, n.id DESC
    LIMIT p_limit
  $$;

CREATE FUNCTION app_mark_notifications_read(p_id uuid) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE notifications SET read_at = now()
    WHERE user_id = app_user_id() AND read_at IS NULL AND (p_id IS NULL OR id = p_id)
  $$;

------------------------------------------------------------------------------
-- RLS·권한
------------------------------------------------------------------------------

ALTER TABLE document_links ENABLE ROW LEVEL SECURITY;
-- 역방향 링크는 출발 문서를 볼 수 있는 사람에게만 보인다.
CREATE POLICY document_links_select ON document_links FOR SELECT
  USING (app_document_access(document_id) IS NOT NULL);
GRANT SELECT ON document_links TO fvoci_app;

ALTER TABLE document_mentions ENABLE ROW LEVEL SECURITY;
ALTER TABLE notifications ENABLE ROW LEVEL SECURITY;
CREATE POLICY notifications_select ON notifications FOR SELECT
  USING (user_id = app_user_id() AND app_document_access(document_id) IS NOT NULL);
GRANT SELECT ON notifications TO fvoci_app;

REVOKE ALL ON FUNCTION app_reference_nodes(jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_document_access_as(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_record_document_references(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_my_notifications(integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_mark_notifications_read(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_record_document_references(uuid, boolean),
  app_my_notifications(integer),
  app_mark_notifications_read(uuid)
TO fvoci_app;
