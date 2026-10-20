-- 댓글과 알림 넓히기 (D-27 댓글, D-20 독자도 댓글, D-39 댓글·기한 알림).
--
-- 댓글은 문서와 업무에 달고 그 대상의 권한을 따른다: 볼 수 있으면 읽고 쓴다(독자 포함, 공유받은 사람 포함).
-- 한 단계 답글(스레드). 고치기는 쓴 사람만, 지우기는 쓴 사람이나 대상의 소유 권한(관리자·만든 사람). 스레드
-- 해결 표시는 스레드를 연 사람이나 대상에 쓸 수 있는 사람. 보관된 항목 안의 대상에는 새 댓글을 달 수 없다.
-- 알림은 언급 외에 댓글(대상을 만든 사람·업무 담당자에게), 답글(스레드에 참여한 사람에게), 기한(업무 담당자,
-- 없으면 만든 사람에게 하루 전과 당일, 백그라운드 작업)을 더한다. 알림은 받는 사람이 지금 대상을 볼 수 있을 때만
-- 만들고, 읽을 때도 다시 확인한다(권한이 철회되면 사라진다).

CREATE TABLE comments (
  id           uuid PRIMARY KEY,
  workspace_id uuid REFERENCES workspaces (id),
  document_id  uuid REFERENCES documents (id) ON DELETE CASCADE,
  task_id      uuid REFERENCES tasks (id) ON DELETE CASCADE,
  parent_id    uuid REFERENCES comments (id) ON DELETE CASCADE,
  body         text NOT NULL CHECK (length(btrim(body)) BETWEEN 1 AND 10000),
  created_by   uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  updated_at   timestamptz NOT NULL DEFAULT now(),
  edited       boolean NOT NULL DEFAULT false,
  deleted_at   timestamptz,
  resolved_at  timestamptz,
  resolved_by  uuid REFERENCES users (id),
  request_id   uuid NOT NULL,
  CONSTRAINT comments_target CHECK ((document_id IS NULL) <> (task_id IS NULL)),
  CONSTRAINT comments_request UNIQUE (created_by, request_id)
);
CREATE INDEX comments_document ON comments (document_id, created_at) WHERE document_id IS NOT NULL;
CREATE INDEX comments_task ON comments (task_id, created_at) WHERE task_id IS NOT NULL;
CREATE INDEX comments_parent ON comments (parent_id) WHERE parent_id IS NOT NULL;

ALTER TABLE notifications ALTER COLUMN document_id DROP NOT NULL;
ALTER TABLE notifications ALTER COLUMN actor DROP NOT NULL;
ALTER TABLE notifications ADD COLUMN task_id uuid REFERENCES tasks (id) ON DELETE CASCADE;
ALTER TABLE notifications ADD COLUMN comment_id uuid REFERENCES comments (id) ON DELETE CASCADE;
ALTER TABLE notifications ADD COLUMN detail jsonb NOT NULL DEFAULT '{}'::jsonb;
ALTER TABLE notifications DROP CONSTRAINT notifications_kind_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_kind_check
  CHECK (kind IN ('mention', 'comment', 'reply', 'due'));
ALTER TABLE notifications ADD CONSTRAINT notifications_target CHECK ((document_id IS NULL) <> (task_id IS NULL));
ALTER TABLE notifications ADD CONSTRAINT notifications_actor CHECK (actor IS NOT NULL OR kind = 'due');
-- 기한 알림은 업무·사람·날짜마다 한 번.
CREATE UNIQUE INDEX notifications_due_once ON notifications (user_id, task_id, (detail->>'date')) WHERE kind = 'due';

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

-- 댓글 대상 권한(문서 또는 업무).
CREATE FUNCTION app_comment_target_access(p_document uuid, p_task uuid) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN p_document IS NOT NULL THEN app_document_access(p_document) ELSE app_task_access(p_task) END
  $$;

-- 다른 사람이 대상을 볼 수 있는지(알림 받는 사람 확인). 같은 판정 함수를 쓰려고 잠시 사용자를 바꾼다.
CREATE FUNCTION app_comment_target_access_as(p_document uuid, p_task uuid, p_user uuid) RETURNS text
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_me     text := current_setting('app.user_id', true);
    v_access text;
  BEGIN
    PERFORM set_config('app.user_id', p_user::text, true);
    v_access := app_comment_target_access(p_document, p_task);
    PERFORM set_config('app.user_id', coalesce(v_me, ''), true);
    RETURN v_access;
  END
  $$;

-- 대상이 보관된 항목 안에 있는지.
CREATE FUNCTION app_comment_target_archived(p_document uuid, p_task uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce(app_item_archived(CASE
      WHEN p_document IS NOT NULL THEN (SELECT item_id FROM documents WHERE id = p_document)
      ELSE (SELECT item_id FROM tasks WHERE id = p_task) END), false)
  $$;

-- 받는 사람이 대상을 볼 수 있을 때만 알림을 만든다.
CREATE FUNCTION app_notify(p_user uuid, p_kind text, p_document uuid, p_task uuid, p_comment uuid, p_actor uuid,
                           p_detail jsonb) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_user IS NULL OR p_user = p_actor THEN
      RETURN;
    END IF;
    IF app_comment_target_access_as(p_document, p_task, p_user) IS NULL THEN
      RETURN;
    END IF;
    INSERT INTO notifications (user_id, kind, document_id, task_id, comment_id, actor, detail)
    VALUES (p_user, p_kind, p_document, p_task, p_comment, p_actor, coalesce(p_detail, '{}'::jsonb))
    ON CONFLICT DO NOTHING;
  END
  $$;

------------------------------------------------------------------------------
-- 댓글
------------------------------------------------------------------------------

-- 댓글 달기(대상을 볼 수 있는 사람). 같은 요청 id면 같은 댓글.
CREATE FUNCTION app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_id     uuid;
    v_parent comments%ROWTYPE;
    v_ws     uuid;
    v_owner  uuid;
    v_body   text := btrim(coalesce(p_body, ''));
    v_excerpt text;
    v_uid    uuid;
  BEGIN
    SELECT id INTO v_id FROM comments WHERE created_by = v_user AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF (p_document IS NULL) = (p_task IS NULL) THEN
      RAISE EXCEPTION 'comment on a document or a task' USING ERRCODE = '22023';
    END IF;
    IF app_comment_target_access(p_document, p_task) IS NULL THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_comment_target_archived(p_document, p_task) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF length(v_body) NOT BETWEEN 1 AND 10000 THEN
      RAISE EXCEPTION 'a comment needs 1 to 10000 characters' USING ERRCODE = '22023';
    END IF;
    IF p_parent IS NOT NULL THEN
      SELECT * INTO v_parent FROM comments WHERE id = p_parent;
      IF v_parent.id IS NULL OR v_parent.document_id IS DISTINCT FROM p_document
         OR v_parent.task_id IS DISTINCT FROM p_task THEN
        RAISE EXCEPTION 'thread not found' USING ERRCODE = 'P0002';
      END IF;
      IF v_parent.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'reply to the first comment of the thread' USING ERRCODE = '22023';
      END IF;
    END IF;
    IF p_document IS NOT NULL THEN
      SELECT workspace_id, coalesce(owner_user_id, created_by) INTO v_ws, v_owner FROM documents WHERE id = p_document;
    ELSE
      SELECT workspace_id, created_by INTO v_ws, v_owner FROM tasks WHERE id = p_task;
    END IF;
    INSERT INTO comments (workspace_id, document_id, task_id, parent_id, body, created_by, request_id)
    VALUES (v_ws, p_document, p_task, p_parent, v_body, v_user, p_request)
    RETURNING id INTO v_id;
    v_excerpt := left(v_body, 140);
    IF p_parent IS NULL THEN
      -- 새 스레드: 대상을 만든 사람과(업무면) 담당자에게.
      PERFORM app_notify(v_owner, 'comment', p_document, p_task, v_id, v_user, jsonb_build_object('excerpt', v_excerpt));
      FOR v_uid IN SELECT user_id FROM task_assignees WHERE task_id = p_task LOOP
        PERFORM app_notify(v_uid, 'comment', p_document, p_task, v_id, v_user, jsonb_build_object('excerpt', v_excerpt));
      END LOOP;
    ELSE
      -- 답글: 스레드를 연 사람과 그 스레드에 답한 사람에게.
      FOR v_uid IN
        SELECT DISTINCT created_by FROM comments WHERE (id = p_parent OR parent_id = p_parent) AND deleted_at IS NULL
      LOOP
        PERFORM app_notify(v_uid, 'reply', p_document, p_task, v_id, v_user, jsonb_build_object('excerpt', v_excerpt));
      END LOOP;
    END IF;
    RETURN v_id;
  END
  $$;

-- 고치기(쓴 사람만).
CREATE FUNCTION app_edit_comment(p_comment uuid, p_body text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_c    comments%ROWTYPE;
    v_body text := btrim(coalesce(p_body, ''));
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    IF v_c.id IS NULL OR v_c.deleted_at IS NOT NULL OR app_comment_target_access(v_c.document_id, v_c.task_id) IS NULL THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.created_by <> app_user_id() THEN
      RAISE EXCEPTION 'only the author can edit a comment' USING ERRCODE = '42501';
    END IF;
    IF length(v_body) NOT BETWEEN 1 AND 10000 THEN
      RAISE EXCEPTION 'a comment needs 1 to 10000 characters' USING ERRCODE = '22023';
    END IF;
    UPDATE comments SET body = v_body, edited = true, updated_at = now() WHERE id = p_comment;
  END
  $$;

-- 지우기(쓴 사람, 또는 대상의 소유 권한). 답글이 있는 스레드 첫 댓글은 "지운 댓글"로 남겨 답글을 보존한다.
CREATE FUNCTION app_delete_comment(p_comment uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_c.id IS NULL OR v_c.deleted_at IS NOT NULL OR v_access IS NULL THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.created_by <> app_user_id() AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the author or the owner can delete a comment' USING ERRCODE = '42501';
    END IF;
    IF EXISTS (SELECT 1 FROM comments WHERE parent_id = p_comment AND deleted_at IS NULL) THEN
      UPDATE comments SET deleted_at = now(), body = '(지운 댓글)', updated_at = now() WHERE id = p_comment;
    ELSE
      DELETE FROM comments WHERE id = p_comment;
    END IF;
    IF v_c.created_by <> app_user_id() THEN
      INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (v_c.workspace_id,
              CASE WHEN v_c.workspace_id IS NULL THEN (SELECT owner_user_id FROM documents WHERE id = v_c.document_id) END,
              app_user_id(), 'delete_comment', CASE WHEN v_c.document_id IS NOT NULL THEN 'document' ELSE 'task' END,
              coalesce(v_c.document_id, v_c.task_id), jsonb_build_object('author', v_c.created_by));
    END IF;
  END
  $$;

-- 스레드 해결 표시·해제(스레드를 연 사람, 또는 대상에 쓸 수 있는 사람).
CREATE FUNCTION app_resolve_comment(p_comment uuid, p_resolved boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_c.id IS NULL OR v_access IS NULL THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'resolve the first comment of the thread' USING ERRCODE = '22023';
    END IF;
    IF v_c.created_by <> app_user_id() AND v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    UPDATE comments SET resolved_at = CASE WHEN p_resolved THEN now() END,
                        resolved_by = CASE WHEN p_resolved THEN app_user_id() END
    WHERE id = p_comment;
  END
  $$;

------------------------------------------------------------------------------
-- 알림 읽기
------------------------------------------------------------------------------

DROP FUNCTION app_my_notifications(integer);
CREATE FUNCTION app_my_notifications(p_limit integer)
  RETURNS TABLE (id uuid, kind text, document_id uuid, task_id uuid, comment_id uuid, actor_name text,
                 detail jsonb, created_at timestamptz, read_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    -- 행위자 이름은 워크스페이스를 함께 쓰거나 그 개인 노트의 소유자일 때만 보인다(users RLS와 같은 기준).
    SELECT n.id, n.kind, n.document_id, n.task_id, n.comment_id,
           CASE WHEN n.actor IS NOT NULL AND (app_shares_workspace(n.actor)
                     OR (SELECT owner_user_id FROM documents WHERE id = n.document_id) = n.actor)
                THEN u.display_name END,
           n.detail, n.created_at, n.read_at
    FROM notifications n LEFT JOIN users u ON u.id = n.actor
    WHERE n.user_id = app_user_id()
      AND app_comment_target_access(n.document_id, n.task_id) IS NOT NULL
    ORDER BY n.created_at DESC, n.id DESC
    LIMIT p_limit
  $$;

DROP POLICY notifications_select ON notifications;
CREATE POLICY notifications_select ON notifications FOR SELECT
  USING (user_id = app_user_id() AND app_comment_target_access(document_id, task_id) IS NOT NULL);

-- 작업: 기한 알림(하루 전·당일). 담당자, 없으면 만든 사람. 같은 날 같은 업무는 한 번.
CREATE FUNCTION app_system_due_notifications(p_today date) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task  tasks%ROWTYPE;
    v_uid   uuid;
    v_count integer := 0;
    v_before integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_task IN
      SELECT t.* FROM tasks t JOIN items i ON i.id = t.item_id JOIN workspaces w ON w.id = t.workspace_id
      WHERE t.deleted_at IS NULL AND i.deleted_at IS NULL AND i.archived_at IS NULL AND w.deleted_at IS NULL
        AND t.status <> 'done' AND t.due_date IN (p_today, p_today + 1)
    LOOP
      FOR v_uid IN
        SELECT user_id FROM task_assignees WHERE task_id = v_task.id
        UNION SELECT v_task.created_by WHERE NOT EXISTS (SELECT 1 FROM task_assignees WHERE task_id = v_task.id)
      LOOP
        SELECT count(*) INTO v_before FROM notifications WHERE user_id = v_uid AND task_id = v_task.id AND kind = 'due';
        PERFORM app_notify(v_uid, 'due', NULL, v_task.id, NULL, NULL,
                           jsonb_build_object('date', p_today, 'due_date', v_task.due_date,
                                              'title', v_task.title));
        IF (SELECT count(*) FROM notifications WHERE user_id = v_uid AND task_id = v_task.id AND kind = 'due') > v_before THEN
          v_count := v_count + 1;
        END IF;
      END LOOP;
    END LOOP;
    RETURN v_count;
  END
  $$;

------------------------------------------------------------------------------
-- 행 보안·권한
------------------------------------------------------------------------------

ALTER TABLE comments ENABLE ROW LEVEL SECURITY;
CREATE POLICY comments_select ON comments FOR SELECT
  USING (app_comment_target_access(document_id, task_id) IS NOT NULL);
GRANT SELECT ON comments TO fvoci_app;

REVOKE ALL ON FUNCTION app_comment_target_access(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_comment_target_access_as(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_comment_target_archived(uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_notify(uuid, text, uuid, uuid, uuid, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_add_comment(uuid, uuid, uuid, uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_edit_comment(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_comment(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_resolve_comment(uuid, boolean) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_my_notifications(integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_due_notifications(date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_comment_target_access(uuid, uuid),
  app_add_comment(uuid, uuid, uuid, uuid, text),
  app_edit_comment(uuid, text),
  app_delete_comment(uuid),
  app_resolve_comment(uuid, boolean),
  app_my_notifications(integer),
  app_system_due_notifications(date)
TO fvoci_app;
