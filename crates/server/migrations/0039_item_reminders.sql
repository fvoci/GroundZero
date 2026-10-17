-- 항목 알림 (D-37 만료·마감 알림, D-36 복습 알림, D-34 제출 예정일·수정 마감일, D-33 산출물 마감, D-39 알림).
--
-- 알림 대상에 항목을 더한다(문서·업무·항목 중 정확히 하나). 새 종류:
--   expiry   강좌 수강권 만료(7일 전·당일)와 목표 완료일(하루 전·당일). 받는 사람: 진도를 남긴 사람, 없으면 강좌를
--            만든 사람.
--   review   스터디 복습 기록의 다음 복습일(하루 전·당일). 받는 사람: 기록을 만든 사람.
--   deadline 연구 투고의 제출 예정일·수정 마감일, 프로젝트 산출물의 마감일(하루 전·당일). 끝난 상태(게재·거절·완료·
--            승인)면 보내지 않는다. 받는 사람: 기록을 만든 사람과 공동저자·담당.
-- 받는 사람이 지금 그 항목을 볼 수 있을 때만 만들고, 읽을 때도 다시 확인한다. 같은 사람·대상·날짜는 한 번.

ALTER TABLE notifications ADD COLUMN item_id uuid REFERENCES items (id) ON DELETE CASCADE;
ALTER TABLE notifications ADD COLUMN record_id uuid REFERENCES item_records (id) ON DELETE CASCADE;
ALTER TABLE notifications DROP CONSTRAINT notifications_target;
ALTER TABLE notifications ADD CONSTRAINT notifications_target CHECK (num_nonnulls(document_id, task_id, item_id) = 1);
ALTER TABLE notifications DROP CONSTRAINT notifications_kind_check;
ALTER TABLE notifications ADD CONSTRAINT notifications_kind_check
  CHECK (kind IN ('mention', 'comment', 'reply', 'due', 'expiry', 'review', 'deadline'));
ALTER TABLE notifications DROP CONSTRAINT notifications_actor;
ALTER TABLE notifications ADD CONSTRAINT notifications_actor
  CHECK (actor IS NOT NULL OR kind IN ('due', 'expiry', 'review', 'deadline'));
CREATE UNIQUE INDEX notifications_reminder_once ON notifications (user_id, item_id, (detail->>'key'), (detail->>'date'))
  WHERE kind IN ('expiry', 'review', 'deadline');
CREATE INDEX notifications_item ON notifications (item_id) WHERE item_id IS NOT NULL;

-- 알림 대상 권한(문서·업무·항목).
CREATE FUNCTION app_notification_target_access(p_document uuid, p_task uuid, p_item uuid) RETURNS text
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT CASE
      WHEN p_document IS NOT NULL THEN app_document_access(p_document)
      WHEN p_task IS NOT NULL THEN app_task_access(p_task)
      ELSE app_item_access(p_item) END
  $$;

-- 항목 알림 하나(받는 사람이 그 항목을 볼 수 있을 때만, 같은 날 같은 대상은 한 번). 만들었으면 true.
CREATE FUNCTION app_notify_item(p_user uuid, p_kind text, p_item uuid, p_record uuid, p_detail jsonb)
  RETURNS boolean
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF p_user IS NULL OR app_item_access_as(p_item, p_user) IS NULL THEN
      RETURN false;
    END IF;
    INSERT INTO notifications (user_id, kind, item_id, record_id, actor, detail)
    VALUES (p_user, p_kind, p_item, p_record, NULL, p_detail)
    ON CONFLICT DO NOTHING;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count > 0;
  END
  $$;

-- 작업: 항목 알림. 만든 알림 수.
CREATE FUNCTION app_system_item_reminders(p_today date) RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row   record;
    v_uid   uuid;
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    -- 강좌 수강권 만료(7일 전·당일)와 목표 완료일(하루 전·당일).
    FOR v_row IN
      SELECT c.item_id, i.title, i.created_by, x.key, x.on_date
      FROM course_details c
      JOIN items i ON i.id = c.item_id
      JOIN modules m ON m.id = i.module_id
      JOIN workspaces w ON w.id = i.workspace_id
      CROSS JOIN LATERAL (VALUES ('course_expiry', c.expires_on, ARRAY[p_today, p_today + 7]),
                                 ('course_target', c.target_on, ARRAY[p_today, p_today + 1])) AS x(key, on_date, days)
      WHERE x.on_date = ANY (x.days) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND w.deleted_at IS NULL
    LOOP
      FOR v_uid IN
        SELECT DISTINCT p.user_id FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
        WHERE e.item_id = v_row.item_id
        UNION SELECT v_row.created_by
        WHERE NOT EXISTS (SELECT 1 FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
                          WHERE e.item_id = v_row.item_id)
      LOOP
        IF app_notify_item(v_uid, 'expiry', v_row.item_id, NULL,
                           jsonb_build_object('key', v_row.key, 'date', p_today, 'due_date', v_row.on_date,
                                              'subject', v_row.title)) THEN
          v_count := v_count + 1;
        END IF;
      END LOOP;
    END LOOP;

    -- 기록 날짜: 복습일(review), 투고 제출 예정일·수정 마감일과 산출물 마감일(deadline).
    FOR v_row IN
      SELECT r.id, r.item_id, r.kind, r.title, r.created_by, r.fields, x.key, x.on_date
      FROM item_records r
      JOIN items i ON i.id = r.item_id
      JOIN modules m ON m.id = i.module_id
      JOIN workspaces w ON w.id = i.workspace_id
      CROSS JOIN LATERAL (VALUES ('date', r.occurred_on),
                                 ('revision_due', CASE WHEN r.kind = 'submission'
                                                       THEN (r.fields->>'revision_due')::date END)) AS x(key, on_date)
      WHERE r.kind IN ('review', 'submission', 'deliverable')
        AND x.on_date IN (p_today, p_today + 1)
        AND coalesce(r.fields->>'status', '') NOT IN ('accepted', 'published', 'rejected', 'done')
        AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL AND w.deleted_at IS NULL
    LOOP
      FOR v_uid IN
        SELECT v_row.created_by
        UNION SELECT (jsonb_array_elements_text(coalesce(v_row.fields->'coauthors', '[]')))::uuid
        WHERE v_row.kind = 'submission'
        UNION SELECT (v_row.fields->>'owner')::uuid WHERE v_row.kind = 'deliverable' AND v_row.fields ? 'owner'
      LOOP
        IF app_notify_item(v_uid, CASE WHEN v_row.kind = 'review' THEN 'review' ELSE 'deadline' END, v_row.item_id,
                           v_row.id,
                           jsonb_build_object('key', v_row.id::text || ':' || v_row.key, 'date', p_today,
                                              'due_date', v_row.on_date, 'subject', v_row.title,
                                              'record_kind', v_row.kind, 'field', v_row.key)) THEN
          v_count := v_count + 1;
        END IF;
      END LOOP;
    END LOOP;
    RETURN v_count;
  END
  $$;

-- 알림 읽기: 대상에 항목을 더한다.
DROP FUNCTION app_my_notifications(integer);
CREATE FUNCTION app_my_notifications(p_limit integer)
  RETURNS TABLE (id uuid, kind text, document_id uuid, task_id uuid, item_id uuid, comment_id uuid,
                 actor_name text, detail jsonb, created_at timestamptz, read_at timestamptz)
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    -- 행위자 이름은 워크스페이스를 함께 쓰거나 그 개인 노트의 소유자일 때만 보인다(users RLS와 같은 기준).
    SELECT n.id, n.kind, n.document_id, n.task_id, n.item_id, n.comment_id,
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
      AND app_notification_target_access(n.document_id, n.task_id, n.item_id) IS NOT NULL
    ORDER BY n.created_at DESC, n.id DESC
    LIMIT p_limit
  $$;

DROP POLICY notifications_select ON notifications;
CREATE POLICY notifications_select ON notifications FOR SELECT
  USING (user_id = app_user_id() AND app_notification_target_access(document_id, task_id, item_id) IS NOT NULL);

REVOKE ALL ON FUNCTION app_notification_target_access(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_notify_item(uuid, text, uuid, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_item_reminders(date) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_my_notifications(integer) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_notification_target_access(uuid, uuid, uuid),
  app_my_notifications(integer),
  app_system_item_reminders(date)
TO fvoci_app;
