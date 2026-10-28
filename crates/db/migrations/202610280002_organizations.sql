-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

CREATE TABLE fvoci.group_members (
    group_id uuid NOT NULL,
    user_id uuid NOT NULL
);


ALTER TABLE fvoci.group_members OWNER TO fvoci_migrator;

CREATE TABLE fvoci.groups (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT groups_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT groups_name_check CHECK ((length(btrim(name)) > 0))
);


ALTER TABLE fvoci.groups OWNER TO fvoci_migrator;

-- 초대(D-51·D-118). workspace_id가 있으면 워크스페이스 초대, 없으면 조직 초대다. 수락·거절은 알림함의 invite 알림에서 한다.
CREATE TABLE fvoci.invitations (
    id uuid NOT NULL,
    organization_id uuid NOT NULL,
    workspace_id uuid,
    email text NOT NULL,
    role text NOT NULL,
    invited_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    accepted_by uuid,
    accepted_at timestamp with time zone,
    closed_at timestamp with time zone,
    close_reason text,
    CONSTRAINT invitations_close_reason_check CHECK ((close_reason = ANY (ARRAY['accepted'::text, 'declined'::text, 'revoked'::text, 'workspace_deleted'::text, 'organization_left'::text]))),
    CONSTRAINT invitations_email_check CHECK (((email = lower(btrim(email))) AND (POSITION(('@'::text) IN (email)) > 1))),
    CONSTRAINT invitations_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT invitations_role_check CHECK ((((workspace_id IS NOT NULL) AND (role = ANY (ARRAY['admin'::text, 'member'::text, 'readonly'::text]))) OR ((workspace_id IS NULL) AND (role = ANY (ARRAY['admin'::text, 'member'::text])))))
);


ALTER TABLE fvoci.invitations OWNER TO fvoci_migrator;

CREATE TABLE fvoci.item_members (
    item_id uuid NOT NULL,
    user_id uuid,
    group_id uuid,
    added_by uuid NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT item_members_one CHECK (((user_id IS NULL) <> (group_id IS NULL)))
);


ALTER TABLE fvoci.item_members OWNER TO fvoci_migrator;

CREATE TABLE fvoci.items (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    module_id uuid NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    visibility text DEFAULT 'private'::text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    archived_at timestamp with time zone,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    leader_id uuid,
    goal text DEFAULT ''::text NOT NULL,
    scope text DEFAULT ''::text NOT NULL,
    starts_on date,
    ends_on date,
    status text DEFAULT 'active'::text NOT NULL,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    is_template boolean DEFAULT false NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    profile_version integer DEFAULT 1 NOT NULL,
    CONSTRAINT items_details_check CHECK ((jsonb_typeof(details) = 'object'::text)),
    CONSTRAINT items_goal_check CHECK ((length(goal) <= 4000)),
    CONSTRAINT items_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT items_period CHECK (((starts_on IS NULL) OR (ends_on IS NULL) OR (starts_on <= ends_on))),
    CONSTRAINT items_scope_check CHECK ((length(scope) <= 4000)),
    CONSTRAINT items_status_check CHECK ((status = ANY (ARRAY['planned'::text, 'active'::text, 'on_hold'::text, 'done'::text, 'cancelled'::text]))),
    CONSTRAINT items_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT items_visibility_check CHECK ((visibility = ANY (ARRAY['workspace'::text, 'private'::text])))
);


ALTER TABLE fvoci.items OWNER TO fvoci_migrator;

CREATE TABLE fvoci.module_keys (
    workspace_id uuid NOT NULL,
    key text NOT NULL,
    module_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    retired_at timestamp with time zone,
    CONSTRAINT module_keys_key_check CHECK (((key ~ '^[A-Z][A-Z0-9]{1,9}$'::text) AND (key <> 'WIKI'::text))),
    CONSTRAINT module_keys_reserved_retired CHECK (((module_id IS NOT NULL) OR (retired_at IS NOT NULL)))
);


ALTER TABLE fvoci.module_keys OWNER TO fvoci_migrator;

CREATE TABLE fvoci.modules (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    kind text NOT NULL,
    name text NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    archived_at timestamp with time zone,
    icon text DEFAULT ''::text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    CONSTRAINT modules_description_check CHECK ((length(description) <= 4000)),
    CONSTRAINT modules_icon_check CHECK ((length(icon) <= 16)),
    CONSTRAINT modules_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT modules_kind_check CHECK ((kind = ANY (ARRAY['project'::text, 'research'::text, 'study'::text, 'course'::text, 'hobby'::text]))),
    CONSTRAINT modules_name_check CHECK ((length(btrim(name)) > 0))
);


ALTER TABLE fvoci.modules OWNER TO fvoci_migrator;

CREATE TABLE fvoci.numbered_targets (
    workspace_id uuid NOT NULL,
    scope_id uuid NOT NULL,
    number bigint NOT NULL,
    target_kind text NOT NULL,
    target_id uuid NOT NULL,
    CONSTRAINT numbered_targets_number_check CHECK ((number > 0)),
    CONSTRAINT numbered_targets_target_kind_check CHECK ((target_kind = ANY (ARRAY['item'::text, 'document'::text, 'task'::text])))
);


ALTER TABLE fvoci.numbered_targets OWNER TO fvoci_migrator;

-- 조직 멤버십(D-111·D-112). 조직 관리자는 메타데이터만 다루고 내용은 "관리자로 참여"한 공간에서만 본다.
CREATE TABLE fvoci.organization_members (
    organization_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT organization_members_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'member'::text])))
);

-- 조직(D-111). 격리는 같은 DB의 RLS 조직 경계로 한다. idp_issuer는 조직별 IdP 연결 자리이며 단일 IdP(D-45)를 유지한다.
-- 한도 칸은 조직 단계 설정(D-124)이고 NULL은 서버 기본값·워크스페이스 설정에 맡긴다는 뜻이다.
CREATE TABLE fvoci.organizations (
    id uuid NOT NULL,
    name text NOT NULL,
    created_by uuid NOT NULL,
    request_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    idp_issuer text,
    public_links_enabled boolean DEFAULT true NOT NULL,
    max_file_bytes bigint,
    max_storage_bytes bigint,
    max_concurrent_uploads integer,
    notification_rate_per_hour integer,
    activity_retention_days integer,
    audit_retention_days integer,
    CONSTRAINT organizations_activity_retention_days_check CHECK (((activity_retention_days IS NULL) OR ((activity_retention_days >= 1) AND (activity_retention_days <= 3650)))),
    CONSTRAINT organizations_audit_retention_days_check CHECK (((audit_retention_days IS NULL) OR ((audit_retention_days >= 1) AND (audit_retention_days <= 3650)))),
    CONSTRAINT organizations_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT organizations_max_concurrent_uploads_check CHECK (((max_concurrent_uploads IS NULL) OR (max_concurrent_uploads >= 1))),
    CONSTRAINT organizations_max_file_bytes_check CHECK (((max_file_bytes IS NULL) OR (max_file_bytes >= 1))),
    CONSTRAINT organizations_max_storage_bytes_check CHECK (((max_storage_bytes IS NULL) OR (max_storage_bytes >= 0))),
    CONSTRAINT organizations_name_check CHECK ((length(btrim(name)) > 0)),
    CONSTRAINT organizations_notification_rate_per_hour_check CHECK (((notification_rate_per_hour IS NULL) OR (notification_rate_per_hour >= 1)))
);

CREATE TABLE fvoci.ownership_offers (
    workspace_id uuid NOT NULL,
    from_user uuid NOT NULL,
    to_user uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.organization_members OWNER TO fvoci_migrator;

ALTER TABLE fvoci.organizations OWNER TO fvoci_migrator;

ALTER TABLE fvoci.ownership_offers OWNER TO fvoci_migrator;

CREATE TABLE fvoci.reference_counters (
    workspace_id uuid NOT NULL,
    scope text NOT NULL,
    next_number bigint NOT NULL,
    CONSTRAINT reference_counters_next_number_check CHECK ((next_number > 0))
);


ALTER TABLE fvoci.reference_counters OWNER TO fvoci_migrator;



CREATE TABLE fvoci.trash_batches (
    id uuid NOT NULL,
    workspace_id uuid,
    owner_user_id uuid,
    root_kind text NOT NULL,
    root_id uuid NOT NULL,
    title text NOT NULL,
    member_count integer NOT NULL,
    deleted_by uuid NOT NULL,
    deleted_at timestamp with time zone DEFAULT now() NOT NULL,
    restored_at timestamp with time zone,
    purged_at timestamp with time zone,
    CONSTRAINT trash_batches_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT trash_batches_place CHECK (((workspace_id IS NULL) <> (owner_user_id IS NULL))),
    CONSTRAINT trash_batches_root_kind_check CHECK ((root_kind = ANY (ARRAY['item'::text, 'document'::text, 'task'::text, 'event'::text, 'file'::text, 'module'::text])))
);


ALTER TABLE fvoci.trash_batches OWNER TO fvoci_migrator;

-- 표시 설정(D-50·D-81·D-109). theme는 "고르지 않음"이 없고 기본이 시스템 설정 따르기다.
CREATE TABLE fvoci.user_settings (
    user_id uuid NOT NULL,
    time_zone text DEFAULT ''::text NOT NULL,
    locale text DEFAULT ''::text NOT NULL,
    week_start text DEFAULT ''::text NOT NULL,
    hour_cycle text DEFAULT ''::text NOT NULL,
    theme text DEFAULT 'system'::text NOT NULL,
    last_organization_id uuid,
    CONSTRAINT user_settings_hour_cycle_check CHECK ((hour_cycle = ANY (ARRAY[''::text, 'h12'::text, 'h23'::text]))),
    CONSTRAINT user_settings_locale_check CHECK ((locale = ANY (ARRAY[''::text, 'ko'::text, 'en'::text]))),
    CONSTRAINT user_settings_theme_check CHECK ((theme = ANY (ARRAY['system'::text, 'light'::text, 'dark'::text]))),
    CONSTRAINT user_settings_time_zone_check CHECK ((length(time_zone) <= 64)),
    CONSTRAINT user_settings_week_start_check CHECK ((week_start = ANY (ARRAY[''::text, 'mon'::text, 'sun'::text])))
);


ALTER TABLE fvoci.user_settings OWNER TO fvoci_migrator;

-- 워크스페이스 역할(D-20·D-112): 관리자·멤버·읽기 전용. 읽기 전용은 댓글을 포함해 쓰지 않는다. 소유자는 workspaces.owner_id다.
CREATE TABLE fvoci.workspace_members (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text NOT NULL,
    role_source text DEFAULT 'admin_assigned' NOT NULL,
    previous_role text,
    previous_role_source text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_members_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'member'::text, 'readonly'::text]))),
    CONSTRAINT workspace_members_role_source_check CHECK (role_source IN ('invitation_accepted', 'workspace_created', 'admin_assigned', 'admin_join')),
    CONSTRAINT workspace_members_previous_role_check CHECK (
      (previous_role IS NULL AND previous_role_source IS NULL)
      OR (role_source = 'admin_join' AND previous_role IS NOT NULL AND previous_role_source IS NOT NULL
          AND previous_role IN ('admin', 'member', 'readonly')
          AND previous_role_source IN ('invitation_accepted', 'workspace_created', 'admin_assigned')))
);


ALTER TABLE fvoci.workspace_members OWNER TO fvoci_migrator;

-- 모든 워크스페이스는 조직 하나에 속한다(D-111). 한도 칸은 워크스페이스 단계 설정(D-124)이고 NULL은 조직·서버 값을 쓴다는 뜻이다.
CREATE TABLE fvoci.workspaces (
    id uuid NOT NULL,
    organization_id uuid NOT NULL,
    name text NOT NULL,
    created_by uuid NOT NULL,
    request_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    owner_id uuid NOT NULL,
    trash_retention_days integer DEFAULT 30 NOT NULL,
    deleted_at timestamp with time zone,
    deleted_by uuid,
    version_retention_days integer,
    realtime_enabled boolean DEFAULT false NOT NULL,
    max_file_bytes bigint,
    max_storage_bytes bigint,
    max_concurrent_uploads integer,
    notification_rate_per_hour integer,
    activity_retention_days integer,
    audit_retention_days integer,
    CONSTRAINT workspaces_activity_retention_days_check CHECK (((activity_retention_days IS NULL) OR ((activity_retention_days >= 1) AND (activity_retention_days <= 3650)))),
    CONSTRAINT workspaces_audit_retention_days_check CHECK (((audit_retention_days IS NULL) OR ((audit_retention_days >= 1) AND (audit_retention_days <= 3650)))),
    CONSTRAINT workspaces_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT workspaces_max_concurrent_uploads_check CHECK (((max_concurrent_uploads IS NULL) OR (max_concurrent_uploads >= 1))),
    CONSTRAINT workspaces_max_file_bytes_check CHECK (((max_file_bytes IS NULL) OR (max_file_bytes >= 1))),
    CONSTRAINT workspaces_max_storage_bytes_check CHECK (((max_storage_bytes IS NULL) OR (max_storage_bytes >= 0))),
    CONSTRAINT workspaces_name_check CHECK ((length(btrim(name)) > 0)),
    CONSTRAINT workspaces_notification_rate_per_hour_check CHECK (((notification_rate_per_hour IS NULL) OR (notification_rate_per_hour >= 1))),
    CONSTRAINT workspaces_trash_retention_days_check CHECK (((trash_retention_days >= 1) AND (trash_retention_days <= 3650))),
    CONSTRAINT workspaces_version_retention_days_check CHECK (((version_retention_days IS NULL) OR ((version_retention_days >= 1) AND (version_retention_days <= 3650))))
);


ALTER TABLE fvoci.workspaces OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_accept_ownership(p_workspace uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_offer ownership_offers%ROWTYPE;
  BEGIN
    PERFORM app_lock_live_workspace(p_workspace);
    SELECT * INTO v_offer FROM ownership_offers WHERE workspace_id = p_workspace FOR UPDATE;
    IF v_user IS NULL OR v_offer.workspace_id IS NULL OR v_offer.to_user <> v_user
       OR app_access('workspace', p_workspace) = 'none' THEN
      RAISE EXCEPTION 'no offer for you' USING ERRCODE = 'P0002';
    END IF;
    UPDATE workspaces SET owner_id = v_user WHERE id = p_workspace;
    UPDATE workspace_members SET role = 'admin', role_source = 'admin_assigned', previous_role = NULL, previous_role_source = NULL WHERE workspace_id = p_workspace AND user_id = v_user;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'accept_ownership', 'member', v_user,
                      jsonb_build_object('from', v_offer.from_user));
  END
  $$;


ALTER FUNCTION fvoci.app_accept_ownership(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 접근 등급(D-113). 조직 경계(D-111)·역할(D-112)·항목 멤버·개별 공유(D-114)·보관·휴지통을 이 한 묶음에서만 계산한다.
-- 바깥에 보이는 것은 app_access(kind, id)뿐이고 none·read·write·owner 넷만 돌려준다. RLS·서버·웹이 모두 이 값을 쓴다.
-- 보관된 항목(또는 모듈) 아래의 문서·업무·일정·첨부·기록·댓글은 read로 내려가고 항목 자체의 등급은 유지한다(보관 해제는
-- 항목 소유 권한). 휴지통에 든 것, 지운 워크스페이스 안의 것, 조직 밖 사람에게는 none이다. 서버 관리자·조직 관리자라는
-- 사실은 내용 등급을 올리지 않는다(D-112). 등급 위의 추가 조건은 D-115(allow_copy·템플릿)·D-106(본인 댓글 회수)·
-- D-20(읽기 전용의 쓰기 금지)뿐이다.

CREATE FUNCTION fvoci.app_access(p_kind text, p_id uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_access_as(p_kind, p_id, app_user_id()) $$;


ALTER FUNCTION fvoci.app_access(p_kind text, p_id uuid) OWNER TO fvoci_migrator;

-- 사용자를 명시하는 내부 판. 알림·담당자·공유 대상처럼 "그 사람이 볼 수 있는지"를 볼 때만 쓰고 앱에는 주지 않는다.
CREATE FUNCTION fvoci.app_access_as(p_kind text, p_id uuid, p_user uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_user IS NULL OR p_id IS NULL THEN
      RETURN 'none';
    END IF;
    CASE p_kind
      WHEN 'organization' THEN RETURN app_grade_organization(p_id, p_user);
      WHEN 'workspace' THEN RETURN app_grade_workspace(p_id, p_user);
      WHEN 'module' THEN RETURN app_grade_module(p_id, p_user);
      WHEN 'item' THEN RETURN app_grade_item(p_id, p_user, false);
      WHEN 'document' THEN RETURN app_grade_document(p_id, p_user);
      WHEN 'task' THEN
        RETURN app_grade_under_item((SELECT item_id FROM tasks WHERE id = p_id AND deleted_at IS NULL), p_user);
      WHEN 'event' THEN
        RETURN app_grade_under_item((SELECT item_id FROM events WHERE id = p_id AND deleted_at IS NULL), p_user);
      WHEN 'record' THEN RETURN app_grade_under_item((SELECT item_id FROM item_records WHERE id = p_id), p_user);
      WHEN 'episode' THEN RETURN app_grade_under_item((SELECT item_id FROM course_episodes WHERE id = p_id), p_user);
      WHEN 'study_unit' THEN RETURN app_grade_under_item((SELECT item_id FROM study_units WHERE id = p_id), p_user);
      WHEN 'file' THEN RETURN app_grade_file(p_id, p_user);
      WHEN 'comment' THEN RETURN app_grade_comment(p_id, p_user);
      ELSE RAISE EXCEPTION 'unknown access kind %', p_kind USING ERRCODE = '22023';
    END CASE;
  END
  $$;


ALTER FUNCTION fvoci.app_access_as(p_kind text, p_id uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 워크스페이스 역할. 워크스페이스가 살아 있고 그 조직의 멤버이면서 워크스페이스 멤버일 때만 역할이 있다(조직 경계).
CREATE FUNCTION fvoci.app_workspace_role(p_workspace uuid, p_user uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT m.role
    FROM workspace_members m
    JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
    JOIN organization_members om ON om.organization_id = w.organization_id AND om.user_id = m.user_id
    WHERE m.workspace_id = p_workspace AND m.user_id = p_user
  $$;


ALTER FUNCTION fvoci.app_workspace_role(p_workspace uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_grade_organization(p_organization uuid, p_user uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT CASE m.role WHEN 'admin' THEN 'owner' ELSE 'read' END
                     FROM organization_members m WHERE m.organization_id = p_organization AND m.user_id = p_user), 'none')
  $$;


ALTER FUNCTION fvoci.app_grade_organization(p_organization uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_grade_workspace(p_workspace uuid, p_user uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE app_workspace_role(p_workspace, p_user)
      WHEN 'admin' THEN 'owner' WHEN 'member' THEN 'write' WHEN 'readonly' THEN 'read' ELSE 'none' END
  $$;


ALTER FUNCTION fvoci.app_grade_workspace(p_workspace uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_grade_module(p_module uuid, p_user uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT app_grade_workspace(m.workspace_id, p_user) FROM modules m
                     WHERE m.id = p_module AND m.deleted_at IS NULL), 'none')
  $$;


ALTER FUNCTION fvoci.app_grade_module(p_module uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 항목 등급. 관리자는 owner, 만든 사람은 멤버면 owner·읽기 전용이면 read, 비공개 항목은 항목 멤버(직접·그룹)만,
-- 그 밖에는 멤버 write·읽기 전용 read. p_ignore_trash는 담당자 정리처럼 "휴지통이라서 못 보는 것"과 "권한을 잃은 것"을
-- 구분할 때만 true다.
CREATE FUNCTION fvoci.app_grade_item(p_item uuid, p_user uuid, p_ignore_trash boolean) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item items%ROWTYPE;
    v_role text;
  BEGIN
    IF p_user IS NULL THEN
      RETURN 'none';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    IF v_item.id IS NULL THEN
      RETURN 'none';
    END IF;
    IF NOT p_ignore_trash AND (v_item.deleted_at IS NOT NULL
       OR EXISTS (SELECT 1 FROM modules WHERE id = v_item.module_id AND deleted_at IS NOT NULL)) THEN
      RETURN 'none';
    END IF;
    v_role := app_workspace_role(v_item.workspace_id, p_user);
    IF v_role IS NULL THEN
      RETURN 'none';
    END IF;
    IF v_role = 'admin' THEN
      RETURN 'owner';
    END IF;
    -- 만든 사람도 읽기 전용으로 내려가면 읽기만 한다.
    IF v_item.created_by = p_user THEN
      RETURN CASE v_role WHEN 'member' THEN 'owner' ELSE 'read' END;
    END IF;
    IF v_item.visibility <> 'workspace' AND NOT app_is_item_member(p_item, p_user) THEN
      RETURN 'none';
    END IF;
    RETURN CASE v_role WHEN 'member' THEN 'write' ELSE 'read' END;
  END
  $$;


ALTER FUNCTION fvoci.app_grade_item(p_item uuid, p_user uuid, p_ignore_trash boolean) OWNER TO fvoci_migrator;

-- 항목 아래의 것(업무·일정·기록·회차·단위·항목 첨부). 보관되면 read로 내려간다.
CREATE FUNCTION fvoci.app_grade_under_item(p_item uuid, p_user uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text;
  BEGIN
    IF p_item IS NULL THEN
      RETURN 'none';
    END IF;
    v_access := app_grade_item(p_item, p_user, false);
    IF v_access IN ('owner', 'write') AND app_item_archived(p_item) THEN
      RETURN 'read';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_grade_under_item(p_item uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 문서 등급(D-08·D-20·D-40·D-49·D-114). 개인 노트는 소유자만 owner이고 공간 관리자에게도 비공개다. 공간 문서는 역할과
-- 공개 범위, 항목 등급 중 낮은 쪽이다. 개별 공유는 공유 행의 조직에 받는 사람(개인 노트는 소유자도)이 아직 속해 있을 때만
-- 살고, 역할이 개별 공유의 상한이다. 보관된 항목 아래의 문서는 read로 내려간다. 자리(워크스페이스·항목·소유자)를 인자로
-- 받는 판은 이동 전후 비교(app_move_preview, D-114)가 쓴다.
CREATE FUNCTION fvoci.app_grade_document_at(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid, p_user uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc    documents%ROWTYPE;
    v_role   text;
    v_base   text;
    v_item   text;
    v_share  text;
    v_access text;
  BEGIN
    IF p_user IS NULL THEN
      RETURN 'none';
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF v_doc.id IS NULL OR v_doc.deleted_at IS NOT NULL THEN
      RETURN 'none';
    END IF;
    -- 워크스페이스를 지웠으면 그 안의 문서는 공유로도 볼 수 없다.
    IF p_workspace IS NOT NULL AND EXISTS (SELECT 1 FROM workspaces WHERE id = p_workspace AND deleted_at IS NOT NULL) THEN
      RETURN 'none';
    END IF;
    -- 항목이나 그 모듈이 휴지통에 있으면 그 안의 문서는 공유로도 볼 수 없다.
    IF p_item IS NOT NULL AND EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
      WHERE i.id = p_item AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL)
    ) THEN
      RETURN 'none';
    END IF;

    IF p_owner IS NOT NULL THEN
      IF p_owner = p_user THEN
        RETURN 'owner';
      END IF;
    ELSE
      v_role := app_workspace_role(p_workspace, p_user);
      IF v_role = 'admin' THEN
        v_base := 'owner';
      ELSIF v_role IS NOT NULL AND (v_doc.visibility = 'workspace' OR v_doc.created_by = p_user) THEN
        v_base := CASE v_role WHEN 'member' THEN 'write' ELSE 'read' END;
      END IF;
      IF p_item IS NOT NULL AND v_base IS NOT NULL THEN
        v_item := app_grade_item(p_item, p_user, false);
        -- 낮은 쪽: owner > write > read > none.
        v_base := CASE
          WHEN v_item = 'none' THEN NULL
          WHEN v_item = 'read' THEN 'read'
          WHEN v_item = 'write' AND v_base = 'owner' THEN 'write'
          ELSE v_base
        END;
      END IF;
    END IF;

    -- 공유는 기록된 조직 안에서만 유효하다(D-114): 받는 사람이 그 조직의 멤버이고, 공간 문서면 그 조직이 지금 문서가 속한
    -- 조직이어야 하며(다른 조직의 공간으로 옮긴 뒤에는 효력 없음), 개인 노트면 소유자도 그 조직의 멤버여야 한다.
    SELECT s.permission INTO v_share
    FROM document_shares s
    JOIN organization_members om ON om.organization_id = s.organization_id AND om.user_id = p_user
    WHERE s.document_id = p_doc AND s.grantee_user_id = p_user
      AND (CASE WHEN p_owner IS NULL
                THEN s.organization_id = (SELECT w.organization_id FROM workspaces w WHERE w.id = p_workspace)
                ELSE EXISTS (SELECT 1 FROM organization_members o2
                             WHERE o2.organization_id = s.organization_id AND o2.user_id = p_owner)
           END);

    v_access := CASE
      WHEN v_base = 'owner' THEN 'owner'
      WHEN v_base = 'write' OR v_share = 'write' THEN 'write'
      ELSE coalesce(v_base, v_share, 'none')
    END;
    -- D-20: 읽기 전용·공간 밖 조직원은 기존 수정 공유도 읽기로 계산한다. 개인 노트에는 공간 역할 상한이 없다.
    IF p_owner IS NULL AND (v_role IS NULL OR v_role = 'readonly') AND v_access = 'write' THEN
      v_access := 'read';
    END IF;
    IF v_access IN ('owner', 'write') AND p_item IS NOT NULL AND app_item_archived(p_item) THEN
      RETURN 'read';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_grade_document_at(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_grade_document(p_doc uuid, p_user uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT app_grade_document_at(d.id, d.workspace_id, d.item_id, d.owner_user_id, p_user)
                     FROM documents d WHERE d.id = p_doc), 'none')
  $$;


ALTER FUNCTION fvoci.app_grade_document(p_doc uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 대상이 속한 조직(개인 노트는 NULL). 참조·공유·이동의 조직 경계(D-114)에 쓴다.
CREATE FUNCTION fvoci.app_target_organization(p_kind text, p_id uuid) RETURNS uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT w.organization_id FROM workspaces w
    WHERE w.id = CASE p_kind
      WHEN 'item' THEN (SELECT i.workspace_id FROM items i WHERE i.id = p_id)
      WHEN 'document' THEN (SELECT d.workspace_id FROM documents d WHERE d.id = p_id)
      WHEN 'task' THEN (SELECT t.workspace_id FROM tasks t WHERE t.id = p_id)
      WHEN 'file' THEN (SELECT f.workspace_id FROM files f WHERE f.id = p_id)
      WHEN 'workspace' THEN p_id
    END
  $$;


ALTER FUNCTION fvoci.app_target_organization(p_kind text, p_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_document_manager(p_doc uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF NOT app_document_can_manage(p_doc) THEN
      PERFORM app_require_document(p_doc, 'owner');
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_require_document_manager(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_grade_file(p_file uuid, p_user uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_file files%ROWTYPE;
  BEGIN
    SELECT * INTO v_file FROM files WHERE id = p_file;
    IF v_file.id IS NULL OR v_file.deleted_at IS NOT NULL THEN
      RETURN 'none';
    END IF;
    IF v_file.document_id IS NOT NULL THEN
      RETURN app_grade_document(v_file.document_id, p_user);
    END IF;
    RETURN app_grade_under_item(v_file.item_id, p_user);
  END
  $$;


ALTER FUNCTION fvoci.app_grade_file(p_file uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_grade_comment(p_comment uuid, p_user uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT CASE
        WHEN c.document_id IS NOT NULL THEN app_grade_document(c.document_id, p_user)
        ELSE app_grade_under_item((SELECT t.item_id FROM tasks t WHERE t.id = c.task_id AND t.deleted_at IS NULL), p_user)
      END FROM comments c WHERE c.id = p_comment), 'none')
  $$;


ALTER FUNCTION fvoci.app_grade_comment(p_comment uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 조직 멤버가 아니면 조직의 존재를 알리지 않는다(P0002). 멤버이지만 관리자가 아니면 42501.
CREATE FUNCTION fvoci.app_require_organization_admin(p_organization uuid) RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_grade text := app_access('organization', p_organization);
  BEGIN
    IF v_user IS NULL OR v_grade = 'none' THEN
      RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_grade <> 'owner' THEN
      RAISE EXCEPTION 'organization admin required' USING ERRCODE = '42501';
    END IF;
    RETURN v_user;
  END
  $$;


ALTER FUNCTION fvoci.app_require_organization_admin(p_organization uuid) OWNER TO fvoci_migrator;

-- 같은 조직에 속한 사람인지. 사용자 행 열람·공유 대상·담당자 후보의 경계다(D-114).
CREATE FUNCTION fvoci.app_shares_organization(other uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT EXISTS (
      SELECT 1 FROM organization_members mine JOIN organization_members theirs
        ON theirs.organization_id = mine.organization_id
      WHERE mine.user_id = app_user_id() AND theirs.user_id = other)
  $$;


ALTER FUNCTION fvoci.app_shares_organization(other uuid) OWNER TO fvoci_migrator;

-- 조직(D-111·D-112·D-51). 서버 관리자가 조직을 만들고 첫 조직 관리자를 정한다. 조직 관리자는 멤버·설정·워크스페이스
-- 목록 같은 메타데이터만 다루고, 내용은 "관리자로 참여"(app_join_workspace_as_admin)한 공간에서만 본다.
-- 새 id: 조직 1, 서버 로그 1, 조직 로그 1.
CREATE FUNCTION fvoci.app_create_organization(p_request_id uuid, p_name text, p_first_admin uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_require_server_admin();
    v_id   uuid;
  BEGIN
    SELECT id INTO v_id FROM organizations WHERE created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF length(btrim(coalesce(p_name, ''))) = 0 THEN
      RAISE EXCEPTION 'the organization needs a name' USING ERRCODE = '22023';
    END IF;
    IF p_first_admin IS NULL OR NOT EXISTS (SELECT 1 FROM users WHERE id = p_first_admin) THEN
      RAISE EXCEPTION 'the first organization admin must be a user' USING ERRCODE = '22023';
    END IF;
    INSERT INTO organizations (id, name, created_by, request_id)
    VALUES (app_id(p_ids, 1), btrim(p_name), v_user, p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO organization_members (organization_id, user_id, role) VALUES (v_id, p_first_admin, 'admin');
    -- 서버 로그(서버 관리자가 본다)와 조직 로그(조직 관리자가 본다)에 각각 남긴다.
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), NULL, NULL, v_user, 'create', 'organization', v_id,
            jsonb_build_object('name', btrim(p_name), 'first_admin', p_first_admin)),
           (app_id(p_ids, 3), NULL, v_id, v_user, 'create', 'organization', v_id,
            jsonb_build_object('name', btrim(p_name), 'first_admin', p_first_admin));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_organization(p_request_id uuid, p_name text, p_first_admin uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_my_organizations() RETURNS TABLE(id uuid, name text, role text, public_links_enabled boolean, created_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT o.id, o.name, m.role, o.public_links_enabled, o.created_at
    FROM organizations o JOIN organization_members m ON m.organization_id = o.id
    WHERE m.user_id = app_user_id()
    ORDER BY o.created_at, o.id
  $$;


ALTER FUNCTION fvoci.app_my_organizations() OWNER TO fvoci_migrator;

-- 서버 관리자의 조직 목록(메타데이터만).
CREATE FUNCTION fvoci.app_server_organizations() RETURNS TABLE(id uuid, name text, created_at timestamp with time zone, member_count integer, workspace_count integer)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_require_server_admin();
    RETURN QUERY
      SELECT o.id, o.name, o.created_at,
             (SELECT count(*)::integer FROM organization_members m WHERE m.organization_id = o.id),
             (SELECT count(*)::integer FROM workspaces w WHERE w.organization_id = o.id AND w.deleted_at IS NULL)
      FROM organizations o
      ORDER BY o.created_at, o.id;
  END
  $$;


ALTER FUNCTION fvoci.app_server_organizations() OWNER TO fvoci_migrator;

-- 조직 멤버 목록. 멤버는 이름만, 조직 관리자는 이메일도 본다(공유·담당 대상 찾기, D-114).
CREATE FUNCTION fvoci.app_organization_members(p_organization uuid) RETURNS TABLE(user_id uuid, display_name text, email text, role text, created_at timestamp with time zone)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_grade text := app_access('organization', p_organization);
  BEGIN
    IF v_grade = 'none' THEN
      RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      SELECT m.user_id, u.display_name, CASE WHEN v_grade = 'owner' THEN u.email END, m.role, m.created_at
      FROM organization_members m JOIN users u ON u.id = m.user_id
      WHERE m.organization_id = p_organization
      ORDER BY u.display_name, m.user_id;
  END
  $$;


ALTER FUNCTION fvoci.app_organization_members(p_organization uuid) OWNER TO fvoci_migrator;

-- 조직 관리자의 워크스페이스 목록(메타데이터만). 멤버가 아닌 공간의 내용은 보이지 않는다.
CREATE FUNCTION fvoci.app_organization_workspaces(p_organization uuid) RETURNS TABLE(id uuid, name text, owner_id uuid, owner_name text, member_count integer, created_at timestamp with time zone, deleted_at timestamp with time zone, my_role text)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_require_organization_admin(p_organization);
    RETURN QUERY
      SELECT w.id, w.name, w.owner_id, u.display_name,
             (SELECT count(*)::integer FROM workspace_members m WHERE m.workspace_id = w.id),
             w.created_at, w.deleted_at, app_member_role(w.id)
      FROM workspaces w JOIN users u ON u.id = w.owner_id
      WHERE w.organization_id = p_organization
      ORDER BY w.created_at, w.id;
  END
  $$;


ALTER FUNCTION fvoci.app_organization_workspaces(p_organization uuid) OWNER TO fvoci_migrator;

-- 조직 멤버 추가·역할 변경(조직 관리자). 마지막 관리자는 내릴 수 없다. 새 id: 조직 로그 1.
CREATE FUNCTION fvoci.app_set_organization_member(p_organization uuid, p_user uuid, p_role text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_admin uuid := app_require_organization_admin(p_organization);
    v_old   text;
    v_row   record;
    v_at    integer := 0;
    v_need  integer;
  BEGIN
    -- 조직 멤버십 변경은 조직 행으로 줄 세운다(탈퇴·초대 수락과 같은 순서).
    PERFORM 1 FROM organizations WHERE id = p_organization FOR NO KEY UPDATE;
    PERFORM app_require_organization_admin(p_organization);
    IF p_role NOT IN ('admin', 'member') THEN
      RAISE EXCEPTION 'invalid role' USING ERRCODE = '22023';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM users WHERE id = p_user) THEN
      RAISE EXCEPTION 'user not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT role INTO v_old FROM organization_members WHERE organization_id = p_organization AND user_id = p_user FOR UPDATE;
    IF v_old = 'admin' AND p_role <> 'admin'
       AND (SELECT count(*) FROM organization_members WHERE organization_id = p_organization AND role = 'admin') = 1 THEN
      RAISE EXCEPTION 'the organization needs at least one admin' USING ERRCODE = '55000';
    END IF;
    INSERT INTO organization_members (organization_id, user_id, role) VALUES (p_organization, p_user, p_role)
    ON CONFLICT (organization_id, user_id) DO UPDATE SET role = EXCLUDED.role;
    IF v_old = 'admin' AND p_role = 'member' THEN
      -- PR4a: 조직 → 조직 멤버 → 공간 id 순 → 문서 id 순. 휴지통 공간의 자기 참여도 함께 회수한다.
      PERFORM 1 FROM workspaces WHERE organization_id = p_organization ORDER BY id FOR NO KEY UPDATE;
      PERFORM 1 FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
        WHERE w.organization_id = p_organization AND m.user_id = p_user AND m.role_source = 'admin_join'
        ORDER BY m.workspace_id FOR UPDATE OF m;
      PERFORM 1 FROM documents d
        WHERE EXISTS (SELECT 1 FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
                      WHERE m.workspace_id = d.workspace_id AND w.organization_id = p_organization
                        AND m.user_id = p_user AND m.role_source = 'admin_join' AND m.previous_role IS NULL)
        ORDER BY d.id FOR UPDATE;
      FOR v_row IN
        SELECT m.workspace_id, m.previous_role, m.previous_role_source
        FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
        WHERE w.organization_id = p_organization AND m.user_id = p_user AND m.role_source = 'admin_join'
        ORDER BY m.workspace_id
      LOOP
        v_need := 1 + 2 * app_unseen_assignees_bound(NULL, v_row.workspace_id, p_user)
          + (SELECT count(*)::integer FROM document_shares s JOIN documents d ON d.id = s.document_id
             WHERE d.workspace_id = v_row.workspace_id AND s.grantee_user_id = p_user);
        IF v_row.previous_role IS NULL THEN
          PERFORM app_remove_member_internal(v_row.workspace_id, p_user, 'organization_admin_demoted',
                                            coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
        ELSE
          UPDATE workspace_members
            SET role = previous_role, role_source = previous_role_source, previous_role = NULL, previous_role_source = NULL
            WHERE workspace_id = v_row.workspace_id AND user_id = p_user;
          PERFORM app_audit(app_id(p_ids, v_at + 1), v_row.workspace_id, 'change_role', 'member', p_user,
                            jsonb_build_object('from', 'admin', 'to', v_row.previous_role, 'reason', 'organization_admin_demoted'));
          PERFORM app_drop_unseen_assignees(NULL, v_row.workspace_id, p_user, 'organization_admin_demoted',
                                           coalesce(p_ids[v_at + 2:v_at + v_need], '{}'::uuid[]));
        END IF;
        v_at := v_at + v_need;
      END LOOP;
    END IF;
    IF v_old IS DISTINCT FROM p_role THEN
      INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, v_at + 1), NULL, p_organization, v_admin,
              CASE WHEN v_old IS NULL THEN 'add_member' ELSE 'change_role' END, 'organization_member', p_user,
              jsonb_build_object('from', v_old, 'to', p_role));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_set_organization_member(p_organization uuid, p_user uuid, p_role text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-88·SR1-14: id 산정부터 조직 잠금을 유지해 관리자 참여와 강등을 직렬화한다.
CREATE FUNCTION fvoci.app_set_organization_member_ids(p_organization uuid, p_user uuid, p_role text) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_require_organization_admin(p_organization);
    PERFORM 1 FROM organizations WHERE id = p_organization FOR NO KEY UPDATE;
    PERFORM app_require_organization_admin(p_organization);
    IF p_role <> 'member' OR NOT EXISTS (
      SELECT 1 FROM organization_members WHERE organization_id = p_organization AND user_id = p_user AND role = 'admin'
    ) THEN
      RETURN 1;
    END IF;
    RETURN 1 + coalesce((SELECT sum(1 + 2 * app_unseen_assignees_bound(NULL, m.workspace_id, p_user)
      + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
         WHERE d.workspace_id = m.workspace_id AND s.grantee_user_id = p_user))
      FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
      WHERE w.organization_id = p_organization AND m.user_id = p_user AND m.role_source = 'admin_join'), 0)::integer;
  END
  $$;

ALTER FUNCTION fvoci.app_set_organization_member_ids(p_organization uuid, p_user uuid, p_role text) OWNER TO fvoci_migrator;

-- 조직에서 빠질 때 풀리는 것의 건수(확인 대화용, D-114).
CREATE FUNCTION fvoci.app_remove_organization_member_preview(p_organization uuid, p_user uuid) RETURNS TABLE(workspaces integer, shares integer, assignments integer)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS DISTINCT FROM p_user THEN
      PERFORM app_require_organization_admin(p_organization);
    ELSIF app_access('organization', p_organization) = 'none' THEN
      RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      SELECT (SELECT count(*)::integer FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
              WHERE w.organization_id = p_organization AND m.user_id = p_user),
             (SELECT count(*)::integer FROM document_shares s JOIN documents d ON d.id = s.document_id
              WHERE s.organization_id = p_organization
                AND (s.grantee_user_id = p_user OR d.owner_user_id = p_user)),
             (SELECT count(*)::integer FROM task_assignees a JOIN tasks t ON t.id = a.task_id
              JOIN workspaces w ON w.id = t.workspace_id
              WHERE w.organization_id = p_organization AND a.user_id = p_user);
  END
  $$;


ALTER FUNCTION fvoci.app_remove_organization_member_preview(p_organization uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 조직 탈퇴·제거에 필요한 새 id 수: 공간마다 app_remove_member_ids + 조직 로그 1.
CREATE FUNCTION fvoci.app_remove_organization_member_ids(p_organization uuid, p_user uuid) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_access('organization', p_organization) = 'none'
       OR (app_user_id() <> p_user AND app_access('organization', p_organization) <> 'owner') THEN
      RETURN 0;
    END IF;
    -- 명령과 같은 순서로 먼저 잠근 뒤 센다. 권한 없는 집계는 잠그지 않고 0(D-88).
    PERFORM 1 FROM organizations WHERE id = p_organization FOR NO KEY UPDATE;
    IF app_user_id() IS NULL OR app_access('organization', p_organization) = 'none'
       OR (app_user_id() <> p_user AND app_access('organization', p_organization) <> 'owner') THEN
      RETURN 0;
    END IF;
    PERFORM 1 FROM organization_members
    WHERE organization_id = p_organization AND user_id = p_user FOR UPDATE;
    IF NOT FOUND THEN
      RETURN 0;
    END IF;
    PERFORM 1 FROM workspaces WHERE organization_id = p_organization ORDER BY id FOR NO KEY UPDATE;
    -- 공유는 조직 멤버·공간 잠금, 새 담당·리더는 공간 멤버 잠금에서 기다린다.
    PERFORM 1 FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
    WHERE w.organization_id = p_organization AND m.user_id = p_user
    ORDER BY m.workspace_id FOR UPDATE OF m;
    RETURN 1 + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
                WHERE s.organization_id = p_organization AND s.grantee_user_id = p_user
                  AND d.workspace_id IS NOT NULL)::integer
      + coalesce((SELECT sum(1 + app_unseen_assignees_bound(NULL, m.workspace_id, p_user) * 2)
                  FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
                  WHERE w.organization_id = p_organization AND m.user_id = p_user), 0)::integer;
  END
  $$;


ALTER FUNCTION fvoci.app_remove_organization_member_ids(p_organization uuid, p_user uuid) OWNER TO fvoci_migrator;

-- 조직에서 빠지기(본인) 또는 내보내기(조직 관리자). 그 조직의 모든 워크스페이스에서 받은 개별 공유·업무 담당과 조직에
-- 귀속된 개인 노트 공유 전부를 철회한다(D-114). 소유한 워크스페이스가 있으면 먼저 인계해야 한다(D-51).
CREATE FUNCTION fvoci.app_remove_organization_member(p_organization uuid, p_user uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_actor  uuid := app_user_id();
    v_self   boolean := v_actor = p_user;
    v_ws     uuid;
    v_need   integer;
    v_at     integer := 0;
    v_shares integer;
    v_email  text;
    v_share  record;
  BEGIN
    IF v_self THEN
      IF app_access('organization', p_organization) = 'none' THEN
        RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
      END IF;
    ELSE
      PERFORM app_require_organization_admin(p_organization);
    END IF;
    PERFORM 1 FROM organizations WHERE id = p_organization FOR NO KEY UPDATE;
    -- 공유 생성(멤버 행 공유 잠금 → 문서 행 → 공유 기록)과 같은 순서로 줄 세운다(D-114). 먼저 온 공유는 커밋을 기다렸다가 아래에서 함께 거두고,
    -- 나중 공유는 이 제거가 커밋된 뒤 멤버 행이 없어 거절된다.
    PERFORM 1 FROM organization_members WHERE organization_id = p_organization AND user_id = p_user FOR UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'organization member not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT v_self THEN
      PERFORM app_require_organization_admin(p_organization);
    END IF;
    IF (SELECT role FROM organization_members WHERE organization_id = p_organization AND user_id = p_user) = 'admin'
       AND (SELECT count(*) FROM organization_members WHERE organization_id = p_organization AND role = 'admin') = 1 THEN
      RAISE EXCEPTION 'the organization needs at least one admin' USING ERRCODE = '55000';
    END IF;
    IF EXISTS (SELECT 1 FROM workspaces WHERE organization_id = p_organization AND owner_id = p_user) THEN
      RAISE EXCEPTION 'transfer workspace ownership first' USING ERRCODE = '55000';
    END IF;
    -- 공간 → 문서(#186). 멤버십이 없는 공간의 공유도 거두므로 조직의 공간을 id 순서로 먼저 잠근다.
    -- 조직 행 잠금이 새 공간 만들기·이동을 막고, 받는 사람의 조직 멤버 행 잠금이 새 공유를 막는다(D-114).
    PERFORM 1 FROM workspaces WHERE organization_id = p_organization ORDER BY id FOR NO KEY UPDATE;
    -- 조직 안 공간 문서에서 받은 개별 공유는 공간 멤버십과 무관하게 모두 거둔다(D-114). 문서 행을 잠근 뒤 지운다.
    FOR v_share IN
      SELECT s.id, s.document_id, d.workspace_id, d.item_id FROM document_shares s JOIN documents d ON d.id = s.document_id
      WHERE s.organization_id = p_organization AND s.grantee_user_id = p_user AND d.workspace_id IS NOT NULL
      ORDER BY s.document_id
    LOOP
      PERFORM 1 FROM documents WHERE id = v_share.document_id FOR UPDATE;
      DELETE FROM document_shares WHERE id = v_share.id;
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_share.workspace_id, v_share.item_id, 'document', v_share.document_id,
                           'unshare', jsonb_build_object('user', p_user, 'reason', 'organization_left'));
    END LOOP;
    FOR v_ws IN
      SELECT m.workspace_id FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
      WHERE w.organization_id = p_organization AND m.user_id = p_user
      ORDER BY m.workspace_id
    LOOP
      v_need := 1 + app_unseen_assignees_bound(NULL, v_ws, p_user) * 2;
      PERFORM app_remove_member_internal(v_ws, p_user, 'organization_left',
                                         coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
      v_at := v_at + v_need;
    END LOOP;
    -- 조직에 귀속된 개인 노트 공유: 받은 것과 내 노트를 준 것 모두. 조직 소속 문서의 언급 기록도 거둔다.
    DELETE FROM document_shares s USING documents d
    WHERE s.document_id = d.id AND s.organization_id = p_organization
      AND d.owner_user_id IS NOT NULL AND (s.grantee_user_id = p_user OR d.owner_user_id = p_user);
    GET DIAGNOSTICS v_shares = ROW_COUNT;
    DELETE FROM document_mentions dm USING documents d JOIN workspaces w ON w.id = d.workspace_id
    WHERE dm.document_id = d.id AND w.organization_id = p_organization AND dm.user_id = p_user;
    SELECT lower(email) INTO v_email FROM users WHERE id = p_user AND email_verified;
    UPDATE invitations SET closed_at = now(), close_reason = 'organization_left'
    WHERE organization_id = p_organization AND closed_at IS NULL AND email = v_email;
    DELETE FROM organization_members WHERE organization_id = p_organization AND user_id = p_user;
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, v_at + 1), NULL, p_organization, v_actor,
            CASE WHEN v_self THEN 'leave' ELSE 'remove_member' END, 'organization_member', p_user,
            jsonb_build_object('note_shares_revoked', v_shares));
  END
  $$;


ALTER FUNCTION fvoci.app_remove_organization_member(p_organization uuid, p_user uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 조직 초대(조직 관리자, D-51·D-118). 그 이메일의 검증된 사용자가 있으면 알림함에 invite를 넣는다. 새 id: 초대 1, 로그 1, 알림 1.
CREATE FUNCTION fvoci.app_invite_organization(p_organization uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user  uuid := app_require_organization_admin(p_organization);
    v_email text := lower(btrim(p_email));
    v_id    uuid;
  BEGIN
    PERFORM 1 FROM organizations WHERE id = p_organization FOR NO KEY UPDATE;
    PERFORM app_require_organization_admin(p_organization);
    IF p_role NOT IN ('admin', 'member') THEN
      RAISE EXCEPTION 'invalid role' USING ERRCODE = '22023';
    END IF;
    IF position('@' in v_email) < 2 THEN
      RAISE EXCEPTION 'invalid email' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (
      SELECT 1 FROM organization_members m JOIN users u ON u.id = m.user_id
      WHERE m.organization_id = p_organization AND u.email_verified AND lower(u.email) = v_email
    ) THEN
      RAISE EXCEPTION 'already a member' USING ERRCODE = '55000';
    END IF;
    UPDATE invitations SET role = p_role, invited_by = v_user, expires_at = now() + make_interval(secs => p_ttl_secs)
    WHERE organization_id = p_organization AND workspace_id IS NULL AND email = v_email AND closed_at IS NULL
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      INSERT INTO invitations (id, organization_id, workspace_id, email, role, invited_by, expires_at)
      VALUES (app_id(p_ids, 1), p_organization, NULL, v_email, p_role, v_user, now() + make_interval(secs => p_ttl_secs))
      RETURNING id INTO v_id;
    END IF;
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), NULL, p_organization, v_user, 'invite', 'invitation', v_id,
            jsonb_build_object('email', v_email, 'role', p_role));
    PERFORM app_notify_invitation(app_id(p_ids, 3), v_id);
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_invite_organization(p_organization uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 조직 설정(조직 관리자, D-114·D-124). p_patch의 키만 바꾸고 JSON null은 비움(서버 기본값·워크스페이스 값으로)이다.
-- 공개 링크를 끄면 기존 링크도 응답하지 않는다(app_public_document). 새 id: 조직 로그 1.
CREATE FUNCTION fvoci.app_set_organization_settings(p_organization uuid, p_patch jsonb, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_require_organization_admin(p_organization);
    v_old  organizations%ROWTYPE;
    v_new  organizations%ROWTYPE;
    v_key  text;
  BEGIN
    IF p_patch IS NULL OR jsonb_typeof(p_patch) <> 'object' THEN
      RAISE EXCEPTION 'settings must be an object' USING ERRCODE = '22023';
    END IF;
    FOR v_key IN SELECT jsonb_object_keys(p_patch) LOOP
      IF v_key NOT IN ('name', 'public_links_enabled', 'max_file_bytes', 'max_storage_bytes', 'max_concurrent_uploads',
                       'notification_rate_per_hour', 'activity_retention_days', 'audit_retention_days') THEN
        RAISE EXCEPTION 'unknown setting %', v_key USING ERRCODE = '22023';
      END IF;
    END LOOP;
    SELECT * INTO v_old FROM organizations WHERE id = p_organization FOR NO KEY UPDATE;
    PERFORM app_require_organization_admin(p_organization);
    UPDATE organizations SET
      name = CASE WHEN p_patch ? 'name' THEN btrim(p_patch ->> 'name') ELSE name END,
      public_links_enabled = CASE WHEN p_patch ? 'public_links_enabled'
                                  THEN coalesce((p_patch ->> 'public_links_enabled')::boolean, true)
                                  ELSE public_links_enabled END,
      max_file_bytes = CASE WHEN p_patch ? 'max_file_bytes' THEN (p_patch ->> 'max_file_bytes')::bigint ELSE max_file_bytes END,
      max_storage_bytes = CASE WHEN p_patch ? 'max_storage_bytes' THEN (p_patch ->> 'max_storage_bytes')::bigint
                               ELSE max_storage_bytes END,
      max_concurrent_uploads = CASE WHEN p_patch ? 'max_concurrent_uploads' THEN (p_patch ->> 'max_concurrent_uploads')::integer
                                    ELSE max_concurrent_uploads END,
      notification_rate_per_hour = CASE WHEN p_patch ? 'notification_rate_per_hour'
                                        THEN (p_patch ->> 'notification_rate_per_hour')::integer
                                        ELSE notification_rate_per_hour END,
      activity_retention_days = CASE WHEN p_patch ? 'activity_retention_days'
                                     THEN (p_patch ->> 'activity_retention_days')::integer ELSE activity_retention_days END,
      audit_retention_days = CASE WHEN p_patch ? 'audit_retention_days' THEN (p_patch ->> 'audit_retention_days')::integer
                                  ELSE audit_retention_days END
    WHERE id = p_organization
    RETURNING * INTO v_new;
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), NULL, p_organization, v_user, 'update_settings', 'organization', p_organization,
            jsonb_build_object('from', to_jsonb(v_old) - 'id' - 'created_by' - 'request_id' - 'created_at',
                               'to', to_jsonb(v_new) - 'id' - 'created_by' - 'request_id' - 'created_at'));
  END
  $$;


ALTER FUNCTION fvoci.app_set_organization_settings(p_organization uuid, p_patch jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 워크스페이스 한도(워크스페이스 관리자, D-124). 실효 한도는 조직·서버 값과의 최솟값이다(app_effective_limit).
CREATE FUNCTION fvoci.app_set_workspace_limits(p_workspace uuid, p_patch jsonb, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
    v_old  workspaces%ROWTYPE;
    v_new  workspaces%ROWTYPE;
    v_key  text;
  BEGIN
    IF p_patch IS NULL OR jsonb_typeof(p_patch) <> 'object' THEN
      RAISE EXCEPTION 'settings must be an object' USING ERRCODE = '22023';
    END IF;
    FOR v_key IN SELECT jsonb_object_keys(p_patch) LOOP
      IF v_key NOT IN ('max_file_bytes', 'max_storage_bytes', 'max_concurrent_uploads', 'notification_rate_per_hour',
                       'activity_retention_days', 'audit_retention_days') THEN
        RAISE EXCEPTION 'unknown setting %', v_key USING ERRCODE = '22023';
      END IF;
    END LOOP;
    SELECT * INTO v_old FROM workspaces WHERE id = p_workspace FOR NO KEY UPDATE;
    -- 개정 2026-10-26(⑥, D-124): 보존 기간은 조직 값 이하로만 설정할 수 있다(초과는 22023 → 400). 조직 값이 없으면 제한 없음.
    IF p_patch ? 'activity_retention_days' AND (p_patch ->> 'activity_retention_days') IS NOT NULL
       AND (p_patch ->> 'activity_retention_days')::integer >
           (SELECT o.activity_retention_days FROM organizations o WHERE o.id = v_old.organization_id) THEN
      RAISE EXCEPTION 'activity_retention_days must not exceed the organization value' USING ERRCODE = '22023';
    END IF;
    IF p_patch ? 'audit_retention_days' AND (p_patch ->> 'audit_retention_days') IS NOT NULL
       AND (p_patch ->> 'audit_retention_days')::integer >
           (SELECT o.audit_retention_days FROM organizations o WHERE o.id = v_old.organization_id) THEN
      RAISE EXCEPTION 'audit_retention_days must not exceed the organization value' USING ERRCODE = '22023';
    END IF;
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = v_user FOR SHARE;
    PERFORM app_require_admin(p_workspace);
    UPDATE workspaces SET
      max_file_bytes = CASE WHEN p_patch ? 'max_file_bytes' THEN (p_patch ->> 'max_file_bytes')::bigint ELSE max_file_bytes END,
      max_storage_bytes = CASE WHEN p_patch ? 'max_storage_bytes' THEN (p_patch ->> 'max_storage_bytes')::bigint
                               ELSE max_storage_bytes END,
      max_concurrent_uploads = CASE WHEN p_patch ? 'max_concurrent_uploads' THEN (p_patch ->> 'max_concurrent_uploads')::integer
                                    ELSE max_concurrent_uploads END,
      notification_rate_per_hour = CASE WHEN p_patch ? 'notification_rate_per_hour'
                                        THEN (p_patch ->> 'notification_rate_per_hour')::integer
                                        ELSE notification_rate_per_hour END,
      activity_retention_days = CASE WHEN p_patch ? 'activity_retention_days'
                                     THEN (p_patch ->> 'activity_retention_days')::integer ELSE activity_retention_days END,
      audit_retention_days = CASE WHEN p_patch ? 'audit_retention_days' THEN (p_patch ->> 'audit_retention_days')::integer
                                  ELSE audit_retention_days END
    WHERE id = p_workspace
    RETURNING * INTO v_new;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'update_limits', 'workspace', p_workspace,
                      jsonb_build_object(
                        'from', jsonb_build_object('max_file_bytes', v_old.max_file_bytes, 'max_storage_bytes', v_old.max_storage_bytes,
                                                   'max_concurrent_uploads', v_old.max_concurrent_uploads,
                                                   'notification_rate_per_hour', v_old.notification_rate_per_hour,
                                                   'activity_retention_days', v_old.activity_retention_days,
                                                   'audit_retention_days', v_old.audit_retention_days),
                        'to', jsonb_build_object('max_file_bytes', v_new.max_file_bytes, 'max_storage_bytes', v_new.max_storage_bytes,
                                                 'max_concurrent_uploads', v_new.max_concurrent_uploads,
                                                 'notification_rate_per_hour', v_new.notification_rate_per_hour,
                                                 'activity_retention_days', v_new.activity_retention_days,
                                                 'audit_retention_days', v_new.audit_retention_days)));
  END
  $$;


ALTER FUNCTION fvoci.app_set_workspace_limits(p_workspace uuid, p_patch jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 실효 한도(D-124): 설정된 단계(워크스페이스·조직)와 서버 기본값 가운데 가장 작은 값. 개인 영역(워크스페이스 없음)은
-- 서버 기본값만 쓴다. 보존 기간은 이 함수를 쓰지 않는다(app_retention_days).
CREATE FUNCTION fvoci.app_effective_limit(p_workspace uuid, p_name text, p_default bigint) RETURNS bigint
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT least(p_default,
      (SELECT CASE p_name
         WHEN 'max_file_bytes' THEN w.max_file_bytes
         WHEN 'max_storage_bytes' THEN w.max_storage_bytes
         WHEN 'max_concurrent_uploads' THEN w.max_concurrent_uploads::bigint
         WHEN 'notification_rate_per_hour' THEN w.notification_rate_per_hour::bigint
       END FROM workspaces w WHERE w.id = p_workspace),
      (SELECT CASE p_name
         WHEN 'max_file_bytes' THEN o.max_file_bytes
         WHEN 'max_storage_bytes' THEN o.max_storage_bytes
         WHEN 'max_concurrent_uploads' THEN o.max_concurrent_uploads::bigint
         WHEN 'notification_rate_per_hour' THEN o.notification_rate_per_hour::bigint
       END FROM workspaces w JOIN organizations o ON o.id = w.organization_id WHERE w.id = p_workspace))
  $$;


ALTER FUNCTION fvoci.app_effective_limit(p_workspace uuid, p_name text, p_default bigint) OWNER TO fvoci_migrator;

-- 보존 기간(D-57·D-119·D-124, 개정 2026-10-26 ⑥). 조직 값이 상한이다: 워크스페이스는 조직 값 이하로만 설정할 수 있고
-- (app_set_workspace_limits), 실효값은 두 값의 최솟값이다. 한쪽만 있으면 그 값, 둘 다 없으면 서버 기본값(활동 365일·감사 730일).
CREATE FUNCTION fvoci.app_retention_days(p_workspace uuid, p_organization uuid, p_name text, p_default integer) RETURNS integer
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws  integer;
    v_org integer;
  BEGIN
    IF p_name NOT IN ('activity_retention_days', 'audit_retention_days') THEN
      RAISE EXCEPTION 'unknown retention %', p_name USING ERRCODE = '22023';
    END IF;
    IF p_workspace IS NOT NULL THEN
      SELECT CASE p_name WHEN 'activity_retention_days' THEN w.activity_retention_days ELSE w.audit_retention_days END,
             CASE p_name WHEN 'activity_retention_days' THEN o.activity_retention_days ELSE o.audit_retention_days END
      INTO v_ws, v_org
      FROM workspaces w JOIN organizations o ON o.id = w.organization_id WHERE w.id = p_workspace;
    ELSIF p_organization IS NOT NULL THEN
      SELECT CASE p_name WHEN 'activity_retention_days' THEN o.activity_retention_days ELSE o.audit_retention_days END
      INTO v_org FROM organizations o WHERE o.id = p_organization;
    END IF;
    -- 개정 2026-10-26(⑥, D-124): 조직 값이 상한이다. 둘 다 있으면 실효값은 최솟값, 한쪽만 있으면 그 값, 둘 다 없으면 서버 기본값.
    IF v_ws IS NOT NULL AND v_org IS NOT NULL THEN
      RETURN least(v_ws, v_org);
    END IF;
    RETURN coalesce(v_ws, v_org, p_default);
  END
  $$;


ALTER FUNCTION fvoci.app_retention_days(p_workspace uuid, p_organization uuid, p_name text, p_default integer) OWNER TO fvoci_migrator;

-- 조직 관리자가 자기를 그 워크스페이스의 관리자로 넣는다("관리자로 참여", D-112). 공간 로그와 조직 로그에 남긴다. 새 id 2.
CREATE FUNCTION fvoci.app_join_workspace_as_admin(p_workspace uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_ws   workspaces%ROWTYPE;
    v_old  text;
    v_source text;
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_workspace AND deleted_at IS NULL;
    IF v_ws.id IS NULL OR app_access('organization', v_ws.organization_id) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_organization_admin(v_ws.organization_id);
    -- 조직 행 → 워크스페이스 행 → 멤버 행 순서로 잠그고 관리자인지 다시 본다(강등·탈퇴와 직렬화).
    PERFORM 1 FROM organizations WHERE id = v_ws.organization_id FOR SHARE;
    PERFORM 1 FROM workspaces WHERE id = p_workspace AND deleted_at IS NULL FOR NO KEY UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_organization_admin(v_ws.organization_id);
    SELECT role, role_source INTO v_old, v_source FROM workspace_members WHERE workspace_id = p_workspace AND user_id = v_user FOR UPDATE;
    IF v_old = 'admin' THEN
      RETURN;
    END IF;
    IF v_source = 'admin_join' THEN
      -- 스스로 낮춘 뒤 다시 참여해도 최초 참여 전 상태를 보존한다.
      UPDATE workspace_members SET role = 'admin' WHERE workspace_id = p_workspace AND user_id = v_user;
    ELSE
      INSERT INTO workspace_members (workspace_id, user_id, role, role_source, previous_role, previous_role_source)
      VALUES (p_workspace, v_user, 'admin', 'admin_join', v_old, v_source)
      ON CONFLICT (workspace_id, user_id) DO UPDATE
        SET role = 'admin', role_source = 'admin_join', previous_role = EXCLUDED.previous_role,
            previous_role_source = EXCLUDED.previous_role_source;
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'admin_join', 'member', v_user, jsonb_build_object('from', v_old));
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), NULL, v_ws.organization_id, v_user, 'admin_join', 'workspace', p_workspace,
            jsonb_build_object('from', v_old));
  END
  $$;


ALTER FUNCTION fvoci.app_join_workspace_as_admin(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 운영자가 지정한 초기 관리자(D-51)가 처음 로그인하면 서버 관리자로 시드하고, 첫 조직과 그 안의 첫 워크스페이스를 만들어
-- 그 사람을 조직 관리자·소유자로 둔다(인스턴스에서 한 번만). 새 id 8: 조직 요청 1 + 조직 3 + 워크스페이스 요청 1 + 워크스페이스 3.
CREATE FUNCTION fvoci.app_system_claim_initial_admin(p_user uuid, p_name text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_me   text := current_setting('app.user_id', true);
    v_name text := coalesce(nullif(btrim(p_name), ''), 'fvoci');
    v_org  uuid;
    v_ws   uuid;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN NULL;
    END IF;
    INSERT INTO instance_state (key, value) VALUES ('initial_admin', jsonb_build_object('user', p_user))
    ON CONFLICT (key) DO NOTHING;
    IF NOT FOUND THEN
      RETURN NULL;
    END IF;
    INSERT INTO server_admins (user_id, detail) VALUES (p_user, jsonb_build_object('seed', 'FVOCI_INITIAL_ADMIN_SUBJECT'))
    ON CONFLICT (user_id) DO NOTHING;
    PERFORM set_config('app.user_id', p_user::text, true);
    v_org := app_create_organization(app_id(p_ids, 1), v_name, p_user, p_ids[2:4]);
    v_ws := app_create_workspace(app_id(p_ids, 5), v_org, v_name, p_ids[6:8]);
    PERFORM set_config('app.user_id', coalesce(v_me, ''), true);
    UPDATE instance_state SET value = value || jsonb_build_object('organization', v_org, 'workspace', v_ws)
    WHERE key = 'initial_admin';
    RETURN v_ws;
  END
  $$;


ALTER FUNCTION fvoci.app_system_claim_initial_admin(p_user uuid, p_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 워크스페이스(D-112): 조직 멤버 누구나 만들고 만든 사람이 소유자·관리자다. 새 id 3: 워크스페이스, 공간 로그, 조직 로그.
CREATE FUNCTION fvoci.app_create_workspace(p_request_id uuid, p_organization uuid, p_name text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'no current user' USING ERRCODE = '42501';
    END IF;
    IF app_access('organization', p_organization) = 'none' THEN
      RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
    END IF;
    -- 조직 탈퇴·제거(조직 행 갱신 잠금)와 줄 세운 뒤 아직 조직 멤버인지 다시 본다.
    PERFORM 1 FROM organizations WHERE id = p_organization FOR SHARE;
    IF app_access('organization', p_organization) = 'none' THEN
      RAISE EXCEPTION 'organization not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO workspaces (id, organization_id, name, created_by, owner_id, request_id)
    VALUES (app_id(p_ids, 1), p_organization, btrim(p_name), v_user, v_user, p_request_id)
    ON CONFLICT ON CONSTRAINT workspaces_request DO NOTHING
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      SELECT id INTO v_id FROM workspaces WHERE created_by = v_user AND request_id = p_request_id;
      RETURN v_id;
    END IF;
    INSERT INTO workspace_members (workspace_id, user_id, role, role_source) VALUES (v_id, v_user, 'admin', 'workspace_created');
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), v_id, p_organization, v_user, 'create', 'workspace', v_id,
            jsonb_build_object('name', btrim(p_name))),
           (app_id(p_ids, 3), NULL, p_organization, v_user, 'create', 'workspace', v_id,
            jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_workspace(p_request_id uuid, p_organization uuid, p_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 워크스페이스 초대(워크스페이스 관리자, D-20·D-118). 받는 사람이 조직 밖이어도 보낸다. 수락하면 그 조직의 멤버로 먼저
-- 가입한다(D-112 개정 2026-10-26, app_respond_invitation). 새 id 3: 초대, 공간 로그, 알림.
CREATE FUNCTION fvoci.app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user  uuid := app_require_admin(p_workspace);
    v_email text := lower(btrim(p_email));
    v_org   uuid;
    v_id    uuid;
  BEGIN
    PERFORM app_lock_live_workspace(p_workspace);
    -- 잠금 뒤에 아직 관리자인지 다시 본다(동시 강등·내보내기).
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = v_user FOR SHARE;
    PERFORM app_require_admin(p_workspace);
    SELECT organization_id INTO v_org FROM workspaces WHERE id = p_workspace;
    IF p_role NOT IN ('admin', 'member', 'readonly') THEN
      RAISE EXCEPTION 'invalid role' USING ERRCODE = '22023';
    END IF;
    IF position('@' in v_email) < 2 THEN
      RAISE EXCEPTION 'invalid email' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (
      SELECT 1 FROM workspace_members m JOIN users u ON u.id = m.user_id
      WHERE m.workspace_id = p_workspace AND u.email_verified AND lower(u.email) = v_email
    ) THEN
      RAISE EXCEPTION 'already a member' USING ERRCODE = '55000';
    END IF;
    -- 열린 초대가 있으면 역할·만료를 갱신한다(재초대).
    UPDATE invitations SET role = p_role, invited_by = v_user,
                           expires_at = now() + make_interval(secs => p_ttl_secs)
    WHERE workspace_id = p_workspace AND email = v_email AND closed_at IS NULL
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      INSERT INTO invitations (id, organization_id, workspace_id, email, role, invited_by, expires_at)
      VALUES (app_id(p_ids, 1), v_org, p_workspace, v_email, p_role, v_user, now() + make_interval(secs => p_ttl_secs))
      RETURNING id INTO v_id;
    END IF;
    PERFORM app_audit(app_id(p_ids, 2), p_workspace, 'invite', 'invitation', v_id,
                      jsonb_build_object('email', v_email, 'role', p_role));
    PERFORM app_notify_invitation(app_id(p_ids, 3), v_id);
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_my_invitations() RETURNS TABLE(id uuid, organization_id uuid, organization_name text, workspace_id uuid, workspace_name text, role text, invited_by_name text, expires_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT i.id, i.organization_id, o.name, i.workspace_id, w.name, i.role, inviter.display_name, i.expires_at
    FROM invitations i
    JOIN organizations o ON o.id = i.organization_id
    LEFT JOIN workspaces w ON w.id = i.workspace_id
    JOIN users inviter ON inviter.id = i.invited_by
    JOIN users me ON me.id = app_user_id()
    WHERE i.closed_at IS NULL AND i.expires_at > now()
      AND (i.workspace_id IS NULL OR w.deleted_at IS NULL)
      AND app_invitation_recipient(i.email) = me.id
    ORDER BY i.created_at
  $$;


ALTER FUNCTION fvoci.app_my_invitations() OWNER TO fvoci_migrator;

-- 초대 이메일의 받는 계정(D-51·D-118, #177 SR1-12). 같은 검증 이메일 계정이 여럿이면 가장 먼저 만든 계정 하나다.
-- 알림(app_notify_invitation)과 수락·거절(app_respond_invitation)이 같은 계정을 가리키도록 이 함수 하나로 고른다.
-- 개별 공유의 받는 계정(app_share_document)도 같은 규칙이라 이 함수를 쓴다. 초대 목록(app_my_invitations)·초대 알림
-- 표시(app_notification_visible)·대기 건수와 대기 알림(app_pending_invitations_count·app_notify_pending_invitations)도
-- 이 계정에게만 보인다(CR183-5).
CREATE FUNCTION fvoci.app_invitation_recipient(p_email text) RETURNS uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT u.id FROM users u WHERE u.email_verified AND lower(u.email) = p_email
    ORDER BY u.created_at, u.id LIMIT 1
  $$;


ALTER FUNCTION fvoci.app_invitation_recipient(p_email text) OWNER TO fvoci_migrator;

-- 초대 수락·거절(D-51·D-118). 조직 초대는 조직 멤버가 되고, 워크스페이스 초대는 조직 밖 사람이면 그 조직의 멤버로 먼저
-- 가입한 뒤(감사로그 join) 워크스페이스 역할을 받는다(D-112 개정 2026-10-26).
-- 돌려주는 값은 워크스페이스 초대면 워크스페이스 id, 조직 초대면 조직 id다. 알림함의 invite 알림에 결과를 남긴다.
CREATE FUNCTION fvoci.app_respond_invitation(p_invitation uuid, p_accept boolean, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_inv  invitations%ROWTYPE;
  BEGIN
    SELECT * INTO v_inv FROM invitations WHERE id = p_invitation;
    -- 대상(알림을 받는 계정)이 아니면 초대의 존재를 알리지 않는다.
    IF v_user IS NULL OR v_inv.id IS NULL OR app_invitation_recipient(v_inv.email) IS DISTINCT FROM v_user THEN
      RAISE EXCEPTION 'invitation not found' USING ERRCODE = 'P0002';
    END IF;
    -- 조직 행 → (워크스페이스 행) → 초대 행 순서로 잠근다(조직 탈퇴·워크스페이스 지우기와 같은 순서).
    PERFORM 1 FROM organizations WHERE id = v_inv.organization_id FOR NO KEY UPDATE;
    IF v_inv.workspace_id IS NOT NULL THEN
      PERFORM app_lock_live_workspace(v_inv.workspace_id);
    END IF;
    SELECT * INTO v_inv FROM invitations WHERE id = p_invitation FOR UPDATE;
    IF v_inv.closed_at IS NOT NULL OR v_inv.expires_at <= now() THEN
      RAISE EXCEPTION 'invitation is no longer open' USING ERRCODE = '55000';
    END IF;
    IF p_accept THEN
      IF v_inv.workspace_id IS NULL THEN
        INSERT INTO organization_members (organization_id, user_id, role)
        VALUES (v_inv.organization_id, v_user, v_inv.role)
        ON CONFLICT (organization_id, user_id) DO NOTHING;
        INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
        VALUES (app_id(p_ids, 1), NULL, v_inv.organization_id, v_user, 'join', 'organization_member', v_user,
                jsonb_build_object('role', v_inv.role));
      ELSE
        -- 개정 2026-10-26(⑬, D-112·D-114): 조직 밖 사람이 워크스페이스 초대를 수락하면 그 조직의 멤버로 먼저 가입하고
        -- 감사로그에 남긴다(조직 멤버 가입 경로 하나 추가). 이미 멤버면 역할은 그대로다.
        INSERT INTO organization_members (organization_id, user_id, role)
        VALUES (v_inv.organization_id, v_user, 'member')
        ON CONFLICT (organization_id, user_id) DO NOTHING;
        IF FOUND THEN
          INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
          VALUES (app_id(p_ids, 2), NULL, v_inv.organization_id, v_user, 'join', 'organization_member', v_user,
                  jsonb_build_object('role', 'member', 'via', 'workspace_invitation', 'workspace_id', v_inv.workspace_id));
        END IF;
        INSERT INTO workspace_members (workspace_id, user_id, role, role_source)
        VALUES (v_inv.workspace_id, v_user, v_inv.role, 'invitation_accepted')
        ON CONFLICT (workspace_id, user_id) DO NOTHING;
        INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
        VALUES (app_id(p_ids, 1), v_inv.workspace_id, v_inv.organization_id, v_user, 'join', 'member', v_user,
                jsonb_build_object('role', v_inv.role));
      END IF;
      UPDATE invitations SET closed_at = now(), close_reason = 'accepted', accepted_by = v_user, accepted_at = now()
      WHERE id = p_invitation;
    ELSE
      UPDATE invitations SET closed_at = now(), close_reason = 'declined' WHERE id = p_invitation;
      INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_inv.workspace_id, v_inv.organization_id, v_user, 'decline', 'invitation',
              p_invitation, '{}'::jsonb);
    END IF;
    UPDATE notifications SET read_at = coalesce(read_at, now()),
                             detail = detail || jsonb_build_object('status', CASE WHEN p_accept THEN 'accepted' ELSE 'declined' END)
    WHERE invitation_id = p_invitation AND user_id = v_user;
    RETURN coalesce(v_inv.workspace_id, v_inv.organization_id);
  END
  $$;


ALTER FUNCTION fvoci.app_respond_invitation(p_invitation uuid, p_accept boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_revoke_invitation(p_invitation uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_inv invitations%ROWTYPE;
  BEGIN
    SELECT * INTO v_inv FROM invitations WHERE id = p_invitation AND closed_at IS NULL;
    IF v_inv.id IS NULL THEN
      RAISE EXCEPTION 'invitation not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_inv.workspace_id IS NULL THEN
      PERFORM app_require_organization_admin(v_inv.organization_id);
      PERFORM 1 FROM organizations WHERE id = v_inv.organization_id FOR NO KEY UPDATE;
      PERFORM app_require_organization_admin(v_inv.organization_id);
    ELSE
      PERFORM app_require_admin(v_inv.workspace_id);
      -- 수락과 같은 순서(워크스페이스 → 초대)로 잠근 뒤 아직 열려 있는지 다시 본다. 그 사이 호출한 사람의 멤버 행도
      -- 잠그고 관리자인지 다시 본다(동시에 강등·내보내기가 먼저 끝났으면 거절).
      PERFORM app_lock_live_workspace(v_inv.workspace_id);
      PERFORM 1 FROM workspace_members WHERE workspace_id = v_inv.workspace_id AND user_id = app_user_id() FOR UPDATE;
      PERFORM app_require_admin(v_inv.workspace_id);
    END IF;
    PERFORM 1 FROM invitations WHERE id = p_invitation AND closed_at IS NULL FOR UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'invitation is no longer open' USING ERRCODE = '55000';
    END IF;
    UPDATE invitations SET closed_at = now(), close_reason = 'revoked' WHERE id = p_invitation;
    UPDATE notifications SET read_at = coalesce(read_at, now()), detail = detail || jsonb_build_object('status', 'revoked')
    WHERE invitation_id = p_invitation;
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_inv.workspace_id, v_inv.organization_id, app_user_id(), 'revoke', 'invitation',
            p_invitation, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_revoke_invitation(p_invitation uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 공간에서 사람을 빼기(내보내기·떠나기·조직 탈퇴). 그 공간 문서에 그 사람이 받은 개별 공유와 업무 담당을 풀고 활동에
-- 남긴다(D-114). 새 id: 담당·리더 해제마다 감사 1 + 활동 1, 공유 철회마다 활동 1, 마지막 감사 1.
CREATE FUNCTION fvoci.app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_used  integer;
    v_share record;
  BEGIN
    -- 소유권 수락·제안과 같은 순서로 워크스페이스 행 → 멤버 행을 먼저 잠근다. 그 뒤에 소유자를 보고(수락이 끝난
    -- 뒤의 값), 항목·그룹 멤버를 지운다(동시에 넣는 쪽은 멤버 행 공유 잠금을 기다린다).
    -- 조직 탈퇴는 휴지통에 든 워크스페이스의 멤버십도 정리하므로 살아 있는지 묻지 않고 행만 잠근다.
    IF p_action IN ('organization_left', 'organization_admin_demoted') THEN
      PERFORM 1 FROM workspaces WHERE id = p_workspace FOR NO KEY UPDATE;
    ELSE
      PERFORM app_lock_live_workspace(p_workspace);
    END IF;
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member FOR UPDATE;
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'the owner cannot leave or be removed; transfer ownership first' USING ERRCODE = '55000';
    END IF;
    DELETE FROM group_members gm USING groups g
    WHERE gm.group_id = g.id AND g.workspace_id = p_workspace AND gm.user_id = p_member;
    -- 항목 멤버도 끝난다. 다시 초대되면 비공개 항목은 다시 멤버로 넣어야 열린다.
    DELETE FROM item_members im USING items i
    WHERE im.item_id = i.id AND i.workspace_id = p_workspace AND im.user_id = p_member;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace AND to_user = p_member;
    DELETE FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    v_used := app_drop_unseen_assignees(NULL, p_workspace, p_member, p_action, p_ids);
    -- 그 공간 문서에 받은 개별 공유를 거둔다(문서 행을 잠근 뒤, 공유 철회와 같은 순서).
    FOR v_share IN
      SELECT s.id, s.document_id, d.item_id FROM document_shares s JOIN documents d ON d.id = s.document_id
      WHERE d.workspace_id = p_workspace AND s.grantee_user_id = p_member
      ORDER BY s.document_id
    LOOP
      PERFORM 1 FROM documents WHERE id = v_share.document_id FOR UPDATE;
      DELETE FROM document_shares WHERE id = v_share.id;
      v_used := v_used + 1;
      PERFORM app_activity(app_id(p_ids, v_used), p_workspace, v_share.item_id, 'document', v_share.document_id,
                           'unshare', jsonb_build_object('user', p_member, 'reason', p_action));
    END LOOP;
    PERFORM app_audit(app_id(p_ids, v_used + 1), p_workspace, p_action, 'member', p_member, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 공간에서 뺄 때 풀리는 것의 건수(확인 대화용, D-114).
CREATE FUNCTION fvoci.app_remove_member_preview(p_workspace uuid, p_member uuid) RETURNS TABLE(shares integer, assignments integer, leaderships integer)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS DISTINCT FROM p_member THEN
      PERFORM app_require_admin(p_workspace);
    ELSIF app_access('workspace', p_workspace) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      SELECT (SELECT count(*)::integer FROM document_shares s JOIN documents d ON d.id = s.document_id
              WHERE d.workspace_id = p_workspace AND s.grantee_user_id = p_member),
             (SELECT count(*)::integer FROM task_assignees a JOIN tasks t ON t.id = a.task_id
              WHERE t.workspace_id = p_workspace AND a.user_id = p_member),
             (SELECT count(*)::integer FROM items i WHERE i.workspace_id = p_workspace AND i.leader_id = p_member);
  END
  $$;


ALTER FUNCTION fvoci.app_remove_member_preview(p_workspace uuid, p_member uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_remove_member_ids(p_workspace uuid, p_member uuid) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) <> 'owner' THEN
      RETURN 0;
    END IF;
    -- 공간 → 멤버 순서로 공유·그룹 변경·담당 추가를 막고 커밋된 뒤의 상한을 센다(D-88).
    PERFORM app_lock_live_workspace(p_workspace);
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = p_workspace AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) <> 'owner' THEN
      RETURN 0;
    END IF;
    RETURN 1 + 2 * app_unseen_assignees_bound(NULL, p_workspace, p_member)
      + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
         WHERE d.workspace_id = p_workspace AND s.grantee_user_id = p_member)::integer;
  END
  $$;


ALTER FUNCTION fvoci.app_remove_member_ids(p_workspace uuid, p_member uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_leave_workspace_ids(p_workspace uuid) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) = 'none' THEN
      RETURN 0;
    END IF;
    -- 공간 → 멤버 순서로 공유·그룹 변경·담당 추가를 막고 커밋된 뒤의 상한을 센다(D-88).
    PERFORM app_lock_live_workspace(p_workspace);
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = p_workspace AND user_id = app_user_id() FOR UPDATE;
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) = 'none' THEN
      RETURN 0;
    END IF;
    RETURN 1 + 2 * app_unseen_assignees_bound(NULL, p_workspace, app_user_id())
      + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
         WHERE d.workspace_id = p_workspace AND s.grantee_user_id = app_user_id())::integer;
  END
  $$;


ALTER FUNCTION fvoci.app_leave_workspace_ids(p_workspace uuid) OWNER TO fvoci_migrator;

-- 워크스페이스 삭제(소유자 또는 조직 관리자, D-51). 확인 뒤 휴지통으로 가고 보존 기간 뒤 영구 삭제한다(D-41). 새 id 2.
CREATE FUNCTION fvoci.app_delete_workspace(p_ws uuid, p_name text, p_ids uuid[]) RETURNS timestamp with time zone
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user      uuid := app_user_id();
    v_ws        workspaces%ROWTYPE;
    v_closed    integer;
    v_org_admin boolean;
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR NO KEY UPDATE;
    v_org_admin := v_ws.id IS NOT NULL AND app_access('organization', v_ws.organization_id) = 'owner';
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NOT NULL
       OR (app_access('workspace', p_ws) = 'none' AND NOT v_org_admin) THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_ws.owner_id IS DISTINCT FROM v_user AND NOT v_org_admin THEN
      RAISE EXCEPTION 'only the owner or an organization admin can delete the workspace' USING ERRCODE = '42501';
    END IF;
    IF btrim(coalesce(p_name, '')) IS DISTINCT FROM btrim(v_ws.name) THEN
      RAISE EXCEPTION 'type the workspace name to confirm' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM ownership_offers WHERE workspace_id = p_ws) THEN
      RAISE EXCEPTION 'cancel the ownership transfer first' USING ERRCODE = '55000';
    END IF;
    UPDATE invitations SET closed_at = now(), close_reason = 'workspace_deleted'
    WHERE workspace_id = p_ws AND closed_at IS NULL;
    GET DIAGNOSTICS v_closed = ROW_COUNT;
    UPDATE workspaces SET deleted_at = now(), deleted_by = v_user WHERE id = p_ws;
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), p_ws, v_ws.organization_id, v_user, 'delete', 'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'invitations_closed', v_closed)),
           (app_id(p_ids, 2), NULL, v_ws.organization_id, v_user, 'delete', 'workspace', p_ws,
            jsonb_build_object('name', v_ws.name));
    RETURN now() + make_interval(days => v_ws.trash_retention_days);
  END
  $$;


ALTER FUNCTION fvoci.app_delete_workspace(p_ws uuid, p_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_restore_workspace(p_ws uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws workspaces%ROWTYPE;
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR NO KEY UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NULL
       OR (v_ws.owner_id IS DISTINCT FROM app_user_id()
           AND app_access('organization', v_ws.organization_id) <> 'owner') THEN
      RAISE EXCEPTION 'deleted workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_ws.deleted_at + make_interval(days => v_ws.trash_retention_days) <= now() THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
    END IF;
    UPDATE workspaces SET deleted_at = NULL, deleted_by = NULL WHERE id = p_ws;
    UPDATE invitations SET closed_at = NULL, close_reason = NULL
    WHERE workspace_id = p_ws AND close_reason = 'workspace_deleted' AND expires_at > now();
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), p_ws, v_ws.organization_id, app_user_id(), 'restore', 'workspace', p_ws,
            jsonb_build_object('name', v_ws.name)),
           (app_id(p_ids, 2), NULL, v_ws.organization_id, app_user_id(), 'restore', 'workspace', p_ws,
            jsonb_build_object('name', v_ws.name));
  END
  $$;


ALTER FUNCTION fvoci.app_restore_workspace(p_ws uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 내가 소유한, 또는 내가 조직 관리자인 조직의 지운 워크스페이스.
CREATE FUNCTION fvoci.app_my_deleted_workspaces() RETURNS TABLE(id uuid, organization_id uuid, name text, deleted_at timestamp with time zone, expires_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT w.id, w.organization_id, w.name, w.deleted_at, w.deleted_at + make_interval(days => w.trash_retention_days)
    FROM workspaces w
    WHERE w.deleted_at IS NOT NULL
      AND (w.owner_id = app_user_id() OR app_access('organization', w.organization_id) = 'owner')
    ORDER BY w.deleted_at DESC, w.id
  $$;


ALTER FUNCTION fvoci.app_my_deleted_workspaces() OWNER TO fvoci_migrator;

-- 소유권 인계 제안(소유자 또는 조직 관리자, D-51·D-112). 수신자가 수락해야 끝난다.
CREATE FUNCTION fvoci.app_offer_ownership(p_workspace uuid, p_to uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_ws   workspaces%ROWTYPE;
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_workspace AND deleted_at IS NULL FOR NO KEY UPDATE;
    IF v_ws.id IS NULL OR v_user IS NULL
       OR (app_access('workspace', p_workspace) = 'none' AND app_access('organization', v_ws.organization_id) <> 'owner') THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_user <> v_ws.owner_id AND app_access('organization', v_ws.organization_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the owner or an organization admin can transfer ownership' USING ERRCODE = '42501';
    END IF;
    IF p_to = v_user OR p_to = v_ws.owner_id OR NOT EXISTS (
      SELECT 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_to
    ) THEN
      RAISE EXCEPTION 'the recipient must be another member' USING ERRCODE = '22023';
    END IF;
    INSERT INTO ownership_offers (workspace_id, from_user, to_user) VALUES (p_workspace, v_ws.owner_id, p_to)
    ON CONFLICT (workspace_id) DO UPDATE SET to_user = EXCLUDED.to_user, created_at = now();
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'offer_ownership', 'member', p_to,
                      jsonb_build_object('by', v_user));
  END
  $$;


ALTER FUNCTION fvoci.app_offer_ownership(p_workspace uuid, p_to uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 번호는 범위별 카운터와 대장으로 할당하며 지운 번호도 다시 쓰지 않는다. D-48: docs/decisions/modules.md#d-48.
CREATE FUNCTION fvoci.app_allocate_number(p_workspace uuid, p_scope uuid, p_kind text, p_target uuid) RETURNS bigint
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_scope_key text := CASE WHEN p_scope = p_workspace THEN 'WIKI' ELSE p_scope::text END;
    v_number    bigint;
  BEGIN
    INSERT INTO reference_counters (workspace_id, scope, next_number) VALUES (p_workspace, v_scope_key, 2)
    ON CONFLICT (workspace_id, scope)
      DO UPDATE SET next_number = reference_counters.next_number + 1
    RETURNING next_number - 1 INTO v_number;
    -- 이미 번호가 있는 대상(이동)이면 새 범위·번호로 옮긴다. 옛 번호는 카운터가 되돌아가지 않으므로 재사용되지 않는다.
    INSERT INTO numbered_targets (workspace_id, scope_id, number, target_kind, target_id)
    VALUES (p_workspace, p_scope, v_number, p_kind, p_target)
    ON CONFLICT ON CONSTRAINT numbered_targets_target
      DO UPDATE SET workspace_id = EXCLUDED.workspace_id, scope_id = EXCLUDED.scope_id, number = EXCLUDED.number;
    RETURN v_number;
  END
  $$;


ALTER FUNCTION fvoci.app_allocate_number(p_workspace uuid, p_scope uuid, p_kind text, p_target uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_archive_item(p_item uuid, p_archived boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item items%ROWTYPE;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('item', p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can archive' USING ERRCODE = '42501';
    END IF;
    IF (v_item.archived_at IS NOT NULL) <> p_archived THEN
      UPDATE items SET archived_at = CASE WHEN p_archived THEN now() END WHERE id = p_item;
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, CASE WHEN p_archived THEN 'archive' ELSE 'unarchive' END,
                        'item', p_item, '{}'::jsonb);
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_archive_item(p_item uuid, p_archived boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_archive_module(p_module uuid, p_archived boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_mod modules%ROWTYPE;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR UPDATE;
    IF v_mod.id IS NULL OR app_access('workspace', v_mod.workspace_id) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_mod.workspace_id);
    IF (v_mod.archived_at IS NOT NULL) <> p_archived THEN
      UPDATE modules SET archived_at = CASE WHEN p_archived THEN now() END WHERE id = p_module;
      PERFORM app_audit(app_id(p_ids, 1), v_mod.workspace_id, CASE WHEN p_archived THEN 'archive' ELSE 'unarchive' END,
                        'module', p_module, '{}'::jsonb);
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_archive_module(p_module uuid, p_archived boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_cancel_ownership_offer(p_workspace uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
  BEGIN
    IF v_user <> (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'only the owner can cancel' USING ERRCODE = '42501';
    END IF;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'no pending offer' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'cancel_ownership_offer', 'workspace', p_workspace,
                      '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_cancel_ownership_offer(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text, p_reset boolean, p_ids uuid[]) RETURNS TABLE(copied_item uuid, skipped_count integer)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_src     items%ROWTYPE;
    v_src_mod modules%ROWTYPE;
    v_dst_mod modules%ROWTYPE;
    v_new     uuid;
    v_existed boolean;
    v_row     record;
    v_id      uuid;
    v_skipped integer := 0;
    v_access  text;
    v_map     jsonb := '{}'::jsonb;
    v_smap    jsonb := '{}'::jsonb;
    v_todo    uuid;
    v_at      integer := 6;
    v_count   integer;
  BEGIN
    SELECT * INTO v_src FROM items WHERE id = p_item;
    IF v_src.id IS NULL OR app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_src_mod FROM modules WHERE id = v_src.module_id;
    SELECT * INTO v_dst_mod FROM modules WHERE id = p_module;
    IF v_dst_mod.id IS NULL OR app_access('workspace', v_dst_mod.workspace_id) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_dst_mod.workspace_id <> v_src.workspace_id OR v_dst_mod.kind <> v_src_mod.kind THEN
      RAISE EXCEPTION 'copy into a module of the same kind in the same workspace' USING ERRCODE = '22023';
    END IF;
    v_existed := EXISTS (SELECT 1 FROM items WHERE module_id = p_module AND created_by = v_user AND request_id = p_request);
    -- 권한·보관·번호·감사는 항목 만들기와 같은 길로(새 id 여섯 개: 항목·감사·기본 상태 셋·활동).
    v_new := app_create_item(p_module, p_request, coalesce(nullif(btrim(p_title), ''), v_src.title), v_src.description,
                             p_ids[1:6]);
    IF v_existed THEN
      RETURN QUERY SELECT v_new, 0;
      RETURN;
    END IF;
    UPDATE items SET goal = v_src.goal, scope = v_src.scope, details = v_src.details,
                     status = CASE WHEN p_reset THEN 'planned' ELSE v_src.status END,
                     starts_on = CASE WHEN p_reset THEN NULL ELSE v_src.starts_on END,
                     ends_on = CASE WHEN p_reset THEN NULL ELSE v_src.ends_on END
    WHERE id = v_new;

    -- 상태 정의(D-116): 기본 세 상태 대신 원본 항목의 정의를 그대로 복사한다. v_smap: 옛 상태 id → 새 상태 id.
    v_at := 6;
    DELETE FROM task_statuses WHERE item_id = v_new;
    FOR v_row IN SELECT * FROM task_statuses WHERE item_id = p_item ORDER BY position, created_at, id LOOP
      v_at := v_at + 1;
      v_id := app_id(p_ids, v_at);
      INSERT INTO task_statuses (id, item_id, name, color, position, category)
      VALUES (v_id, v_new, v_row.name, v_row.color, v_row.position, v_row.category);
      v_smap := v_smap || jsonb_build_object(v_row.id::text, v_id);
    END LOOP;
    v_todo := app_default_task_status(v_new);

    -- 업무: 부모 먼저(하위 업무는 한 단계). v_map: 옛 id → 새 id.
    FOR v_row IN
      SELECT t.*, s.category FROM tasks t JOIN task_statuses s ON s.id = t.status_id
      WHERE t.item_id = p_item AND t.deleted_at IS NULL
      ORDER BY (t.parent_id IS NOT NULL), t.number
    LOOP
      v_id := app_id(p_ids, v_at + 1);
      INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, description, status_id, priority,
                         start_date, due_date, estimate_minutes, tags, position, milestone, completed_at,
                         created_by, updated_by, request_id)
      VALUES (v_id, v_src.workspace_id, v_new, (v_map->>v_row.parent_id::text)::uuid,
              app_allocate_number(v_src.workspace_id, p_module, 'task', v_id), v_row.title, v_row.description,
              CASE WHEN p_reset THEN v_todo ELSE (v_smap->>v_row.status_id::text)::uuid END, v_row.priority,
              CASE WHEN p_reset THEN NULL ELSE v_row.start_date END,
              CASE WHEN p_reset THEN NULL ELSE v_row.due_date END,
              v_row.estimate_minutes, v_row.tags, v_row.position, v_row.milestone,
              CASE WHEN (CASE WHEN p_reset THEN (SELECT category FROM task_statuses WHERE id = v_todo)
                              ELSE v_row.category END) = 'done' THEN now() END,
              v_user, v_user, app_id(p_ids, v_at + 2));
      v_at := v_at + 2;
      v_map := v_map || jsonb_build_object(v_row.id::text, v_id);
      INSERT INTO checklist_items (id, task_id, text, checked, position, request_id, created_by)
        SELECT app_id(p_ids, v_at + 2 * c.n - 1), v_id, c.text, CASE WHEN p_reset THEN false ELSE c.checked END,
               c.position, app_id(p_ids, v_at + 2 * c.n), v_user
        FROM (SELECT c.*, (row_number() OVER (ORDER BY c.position, c.id))::integer AS n
              FROM checklist_items c WHERE c.task_id = v_row.id) c;
      GET DIAGNOSTICS v_count = ROW_COUNT;
      v_at := v_at + 2 * v_count;
    END LOOP;
    -- 초기화 복제는 모든 열을 기본 상태로 합치므로 원본 열마다의 위치값이 겹친다. 보이는 순서대로 다시 매긴다.
    IF p_reset THEN
      PERFORM app_renumber_lane(v_todo);
    END IF;
    -- 종류·지연일도 복사한다.
    INSERT INTO task_dependencies (predecessor_id, successor_id, item_id, created_by, kind, lag_days)
      SELECT (v_map->>d.predecessor_id::text)::uuid, (v_map->>d.successor_id::text)::uuid, v_new, v_user,
             d.kind, d.lag_days
      FROM task_dependencies d
      WHERE v_map ? d.predecessor_id::text AND v_map ? d.successor_id::text;

    -- 항목 문서: 볼 수 있고 복제가 허용된 것(또는 소유 권한), 부모 먼저.
    v_map := '{}'::jsonb;
    FOR v_row IN
      WITH RECURSIVE tree AS (
        SELECT d.*, 0 AS depth FROM documents d
        WHERE d.item_id = p_item AND d.deleted_at IS NULL AND d.parent_id IS NULL
        UNION ALL
        SELECT d.*, t.depth + 1 FROM documents d JOIN tree t ON d.parent_id = t.id
        WHERE d.deleted_at IS NULL
      )
      SELECT * FROM tree ORDER BY depth, position
    LOOP
      v_access := app_access('document', v_row.id);
      IF v_access = 'none' OR (v_access <> 'owner' AND NOT v_row.allow_copy)
         OR (v_row.parent_id IS NOT NULL AND NOT v_map ? v_row.parent_id::text) THEN
        v_skipped := v_skipped + 1;
        CONTINUE;
      END IF;
      v_id := app_id(p_ids, v_at + 1);
      INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility, parent_id, position)
      VALUES (v_id, v_src.workspace_id, v_new, 'module', p_module::text,
              app_allocate_number(v_src.workspace_id, p_module, 'document', v_id), v_row.title,
              v_row.schema_version, v_row.body, 1, v_user, v_user, app_id(p_ids, v_at + 2),
              v_row.visibility,
              (v_map->>v_row.parent_id::text)::uuid, v_row.position);
      INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
      VALUES (v_id, 1, v_row.title, v_row.schema_version, v_row.body, v_user, app_id(p_ids, v_at + 2));
      v_at := v_at + 2;
      v_map := v_map || jsonb_build_object(v_row.id::text, v_id);
    END LOOP;

    -- 강좌 정보·회차(진도 제외), 스터디 단위(진행 제외).
    INSERT INTO course_details (item_id, workspace_id, subject, instructor, platform, url, textbook, starts_on,
                                expires_on, target_on, updated_by)
      SELECT v_new, workspace_id, subject, instructor, platform, url, textbook,
             CASE WHEN p_reset THEN NULL ELSE starts_on END, CASE WHEN p_reset THEN NULL ELSE expires_on END,
             CASE WHEN p_reset THEN NULL ELSE target_on END, v_user
      FROM course_details WHERE item_id = p_item;
    INSERT INTO course_episodes (id, item_id, workspace_id, number, title, chapter, duration_seconds, url, materials,
                                 planned_on, created_by, request_id)
      SELECT app_id(p_ids, v_at + 2 * e.n - 1), v_new, e.workspace_id, e.number, e.title, e.chapter,
             e.duration_seconds, e.url, e.materials,
             CASE WHEN p_reset THEN NULL ELSE e.planned_on END, v_user, app_id(p_ids, v_at + 2 * e.n)
      FROM (SELECT e.*, (row_number() OVER (ORDER BY e.number, e.id))::integer AS n
            FROM course_episodes e WHERE e.item_id = p_item) e;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_at := v_at + 2 * v_count;
    INSERT INTO study_units (id, item_id, workspace_id, number, title, chapter, unit, amount, planned_on, created_by,
                             request_id)
      SELECT app_id(p_ids, v_at + 2 * u.n - 1), v_new, u.workspace_id, u.number, u.title, u.chapter, u.unit,
             u.amount, CASE WHEN p_reset THEN NULL ELSE u.planned_on END, v_user, app_id(p_ids, v_at + 2 * u.n)
      FROM (SELECT u.*, (row_number() OVER (ORDER BY u.number, u.id))::integer AS n
            FROM study_units u WHERE u.item_id = p_item) u;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_at := v_at + 2 * v_count;

    PERFORM app_audit(app_id(p_ids, v_at + 1), v_src.workspace_id, 'copy', 'item', v_new,
                      jsonb_build_object('from', p_item, 'reset', p_reset, 'skipped_documents', v_skipped));
    RETURN QUERY SELECT v_new, v_skipped;
  END
  $$;


ALTER FUNCTION fvoci.app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text, p_reset boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_copy_item_ids(p_item uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 항목 만들기 6 + 복사한 상태 정의 수 + 업무마다 2 + 체크 항목마다 2 + 문서마다 2 + 회차·단위마다 2 + 마지막 감사 1.
    SELECT CASE WHEN app_access('item', p_item) = 'none' THEN 0 ELSE
      7 + (SELECT count(*) FROM task_statuses WHERE item_id = p_item)::integer
        + 2 * (SELECT count(*) FROM tasks WHERE item_id = p_item AND deleted_at IS NULL)::integer
        + 2 * (SELECT count(*) FROM checklist_items c JOIN tasks t ON t.id = c.task_id
               WHERE t.item_id = p_item AND t.deleted_at IS NULL)::integer
        + 2 * (SELECT count(*) FROM documents WHERE item_id = p_item AND deleted_at IS NULL)::integer
        + 2 * (SELECT count(*) FROM course_episodes WHERE item_id = p_item)::integer
        + 2 * (SELECT count(*) FROM study_units WHERE item_id = p_item)::integer
    END
  $$;


ALTER FUNCTION fvoci.app_copy_item_ids(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_group(p_workspace uuid, p_name text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id uuid;
  BEGIN
    PERFORM app_require_admin(p_workspace);
    INSERT INTO groups (id, workspace_id, name) VALUES (app_id(p_ids, 1), p_workspace, btrim(p_name))
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), p_workspace, 'create', 'group', v_id, jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  EXCEPTION WHEN unique_violation THEN
    RAISE EXCEPTION 'a group with this name exists' USING ERRCODE = '55000';
  END
  $$;


ALTER FUNCTION fvoci.app_create_group(p_workspace uuid, p_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_mod  modules%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR SHARE;
    -- 휴지통에 든 모듈은 없는 것으로 본다(모듈 지우기는 행을 갱신해 잠그므로 공유 잠금 뒤에 보면 된다).
    IF v_user IS NULL OR v_mod.id IS NULL OR v_mod.deleted_at IS NOT NULL
       OR app_access('workspace', v_mod.workspace_id) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('workspace', v_mod.workspace_id) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF v_mod.archived_at IS NOT NULL THEN
      RAISE EXCEPTION 'the module is archived' USING ERRCODE = '55000';
    END IF;
    SELECT id INTO v_id FROM items WHERE module_id = p_module AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    v_id := app_id(p_ids, 1);
    PERFORM app_allocate_number(v_mod.workspace_id, p_module, 'item', v_id);
    INSERT INTO items (id, workspace_id, module_id, title, description, created_by, updated_by, request_id)
    VALUES (v_id, v_mod.workspace_id, p_module, btrim(p_title), coalesce(p_description, ''),
            v_user, v_user, p_request_id);
    PERFORM app_audit(app_id(p_ids, 2), v_mod.workspace_id, 'create', 'item', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    -- 기본 세 상태(D-116)와 활동 피드(D-119). 새 id는 모두 여섯 개다.
    PERFORM app_seed_task_statuses(v_id, app_ids_from(p_ids, 3));
    PERFORM app_activity(app_id(p_ids, 6), v_mod.workspace_id, v_id, 'item', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_module(p_workspace uuid, p_request_id uuid, p_kind text, p_name text, p_key text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_require_admin(p_workspace);
    v_key  text := app_normalize_key(p_key);
    v_id   uuid;
  BEGIN
    SELECT id INTO v_id FROM modules
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_kind NOT IN ('project', 'research', 'study', 'course', 'hobby') THEN
      RAISE EXCEPTION 'unknown module kind' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM module_keys WHERE workspace_id = p_workspace AND key = v_key) THEN
      RAISE EXCEPTION 'this key is in use or was used before' USING ERRCODE = '55000';
    END IF;
    INSERT INTO modules (id, workspace_id, kind, name, created_by, request_id)
    VALUES (app_id(p_ids, 1), p_workspace, p_kind, btrim(p_name), v_user, p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO module_keys (workspace_id, key, module_id) VALUES (p_workspace, v_key, v_id);
    PERFORM app_audit(app_id(p_ids, 2), p_workspace, 'create', 'module', v_id,
                      jsonb_build_object('kind', p_kind, 'name', btrim(p_name), 'key', v_key));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_module(p_workspace uuid, p_request_id uuid, p_kind text, p_name text, p_key text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_attached_files() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF TG_ARGV[0] = 'document' THEN
      PERFORM app_delete_file_rows(ARRAY(SELECT id FROM files WHERE document_id = OLD.id));
    ELSE
      PERFORM app_delete_file_rows(ARRAY(SELECT id FROM files WHERE item_id = OLD.id));
    END IF;
    RETURN OLD;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_attached_files() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_group(p_group uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws   uuid;
    v_used integer;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_access('workspace', v_ws) = 'none' THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    -- 멤버 행을 먼저 잠근다: 동시에 담당자로 지정하는 쪽(멤버 행 공유 잠금)이 정리가 끝날 때까지 기다린다.
    PERFORM 1 FROM workspace_members m JOIN group_members gm ON gm.user_id = m.user_id
    WHERE m.workspace_id = v_ws AND gm.group_id = p_group
    ORDER BY m.user_id
    FOR UPDATE OF m;
    DELETE FROM groups WHERE id = p_group;
    v_used := app_drop_unseen_assignees(NULL, v_ws, NULL, 'group_deleted', p_ids);
    PERFORM app_audit(app_id(p_ids, v_used + 1), v_ws, 'delete', 'group', p_group, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_delete_group(p_group uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_group_ids(p_group uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('workspace', g.workspace_id) = 'owner'
                THEN 1 + 2 * app_unseen_assignees_bound(NULL, g.workspace_id, NULL) ELSE 0 END
    FROM groups g WHERE g.id = p_group
  $$;


ALTER FUNCTION fvoci.app_delete_group_ids(p_group uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_item_course_rows() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    DELETE FROM course_episodes WHERE item_id = OLD.id;
    DELETE FROM course_details WHERE item_id = OLD.id;
    RETURN OLD;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_item_course_rows() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_is_item_member(p_item uuid, p_user uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT EXISTS (SELECT 1 FROM item_members WHERE item_id = p_item AND user_id = p_user)
        OR EXISTS (SELECT 1 FROM item_members im JOIN group_members gm ON gm.group_id = im.group_id
                   WHERE im.item_id = p_item AND gm.user_id = p_user)
  $$;


ALTER FUNCTION fvoci.app_is_item_member(p_item uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_item_archived(p_item uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
      WHERE i.id = p_item AND (i.archived_at IS NOT NULL OR m.archived_at IS NOT NULL))
  $$;


ALTER FUNCTION fvoci.app_item_archived(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_item_assignable(p_item uuid) RETURNS TABLE(user_id uuid, display_name text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item;
    RETURN QUERY
    SELECT u.id, u.display_name
    FROM workspace_members m JOIN users u ON u.id = m.user_id
    WHERE m.workspace_id = v_ws AND app_access_as('item', p_item, m.user_id) <> 'none'
    ORDER BY u.display_name, u.id;
  END
  $$;


ALTER FUNCTION fvoci.app_item_assignable(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_item_reference(p_item uuid) RETURNS text
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT k.key || '-' || n.number
    FROM numbered_targets n
    JOIN module_keys k ON k.module_id = n.scope_id AND k.retired_at IS NULL
    WHERE n.target_kind = 'item' AND n.target_id = p_item
  $$;


ALTER FUNCTION fvoci.app_item_reference(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_items_versions() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF (NEW.title, NEW.description, NEW.visibility) IS DISTINCT FROM (OLD.title, OLD.description, OLD.visibility) THEN
      NEW.version := OLD.version + 1;
    ELSE
      NEW.version := OLD.version;
    END IF;
    IF (NEW.leader_id, NEW.goal, NEW.scope, NEW.starts_on, NEW.ends_on, NEW.status, NEW.details)
       IS DISTINCT FROM (OLD.leader_id, OLD.goal, OLD.scope, OLD.starts_on, OLD.ends_on, OLD.status, OLD.details) THEN
      NEW.profile_version := OLD.profile_version + 1;
    ELSE
      NEW.profile_version := OLD.profile_version;
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_items_versions() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_leave_workspace(p_workspace uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_remove_member_internal(p_workspace, app_user_id(), 'leave', p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_leave_workspace(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_item(p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_item IS NULL THEN
      RETURN;
    END IF;
    PERFORM 1 FROM modules m JOIN items i ON i.module_id = m.id WHERE i.id = p_item FOR SHARE OF m;
    PERFORM 1 FROM items WHERE id = p_item FOR SHARE;
  END
  $$;


ALTER FUNCTION fvoci.app_lock_item(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_live_workspace(p_ws uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM 1 FROM workspaces WHERE id = p_ws AND deleted_at IS NULL FOR NO KEY UPDATE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_lock_live_workspace(p_ws uuid) OWNER TO fvoci_migrator;

-- 멤버십 정책에서 같은 테이블을 RLS로 다시 읽으면 재귀하므로 소유자 권한으로 현재 사용자의 역할만 확인한다.
-- 현재 사용자의 워크스페이스 역할(관리자·멤버·읽기 전용, 표시용). 권한 판정은 역할이 아니라 app_access('workspace', …)로 한다.
CREATE FUNCTION fvoci.app_member_role(ws uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_workspace_role(ws, app_user_id()) $$;


ALTER FUNCTION fvoci.app_member_role(ws uuid) OWNER TO fvoci_migrator;

-- 내가 속한(조직 멤버이기도 한) 살아 있는 워크스페이스. p_organization이 있으면 그 조직(현재 조직 화면, D-111)으로 좁힌다.
CREATE FUNCTION fvoci.app_my_workspace_ids(p_organization uuid) RETURNS SETOF uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT w.id
    FROM workspace_members m
    JOIN workspaces w ON w.id = m.workspace_id AND w.deleted_at IS NULL
    JOIN organization_members om ON om.organization_id = w.organization_id AND om.user_id = m.user_id
    WHERE m.user_id = app_user_id() AND (p_organization IS NULL OR w.organization_id = p_organization)
  $$;


ALTER FUNCTION fvoci.app_my_workspace_ids(p_organization uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_remove_member(p_workspace uuid, p_member uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    -- 워크스페이스 행 → 두 사람의 멤버 행(id 순서)을 잠근 뒤 관리자인지 본다(관리자끼리 동시에 서로 내보내기).
    PERFORM app_lock_live_workspace(p_workspace);
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
    PERFORM app_require_admin(p_workspace);
    PERFORM app_remove_member_internal(p_workspace, p_member, 'remove', p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_remove_member(p_workspace uuid, p_member uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_file_write(p_file uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('file', p_file);
  BEGIN
    -- 문서·업무와 같은 규칙: 볼 수 없으면 404, 보관된 항목 아래면 409가 먼저, 그다음 권한 부족 403(D-106·D-113).
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      IF app_file_archived(p_file) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_require_file_write(p_file uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_admin(p_workspace uuid) RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_grade text := app_access('workspace', p_workspace);
  BEGIN
    IF v_user IS NULL OR v_grade = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_grade <> 'owner' THEN
      RAISE EXCEPTION 'admin role required' USING ERRCODE = '42501';
    END IF;
    RETURN v_user;
  END
  $$;


ALTER FUNCTION fvoci.app_require_admin(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_course_write(p_item uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_course_access(p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_require_course_write(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_document(p_doc uuid, p_need text) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
    v_item   uuid;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF (p_need = 'owner' AND v_access <> 'owner')
       OR (p_need = 'write' AND v_access NOT IN ('owner', 'write')) THEN
      -- 보관된 항목 아래 문서는 read로 낮아진다(D-113). 볼 수 있는 사람에게는 권한 부족이 아니라 보관 상태를 먼저 알린다
      -- (D-106: 보관 중 변경은 409, 보관 해제 뒤 다시). 댓글·업무(app_require_task_write)와 같은 규칙.
      SELECT item_id INTO v_item FROM documents WHERE id = p_doc;
      IF v_item IS NOT NULL AND app_item_archived(v_item) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'insufficient permission' USING ERRCODE = '42501';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_require_document(p_doc uuid, p_need text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_event_write(p_event uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('event', p_event);
    v_item   uuid;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'event not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      SELECT item_id INTO v_item FROM events WHERE id = p_event;
      IF app_item_archived(v_item) AND app_access('item', v_item) IN ('owner', 'write') THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_require_event_write(p_event uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_study_write(p_item uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_study_access(p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_require_study_write(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_task_write(p_task uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('task', p_task);
    v_item   uuid;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      -- 보관된 항목 아래에서는 누구에게나 보관 상태(409)가 먼저다(D-106: 보관 중 변경은 409, 댓글과 같은 규칙).
      SELECT item_id INTO v_item FROM tasks WHERE id = p_task;
      IF app_item_archived(v_item) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_require_task_write(p_task uuid) OWNER TO fvoci_migrator;

-- 표시 설정(D-50·D-81·D-109). 화면 밝기(theme)는 system·light·dark만 받고 "고르지 않음"이 없다.
CREATE FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_zone text := btrim(p_zone);
  BEGIN
    IF app_user_id() IS NULL THEN
      RAISE EXCEPTION 'sign in first' USING ERRCODE = '42501';
    END IF;
    IF v_zone IS NOT NULL AND v_zone <> '' AND NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = v_zone) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
    IF p_locale IS NOT NULL AND p_locale NOT IN ('', 'ko', 'en') THEN
      RAISE EXCEPTION 'unknown language' USING ERRCODE = '22023';
    END IF;
    IF p_week_start IS NOT NULL AND p_week_start NOT IN ('', 'mon', 'sun') THEN
      RAISE EXCEPTION 'unknown week start' USING ERRCODE = '22023';
    END IF;
    IF p_hour_cycle IS NOT NULL AND p_hour_cycle NOT IN ('', 'h12', 'h23') THEN
      RAISE EXCEPTION 'unknown hour cycle' USING ERRCODE = '22023';
    END IF;
    -- 다른 칸과 달리 "고르지 않음"(빈 값)이 없다. 기본이 시스템 설정 따르기다.
    IF p_theme IS NOT NULL AND p_theme NOT IN ('system', 'light', 'dark') THEN
      RAISE EXCEPTION 'unknown theme' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_settings (user_id) VALUES (app_user_id()) ON CONFLICT (user_id) DO NOTHING;
    UPDATE user_settings SET
      time_zone = coalesce(v_zone, time_zone),
      locale = coalesce(p_locale, locale),
      week_start = coalesce(p_week_start, week_start),
      hour_cycle = coalesce(p_hour_cycle, hour_cycle),
      theme = coalesce(p_theme, theme)
    WHERE user_id = app_user_id();
  END
  $$;


ALTER FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) OWNER TO fvoci_migrator;

-- 개정 2026-10-26(⑩, D-121·D-111): 마지막으로 고른 조직을 계정에 남겨 기기 간 이어진다. 멤버인 조직만 받고 NULL은 지운다.
-- 조직이 지워지면 FK가 NULL로 돌린다. 멤버에서 빠진 조직이 남아 있으면 화면이 무시한다.
CREATE FUNCTION fvoci.app_set_last_organization(p_organization uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NULL THEN
      RAISE EXCEPTION 'sign in first' USING ERRCODE = '42501';
    END IF;
    IF p_organization IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM organization_members WHERE organization_id = p_organization AND user_id = app_user_id()
    ) THEN
      RAISE EXCEPTION 'not a member of the organization' USING ERRCODE = '22023';
    END IF;
    INSERT INTO user_settings (user_id) VALUES (app_user_id()) ON CONFLICT (user_id) DO NOTHING;
    UPDATE user_settings SET last_organization_id = p_organization WHERE user_id = app_user_id();
  END
  $$;


ALTER FUNCTION fvoci.app_set_last_organization(p_organization uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_group_member(p_group uuid, p_member uuid, p_present boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws   uuid;
    v_used integer;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_access('workspace', v_ws) = 'none' THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    -- 호출한 사람과 대상의 멤버 행을 id 순서로 잠근 뒤 관리자인지 다시 보고 대상을 확인한다(동시에 내보내기·강등이
    -- 먼저 끝났으면 끝난 뒤의 상태로 본다).
    PERFORM 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
    PERFORM app_require_admin(v_ws);
    IF p_present THEN
      IF NOT EXISTS (SELECT 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id = p_member) THEN
        RAISE EXCEPTION 'only workspace members can join a group' USING ERRCODE = '22023';
      END IF;
      INSERT INTO group_members (group_id, user_id) VALUES (p_group, p_member) ON CONFLICT DO NOTHING;
      PERFORM app_audit(app_id(p_ids, 1), v_ws, 'add_group_member', 'group', p_group,
                        jsonb_build_object('user', p_member));
    ELSE
      DELETE FROM group_members WHERE group_id = p_group AND user_id = p_member;
      v_used := app_drop_unseen_assignees(NULL, v_ws, p_member, 'group_member_removed', p_ids);
      PERFORM app_audit(app_id(p_ids, v_used + 1), v_ws, 'remove_group_member', 'group', p_group,
                        jsonb_build_object('user', p_member));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_set_group_member(p_group uuid, p_member uuid, p_present boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_group_member_ids(p_group uuid, p_member uuid, p_present boolean) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('workspace', g.workspace_id) <> 'owner' THEN 0
                WHEN p_present THEN 1
                ELSE 1 + 2 * app_unseen_assignees_bound(NULL, g.workspace_id, p_member) END
    FROM groups g WHERE g.id = p_group
  $$;


ALTER FUNCTION fvoci.app_set_group_member_ids(p_group uuid, p_member uuid, p_present boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[], p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
    v_users  uuid[] := ARRAY(SELECT DISTINCT unnest(coalesce(p_users, '{}')));
    v_groups uuid[] := ARRAY(SELECT DISTINCT unnest(coalesce(p_groups, '{}')));
    v_before jsonb;
    v_after  jsonb;
  BEGIN
    -- 넣을 사람들의 멤버 행을 id 순서로 공유 잠금한 뒤(멤버 행 → 항목 행) 확인한다. 동시에 내보내는 중이면 끝날
    -- 때까지 기다리고, 끝난 뒤의 멤버 목록으로 본다.
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = (SELECT workspace_id FROM items WHERE id = p_item) AND user_id = ANY (v_users)
    ORDER BY user_id FOR SHARE;
    -- 담당자·리더 정리(`app_drop_unseen_assignees`)의 워크스페이스 잠금을 항목 행보다 먼저 잡는다. 역할 바꾸기·내보내기는
    -- 멤버 행 → 이 잠금 → 항목 행 순서라, 항목 행을 쥔 채 이 잠금을 기다리면 그 리더의 역할을 바꾸는 요청과 교착할 수 있다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-assignees:' || (SELECT workspace_id FROM items WHERE id = p_item)::text, 0));
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_access('item', p_item);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can choose members' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF cardinality(v_users) + cardinality(v_groups) > 500 THEN
      RAISE EXCEPTION 'too many members' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM unnest(v_users) u
               WHERE NOT EXISTS (SELECT 1 FROM workspace_members m
                                 WHERE m.workspace_id = v_item.workspace_id AND m.user_id = u)) THEN
      RAISE EXCEPTION 'members must belong to the workspace' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM unnest(v_groups) g
               WHERE NOT EXISTS (SELECT 1 FROM groups gr WHERE gr.id = g AND gr.workspace_id = v_item.workspace_id)) THEN
      RAISE EXCEPTION 'groups must belong to the workspace' USING ERRCODE = '22023';
    END IF;
    SELECT jsonb_build_object('users', coalesce(jsonb_agg(user_id) FILTER (WHERE user_id IS NOT NULL), '[]'),
                              'groups', coalesce(jsonb_agg(group_id) FILTER (WHERE group_id IS NOT NULL), '[]'))
      INTO v_before FROM item_members WHERE item_id = p_item;
    DELETE FROM item_members WHERE item_id = p_item
      AND ((user_id IS NOT NULL AND user_id <> ALL (v_users)) OR (group_id IS NOT NULL AND group_id <> ALL (v_groups)));
    INSERT INTO item_members (item_id, user_id, added_by)
      SELECT p_item, u, app_user_id() FROM unnest(v_users) u
      ON CONFLICT (item_id, user_id) WHERE user_id IS NOT NULL DO NOTHING;
    INSERT INTO item_members (item_id, group_id, added_by)
      SELECT p_item, g, app_user_id() FROM unnest(v_groups) g
      ON CONFLICT (item_id, group_id) WHERE group_id IS NOT NULL DO NOTHING;
    v_after := jsonb_build_object('users', to_jsonb(v_users), 'groups', to_jsonb(v_groups));
    IF v_before IS DISTINCT FROM v_after THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'set_members', 'item', p_item,
                        jsonb_build_object('users', cardinality(v_users), 'groups', cardinality(v_groups)));
    END IF;
    PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_member_removed', app_ids_from(p_ids, 2));
    IF v_item.leader_id IS NOT NULL AND app_access_as('item', p_item, v_item.leader_id) = 'none' THEN
      UPDATE items SET leader_id = NULL WHERE id = p_item;
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[], p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_item_members_ids(p_item uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('item', p_item) <> 'owner' THEN 0
                ELSE 1 + 2 * app_unseen_assignees_bound(p_item, NULL, NULL) END
  $$;


ALTER FUNCTION fvoci.app_set_item_members_ids(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_item_profile(p_item uuid, p_base_version integer, p_leader uuid, p_goal text, p_scope text, p_starts date, p_ends date, p_status text, p_details jsonb, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item   items%ROWTYPE;
    v_access text;
  BEGIN
    -- 새 리더의 멤버 행을 먼저 공유 잠금한다(멤버 행 → 항목 행). 동시에 내보내는 중이면 끝난 뒤의 상태로 본다.
    IF p_leader IS NOT NULL THEN
      PERFORM 1 FROM workspace_members
      WHERE workspace_id = (SELECT workspace_id FROM items WHERE id = p_item) AND user_id = p_leader FOR SHARE;
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_access('item', p_item);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 읽은 판이 지났으면 바꾸지 않는다(바꿀 것이 없는 재시도는 그대로 성공).
    IF v_item.profile_version IS DISTINCT FROM p_base_version THEN
      IF v_item.leader_id IS NOT DISTINCT FROM p_leader AND v_item.goal = coalesce(p_goal, '')
         AND v_item.scope = coalesce(p_scope, '') AND v_item.starts_on IS NOT DISTINCT FROM p_starts
         AND v_item.ends_on IS NOT DISTINCT FROM p_ends AND v_item.status = p_status
         AND v_item.details = coalesce(p_details, '{}'::jsonb) THEN
        RETURN 'updated';
      END IF;
      RETURN 'conflict';
    END IF;
    IF p_status NOT IN ('planned', 'active', 'on_hold', 'done', 'cancelled') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF p_starts IS NOT NULL AND p_ends IS NOT NULL AND p_starts > p_ends THEN
      RAISE EXCEPTION 'the period ends before it starts' USING ERRCODE = '22023';
    END IF;
    IF length(coalesce(p_goal, '')) > 4000 OR length(coalesce(p_scope, '')) > 4000
       OR length(coalesce(p_details, '{}'::jsonb)::text) > 20000 THEN
      RAISE EXCEPTION 'item details are too long' USING ERRCODE = '22023';
    END IF;
    IF p_leader IS NOT NULL AND p_leader IS DISTINCT FROM v_item.leader_id
       AND app_access_as('item', p_item, p_leader) = 'none' THEN
      RAISE EXCEPTION 'the leader must be able to see the item' USING ERRCODE = '22023';
    END IF;
    UPDATE items SET leader_id = p_leader, goal = coalesce(p_goal, ''), scope = coalesce(p_scope, ''),
                     starts_on = p_starts, ends_on = p_ends, status = p_status,
                     details = coalesce(p_details, '{}'::jsonb), updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.status <> p_status THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'change_status', 'item', p_item,
                        jsonb_build_object('from', v_item.status, 'to', p_status));
    END IF;
    IF v_item.leader_id IS DISTINCT FROM p_leader THEN
      PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'change_leader', 'item', p_item,
                        jsonb_build_object('from', v_item.leader_id, 'to', p_leader));
    END IF;
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_set_item_profile(p_item uuid, p_base_version integer, p_leader uuid, p_goal text, p_scope text, p_starts date, p_ends date, p_status text, p_details jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_item_template(p_item uuid, p_template boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR UPDATE;
    PERFORM app_record_write_check(p_item);
    UPDATE items SET is_template = p_template, updated_at = now() WHERE id = p_item;
    PERFORM app_audit(app_id(p_ids, 1), v_ws, CASE WHEN p_template THEN 'mark_template' ELSE 'unmark_template' END,
                      'item', p_item, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_set_item_template(p_item uuid, p_template boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old text;
  BEGIN
    -- 공간 → 멤버 순서로 잠가 조직 강등 회수·공유 상한 검사와 줄 세운다(PR4a).
    PERFORM app_lock_live_workspace(p_workspace);
    -- 두 사람의 멤버 행을 id 순서로 잠근 뒤 관리자인지 본다(관리자끼리 동시에 서로 강등하기).
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
    PERFORM app_require_admin(p_workspace);
    IF p_role NOT IN ('admin', 'member', 'readonly') THEN
      RAISE EXCEPTION 'invalid role' USING ERRCODE = '22023';
    END IF;
    SELECT role INTO v_old FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member FOR UPDATE;
    IF v_old IS NULL THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) AND p_role <> 'admin' THEN
      RAISE EXCEPTION 'the owner must stay an admin' USING ERRCODE = '55000';
    END IF;
    -- 자기 역할 변경으로 admin_join의 회수 근거를 지우지 않는다. 독립 부여는 다른 관리자의 변경뿐이다(D-112).
    UPDATE workspace_members SET role = p_role,
      role_source = CASE WHEN p_member <> app_user_id() THEN 'admin_assigned' ELSE role_source END,
      previous_role = CASE WHEN p_member <> app_user_id() THEN NULL ELSE previous_role END,
      previous_role_source = CASE WHEN p_member <> app_user_id() THEN NULL ELSE previous_role_source END
    WHERE workspace_id = p_workspace AND user_id = p_member;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'change_role', 'member', p_member,
                      jsonb_build_object('from', v_old, 'to', p_role));
    PERFORM app_drop_unseen_assignees(NULL, p_workspace, p_member, 'role_change', app_ids_from(p_ids, 2));
  END
  $$;


ALTER FUNCTION fvoci.app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_member_role_ids(p_workspace uuid, p_member uuid) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) <> 'owner' THEN
      RETURN 0;
    END IF;
    -- 공간 → 멤버 순서로 공유·그룹 변경·담당 추가를 막고 커밋된 뒤의 상한을 센다(D-88).
    PERFORM app_lock_live_workspace(p_workspace);
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = p_workspace AND user_id IN (app_user_id(), p_member)
    ORDER BY user_id FOR UPDATE;
    IF app_user_id() IS NULL OR app_access('workspace', p_workspace) <> 'owner' THEN
      RETURN 0;
    END IF;
    RETURN 1 + 2 * app_unseen_assignees_bound(NULL, p_workspace, p_member);
  END
  $$;


ALTER FUNCTION fvoci.app_set_member_role_ids(p_workspace uuid, p_member uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_module_description(p_module uuid, p_text text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws uuid := (SELECT workspace_id FROM modules WHERE id = p_module AND deleted_at IS NULL);
  BEGIN
    IF v_ws IS NULL OR app_access('workspace', v_ws) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    IF length(btrim(coalesce(p_text, ''))) > 4000 THEN
      RAISE EXCEPTION 'the description is too long' USING ERRCODE = '22023';
    END IF;
    UPDATE modules SET description = btrim(coalesce(p_text, '')) WHERE id = p_module;
  END
  $$;


ALTER FUNCTION fvoci.app_set_module_description(p_module uuid, p_text text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_module_icon(p_module uuid, p_icon text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws uuid := (SELECT workspace_id FROM modules WHERE id = p_module);
  BEGIN
    IF v_ws IS NULL OR app_access('workspace', v_ws) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    IF length(btrim(coalesce(p_icon, ''))) > 16 THEN
      RAISE EXCEPTION 'the icon is too long' USING ERRCODE = '22023';
    END IF;
    UPDATE modules SET icon = btrim(coalesce(p_icon, '')) WHERE id = p_module;
  END
  $$;


ALTER FUNCTION fvoci.app_set_module_icon(p_module uuid, p_icon text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_trash_retention(p_workspace uuid, p_days integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old integer;
  BEGIN
    IF app_access('workspace', p_workspace) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can change the retention period' USING ERRCODE = '42501';
    END IF;
    IF p_days NOT BETWEEN 1 AND 3650 THEN
      RAISE EXCEPTION 'retention must be 1 to 3650 days' USING ERRCODE = '22023';
    END IF;
    SELECT trash_retention_days INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET trash_retention_days = p_days WHERE id = p_workspace;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'set_trash_retention', 'workspace', p_workspace,
                      jsonb_build_object('from', v_old, 'to', p_days));
  END
  $$;


ALTER FUNCTION fvoci.app_set_trash_retention(p_workspace uuid, p_days integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_version_retention(p_workspace uuid, p_days integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old integer;
  BEGIN
    IF app_access('workspace', p_workspace) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can change version retention' USING ERRCODE = '42501';
    END IF;
    IF p_days IS NOT NULL AND p_days NOT BETWEEN 1 AND 3650 THEN
      RAISE EXCEPTION 'retention must be 1 to 3650 days' USING ERRCODE = '22023';
    END IF;
    SELECT version_retention_days INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET version_retention_days = p_days WHERE id = p_workspace;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'set_version_retention', 'workspace', p_workspace,
                      jsonb_build_object('from', v_old, 'to', p_days));
  END
  $$;


ALTER FUNCTION fvoci.app_set_version_retention(p_workspace uuid, p_days integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_workspace_realtime(p_workspace uuid, p_enabled boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old boolean;
  BEGIN
    IF app_access('workspace', p_workspace) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can change realtime editing' USING ERRCODE = '42501';
    END IF;
    SELECT realtime_enabled INTO v_old FROM workspaces WHERE id = p_workspace FOR UPDATE;
    UPDATE workspaces SET realtime_enabled = p_enabled WHERE id = p_workspace;
    IF v_old IS DISTINCT FROM p_enabled THEN
      PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'set_realtime', 'workspace', p_workspace,
                        jsonb_build_object('enabled', p_enabled));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_set_workspace_realtime(p_workspace uuid, p_enabled boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_item(p_item uuid, p_base_version integer, p_title text, p_description text, p_visibility text, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item       items%ROWTYPE;
    v_access     text;
    v_visibility text;
  BEGIN
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    v_access := app_access('item', p_item);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(v_item.id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 읽은 판이 지났으면 바꾸지 않는다(바꿀 것이 없는 재시도는 그대로 성공).
    IF v_item.version IS DISTINCT FROM p_base_version THEN
      IF v_item.title = btrim(p_title) AND v_item.description = coalesce(p_description, '')
         AND (p_visibility IS NULL OR p_visibility = v_item.visibility) THEN
        RETURN 'updated';
      END IF;
      RETURN 'conflict';
    END IF;
    -- 보내지 않았으면 지금 값 그대로(잠근 뒤의 값).
    v_visibility := coalesce(p_visibility, v_item.visibility);
    IF v_visibility NOT IN ('workspace', 'private') THEN
      RAISE EXCEPTION 'invalid visibility' USING ERRCODE = '22023';
    END IF;
    IF v_visibility <> v_item.visibility AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can change visibility' USING ERRCODE = '42501';
    END IF;
    UPDATE items SET title = btrim(p_title), description = coalesce(p_description, ''),
                     visibility = v_visibility, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_item;
    IF v_item.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'rename', 'item', p_item,
                        jsonb_build_object('from', v_item.title, 'to', btrim(p_title)));
    END IF;
    IF v_item.visibility <> v_visibility THEN
      PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'change_visibility', 'item', p_item,
                        jsonb_build_object('from', v_item.visibility, 'to', v_visibility));
      -- 비공개가 되어 볼 수 없게 된 사람은 그 항목 업무의 담당에서 뺀다.
      PERFORM app_drop_unseen_assignees(p_item, NULL, NULL, 'item_private', app_ids_from(p_ids, 3));
    END IF;
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_update_item(p_item uuid, p_base_version integer, p_title text, p_description text, p_visibility text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_item_ids(p_item uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('item', p_item) = 'none' THEN 0
                ELSE 2 + 2 * app_unseen_assignees_bound(p_item, NULL, NULL) END
  $$;


ALTER FUNCTION fvoci.app_update_item_ids(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_module(p_module uuid, p_name text, p_key text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_mod     modules%ROWTYPE;
    v_key     text := app_normalize_key(p_key);
    v_current text;
    v_owner   uuid;
  BEGIN
    SELECT * INTO v_mod FROM modules WHERE id = p_module FOR UPDATE;
    IF v_mod.id IS NULL OR app_access('workspace', v_mod.workspace_id) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_mod.workspace_id);

    IF v_mod.name <> btrim(p_name) THEN
      UPDATE modules SET name = btrim(p_name) WHERE id = p_module;
      PERFORM app_audit(app_id(p_ids, 1), v_mod.workspace_id, 'rename', 'module', p_module,
                        jsonb_build_object('from', v_mod.name, 'to', btrim(p_name)));
    END IF;

    SELECT key INTO v_current FROM module_keys WHERE module_id = p_module AND retired_at IS NULL;
    IF v_current <> v_key THEN
      SELECT module_id INTO v_owner FROM module_keys WHERE workspace_id = v_mod.workspace_id AND key = v_key;
      -- 주인 없는 예약 키(영구 삭제한 모듈의 것)도 거절한다.
      IF FOUND AND v_owner IS DISTINCT FROM p_module THEN
        RAISE EXCEPTION 'this key is in use or was used before' USING ERRCODE = '55000';
      END IF;
      UPDATE module_keys SET retired_at = now() WHERE module_id = p_module AND retired_at IS NULL;
      IF v_owner = p_module THEN
        -- 같은 모듈의 옛 키를 되찾는다.
        UPDATE module_keys SET retired_at = NULL WHERE workspace_id = v_mod.workspace_id AND key = v_key;
      ELSE
        INSERT INTO module_keys (workspace_id, key, module_id) VALUES (v_mod.workspace_id, v_key, p_module);
      END IF;
      PERFORM app_audit(app_id(p_ids, 2), v_mod.workspace_id, 'change_key', 'module', p_module,
                        jsonb_build_object('from', v_current, 'to', v_key));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_update_module(p_module uuid, p_name text, p_key text, p_ids uuid[]) OWNER TO fvoci_migrator;

ALTER TABLE ONLY fvoci.group_members
    ADD CONSTRAINT group_members_pkey PRIMARY KEY (group_id, user_id);

ALTER TABLE ONLY fvoci.groups
    ADD CONSTRAINT groups_name UNIQUE (workspace_id, name);

ALTER TABLE ONLY fvoci.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.invitations
    ADD CONSTRAINT invitations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_request UNIQUE (module_id, created_by, request_id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_workspace UNIQUE (id, workspace_id);

ALTER TABLE ONLY fvoci.module_keys
    ADD CONSTRAINT module_keys_pkey PRIMARY KEY (workspace_id, key);

ALTER TABLE ONLY fvoci.modules
    ADD CONSTRAINT modules_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.modules
    ADD CONSTRAINT modules_request UNIQUE (workspace_id, created_by, request_id);

ALTER TABLE ONLY fvoci.modules
    ADD CONSTRAINT modules_workspace UNIQUE (id, workspace_id);

ALTER TABLE ONLY fvoci.numbered_targets
    ADD CONSTRAINT numbered_targets_pkey PRIMARY KEY (scope_id, number);

ALTER TABLE ONLY fvoci.numbered_targets
    ADD CONSTRAINT numbered_targets_target UNIQUE (target_kind, target_id);

ALTER TABLE ONLY fvoci.organization_members
    ADD CONSTRAINT organization_members_pkey PRIMARY KEY (organization_id, user_id);

ALTER TABLE ONLY fvoci.organizations
    ADD CONSTRAINT organizations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.organizations
    ADD CONSTRAINT organizations_request UNIQUE (created_by, request_id);

ALTER TABLE ONLY fvoci.ownership_offers
    ADD CONSTRAINT ownership_offers_pkey PRIMARY KEY (workspace_id);

ALTER TABLE ONLY fvoci.reference_counters
    ADD CONSTRAINT reference_counters_pkey PRIMARY KEY (workspace_id, scope);

ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.user_settings
    ADD CONSTRAINT user_settings_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY fvoci.workspace_members
    ADD CONSTRAINT workspace_members_pkey PRIMARY KEY (workspace_id, user_id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_request UNIQUE (created_by, request_id);

CREATE UNIQUE INDEX invitations_open_organization ON fvoci.invitations USING btree (organization_id, email) WHERE ((closed_at IS NULL) AND (workspace_id IS NULL));

CREATE UNIQUE INDEX invitations_open_workspace ON fvoci.invitations USING btree (workspace_id, email) WHERE ((closed_at IS NULL) AND (workspace_id IS NOT NULL));

CREATE INDEX item_members_by_user ON fvoci.item_members USING btree (user_id) WHERE (user_id IS NOT NULL);

CREATE UNIQUE INDEX item_members_group ON fvoci.item_members USING btree (item_id, group_id) WHERE (group_id IS NOT NULL);

CREATE UNIQUE INDEX item_members_user ON fvoci.item_members USING btree (item_id, user_id) WHERE (user_id IS NOT NULL);

CREATE INDEX items_module ON fvoci.items USING btree (module_id);

CREATE INDEX items_trash ON fvoci.items USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX items_workspace_updated ON fvoci.items USING btree (workspace_id, updated_at DESC);

CREATE UNIQUE INDEX module_keys_current ON fvoci.module_keys USING btree (module_id) WHERE (retired_at IS NULL);

CREATE INDEX organization_members_user ON fvoci.organization_members USING btree (user_id);

CREATE INDEX trash_batches_owner ON fvoci.trash_batches USING btree (owner_user_id, deleted_at) WHERE ((restored_at IS NULL) AND (purged_at IS NULL));

CREATE INDEX trash_batches_workspace ON fvoci.trash_batches USING btree (workspace_id, deleted_at) WHERE ((restored_at IS NULL) AND (purged_at IS NULL));

CREATE INDEX workspace_members_user ON fvoci.workspace_members USING btree (user_id);

CREATE INDEX workspaces_deleted ON fvoci.workspaces USING btree (deleted_at) WHERE (deleted_at IS NOT NULL);

CREATE INDEX workspaces_organization ON fvoci.workspaces USING btree (organization_id, created_at);

CREATE TRIGGER items_course BEFORE DELETE ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_delete_item_course_rows();

CREATE TRIGGER items_files BEFORE DELETE ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_delete_attached_files('item');

CREATE TRIGGER items_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, visibility, archived_at, updated_at ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('item');

CREATE TRIGGER items_search_children AFTER UPDATE OF deleted_at ON fvoci.items FOR EACH ROW WHEN ((old.deleted_at IS DISTINCT FROM new.deleted_at)) EXECUTE FUNCTION fvoci.app_search_enqueue_item_children();

CREATE TRIGGER items_versions BEFORE UPDATE ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_items_versions();

CREATE TRIGGER module_keys_search AFTER INSERT OR UPDATE ON fvoci.module_keys FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue_module();

ALTER TABLE ONLY fvoci.group_members
    ADD CONSTRAINT group_members_group_id_fkey FOREIGN KEY (group_id) REFERENCES fvoci.groups(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.group_members
    ADD CONSTRAINT group_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.groups
    ADD CONSTRAINT groups_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.invitations
    ADD CONSTRAINT invitations_accepted_by_fkey FOREIGN KEY (accepted_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.invitations
    ADD CONSTRAINT invitations_invited_by_fkey FOREIGN KEY (invited_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.invitations
    ADD CONSTRAINT invitations_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);
ALTER TABLE ONLY fvoci.invitations
    ADD CONSTRAINT invitations_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.item_members
    ADD CONSTRAINT item_members_added_by_fkey FOREIGN KEY (added_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.item_members
    ADD CONSTRAINT item_members_group_id_fkey FOREIGN KEY (group_id) REFERENCES fvoci.groups(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.item_members
    ADD CONSTRAINT item_members_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.item_members
    ADD CONSTRAINT item_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_leader_id_fkey FOREIGN KEY (leader_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_module_id_fkey FOREIGN KEY (module_id) REFERENCES fvoci.modules(id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_module_workspace FOREIGN KEY (module_id, workspace_id) REFERENCES fvoci.modules(id, workspace_id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.items
    ADD CONSTRAINT items_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.module_keys
    ADD CONSTRAINT module_keys_module_id_fkey FOREIGN KEY (module_id) REFERENCES fvoci.modules(id);

ALTER TABLE ONLY fvoci.module_keys
    ADD CONSTRAINT module_keys_module_workspace FOREIGN KEY (module_id, workspace_id) REFERENCES fvoci.modules(id, workspace_id);

ALTER TABLE ONLY fvoci.module_keys
    ADD CONSTRAINT module_keys_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.modules
    ADD CONSTRAINT modules_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.modules
    ADD CONSTRAINT modules_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.modules
    ADD CONSTRAINT modules_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);
ALTER TABLE ONLY fvoci.organization_members
    ADD CONSTRAINT organization_members_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);
ALTER TABLE ONLY fvoci.organization_members
    ADD CONSTRAINT organization_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.organizations
    ADD CONSTRAINT organizations_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.numbered_targets
    ADD CONSTRAINT numbered_targets_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.ownership_offers
    ADD CONSTRAINT ownership_offers_from_user_fkey FOREIGN KEY (from_user) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.ownership_offers
    ADD CONSTRAINT ownership_offers_to_user_fkey FOREIGN KEY (to_user) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.ownership_offers
    ADD CONSTRAINT ownership_offers_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.reference_counters
    ADD CONSTRAINT reference_counters_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);


ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_deleted_by_fkey FOREIGN KEY (deleted_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.user_settings
    ADD CONSTRAINT user_settings_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);


ALTER TABLE ONLY fvoci.user_settings
    ADD CONSTRAINT user_settings_last_organization_id_fkey FOREIGN KEY (last_organization_id) REFERENCES fvoci.organizations(id) ON DELETE SET NULL;

ALTER TABLE ONLY fvoci.workspace_members
    ADD CONSTRAINT workspace_members_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.workspace_members
    ADD CONSTRAINT workspace_members_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_deleted_by_fkey FOREIGN KEY (deleted_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);
ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_owner_id_fkey FOREIGN KEY (owner_id) REFERENCES fvoci.users(id);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
