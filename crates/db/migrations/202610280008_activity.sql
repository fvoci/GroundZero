-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

-- 협업 활동 피드(D-119). 감사로그와 분리하며 열람은 대상의 app_access로 검사한다.
CREATE TABLE fvoci.activity (
    id uuid NOT NULL,
    organization_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid,
    target_kind text NOT NULL,
    target_id uuid NOT NULL,
    actor uuid,
    kind text NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT activity_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT activity_kind_check CHECK ((kind = ANY (ARRAY['create'::text, 'move'::text, 'comment'::text, 'status'::text, 'kanban_move'::text, 'assign'::text, 'unassign'::text, 'unshare'::text]))),
    CONSTRAINT activity_target_kind_check CHECK ((target_kind = ANY (ARRAY['workspace'::text, 'module'::text, 'item'::text, 'document'::text, 'task'::text, 'event'::text, 'file'::text, 'record'::text])))
);

-- 감사로그(D-57). 공간 로그(workspace_id)·조직 로그(organization_id만)·서버 로그(둘 다 NULL)·개인 로그(owner_user_id)로 나뉜다.
CREATE TABLE fvoci.audit_log (
    id uuid NOT NULL,
    workspace_id uuid,
    organization_id uuid,
    actor uuid,
    action text NOT NULL,
    target_type text NOT NULL,
    target_id uuid NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    owner_user_id uuid,
    CONSTRAINT audit_log_actor CHECK (((actor IS NOT NULL) OR (detail ? 'system'::text))),
    CONSTRAINT audit_log_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.activity OWNER TO fvoci_migrator;

ALTER TABLE fvoci.audit_log OWNER TO fvoci_migrator;

CREATE TABLE fvoci.comments (
    id uuid NOT NULL,
    document_id uuid,
    task_id uuid,
    parent_id uuid,
    body text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    edited boolean DEFAULT false NOT NULL,
    deleted_at timestamp with time zone,
    resolved_at timestamp with time zone,
    resolved_by uuid,
    request_id uuid NOT NULL,
    CONSTRAINT comments_body_check CHECK (((length(btrim(body)) >= 1) AND (length(btrim(body)) <= 10000))),
    CONSTRAINT comments_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT comments_target CHECK (((document_id IS NULL) <> (task_id IS NULL)))
);


ALTER TABLE fvoci.comments OWNER TO fvoci_migrator;

-- 알림함(D-118). 조직 범위로 보되 invite는 어느 조직 문맥에서나 보인다. 개인 노트 대상은 organization_id가 NULL이다.
CREATE TABLE fvoci.notifications (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    organization_id uuid,
    kind text NOT NULL,
    document_id uuid,
    actor uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    read_at timestamp with time zone,
    task_id uuid,
    comment_id uuid,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL,
    item_id uuid,
    record_id uuid,
    invitation_id uuid,
    CONSTRAINT notifications_actor CHECK (((actor IS NOT NULL) OR (kind = ANY (ARRAY['due'::text, 'expiry'::text, 'review'::text, 'deadline'::text])))),
    CONSTRAINT notifications_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT notifications_invite_shape CHECK (((kind = 'invite'::text) = (invitation_id IS NOT NULL))),
    CONSTRAINT notifications_kind_check CHECK ((kind = ANY (ARRAY['mention'::text, 'comment'::text, 'reply'::text, 'due'::text, 'expiry'::text, 'review'::text, 'deadline'::text, 'invite'::text]))),
    CONSTRAINT notifications_target CHECK ((num_nonnulls(document_id, task_id, item_id, invitation_id) = 1))
);


ALTER TABLE fvoci.notifications OWNER TO fvoci_migrator;

CREATE TABLE fvoci.user_marks (
    user_id uuid NOT NULL,
    mark text NOT NULL,
    target_kind text NOT NULL,
    target_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_marks_mark_check CHECK ((mark = ANY (ARRAY['pin'::text, 'favorite'::text]))),
    CONSTRAINT user_marks_target_kind_check CHECK ((target_kind = ANY (ARRAY['item'::text, 'document'::text, 'task'::text])))
);


ALTER TABLE fvoci.user_marks OWNER TO fvoci_migrator;

-- 알림(D-39·D-118). 알림 행은 대상의 조직을 함께 적어 현재 조직 범위로 보이게 한다(개인 노트는 NULL). 받는 사람이 대상을
-- 볼 수 없으면 만들지 않는다. 기한 알림(due)은 항목 공유 잠금 아래에서 보관 여부를 다시 봐, 보관과 겹쳐도 보관 뒤에는
-- 알림이 가지 않는다(#86).
CREATE FUNCTION fvoci.app_notify(p_id uuid, p_user uuid, p_kind text, p_document uuid, p_task uuid, p_comment uuid, p_actor uuid, p_detail jsonb) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item uuid;
    v_org  uuid;
  BEGIN
    IF p_user IS NULL OR p_user = p_actor THEN
      RETURN;
    END IF;
    IF (CASE WHEN p_document IS NOT NULL THEN app_access_as('document', p_document, p_user)
             ELSE app_access_as('task', p_task, p_user) END) = 'none' THEN
      RETURN;
    END IF;
    IF p_kind = 'due' THEN
      SELECT item_id INTO v_item FROM tasks WHERE id = p_task;
      PERFORM app_lock_item(v_item);
      IF app_item_archived(v_item) THEN
        RETURN;
      END IF;
    END IF;
    v_org := CASE WHEN p_document IS NOT NULL THEN app_target_organization('document', p_document)
                  ELSE app_target_organization('task', p_task) END;
    INSERT INTO notifications (id, user_id, organization_id, kind, document_id, task_id, comment_id, actor, detail)
    VALUES (p_id, p_user, v_org, p_kind, p_document, p_task, p_comment, p_actor, coalesce(p_detail, '{}'::jsonb))
    ON CONFLICT DO NOTHING;
  END
  $$;


ALTER FUNCTION fvoci.app_notify(p_id uuid, p_user uuid, p_kind text, p_document uuid, p_task uuid, p_comment uuid, p_actor uuid, p_detail jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_notify_item(p_id uuid, p_user uuid, p_kind text, p_item uuid, p_record uuid, p_detail jsonb) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF p_user IS NULL OR app_access_as('item', p_item, p_user) = 'none' THEN
      RETURN false;
    END IF;
    INSERT INTO notifications (id, user_id, organization_id, kind, item_id, record_id, actor, detail)
    VALUES (p_id, p_user, app_target_organization('item', p_item), p_kind, p_item, p_record, NULL, p_detail)
    ON CONFLICT DO NOTHING;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count > 0;
  END
  $$;


ALTER FUNCTION fvoci.app_notify_item(p_id uuid, p_user uuid, p_kind text, p_item uuid, p_record uuid, p_detail jsonb) OWNER TO fvoci_migrator;

-- 알림이 아직 보여도 되는지: 대상을 볼 수 있어야 한다(D-39). 초대 알림은 그 초대가 내게 온 것이면 결과(수락·거절·철회)까지 보인다.
CREATE FUNCTION fvoci.app_notification_visible(p_document uuid, p_task uuid, p_item uuid, p_invitation uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE
      WHEN p_document IS NOT NULL THEN app_access('document', p_document) <> 'none'
      WHEN p_task IS NOT NULL THEN app_access('task', p_task) <> 'none'
      WHEN p_item IS NOT NULL THEN app_access('item', p_item) <> 'none'
      WHEN p_invitation IS NOT NULL THEN EXISTS (
        SELECT 1 FROM invitations i JOIN users me ON me.id = app_user_id()
        WHERE i.id = p_invitation AND app_invitation_recipient(i.email) = me.id)
      ELSE false
    END
  $$;


ALTER FUNCTION fvoci.app_notification_visible(p_document uuid, p_task uuid, p_item uuid, p_invitation uuid) OWNER TO fvoci_migrator;

-- 알림함 목록(D-118). 현재 조직 범위(p_organization; NULL이면 전체)이고 invite는 어느 조직 문맥에서나 보인다. 커서는
-- (created_at, id) 내림차순 키셋이다.
CREATE FUNCTION fvoci.app_my_notifications(p_organization uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) RETURNS TABLE(id uuid, organization_id uuid, kind text, document_id uuid, task_id uuid, item_id uuid, comment_id uuid, invitation_id uuid, actor_name text, detail jsonb, created_at timestamp with time zone, read_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 행위자 이름은 같은 조직 사람이거나 그 개인 노트의 소유자일 때만 보인다(users RLS와 같은 기준).
    SELECT n.id, n.organization_id, n.kind, n.document_id, n.task_id, n.item_id, n.comment_id, n.invitation_id,
           CASE WHEN n.actor IS NOT NULL AND (app_shares_organization(n.actor)
                     OR (SELECT owner_user_id FROM documents WHERE documents.id = n.document_id) = n.actor)
                THEN u.display_name END,
           CASE WHEN c.id IS NOT NULL AND c.deleted_at IS NULL
                THEN n.detail || jsonb_build_object('excerpt', left(c.body, 140))
                ELSE n.detail END,
           n.created_at, n.read_at
    FROM notifications n
    LEFT JOIN users u ON u.id = n.actor
    LEFT JOIN comments c ON c.id = n.comment_id
    WHERE n.user_id = app_user_id()
      AND (p_organization IS NULL OR n.kind = 'invite' OR n.organization_id = p_organization)
      AND (p_before IS NULL OR (n.created_at, n.id) < (p_before, coalesce(p_before_id, '00000000-0000-0000-0000-000000000000'::uuid)))
      AND app_notification_visible(n.document_id, n.task_id, n.item_id, n.invitation_id)
    ORDER BY n.created_at DESC, n.id DESC
    LIMIT greatest(1, least(coalesce(p_limit, 50), 200))
  $$;


ALTER FUNCTION fvoci.app_my_notifications(p_organization uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_notification_unread_count(p_organization uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT count(*)::integer FROM notifications n
    WHERE n.user_id = app_user_id() AND n.read_at IS NULL
      AND (p_organization IS NULL OR n.kind = 'invite' OR n.organization_id = p_organization)
      AND app_notification_visible(n.document_id, n.task_id, n.item_id, n.invitation_id)
  $$;


ALTER FUNCTION fvoci.app_notification_unread_count(p_organization uuid) OWNER TO fvoci_migrator;

-- 읽음 처리: p_id가 있으면 그 알림만, 없으면 현재 조직 범위(p_organization; NULL이면 전체)의 안 읽은 알림 전부.
CREATE FUNCTION fvoci.app_mark_notifications_read(p_id uuid, p_organization uuid) RETURNS integer
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH done AS (
      UPDATE notifications SET read_at = now()
      WHERE user_id = app_user_id() AND read_at IS NULL AND (p_id IS NULL OR id = p_id)
        AND (p_id IS NOT NULL OR p_organization IS NULL OR kind = 'invite' OR organization_id = p_organization)
      RETURNING 1)
    SELECT count(*)::integer FROM done
  $$;


ALTER FUNCTION fvoci.app_mark_notifications_read(p_id uuid, p_organization uuid) OWNER TO fvoci_migrator;

-- 로그인 때 내 이메일로 온 열린 초대 가운데 아직 알림이 없는 것의 수와 그 알림 만들기(D-118). 받는 사람이 초대 뒤에
-- 가입했을 때 쓴다.
CREATE FUNCTION fvoci.app_pending_invitations_count() RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT count(*)::integer FROM invitations i JOIN users me ON me.id = app_user_id()
    WHERE i.closed_at IS NULL AND i.expires_at > now() AND app_invitation_recipient(i.email) = me.id
      AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.invitation_id = i.id AND n.user_id = me.id)
  $$;


ALTER FUNCTION fvoci.app_pending_invitations_count() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_notify_pending_invitations(p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_inv  uuid;
    v_at   integer := 0;
    v_made integer := 0;
  BEGIN
    IF app_user_id() IS NULL THEN
      RETURN 0;
    END IF;
    FOR v_inv IN
      SELECT i.id FROM invitations i JOIN users me ON me.id = app_user_id()
      WHERE i.closed_at IS NULL AND i.expires_at > now() AND app_invitation_recipient(i.email) = me.id
        AND NOT EXISTS (SELECT 1 FROM notifications n WHERE n.invitation_id = i.id AND n.user_id = me.id)
      ORDER BY i.created_at, i.id
    LOOP
      v_at := v_at + 1;
      IF app_notify_invitation(app_id(p_ids, v_at), v_inv) THEN
        v_made := v_made + 1;
      END IF;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_notify_pending_invitations(p_ids uuid[]) OWNER TO fvoci_migrator;

-- 홈(D-19·D-120). 호출자 권한/RLS로 읽으며 한 집계의 모든 목록은 같은 문장 스냅숏을 쓴다.
CREATE FUNCTION fvoci.app_home(p_organization uuid, p_tz text) RETURNS jsonb
    LANGUAGE plpgsql STABLE SECURITY INVOKER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
DECLARE
  v_tz text;
  v_today date;
BEGIN
  IF app_access('organization', p_organization) = 'none' THEN
    RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
  END IF;
  SELECT coalesce(nullif(btrim(p_tz), ''), nullif(s.time_zone, ''), 'UTC') INTO v_tz
    FROM (SELECT 1) seed LEFT JOIN user_settings s ON s.user_id = app_user_id();
  PERFORM app_check_timezone(v_tz);
  v_today := (now() AT TIME ZONE v_tz)::date;
  RETURN (
    WITH mine AS MATERIALIZED (
      SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id) AS reference,
             t.title, s.name AS status, s.category AS status_category, t.due_date,
             CASE WHEN t.due_date < v_today THEN 'overdue'
                  WHEN t.due_date = v_today THEN 'today' ELSE 'upcoming' END AS bucket,
             t.number
      FROM tasks t JOIN task_statuses s ON s.id = t.status_id
      JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
      WHERE s.category <> 'done' AND t.due_date IS NOT NULL AND t.due_date <= v_today + 7
        AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND t.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
        AND EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = t.id AND a.user_id = app_user_id())
    ), ranked AS (
      SELECT mine.*, row_number() OVER (PARTITION BY bucket ORDER BY due_date, number, id) AS rn FROM mine
    ), upcoming AS (
      SELECT id, item_id, workspace_id, reference, title, status, status_category, due_date, bucket
      FROM ranked WHERE rn <= 50 ORDER BY due_date, number, id
    ), recent_tasks AS (
      SELECT t.id, t.item_id, t.workspace_id, app_task_reference(t.id) AS reference,
             t.title, s.name AS status, s.category AS status_category, t.due_date, NULL::text AS bucket
      FROM tasks t JOIN task_statuses s ON s.id = t.status_id
      WHERE t.updated_by = app_user_id()
        AND t.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
      ORDER BY t.updated_at DESC, t.id LIMIT 8
    ), recent_documents AS (
      SELECT d.id, d.title, app_document_reference(d.id) AS reference, d.workspace_id, d.item_id, d.updated_at
      FROM documents d WHERE d.updated_by = app_user_id()
        AND (d.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
             OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id()))
      ORDER BY d.updated_at DESC, d.id LIMIT 8
    ), active AS (
      SELECT i.id, i.workspace_id, m.kind AS module_kind, app_item_reference(i.id) AS reference,
             i.title, app_access('item', i.id) IN ('write', 'owner') AS can_write,
             CASE m.kind
               WHEN 'course' THEN (
                 SELECT jsonb_build_object('kind', 'course', 'done', count(*) FILTER (WHERE p.status = 'done'), 'total', count(*))
                 FROM course_episodes e LEFT JOIN episode_progress p ON p.episode_id = e.id AND p.user_id = app_user_id()
                 WHERE e.item_id = i.id
               )
               WHEN 'study' THEN jsonb_build_object('kind', 'study', 'amounts', coalesce((
                 SELECT jsonb_agg(to_jsonb(amounts) ORDER BY amounts.unit) FROM (
                   SELECT u.unit, sum(u.amount) AS target,
                          sum(CASE WHEN p.status = 'done' THEN u.amount ELSE least(coalesce(p.done_amount, 0), u.amount) END) AS done
                   FROM study_units u LEFT JOIN unit_progress p ON p.unit_id = u.id AND p.user_id = app_user_id()
                   WHERE u.item_id = i.id GROUP BY u.unit
                 ) amounts
               ), '[]'::jsonb))
               ELSE jsonb_build_object('kind', 'tasks', 'done', c.done, 'total', c.total)
             END AS progress
      FROM items i JOIN modules m ON m.id = i.module_id
      CROSS JOIN LATERAL (
        SELECT count(*) FILTER (WHERE s.category = 'done') AS done, count(*) AS total, max(t.updated_at) AS active
        FROM tasks t JOIN task_statuses s ON s.id = t.status_id WHERE t.item_id = i.id
      ) c
      WHERE i.status = 'active' AND i.archived_at IS NULL AND m.archived_at IS NULL
        AND i.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
      ORDER BY greatest(i.updated_at, c.active) DESC, i.id LIMIT 8
    ), my_tasks AS MATERIALIZED (
      SELECT * FROM app_my_tasks(p_organization, v_today) WITH ORDINALITY
    ), workspace_tasks AS (
      SELECT workspace_id, workspace_name,
             jsonb_agg((to_jsonb(t) - 'workspace_id' - 'workspace_name' - 'task_id' - 'ordinality')
                       || jsonb_build_object('id', task_id) ORDER BY ordinality) AS tasks,
             min(ordinality) AS first_row
      FROM my_tasks t GROUP BY workspace_id, workspace_name
    ), marks AS MATERIALIZED (
      SELECT m.mark, m.target_kind AS kind, m.target_id AS id,
             CASE WHEN m.target_kind = 'document' AND d.item_id IS NOT NULL
                       AND app_access('item', d.item_id) = 'none'
                  THEN d.title
                  ELSE app_time_target_label(m.target_kind, m.target_id) END AS label,
             coalesce(t.workspace_id, d.workspace_id, i.workspace_id) AS workspace_id,
             CASE WHEN app_access('item', coalesce(t.item_id, d.item_id)) <> 'none'
                  THEN coalesce(t.item_id, d.item_id) END AS item_id, m.created_at
      FROM user_marks m
      LEFT JOIN tasks t ON m.target_kind = 'task' AND t.id = m.target_id
      LEFT JOIN documents d ON m.target_kind = 'document' AND d.id = m.target_id
      LEFT JOIN items i ON m.target_kind = 'item' AND i.id = m.target_id
      WHERE m.mark IN ('pin', 'favorite')
        AND (coalesce(t.workspace_id, d.workspace_id, i.workspace_id) IN (SELECT app_my_workspace_ids(p_organization))
             OR (d.workspace_id IS NULL AND d.owner_user_id = app_user_id())
             OR d.id IN (SELECT id FROM app_shared_with_me(p_organization)))
    )
    SELECT jsonb_build_object(
      'today', to_char(v_today, 'YYYY-MM-DD'),
      'my_tasks', jsonb_build_object(
        'counts', (SELECT jsonb_build_object(
          'overdue', count(*) FILTER (WHERE bucket = 'overdue')::integer,
          'today', count(*) FILTER (WHERE bucket = 'today')::integer,
          'upcoming', count(*) FILTER (WHERE bucket = 'upcoming')::integer,
          'undated', count(*) FILTER (WHERE bucket = 'none')::integer,
          'done', count(*) FILTER (WHERE bucket = 'done')::integer) FROM my_tasks),
        'workspaces', coalesce((SELECT jsonb_agg(to_jsonb(w) - 'first_row' ORDER BY first_row) FROM workspace_tasks w), '[]'::jsonb)
      ),
      'up_next', coalesce((SELECT jsonb_agg(to_jsonb(u)) FROM upcoming u), '[]'::jsonb),
      'up_next_counts', (SELECT jsonb_build_object(
        'overdue', count(*) FILTER (WHERE bucket = 'overdue'),
        'today', count(*) FILTER (WHERE bucket = 'today'),
        'upcoming', count(*) FILTER (WHERE bucket = 'upcoming')) FROM mine),
      'recent_tasks', coalesce((SELECT jsonb_agg(to_jsonb(t)) FROM recent_tasks t), '[]'::jsonb),
      'recent_documents', coalesce((SELECT jsonb_agg(to_jsonb(d)) FROM recent_documents d), '[]'::jsonb),
      'active_items', coalesce((SELECT jsonb_agg(to_jsonb(i)) FROM active i), '[]'::jsonb),
      'pins', coalesce((SELECT jsonb_agg(to_jsonb(m) - 'mark' - 'created_at' ORDER BY m.created_at, m.id)
                       FROM marks m WHERE m.mark = 'pin' AND m.label IS NOT NULL), '[]'::jsonb),
      'favorites', coalesce((SELECT jsonb_agg(to_jsonb(m) - 'mark' - 'created_at' ORDER BY m.created_at, m.id)
                            FROM marks m WHERE m.mark = 'favorite' AND m.label IS NOT NULL), '[]'::jsonb)
    )
  );
END
$$;

ALTER FUNCTION fvoci.app_home(p_organization uuid, p_tz text) OWNER TO fvoci_migrator;

-- 내 업무(D-120): 담당자가 나인 업무. 현재 조직(p_organization; NULL이면 전체) 안에서 워크스페이스별로 묶고 기한 없는 업무도
-- 'none' 묶음으로 넣는다. 범주만 보며 완료는 'done' 묶음이다.
CREATE FUNCTION fvoci.app_my_tasks(p_organization uuid, p_today date) RETURNS TABLE(workspace_id uuid, workspace_name text, item_id uuid, task_id uuid, reference text, title text, status text, status_category text, priority text, start_date date, due_date date, bucket text, updated_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT w.id, w.name, t.item_id, t.id, app_task_reference(t.id), t.title, s.name, s.category, t.priority,
           t.start_date, t.due_date,
           CASE WHEN s.category = 'done' THEN 'done'
                WHEN t.due_date IS NULL THEN 'none'
                WHEN t.due_date < p_today THEN 'overdue'
                WHEN t.due_date = p_today THEN 'today'
                ELSE 'upcoming' END,
           t.updated_at
    FROM tasks t
    JOIN task_statuses s ON s.id = t.status_id
    JOIN workspaces w ON w.id = t.workspace_id
    JOIN task_assignees a ON a.task_id = t.id AND a.user_id = app_user_id()
    WHERE t.deleted_at IS NULL AND w.deleted_at IS NULL
      AND (p_organization IS NULL OR w.organization_id = p_organization)
      AND app_access('task', t.id) <> 'none'
    ORDER BY w.created_at, w.id, (t.due_date IS NULL), t.due_date, t.number
  $$;


ALTER FUNCTION fvoci.app_my_tasks(p_organization uuid, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_my_task_counts(p_organization uuid, p_today date) RETURNS TABLE(overdue integer, today integer, upcoming integer, undated integer, done integer)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT count(*) FILTER (WHERE m.bucket = 'overdue')::integer,
           count(*) FILTER (WHERE m.bucket = 'today')::integer,
           count(*) FILTER (WHERE m.bucket = 'upcoming')::integer,
           count(*) FILTER (WHERE m.bucket = 'none')::integer,
           count(*) FILTER (WHERE m.bucket = 'done')::integer
    FROM app_my_tasks(p_organization, p_today) m
  $$;


ALTER FUNCTION fvoci.app_my_task_counts(p_organization uuid, p_today date) OWNER TO fvoci_migrator;

-- 활동 피드 읽기(D-119). 항목의 활동 탭은 그 항목 아래 모든 대상의 활동을, 다른 대상은 자기 활동을 본다. 열람은 app_access다.
CREATE FUNCTION fvoci.app_target_activity(p_kind text, p_id uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) RETURNS TABLE(id uuid, kind text, actor_id uuid, actor_name text, target_kind text, target_id uuid, item_id uuid, detail jsonb, created_at timestamp with time zone)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access(p_kind, p_id) = 'none' THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      SELECT a.id, a.kind, a.actor, CASE WHEN app_shares_organization(a.actor) THEN u.display_name END,
             a.target_kind, a.target_id, a.item_id, a.detail, a.created_at
      FROM activity a LEFT JOIN users u ON u.id = a.actor
      WHERE ((p_kind = 'item' AND a.item_id = p_id) OR (a.target_kind = p_kind AND a.target_id = p_id))
        AND app_access(a.target_kind, a.target_id) <> 'none'
        AND (p_before IS NULL OR (a.created_at, a.id) < (p_before, coalesce(p_before_id, '00000000-0000-0000-0000-000000000000'::uuid)))
      ORDER BY a.created_at DESC, a.id DESC
      LIMIT greatest(1, least(coalesce(p_limit, 50), 200));
  END
  $$;


ALTER FUNCTION fvoci.app_target_activity(p_kind text, p_id uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) OWNER TO fvoci_migrator;

-- 홈의 최근 활동(D-19·D-119): 현재 조직(p_organization; NULL이면 전체)에서 내가 볼 수 있는 대상의 활동.
CREATE FUNCTION fvoci.app_recent_activity(p_organization uuid, p_limit integer) RETURNS TABLE(id uuid, kind text, actor_id uuid, actor_name text, workspace_id uuid, target_kind text, target_id uuid, item_id uuid, detail jsonb, created_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT a.id, a.kind, a.actor, CASE WHEN app_shares_organization(a.actor) THEN u.display_name END,
           a.workspace_id, a.target_kind, a.target_id, a.item_id, a.detail, a.created_at
    FROM activity a LEFT JOIN users u ON u.id = a.actor
    WHERE a.workspace_id IN (SELECT app_my_workspace_ids(p_organization))
      AND app_access(a.target_kind, a.target_id) <> 'none'
    ORDER BY a.created_at DESC, a.id DESC
    LIMIT greatest(1, least(coalesce(p_limit, 20), 100))
  $$;


ALTER FUNCTION fvoci.app_recent_activity(p_organization uuid, p_limit integer) OWNER TO fvoci_migrator;

-- 감사로그 만료 삭제(D-57·D-124). 행마다 보존 기간은 app_retention_days(공간·조직 설정, 서버 기본값 p_default_days)이고
-- 그 값이 NULL이면(미설정 또는 두 단계가 다른 값으로 충돌) 지우지 않는다. 지운 행 수를 돌려준다.
CREATE FUNCTION fvoci.app_system_purge_audit(p_default_days integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    DELETE FROM audit_log a
    WHERE app_retention_days(a.workspace_id, a.organization_id, 'audit_retention_days', p_default_days) IS NOT NULL
      AND a.created_at < now() - make_interval(days =>
            app_retention_days(a.workspace_id, a.organization_id, 'audit_retention_days', p_default_days));
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_purge_audit(p_default_days integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_purge_activity(p_default_days integer) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    DELETE FROM activity a
    WHERE app_retention_days(a.workspace_id, a.organization_id, 'activity_retention_days', p_default_days) IS NOT NULL
      AND a.created_at < now() - make_interval(days =>
            app_retention_days(a.workspace_id, a.organization_id, 'activity_retention_days', p_default_days));
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_purge_activity(p_default_days integer) OWNER TO fvoci_migrator;

-- 댓글 대상(문서 또는 업무)의 등급. 댓글 행이 아직 없을 때(만들기) 쓴다.
CREATE FUNCTION fvoci.app_comment_target_access(p_document uuid, p_task uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN p_document IS NOT NULL THEN app_access('document', p_document) ELSE app_access('task', p_task) END
  $$;


ALTER FUNCTION fvoci.app_comment_target_access(p_document uuid, p_task uuid) OWNER TO fvoci_migrator;

-- 활동 피드 기록(D-119). 대상의 워크스페이스로 조직을 정하고 행위자는 현재 사용자(작업 프로세스는 NULL)다.
-- 감사로그(D-57)와 별개이며 두 목록에 다 있는 사건(생성·이동)은 두 곳에 모두 남긴다.
CREATE FUNCTION fvoci.app_activity(p_id uuid, p_workspace uuid, p_item uuid, p_target_kind text, p_target uuid, p_kind text, p_detail jsonb) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO activity (id, organization_id, workspace_id, item_id, target_kind, target_id, actor, kind, detail)
    SELECT p_id, w.organization_id, w.id, p_item, p_target_kind, p_target, app_user_id(), p_kind,
           coalesce(p_detail, '{}'::jsonb)
    FROM workspaces w WHERE w.id = p_workspace
  $$;


ALTER FUNCTION fvoci.app_activity(p_id uuid, p_workspace uuid, p_item uuid, p_target_kind text, p_target uuid, p_kind text, p_detail jsonb) OWNER TO fvoci_migrator;

-- 초대 알림(D-118). 받는 사람이 아직 가입하지 않았으면 로그인 때(app_upsert_user) 만든다. 다시 보내면 안 읽음으로 되살린다.
CREATE FUNCTION fvoci.app_notify_invitation(p_id uuid, p_invitation uuid) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_inv  invitations%ROWTYPE;
    v_user uuid;
  BEGIN
    SELECT * INTO v_inv FROM invitations WHERE id = p_invitation;
    IF v_inv.id IS NULL OR v_inv.closed_at IS NOT NULL THEN
      RETURN false;
    END IF;
    v_user := app_invitation_recipient(v_inv.email);
    IF v_user IS NULL THEN
      RETURN false;
    END IF;
    UPDATE notifications SET read_at = NULL, created_at = now(), actor = v_inv.invited_by,
                             detail = jsonb_build_object('role', v_inv.role, 'workspace', v_inv.workspace_id)
    WHERE invitation_id = p_invitation AND user_id = v_user;
    IF FOUND THEN
      RETURN true;
    END IF;
    INSERT INTO notifications (id, user_id, organization_id, kind, invitation_id, actor, detail)
    VALUES (p_id, v_user, v_inv.organization_id, 'invite', p_invitation, v_inv.invited_by,
            jsonb_build_object('role', v_inv.role, 'workspace', v_inv.workspace_id));
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_notify_invitation(p_id uuid, p_invitation uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_id     uuid;
    v_parent comments%ROWTYPE;
    v_owner  uuid;
    v_body   text := btrim(coalesce(p_body, ''));
    v_uid    uuid;
    v_at     integer := 1;
    v_ws     uuid;
    v_item   uuid;
    v_access text;
  BEGIN
    SELECT id INTO v_id FROM comments WHERE created_by = v_user AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF (p_document IS NULL) = (p_task IS NULL) THEN
      RAISE EXCEPTION 'comment on a document or a task' USING ERRCODE = '22023';
    END IF;
    IF app_comment_target_access(p_document, p_task) = 'none' THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    -- 보관(항목 행 FOR UPDATE)과 직렬화한다(#86): 모듈·항목을 공유 잠금한 뒤 등급·보관을 다시 본다. 댓글은 쓰기라
    -- 읽기 전용·읽기 공유는 쓸 수 없고(D-20), 보관되어 read로 내려간 대상은 409다.
    PERFORM app_lock_item(coalesce((SELECT d.item_id FROM documents d WHERE d.id = p_document),
                                   (SELECT t.item_id FROM tasks t WHERE t.id = p_task)));
    v_access := app_comment_target_access(p_document, p_task);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      IF app_comment_target_archived(p_document, p_task) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'write access is required to comment' USING ERRCODE = '42501';
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
    INSERT INTO comments (id, document_id, task_id, parent_id, body, created_by, request_id)
    VALUES (app_id(p_ids, 1), p_document, p_task, p_parent, v_body, v_user, p_request)
    RETURNING id INTO v_id;
    -- 활동 피드(D-119). 개인 노트의 댓글은 워크스페이스가 없어 남기지 않는다.
    SELECT coalesce(d.workspace_id, t.workspace_id), coalesce(d.item_id, t.item_id) INTO v_ws, v_item
    FROM (SELECT 1) x
    LEFT JOIN documents d ON d.id = p_document
    LEFT JOIN tasks t ON t.id = p_task;
    IF v_ws IS NOT NULL THEN
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_ws, v_item,
                           CASE WHEN p_document IS NOT NULL THEN 'document' ELSE 'task' END,
                           coalesce(p_document, p_task), 'comment',
                           jsonb_build_object('comment', v_id, 'reply', p_parent IS NOT NULL));
    END IF;
    IF p_parent IS NULL THEN
      -- 새 스레드: 대상을 만든 사람과(업무면) 담당자에게, 사람마다 한 번.
      FOR v_uid IN
        SELECT v_owner WHERE v_owner IS NOT NULL
        UNION SELECT user_id FROM task_assignees WHERE task_id = p_task
      LOOP
        v_at := v_at + 1;
        PERFORM app_notify(app_id(p_ids, v_at), v_uid, 'comment', p_document, p_task, v_id, v_user, '{}'::jsonb);
      END LOOP;
    ELSE
      -- 답글: 스레드를 연 사람과 그 스레드에 답한 사람에게.
      FOR v_uid IN
        SELECT DISTINCT created_by FROM comments WHERE (id = p_parent OR parent_id = p_parent) AND deleted_at IS NULL
      LOOP
        v_at := v_at + 1;
        PERFORM app_notify(app_id(p_ids, v_at), v_uid, 'reply', p_document, p_task, v_id, v_user, '{}'::jsonb);
      END LOOP;
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_comment_ids(p_document uuid, p_task uuid, p_parent uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 댓글 1 + 활동 1 + 대상을 만든 사람 알림 1 + 담당자(또는 스레드 참여자)마다 알림 1.
    SELECT CASE
      WHEN app_comment_target_access(p_document, p_task) = 'none' THEN 0
      WHEN p_parent IS NULL THEN 3 + (SELECT count(*) FROM task_assignees WHERE task_id = p_task)::integer
      ELSE 3 + (SELECT count(DISTINCT created_by) FROM comments
                WHERE (id = p_parent OR parent_id = p_parent) AND deleted_at IS NULL)::integer
    END
  $$;


ALTER FUNCTION fvoci.app_add_comment_ids(p_document uuid, p_task uuid, p_parent uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_audit(p_id uuid, p_workspace uuid, p_action text, p_target_type text, p_target uuid, p_detail jsonb) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
    VALUES (p_id, p_workspace, app_user_id(), p_action, p_target_type, p_target, coalesce(p_detail, '{}'::jsonb))
  $$;


ALTER FUNCTION fvoci.app_audit(p_id uuid, p_workspace uuid, p_action text, p_target_type text, p_target uuid, p_detail jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_audit_document(p_id uuid, p_doc uuid, p_action text, p_detail jsonb) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    SELECT p_id, d.workspace_id, d.owner_user_id, app_user_id(), p_action, 'document', d.id,
           coalesce(p_detail, '{}'::jsonb)
    FROM documents d WHERE d.id = p_doc
  $$;


ALTER FUNCTION fvoci.app_audit_document(p_id uuid, p_doc uuid, p_action text, p_detail jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_audit_document_export(p_doc uuid, p_revision integer, p_format text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_workspace uuid;
    v_owner     uuid;
  BEGIN
    IF app_access('document', p_doc) = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT app_document_can_export(p_doc) THEN
      RAISE EXCEPTION 'exporting this document is not allowed' USING ERRCODE = '42501';
    END IF;
    IF p_format NOT IN ('markdown', 'zip', 'pdf') THEN
      RAISE EXCEPTION 'unknown export format' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id, owner_user_id INTO v_workspace, v_owner FROM documents WHERE id = p_doc;
    IF v_owner IS NULL THEN
      PERFORM app_audit(app_id(p_ids, 1), v_workspace, 'export', 'document', p_doc,
                        jsonb_build_object('revision', p_revision, 'format', p_format));
    ELSE
      INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), NULL, v_owner, app_user_id(), 'export', 'document', p_doc,
              jsonb_build_object('revision', p_revision, 'format', p_format));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_audit_document_export(p_doc uuid, p_revision integer, p_format text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-88: 제목 변경 감사의 UUID도 앱이 공급한다. 저장·복원 호출자가 리비전 기록 뒤 이 함수를 부른다.
-- 이전·새 제목은 두 리비전에서 읽고, 고유 인덱스로 같은 리비전의 감사를 한 번만 남긴다.
CREATE FUNCTION fvoci.app_audit_document_rename(p_id uuid, p_doc uuid, p_revision integer) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE;
    v_new document_revisions%ROWTYPE;
    v_old text;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    SELECT * INTO v_new FROM document_revisions WHERE document_id = p_doc AND revision = p_revision;
    IF v_new.document_id IS NULL OR v_doc.revision <> p_revision OR v_new.author IS DISTINCT FROM app_user_id() THEN
      RAISE EXCEPTION 'revision not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT title INTO v_old FROM document_revisions WHERE document_id = p_doc AND revision = p_revision - 1;
    IF v_old IS NULL OR v_old = v_new.title THEN
      RETURN;
    END IF;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (p_id, v_doc.workspace_id, v_doc.owner_user_id, app_user_id(), 'rename', 'document', p_doc,
            jsonb_build_object('from', v_old, 'to', v_new.title, 'revision', p_revision))
    ON CONFLICT DO NOTHING;
  END
  $$;


ALTER FUNCTION fvoci.app_audit_document_rename(p_id uuid, p_doc uuid, p_revision integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_audit_personal_export(p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    SELECT app_id(p_ids, 1), NULL, app_user_id(), app_user_id(), 'export', 'user', app_user_id(), '{}'::jsonb
    WHERE app_user_id() IS NOT NULL
  $$;


ALTER FUNCTION fvoci.app_audit_personal_export(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_audit_workspace_export(p_workspace uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can export the workspace' USING ERRCODE = '42501';
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'export', 'workspace', p_workspace, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_audit_workspace_export(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_comment_target_archived(p_document uuid, p_task uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce(app_item_archived(CASE
      WHEN p_document IS NOT NULL THEN (SELECT item_id FROM documents WHERE id = p_document)
      ELSE (SELECT item_id FROM tasks WHERE id = p_task) END), false)
  $$;


ALTER FUNCTION fvoci.app_comment_target_archived(p_document uuid, p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_comment_target_state(p_document uuid, p_task uuid) RETURNS TABLE(access text, archived boolean)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT a.access, app_comment_target_archived(p_document, p_task)
    FROM (SELECT app_comment_target_access(p_document, p_task) AS access) a
    WHERE a.access <> 'none'
  $$;


ALTER FUNCTION fvoci.app_comment_target_state(p_document uuid, p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_comment(p_comment uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
    v_ws     uuid;
    v_owner  uuid;
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_c.id IS NULL OR v_c.deleted_at IS NOT NULL OR v_access = 'none' THEN
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
      INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_ws, v_owner, app_user_id(), 'delete_comment',
              CASE WHEN v_c.document_id IS NOT NULL THEN 'document' ELSE 'task' END,
              coalesce(v_c.document_id, v_c.task_id), jsonb_build_object('author', v_c.created_by));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_comment(p_comment uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_edit_comment(p_comment uuid, p_body text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_body   text := btrim(coalesce(p_body, ''));
    v_access text;
  BEGIN
    -- 댓글 만들기(답글의 부모 SHARE)와 같은 모듈 → 항목 → 댓글 순서다(#86).
    PERFORM app_lock_item((SELECT coalesce(d.item_id, t.item_id) FROM comments c
                          LEFT JOIN documents d ON d.id = c.document_id LEFT JOIN tasks t ON t.id = c.task_id
                          WHERE c.id = p_comment));
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    IF v_c.id IS NULL OR v_c.deleted_at IS NOT NULL OR app_comment_target_access(v_c.document_id, v_c.task_id) = 'none' THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.created_by <> app_user_id() THEN
      RAISE EXCEPTION 'only the author can edit a comment' USING ERRCODE = '42501';
    END IF;
    -- 보관과 직렬화(#86): 항목을 공유 잠금한 뒤 등급·보관을 다시 본다. 본인 댓글이라도 쓰기 등급이 필요하다(D-20).
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_access NOT IN ('owner', 'write') THEN
      IF app_comment_target_archived(v_c.document_id, v_c.task_id) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'write access is required to comment' USING ERRCODE = '42501';
    END IF;
    IF length(v_body) NOT BETWEEN 1 AND 10000 THEN
      RAISE EXCEPTION 'a comment needs 1 to 10000 characters' USING ERRCODE = '22023';
    END IF;
    UPDATE comments SET body = v_body, edited = true, updated_at = now() WHERE id = p_comment;
  END
  $$;


ALTER FUNCTION fvoci.app_edit_comment(p_comment uuid, p_body text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_resolve_comment(p_comment uuid, p_resolved boolean) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
  BEGIN
    -- 해결·재개도 모듈 → 항목 → 댓글 순서로 보관 장벽을 잡는다(#86).
    PERFORM app_lock_item((SELECT coalesce(d.item_id, t.item_id) FROM comments c
                          LEFT JOIN documents d ON d.id = c.document_id LEFT JOIN tasks t ON t.id = c.task_id
                          WHERE c.id = p_comment));
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_c.id IS NULL OR v_access = 'none' THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'resolve the first comment of the thread' USING ERRCODE = '22023';
    END IF;
    -- 보관과 직렬화(#86): 항목을 공유 잠금한 뒤 등급·보관을 다시 본다. 스레드 해결은 쓰기라 쓰기 등급이 필요하다(D-20).
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_access NOT IN ('owner', 'write') THEN
      IF app_comment_target_archived(v_c.document_id, v_c.task_id) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    UPDATE comments SET resolved_at = CASE WHEN p_resolved THEN now() END,
                        resolved_by = CASE WHEN p_resolved THEN app_user_id() END
    WHERE id = p_comment;
  END
  $$;


ALTER FUNCTION fvoci.app_resolve_comment(p_comment uuid, p_resolved boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_mark(p_mark text, p_kind text, p_id uuid, p_on boolean) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    IF p_mark NOT IN ('pin', 'favorite') OR p_kind NOT IN ('item', 'document', 'task') THEN
      RAISE EXCEPTION 'invalid mark' USING ERRCODE = '22023';
    END IF;
    IF NOT p_on THEN
      DELETE FROM user_marks WHERE user_id = v_user AND mark = p_mark AND target_kind = p_kind AND target_id = p_id;
      RETURN;
    END IF;
    IF app_access(p_kind, p_id) = 'none' THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-marks:' || v_user::text, 0));
    IF EXISTS (SELECT 1 FROM user_marks WHERE user_id = v_user AND mark = p_mark AND target_kind = p_kind
               AND target_id = p_id) THEN
      RETURN;
    END IF;
    IF (SELECT count(*) FROM user_marks m WHERE m.user_id = v_user AND m.mark = p_mark
        AND app_access(m.target_kind, m.target_id) <> 'none') >= 100 THEN
      RAISE EXCEPTION 'too many marks' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_marks (user_id, mark, target_kind, target_id) VALUES (v_user, p_mark, p_kind, p_id)
    ON CONFLICT DO NOTHING;
  END
  $$;


ALTER FUNCTION fvoci.app_set_mark(p_mark text, p_kind text, p_id uuid, p_on boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_due_notifications(p_today date, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
        AND w.deleted_at IS NULL AND (SELECT ts.category FROM task_statuses ts WHERE ts.id = t.status_id) <> 'done'
        AND t.due_date IN (p_today, p_today + 1)
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


ALTER FUNCTION fvoci.app_system_due_notifications(p_today date, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_due_notifications_ids(p_today date) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      (SELECT count(*)
       FROM tasks t JOIN items i ON i.id = t.item_id JOIN modules m ON m.id = i.module_id
       JOIN workspaces w ON w.id = t.workspace_id
       CROSS JOIN LATERAL (
         SELECT user_id FROM task_assignees WHERE task_id = t.id
         UNION SELECT t.created_by WHERE NOT EXISTS (SELECT 1 FROM task_assignees WHERE task_id = t.id)) r
       WHERE t.deleted_at IS NULL AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
         AND w.deleted_at IS NULL AND (SELECT ts.category FROM task_statuses ts WHERE ts.id = t.status_id) <> 'done'
        AND t.due_date IN (p_today, p_today + 1))::integer
    END
  $$;


ALTER FUNCTION fvoci.app_system_due_notifications_ids(p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_item_reminders(p_today date, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row   record;
    v_uid   uuid;
    v_count integer := 0;
    v_at    integer := 0;
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
        v_at := v_at + 1;
        IF app_notify_item(app_id(p_ids, v_at), v_uid, 'expiry', v_row.item_id, NULL,
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
                                                       THEN app_iso_date(r.fields->>'revision_due') END)) AS x(key, on_date)
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
        v_at := v_at + 1;
        IF app_notify_item(app_id(p_ids, v_at), v_uid, CASE WHEN v_row.kind = 'review' THEN 'review' ELSE 'deadline' END,
                           v_row.item_id, v_row.id,
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


ALTER FUNCTION fvoci.app_system_item_reminders(p_today date, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_item_reminders_ids(p_today date) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      (SELECT count(*)
       FROM course_details c
       JOIN items i ON i.id = c.item_id
       JOIN modules m ON m.id = i.module_id
       JOIN workspaces w ON w.id = i.workspace_id
       CROSS JOIN LATERAL (VALUES ('course_expiry', c.expires_on, ARRAY[p_today, p_today + 7]),
                                  ('course_target', c.target_on, ARRAY[p_today, p_today + 1])) AS x(key, on_date, days)
       CROSS JOIN LATERAL (
         SELECT DISTINCT p.user_id FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
         WHERE e.item_id = c.item_id
         UNION SELECT i.created_by
         WHERE NOT EXISTS (SELECT 1 FROM episode_progress p JOIN course_episodes e ON e.id = p.episode_id
                           WHERE e.item_id = c.item_id)) u
       WHERE x.on_date = ANY (x.days) AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
         AND w.deleted_at IS NULL)::integer
      + (SELECT count(*)
         FROM item_records r
         JOIN items i ON i.id = r.item_id
         JOIN modules m ON m.id = i.module_id
         JOIN workspaces w ON w.id = i.workspace_id
         CROSS JOIN LATERAL (VALUES ('date', r.occurred_on),
                                    ('revision_due', CASE WHEN r.kind = 'submission'
                                                          THEN app_iso_date(r.fields->>'revision_due') END)) AS x(key, on_date)
         CROSS JOIN LATERAL (
           SELECT r.created_by
           UNION SELECT (jsonb_array_elements_text(coalesce(r.fields->'coauthors', '[]')))::uuid
           WHERE r.kind = 'submission'
           UNION SELECT (r.fields->>'owner')::uuid WHERE r.kind = 'deliverable' AND r.fields ? 'owner') u
         WHERE r.kind IN ('review', 'submission', 'deliverable')
           AND x.on_date IN (p_today, p_today + 1)
           AND coalesce(r.fields->>'status', '') NOT IN ('accepted', 'published', 'rejected', 'done')
           AND i.deleted_at IS NULL AND i.archived_at IS NULL AND m.archived_at IS NULL
           AND w.deleted_at IS NULL)::integer
    END
  $$;


ALTER FUNCTION fvoci.app_system_item_reminders_ids(p_today date) OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.activity
    ADD CONSTRAINT activity_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.audit_log
    ADD CONSTRAINT audit_log_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_request UNIQUE (created_by, request_id);

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.user_marks
    ADD CONSTRAINT user_marks_pkey PRIMARY KEY (user_id, mark, target_kind, target_id);

CREATE UNIQUE INDEX audit_log_document_rename ON fvoci.audit_log USING btree (target_id, ((detail ->> 'revision'::text))) WHERE ((action = 'rename'::text) AND (target_type = 'document'::text) AND (detail ? 'revision'::text));

CREATE INDEX activity_item ON fvoci.activity USING btree (item_id, created_at DESC) WHERE (item_id IS NOT NULL);

CREATE INDEX activity_target ON fvoci.activity USING btree (target_kind, target_id, created_at DESC);

CREATE INDEX activity_workspace ON fvoci.activity USING btree (workspace_id, created_at DESC);

CREATE INDEX audit_log_organization ON fvoci.audit_log USING btree (organization_id, created_at) WHERE (organization_id IS NOT NULL);

CREATE INDEX audit_log_workspace ON fvoci.audit_log USING btree (workspace_id, created_at);

CREATE INDEX comments_document ON fvoci.comments USING btree (document_id, created_at) WHERE (document_id IS NOT NULL);

CREATE INDEX comments_parent ON fvoci.comments USING btree (parent_id) WHERE (parent_id IS NOT NULL);

CREATE INDEX comments_task ON fvoci.comments USING btree (task_id, created_at) WHERE (task_id IS NOT NULL);

CREATE UNIQUE INDEX notifications_due_once ON fvoci.notifications USING btree (user_id, task_id, ((detail ->> 'date'::text))) WHERE (kind = 'due'::text);

CREATE INDEX notifications_item ON fvoci.notifications USING btree (item_id) WHERE (item_id IS NOT NULL);

CREATE UNIQUE INDEX notifications_reminder_once ON fvoci.notifications USING btree (user_id, item_id, ((detail ->> 'key'::text)), ((detail ->> 'date'::text))) WHERE (kind = ANY (ARRAY['expiry'::text, 'review'::text, 'deadline'::text]));

CREATE INDEX notifications_invitation ON fvoci.notifications USING btree (invitation_id) WHERE (invitation_id IS NOT NULL);

CREATE INDEX notifications_user ON fvoci.notifications USING btree (user_id, created_at DESC, id DESC);

CREATE INDEX notifications_user_unread ON fvoci.notifications USING btree (user_id, organization_id) WHERE (read_at IS NULL);

ALTER TABLE ONLY fvoci.activity
    ADD CONSTRAINT activity_actor_fkey FOREIGN KEY (actor) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.activity
    ADD CONSTRAINT activity_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);
ALTER TABLE ONLY fvoci.activity
    ADD CONSTRAINT activity_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id) ON DELETE CASCADE;
ALTER TABLE ONLY fvoci.audit_log
    ADD CONSTRAINT audit_log_actor_fkey FOREIGN KEY (actor) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.audit_log
    ADD CONSTRAINT audit_log_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);

ALTER TABLE ONLY fvoci.audit_log
    ADD CONSTRAINT audit_log_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.audit_log
    ADD CONSTRAINT audit_log_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES fvoci.comments(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_resolved_by_fkey FOREIGN KEY (resolved_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_task_id_fkey FOREIGN KEY (task_id) REFERENCES fvoci.tasks(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_actor_fkey FOREIGN KEY (actor) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_comment_id_fkey FOREIGN KEY (comment_id) REFERENCES fvoci.comments(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_record_id_fkey FOREIGN KEY (record_id) REFERENCES fvoci.item_records(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_task_id_fkey FOREIGN KEY (task_id) REFERENCES fvoci.tasks(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_invitation_id_fkey FOREIGN KEY (invitation_id) REFERENCES fvoci.invitations(id) ON DELETE CASCADE;
ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);
ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.user_marks
    ADD CONSTRAINT user_marks_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
