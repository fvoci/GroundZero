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
  id          uuid PRIMARY KEY,
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
-- 바꾼 값은 끝에서 되돌린다. 예외가 나면 트랜잭션(또는 하위 트랜잭션) 되돌림이 set_config(..., true)도 되돌린다.
-- (함수 정의에 SET app.user_id를 붙이는 방식은 마이그레이션 역할이 사용자 정의 설정을 지정할 권한이 없어 쓰지 않는다.)
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
-- 언급 알림은 이번 저장에서 새로 넣은 언급(직전 리비전 본문에 없던 사람)에만, 저장한 사람 이름으로 만든다.
-- p_notify가 거짓이면(복제 등) 알림을 만들지 않는다. 사본의 언급은 원작성자가 한 것이며, 이후 편집에서도
-- 직전 리비전에 이미 있으므로 새 언급으로 보지 않는다.
CREATE FUNCTION app_record_document_references(p_doc uuid, p_notify boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_body     jsonb;
    v_revision integer;
    v_previous jsonb;
    v_uid      uuid;
    v_new      integer;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT body, revision INTO v_body, v_revision FROM documents WHERE id = p_doc;

    DELETE FROM document_links WHERE document_id = p_doc;
    INSERT INTO document_links (document_id, target_kind, target_id)
    SELECT p_doc, r.kind, r.id FROM app_reference_nodes(v_body) r
    WHERE r.kind IN ('item', 'document') AND NOT (r.kind = 'document' AND r.id = p_doc);

    IF NOT p_notify THEN
      RETURN;
    END IF;
    SELECT body INTO v_previous FROM document_revisions
    WHERE document_id = p_doc AND revision = v_revision - 1;
    FOR v_uid IN
      SELECT r.id FROM app_reference_nodes(v_body) r JOIN users u ON u.id = r.id
      WHERE r.kind = 'user' AND r.id <> v_user
        AND NOT EXISTS (SELECT 1 FROM app_reference_nodes(coalesce(v_previous, '{}'::jsonb)) p
                        WHERE p.kind = 'user' AND p.id = r.id)
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
    -- 언급한 사람 이름은 워크스페이스를 함께 쓰거나 그 개인 노트의 소유자일 때만 보인다(users RLS와 같은 기준).
    SELECT n.id, n.kind, n.document_id,
           CASE WHEN app_shares_workspace(n.actor) OR d.owner_user_id = n.actor THEN u.display_name END,
           n.created_at, n.read_at
    FROM notifications n JOIN users u ON u.id = n.actor JOIN documents d ON d.id = n.document_id
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
-- 후보 검색 범위: 내가 속한 워크스페이스. 검색이 RLS 판정 함수를 모든 테넌트의 행에 돌리지 않도록
-- 이 목록과 인덱스로 먼저 좁힌다(같음 비교는 RLS보다 먼저 적용될 수 있다).
------------------------------------------------------------------------------

CREATE FUNCTION app_my_workspace_ids() RETURNS SETOF uuid
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT workspace_id FROM workspace_members WHERE user_id = app_user_id()
  $$;

CREATE INDEX items_workspace_updated ON items (workspace_id, updated_at DESC);
CREATE INDEX documents_workspace_updated ON documents (workspace_id, updated_at DESC);
CREATE INDEX documents_owner_updated ON documents (owner_user_id, updated_at DESC)
  WHERE owner_user_id IS NOT NULL;

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
REVOKE ALL ON FUNCTION app_my_workspace_ids() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_record_document_references(uuid, boolean),
  app_my_notifications(integer),
  app_mark_notifications_read(uuid),
  app_my_workspace_ids()
TO fvoci_app;
