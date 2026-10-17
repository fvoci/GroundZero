-- 댓글·강좌 리뷰 반영 (D-27, D-37, D-39).
--
-- 1. 댓글의 워크스페이스는 대상(문서·업무)에서 읽는다. 문서를 다른 워크스페이스로 옮기면 따로 적어 둔 값이 옛
--    워크스페이스를 가리켜, 그 워크스페이스를 영구 삭제할 때 외래 키에 걸리고 반출에서 빠졌다.
-- 2. 알림의 댓글 첫 부분은 알림을 읽을 때 댓글에서 가져온다. 고치거나 지운 댓글의 옛 글이 알림에 남지 않는다.
-- 3. 답글을 다는 동안 스레드 첫 댓글을 잠근다. 동시에 그 댓글을 지우면 답글이 함께 사라지던 경합을 막는다.
-- 4. 새 스레드 알림은 받는 사람마다 한 번(만든 사람이 담당자이기도 할 때 두 번 가던 것).

ALTER TABLE comments DROP COLUMN workspace_id;
UPDATE notifications SET detail = detail - 'excerpt' WHERE detail ? 'excerpt';

CREATE OR REPLACE FUNCTION app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_id     uuid;
    v_parent comments%ROWTYPE;
    v_owner  uuid;
    v_body   text := btrim(coalesce(p_body, ''));
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
      -- 지우기(FOR NO KEY UPDATE)와 겨루지 않게 잠근다. 먼저 지워졌으면 찾지 못한다(404).
      SELECT * INTO v_parent FROM comments WHERE id = p_parent FOR SHARE;
      IF v_parent.id IS NULL OR v_parent.document_id IS DISTINCT FROM p_document
         OR v_parent.task_id IS DISTINCT FROM p_task THEN
        RAISE EXCEPTION 'thread not found' USING ERRCODE = 'P0002';
      END IF;
      IF v_parent.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'reply to the first comment of the thread' USING ERRCODE = '22023';
      END IF;
    END IF;
    IF p_document IS NOT NULL THEN
      SELECT coalesce(owner_user_id, created_by) INTO v_owner FROM documents WHERE id = p_document;
    ELSE
      SELECT created_by INTO v_owner FROM tasks WHERE id = p_task;
    END IF;
    INSERT INTO comments (document_id, task_id, parent_id, body, created_by, request_id)
    VALUES (p_document, p_task, p_parent, v_body, v_user, p_request)
    RETURNING id INTO v_id;
    IF p_parent IS NULL THEN
      -- 새 스레드: 대상을 만든 사람과(업무면) 담당자에게, 사람마다 한 번.
      FOR v_uid IN
        SELECT v_owner WHERE v_owner IS NOT NULL
        UNION SELECT user_id FROM task_assignees WHERE task_id = p_task
      LOOP
        PERFORM app_notify(v_uid, 'comment', p_document, p_task, v_id, v_user, '{}'::jsonb);
      END LOOP;
    ELSE
      -- 답글: 스레드를 연 사람과 그 스레드에 답한 사람에게.
      FOR v_uid IN
        SELECT DISTINCT created_by FROM comments WHERE (id = p_parent OR parent_id = p_parent) AND deleted_at IS NULL
      LOOP
        PERFORM app_notify(v_uid, 'reply', p_document, p_task, v_id, v_user, '{}'::jsonb);
      END LOOP;
    END IF;
    RETURN v_id;
  END
  $$;

CREATE OR REPLACE FUNCTION app_delete_comment(p_comment uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
    v_ws     uuid;
    v_owner  uuid;
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
      IF v_c.document_id IS NOT NULL THEN
        SELECT workspace_id, CASE WHEN workspace_id IS NULL THEN owner_user_id END INTO v_ws, v_owner
        FROM documents WHERE id = v_c.document_id;
      ELSE
        SELECT workspace_id INTO v_ws FROM tasks WHERE id = v_c.task_id;
      END IF;
      INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (v_ws, v_owner, app_user_id(), 'delete_comment',
              CASE WHEN v_c.document_id IS NOT NULL THEN 'document' ELSE 'task' END,
              coalesce(v_c.document_id, v_c.task_id), jsonb_build_object('author', v_c.created_by));
    END IF;
  END
  $$;

-- 댓글 첫 부분은 지금의 댓글에서(지운 댓글이면 없음).
CREATE OR REPLACE FUNCTION app_my_notifications(p_limit integer)
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
           CASE WHEN c.id IS NOT NULL AND c.deleted_at IS NULL
                THEN n.detail || jsonb_build_object('excerpt', left(c.body, 140))
                ELSE n.detail END,
           n.created_at, n.read_at
    FROM notifications n
    LEFT JOIN users u ON u.id = n.actor
    LEFT JOIN comments c ON c.id = n.comment_id
    WHERE n.user_id = app_user_id()
      AND app_comment_target_access(n.document_id, n.task_id) IS NOT NULL
    ORDER BY n.created_at DESC, n.id DESC
    LIMIT p_limit
  $$;

------------------------------------------------------------------------------
-- 강좌
------------------------------------------------------------------------------
-- 5. 완료한 날은 사용자의 오늘(화면이 보낸 날짜)로 적고, 완료를 풀면 지운다(UTC 날짜로 적혀 아침 완료가 어제로
--    셈되고, 완료를 풀어도 오늘 완료로 남던 것).
-- 7. 함께 수강하는 사람 집계에는 지금 이 강좌를 볼 수 있는 사람만(비공개로 바뀐 항목의 멤버가 남던 것).
-- 8. 강좌·회차 주소는 http·https만(읽는 사람에게 링크로 보인다).

CREATE FUNCTION app_web_url_ok(p_url text) RETURNS boolean
  LANGUAGE sql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce(p_url, '') = '' OR p_url ~* '^https?://[^[:space:]]+$'
  $$;

UPDATE course_details SET url = '' WHERE NOT app_web_url_ok(url);
UPDATE course_episodes SET url = '' WHERE NOT app_web_url_ok(url);
ALTER TABLE course_details ADD CONSTRAINT course_details_url CHECK (app_web_url_ok(url));
ALTER TABLE course_episodes ADD CONSTRAINT course_episodes_url CHECK (app_web_url_ok(url));

CREATE OR REPLACE FUNCTION app_set_course_details(
  p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text,
  p_starts date, p_expires date, p_target date) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_require_course_write(p_item);
    IF length(coalesce(p_url, '')) > 2000 OR length(coalesce(p_subject, '') || coalesce(p_instructor, '')
       || coalesce(p_platform, '') || coalesce(p_textbook, '')) > 2000 THEN
      RAISE EXCEPTION 'course details are too long' USING ERRCODE = '22023';
    END IF;
    IF NOT app_web_url_ok(btrim(coalesce(p_url, ''))) THEN
      RAISE EXCEPTION 'the link must start with http:// or https://' USING ERRCODE = '22023';
    END IF;
    INSERT INTO course_details (item_id, workspace_id, subject, instructor, platform, url, textbook,
                                starts_on, expires_on, target_on, updated_by, updated_at)
    SELECT p_item, i.workspace_id, btrim(coalesce(p_subject, '')), btrim(coalesce(p_instructor, '')),
           btrim(coalesce(p_platform, '')), btrim(coalesce(p_url, '')), btrim(coalesce(p_textbook, '')),
           p_starts, p_expires, p_target, app_user_id(), now()
    FROM items i WHERE i.id = p_item
    ON CONFLICT (item_id) DO UPDATE SET
      subject = EXCLUDED.subject, instructor = EXCLUDED.instructor, platform = EXCLUDED.platform,
      url = EXCLUDED.url, textbook = EXCLUDED.textbook, starts_on = EXCLUDED.starts_on,
      expires_on = EXCLUDED.expires_on, target_on = EXCLUDED.target_on,
      updated_by = EXCLUDED.updated_by, updated_at = now();
    PERFORM app_audit((SELECT workspace_id FROM items WHERE id = p_item), 'update_course', 'item', p_item,
                      '{}'::jsonb);
  END
  $$;

DROP FUNCTION app_set_episode_progress(uuid, text, date, integer, text);
CREATE FUNCTION app_set_episode_progress(
  p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_item  uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_today date := coalesce(p_today, current_date);
  BEGIN
    IF v_item IS NULL OR app_item_access(v_item) IS NULL THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_status NOT IN ('not_started', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF length(coalesce(p_note, '')) > 20000 OR p_position < 0 THEN
      RAISE EXCEPTION 'invalid progress' USING ERRCODE = '22023';
    END IF;
    IF abs(v_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
    INSERT INTO episode_progress (episode_id, user_id, status, planned_on, completed_on, watch_count,
                                  last_position_seconds, note, updated_at)
    VALUES (p_episode, app_user_id(), p_status, p_planned,
            CASE WHEN p_status = 'done' THEN v_today END,
            CASE WHEN p_status = 'done' THEN 1 ELSE 0 END, p_position, coalesce(p_note, ''), now())
    ON CONFLICT (episode_id, user_id) DO UPDATE SET
      status = EXCLUDED.status,
      planned_on = EXCLUDED.planned_on,
      -- 이미 완료였으면 그날을 지키고, 완료를 풀면 지운다.
      completed_on = CASE WHEN EXCLUDED.status <> 'done' THEN NULL
                          WHEN episode_progress.status = 'done' THEN episode_progress.completed_on
                          ELSE EXCLUDED.completed_on END,
      watch_count = episode_progress.watch_count
        + CASE WHEN EXCLUDED.status = 'done' AND episode_progress.status <> 'done' THEN 1 ELSE 0 END,
      last_position_seconds = EXCLUDED.last_position_seconds,
      note = EXCLUDED.note,
      updated_at = now();
  END
  $$;

CREATE OR REPLACE FUNCTION app_course_participants(p_item uuid, p_today date)
  RETURNS TABLE (user_id uuid, display_name text, done integer, in_progress integer, overdue integer,
                 done_seconds bigint)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_course_access(p_item);
    RETURN QUERY
      SELECT u.id, u.display_name,
             count(*) FILTER (WHERE p.status = 'done')::integer,
             count(*) FILTER (WHERE p.status = 'in_progress')::integer,
             (SELECT count(*)::integer FROM course_episodes e2
              LEFT JOIN episode_progress p2 ON p2.episode_id = e2.id AND p2.user_id = u.id
              WHERE e2.item_id = p_item AND coalesce(p2.status, 'not_started') <> 'done'
                AND coalesce(p2.planned_on, e2.planned_on) < p_today),
             coalesce(sum(e.duration_seconds) FILTER (WHERE p.status = 'done'), 0)::bigint
      FROM episode_progress p
      JOIN course_episodes e ON e.id = p.episode_id AND e.item_id = p_item
      JOIN users u ON u.id = p.user_id
      -- 지금 이 강좌를 볼 수 있는 사람만(나간 멤버, 비공개로 바뀐 항목을 볼 수 없는 멤버는 빠진다).
      WHERE app_item_access_as(p_item, u.id) IS NOT NULL
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;

REVOKE ALL ON FUNCTION app_web_url_ok(text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_episode_progress(uuid, text, date, integer, text, date) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_set_episode_progress(uuid, text, date, integer, text, date) TO fvoci_app;
