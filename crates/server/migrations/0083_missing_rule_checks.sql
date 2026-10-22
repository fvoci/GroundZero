-- 형제 경로에는 있는데 빠져 있던 서버 검사 (코드 전반 감사 S9·S18·S19·W11).
--
-- 1. 함께 하는 사람 집계의 '오늘'(S9): 강좌·스터디 참여자 집계가 요청의 today를 검사 없이 받아, 볼 수 있는 사람이
--    today를 바꿔 가며 물으면 밀린 수의 계단에서 다른 사람의 개인 계획일(본인만 보는 값)을 알아낼 수 있었다. 진도
--    쓰기와 같이 서버 날짜에서 하루 넘게 떨어진 today는 22023(400)으로 받지 않는다.
-- 2. 반복 일정의 끝(S18): 일정 수정이 반복 규칙을 보지 않아 시작을 규칙의 `until` 뒤로 옮기거나(시간대를 바꿔 첫 회차
--    날짜가 밀려도) 저장했고, 그 일정은 모든 달력에서 사라졌다. 규칙 설정과 같은 검사로 22023(400)을 낸다.
-- 3. 보관된 모듈의 마감 알림(S19): 모듈을 보관하면 그 아래 항목은 읽기 전용인데 마감 알림은 모듈 보관을 보지 않았다.
--    같은 작업의 항목 알림·홈처럼 보관된 모듈의 업무는 알리지 않는다(`_ids` 개수도 같은 조건).
-- 4. 보관된 항목의 댓글(W11): 새 댓글·답글은 막히는데(55000, 409) 쓴 사람의 고치기와 스레드 해결·다시 열기는
--    통과했다. 둘 다 같은 409로 막는다. 댓글 목록의 고치기·해결 표시(`can_edit`·`can_resolve`)도 서버가 보관 상태를
--    보고 정하도록 대상의 권한과 보관 여부를 함께 돌려주는 함수를 둔다. 지우기는 보존 정책 문제라 바꾸지 않는다.
-- 함수 본문은 각 함수의 마지막 정의(0032·0034·0037·0073·0074)와 같고 위 부분만 다르다.

------------------------------------------------------------------------------------------------------------------
-- 1. 함께 하는 사람 집계
------------------------------------------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_course_participants(p_item uuid, p_today date)
  RETURNS TABLE (user_id uuid, display_name text, done integer, in_progress integer, overdue integer,
                 done_seconds bigint)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_course_access(p_item);
    IF p_today IS NULL OR abs(p_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
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

-- 함께 하는 사람의 진행 집계(스터디를 볼 수 있는 사람). 메모·개인 날짜는 넣지 않는다.
CREATE OR REPLACE FUNCTION app_study_participants(p_item uuid, p_today date)
  RETURNS TABLE (user_id uuid, display_name text, done integer, in_progress integer, overdue integer,
                 done_amount bigint)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    PERFORM app_study_access(p_item);
    IF p_today IS NULL OR abs(p_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
    RETURN QUERY
      SELECT u.id, u.display_name,
             count(*) FILTER (WHERE p.status = 'done')::integer,
             count(*) FILTER (WHERE p.status = 'in_progress')::integer,
             (SELECT count(*)::integer FROM study_units s2
              LEFT JOIN unit_progress p2 ON p2.unit_id = s2.id AND p2.user_id = u.id
              WHERE s2.item_id = p_item AND coalesce(p2.status, 'not_started') <> 'done'
                AND coalesce(p2.planned_on, s2.planned_on) < p_today),
             coalesce(sum(p.done_amount), 0)::bigint
      FROM unit_progress p
      JOIN study_units s ON s.id = p.unit_id AND s.item_id = p_item
      JOIN users u ON u.id = p.user_id
      WHERE app_item_access_as(p_item, u.id) IS NOT NULL
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 2. 반복 일정의 끝
------------------------------------------------------------------------------------------------------------------

-- id 2개: 이름 바꾸기, 일정 바꾸기 감사 기록.
CREATE OR REPLACE FUNCTION app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text,
                                            p_description text, p_all_day boolean, p_start_date date,
                                            p_end_date date, p_starts_at timestamptz, p_ends_at timestamptz,
                                            p_timezone text, p_ids uuid[])
  RETURNS text
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF v_event.version <> p_base_version THEN
      RETURN 'conflict';
    END IF;
    PERFORM app_check_timezone(p_timezone);
    -- 규칙 설정(app_set_event_recurrence)과 같은 조건: 첫 회차가 반복의 끝보다 뒤면 회차가 하나도 없다.
    IF (v_event.recurrence ->> 'until')::date < coalesce(p_start_date, (p_starts_at AT TIME ZONE p_timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    UPDATE events SET
      kind = p_kind, title = btrim(p_title), description = coalesce(p_description, ''), all_day = p_all_day,
      start_date = p_start_date, end_date = p_end_date, starts_at = p_starts_at, ends_at = p_ends_at,
      timezone = p_timezone, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    IF v_event.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'rename', 'event', p_event,
                        jsonb_build_object('from', v_event.title, 'to', btrim(p_title)));
    END IF;
    IF (v_event.start_date, v_event.end_date, v_event.starts_at, v_event.ends_at, v_event.timezone)
       IS DISTINCT FROM (p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone) THEN
      PERFORM app_audit(app_id(p_ids, 2), v_event.workspace_id, 'reschedule', 'event', p_event, jsonb_build_object(
        'from', jsonb_build_array(coalesce(v_event.start_date::text, app_utc_text(v_event.starts_at)),
                                  coalesce(v_event.end_date::text, app_utc_text(v_event.ends_at)), v_event.timezone),
        'to', jsonb_build_array(coalesce(p_start_date::text, app_utc_text(p_starts_at)),
                                coalesce(p_end_date::text, app_utc_text(p_ends_at)), p_timezone)));
    END IF;
    RETURN 'updated';
  END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 3. 보관된 모듈의 마감 알림
------------------------------------------------------------------------------------------------------------------

-- id: 알릴 업무·사람 쌍마다 1개(개수는 `_ids`).
CREATE OR REPLACE FUNCTION app_system_due_notifications(p_today date, p_ids uuid[]) RETURNS integer
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_task   tasks%ROWTYPE;
    v_uid    uuid;
    v_count  integer := 0;
    v_before integer;
    v_at     integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_task IN
      SELECT t.* FROM tasks t JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
      JOIN workspaces w ON w.id = t.workspace_id
      WHERE t.deleted_at IS NULL AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND w.deleted_at IS NULL AND t.status <> 'done' AND t.due_date IN (p_today, p_today + 1)
    LOOP
      FOR v_uid IN
        SELECT user_id FROM task_assignees WHERE task_id = v_task.id
        UNION SELECT v_task.created_by WHERE NOT EXISTS (SELECT 1 FROM task_assignees WHERE task_id = v_task.id)
      LOOP
        SELECT count(*) INTO v_before FROM notifications WHERE user_id = v_uid AND task_id = v_task.id AND kind = 'due';
        v_at := v_at + 1;
        PERFORM app_notify(app_id(p_ids, v_at), v_uid, 'due', NULL, v_task.id, NULL, NULL,
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

CREATE OR REPLACE FUNCTION app_system_due_notifications_ids(p_today date) RETURNS integer
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      (SELECT count(*)
       FROM tasks t JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
       JOIN workspaces w ON w.id = t.workspace_id
       CROSS JOIN LATERAL (
         SELECT user_id FROM task_assignees WHERE task_id = t.id
         UNION SELECT t.created_by WHERE NOT EXISTS (SELECT 1 FROM task_assignees WHERE task_id = t.id)) r
       WHERE t.deleted_at IS NULL AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
         AND w.deleted_at IS NULL AND t.status <> 'done' AND t.due_date IN (p_today, p_today + 1))::integer
    END
  $$;

------------------------------------------------------------------------------------------------------------------
-- 4. 보관된 항목의 댓글
------------------------------------------------------------------------------------------------------------------

-- 고치기(쓴 사람만). 보관된 항목의 댓글은 고치지 않는다(새 댓글과 같은 409).
CREATE OR REPLACE FUNCTION app_edit_comment(p_comment uuid, p_body text) RETURNS void
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
    IF app_comment_target_archived(v_c.document_id, v_c.task_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF length(v_body) NOT BETWEEN 1 AND 10000 THEN
      RAISE EXCEPTION 'a comment needs 1 to 10000 characters' USING ERRCODE = '22023';
    END IF;
    UPDATE comments SET body = v_body, edited = true, updated_at = now() WHERE id = p_comment;
  END
  $$;

-- 스레드 해결 표시·해제(스레드를 연 사람, 또는 대상에 쓸 수 있는 사람). 보관된 항목에서는 하지 않는다.
CREATE OR REPLACE FUNCTION app_resolve_comment(p_comment uuid, p_resolved boolean) RETURNS void
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
    IF app_comment_target_archived(v_c.document_id, v_c.task_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    UPDATE comments SET resolved_at = CASE WHEN p_resolved THEN now() END,
                        resolved_by = CASE WHEN p_resolved THEN app_user_id() END
    WHERE id = p_comment;
  END
  $$;

-- 댓글 목록이 고치기·해결 표시를 정할 대상의 권한과 보관 여부. 볼 수 없으면 행이 없다.
CREATE FUNCTION app_comment_target_state(p_document uuid, p_task uuid)
  RETURNS TABLE (access text, archived boolean)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT a.access, app_comment_target_archived(p_document, p_task)
    FROM (SELECT app_comment_target_access(p_document, p_task) AS access) a
    WHERE a.access IS NOT NULL
  $$;

REVOKE ALL ON FUNCTION app_comment_target_state(uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION app_comment_target_state(uuid, uuid) TO fvoci_app;
