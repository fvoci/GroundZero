-- GroundZero 새 설치 기준(2026-10-25 오버홀 1단계, D-126). 역할·스키마는 먼저 db bootstrap으로 준비한다.
-- PostgreSQL 17 이상에서 환경 공통 SQL을 쓴다(D-100: docs/decisions/database.md#d-100).
-- 기존 개발·시험 DB는 대상·백업·삭제 승인을 거쳐 재생성한다(D-107: docs/decisions/database.md#d-107).
-- 비교용 과거 기준 SQL: 355fe26407b91b5b560a4f13dd34981f74c0ee6f의
-- crates/server/migrations/202610230001_initial_state.sql과 202610250001_user_settings_theme.sql(여기에 합침).
--
-- 계층은 서버 › 조직 › 워크스페이스 › 모듈 › 항목이고(D-111) 접근 등급은 app_access(kind, id) 하나가 계산한다(D-113).
-- D-08·D-88: 소유자는 fvoci_migrator, 앱 접속은 소유권·BYPASSRLS 없는 fvoci_app이다.
-- SECURITY DEFINER는 소유자 권한으로 불변식을 지키므로 search_path를 fvoci, pg_temp로 고정하고
-- PUBLIC 실행 권한을 회수한다. 사용자 권한·소유 범위는 함수와 RLS에서 확인한다.
-- 트리거 전용·내부 함수의 실행 권한도 마지막 ACL에서 제한한다.
-- 순수 도우미 app_local_instant·app_normalize_key의 PUBLIC 실행은 기존 계약대로 유지한다.
--
-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

ALTER SCHEMA fvoci OWNER TO fvoci_migrator;

-- 테이블 CHECK가 사용하는 순수 도우미.

CREATE FUNCTION fvoci.app_web_url_ok(p_url text) RETURNS boolean
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $_$
    SELECT coalesce(p_url, '') = '' OR p_url ~* '^https?://[^[:space:]]+$'
  $_$;


ALTER FUNCTION fvoci.app_web_url_ok(p_url text) OWNER TO fvoci_migrator;

-- 테이블·행 타입. 새 UUID는 앱이 공급하며 최종 기본값·CHECK를 처음부터 적용한다.
-- 필수 초기 행은 없으며 개발 예제 seed는 별도다.

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

CREATE TABLE fvoci.checklist_items (
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    text text NOT NULL,
    checked boolean DEFAULT false NOT NULL,
    "position" numeric NOT NULL,
    request_id uuid NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT checklist_items_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT checklist_items_text_check CHECK ((length(btrim(text)) > 0))
);


ALTER TABLE fvoci.checklist_items OWNER TO fvoci_migrator;

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

CREATE TABLE fvoci.course_details (
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    subject text DEFAULT ''::text NOT NULL,
    instructor text DEFAULT ''::text NOT NULL,
    platform text DEFAULT ''::text NOT NULL,
    url text DEFAULT ''::text NOT NULL,
    textbook text DEFAULT ''::text NOT NULL,
    starts_on date,
    expires_on date,
    target_on date,
    updated_by uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT course_details_url CHECK (fvoci.app_web_url_ok(url))
);


ALTER TABLE fvoci.course_details OWNER TO fvoci_migrator;

CREATE TABLE fvoci.course_episodes (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    number integer NOT NULL,
    title text NOT NULL,
    chapter text DEFAULT ''::text NOT NULL,
    duration_seconds integer DEFAULT 0 NOT NULL,
    url text DEFAULT ''::text NOT NULL,
    materials text DEFAULT ''::text NOT NULL,
    planned_on date,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    CONSTRAINT course_episodes_duration_seconds_check CHECK (((duration_seconds >= 0) AND (duration_seconds <= 360000))),
    CONSTRAINT course_episodes_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT course_episodes_number_check CHECK ((number > 0)),
    CONSTRAINT course_episodes_title_check CHECK (((length(btrim(title)) >= 1) AND (length(btrim(title)) <= 300))),
    CONSTRAINT course_episodes_url CHECK (fvoci.app_web_url_ok(url)),
    CONSTRAINT course_episodes_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.course_episodes OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_collab_file_refs (
    document_id uuid NOT NULL,
    file_id uuid NOT NULL,
    version integer NOT NULL
);


ALTER TABLE fvoci.document_collab_file_refs OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_collab_states (
    document_id uuid NOT NULL,
    revision integer NOT NULL,
    state bytea NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    pending boolean DEFAULT false NOT NULL,
    editors uuid[] DEFAULT '{}'::uuid[] NOT NULL
);


ALTER TABLE fvoci.document_collab_states OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_file_refs (
    document_id uuid NOT NULL,
    revision integer NOT NULL,
    file_id uuid NOT NULL,
    version integer NOT NULL
);


ALTER TABLE fvoci.document_file_refs OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_links (
    document_id uuid NOT NULL,
    target_kind text NOT NULL,
    target_id uuid NOT NULL,
    CONSTRAINT document_links_target_kind_check CHECK ((target_kind = ANY (ARRAY['item'::text, 'document'::text, 'task'::text])))
);


ALTER TABLE fvoci.document_links OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_mentions (
    document_id uuid NOT NULL,
    user_id uuid NOT NULL,
    mentioned_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.document_mentions OWNER TO fvoci_migrator;

CREATE TABLE fvoci.document_revisions (
    document_id uuid NOT NULL,
    revision integer NOT NULL,
    title text NOT NULL,
    schema_version integer NOT NULL,
    body jsonb NOT NULL,
    author uuid NOT NULL,
    request_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    restored_from integer,
    edited_by uuid[],
    CONSTRAINT document_revisions_restored_from_check CHECK ((restored_from > 0)),
    CONSTRAINT document_revisions_revision_check CHECK ((revision > 0))
);


ALTER TABLE fvoci.document_revisions OWNER TO fvoci_migrator;

-- 개별 공유(D-23·D-49·D-114). 받는 사람은 조직 멤버이고 공유 행에 조직을 기록한다. 둘 중 하나가 그 조직에서 빠지면 철회한다.
CREATE TABLE fvoci.document_shares (
    id uuid NOT NULL,
    document_id uuid NOT NULL,
    organization_id uuid NOT NULL,
    grantee_user_id uuid NOT NULL,
    permission text NOT NULL,
    granted_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT document_shares_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT document_shares_permission_check CHECK ((permission = ANY (ARRAY['read'::text, 'write'::text])))
);


ALTER TABLE fvoci.document_shares OWNER TO fvoci_migrator;

CREATE TABLE fvoci.documents (
    id uuid NOT NULL,
    workspace_id uuid,
    placement text NOT NULL,
    scope text NOT NULL,
    number bigint,
    title text NOT NULL,
    schema_version integer NOT NULL,
    body jsonb NOT NULL,
    revision integer NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    owner_user_id uuid,
    visibility text DEFAULT 'workspace'::text NOT NULL,
    allow_copy boolean DEFAULT false NOT NULL,
    item_id uuid,
    parent_id uuid,
    "position" numeric DEFAULT 0 NOT NULL,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    is_template boolean DEFAULT false NOT NULL,
    CONSTRAINT documents_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT documents_module_shape CHECK (((placement = 'module'::text) = (item_id IS NOT NULL))),
    CONSTRAINT documents_one_owner CHECK (((workspace_id IS NULL) <> (owner_user_id IS NULL))),
    CONSTRAINT documents_personal_shape CHECK ((((placement = 'personal'::text) = (owner_user_id IS NOT NULL)) AND ((placement = 'personal'::text) = (number IS NULL)))),
    CONSTRAINT documents_placement_check CHECK ((placement = ANY (ARRAY['wiki'::text, 'module'::text, 'personal'::text]))),
    CONSTRAINT documents_revision_check CHECK ((revision > 0)),
    CONSTRAINT documents_schema_version_check CHECK ((schema_version > 0)),
    CONSTRAINT documents_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT documents_visibility_check CHECK ((visibility = ANY (ARRAY['workspace'::text, 'private'::text])))
);


ALTER TABLE fvoci.documents OWNER TO fvoci_migrator;

CREATE TABLE fvoci.episode_progress (
    episode_id uuid NOT NULL,
    user_id uuid NOT NULL,
    status text DEFAULT 'not_started'::text NOT NULL,
    planned_on date,
    completed_on date,
    watch_count integer DEFAULT 0 NOT NULL,
    last_position_seconds integer,
    note text DEFAULT ''::text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT episode_progress_last_position_seconds_check CHECK ((last_position_seconds >= 0)),
    CONSTRAINT episode_progress_status_check CHECK ((status = ANY (ARRAY['not_started'::text, 'in_progress'::text, 'done'::text]))),
    CONSTRAINT episode_progress_watch_count_check CHECK ((watch_count >= 0))
);


ALTER TABLE fvoci.episode_progress OWNER TO fvoci_migrator;

CREATE TABLE fvoci.events (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid NOT NULL,
    kind text NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    all_day boolean NOT NULL,
    start_date date,
    end_date date,
    starts_at timestamp with time zone,
    ends_at timestamp with time zone,
    timezone text NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    deleted_by uuid,
    request_id uuid NOT NULL,
    recurrence jsonb,
    excluded_dates date[] DEFAULT '{}'::date[] NOT NULL,
    trash_batch_id uuid,
    CONSTRAINT events_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT events_kind_check CHECK ((kind = ANY (ARRAY['meeting'::text, 'presentation'::text, 'event'::text]))),
    CONSTRAINT events_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT events_version_check CHECK ((version > 0)),
    CONSTRAINT events_when CHECK (((all_day AND (start_date IS NOT NULL) AND (end_date IS NOT NULL) AND (start_date <= end_date) AND (starts_at IS NULL) AND (ends_at IS NULL)) OR ((NOT all_day) AND (starts_at IS NOT NULL) AND (ends_at IS NOT NULL) AND (starts_at <= ends_at) AND (start_date IS NULL) AND (end_date IS NULL))))
);


ALTER TABLE fvoci.events OWNER TO fvoci_migrator;

CREATE TABLE fvoci.file_versions (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    version integer NOT NULL,
    size bigint NOT NULL,
    sha256 text NOT NULL,
    content_type text NOT NULL,
    storage_key text NOT NULL,
    note text DEFAULT ''::text NOT NULL,
    uploaded_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT file_versions_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT file_versions_sha256_check CHECK ((sha256 ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT file_versions_size_check CHECK ((size >= 0)),
    CONSTRAINT file_versions_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.file_versions OWNER TO fvoci_migrator;

CREATE TABLE fvoci.files (
    id uuid NOT NULL,
    workspace_id uuid,
    owner_user_id uuid,
    document_id uuid,
    item_id uuid,
    name text NOT NULL,
    current_version integer DEFAULT 0 NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    CONSTRAINT files_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT files_name_check CHECK ((((length(btrim(name)) >= 1) AND (length(btrim(name)) <= 255)) AND (name !~ '[\x00-\x1f/\\]'::text))),
    CONSTRAINT files_place CHECK (((workspace_id IS NULL) <> (owner_user_id IS NULL))),
    CONSTRAINT files_target CHECK (((document_id IS NULL) <> (item_id IS NULL)))
);


ALTER TABLE fvoci.files OWNER TO fvoci_migrator;

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

CREATE TABLE fvoci.instance_state (
    key text NOT NULL,
    value jsonb DEFAULT '{}'::jsonb NOT NULL,
    set_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.instance_state OWNER TO fvoci_migrator;

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

CREATE TABLE fvoci.item_records (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    kind text NOT NULL,
    title text NOT NULL,
    occurred_on date,
    body text DEFAULT ''::text NOT NULL,
    fields jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_by uuid NOT NULL,
    updated_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    request_id uuid NOT NULL,
    CONSTRAINT item_records_body_check CHECK ((length(body) <= 20000)),
    CONSTRAINT item_records_fields_check CHECK ((jsonb_typeof(fields) = 'object'::text)),
    CONSTRAINT item_records_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT item_records_kind_check CHECK ((kind ~ '^[a-z_]{2,32}$'::text)),
    CONSTRAINT item_records_title_check CHECK (((length(btrim(title)) >= 1) AND (length(btrim(title)) <= 300)))
);


ALTER TABLE fvoci.item_records OWNER TO fvoci_migrator;

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

CREATE TABLE fvoci.oidc_login_states (
    secret_hash bytea NOT NULL,
    state text NOT NULL,
    pkce_verifier text NOT NULL,
    nonce text NOT NULL,
    return_to text NOT NULL,
    expires_at timestamp with time zone NOT NULL
);


ALTER TABLE fvoci.oidc_login_states OWNER TO fvoci_migrator;

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

CREATE TABLE fvoci.public_links (
    id uuid NOT NULL,
    document_id uuid NOT NULL,
    token_hash bytea NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone,
    revoked_at timestamp with time zone,
    CONSTRAINT public_links_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.public_links OWNER TO fvoci_migrator;

CREATE TABLE fvoci.record_files (
    record_id uuid NOT NULL,
    "position" integer NOT NULL,
    role text NOT NULL,
    file_id uuid,
    version_id uuid,
    file_name text NOT NULL,
    version integer NOT NULL,
    sha256 text NOT NULL,
    size bigint NOT NULL,
    CONSTRAINT record_files_position_check CHECK (("position" >= 0)),
    CONSTRAINT record_files_role_check CHECK ((role = ANY (ARRAY['input'::text, 'code'::text, 'result'::text, 'reference'::text, 'other'::text])))
);


ALTER TABLE fvoci.record_files OWNER TO fvoci_migrator;

CREATE TABLE fvoci.reference_counters (
    workspace_id uuid NOT NULL,
    scope text NOT NULL,
    next_number bigint NOT NULL,
    CONSTRAINT reference_counters_next_number_check CHECK ((next_number > 0))
);


ALTER TABLE fvoci.reference_counters OWNER TO fvoci_migrator;

CREATE TABLE fvoci.saved_views (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    item_id uuid NOT NULL,
    name text NOT NULL,
    config jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    updated_request_id uuid,
    CONSTRAINT saved_views_config_check CHECK (((jsonb_typeof(config) = 'object'::text) AND (length((config)::text) <= 4000))),
    CONSTRAINT saved_views_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT saved_views_name_check CHECK (((length(btrim(name)) >= 1) AND (length(btrim(name)) <= 100)))
);


ALTER TABLE fvoci.saved_views OWNER TO fvoci_migrator;

CREATE TABLE fvoci.scheduled_runs (
    name text NOT NULL,
    last_run_at timestamp with time zone NOT NULL
);


ALTER TABLE fvoci.scheduled_runs OWNER TO fvoci_migrator;

CREATE TABLE fvoci.search_queue (
    kind text NOT NULL,
    target_id uuid NOT NULL,
    queued_at timestamp with time zone DEFAULT now() NOT NULL,
    claimed_until timestamp with time zone,
    attempts integer DEFAULT 0 NOT NULL,
    task_uid bigint,
    task_queued_at timestamp with time zone,
    CONSTRAINT search_queue_kind_check CHECK ((kind = ANY (ARRAY['document'::text, 'task'::text, 'item'::text, 'event'::text, 'file'::text, 'record'::text]))),
    CONSTRAINT search_queue_task_pair CHECK (((task_uid IS NULL) = (task_queued_at IS NULL)))
);


ALTER TABLE fvoci.search_queue OWNER TO fvoci_migrator;

CREATE TABLE fvoci.sessions (
    id uuid NOT NULL,
    token_hash bytea NOT NULL,
    user_id uuid NOT NULL,
    id_token text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    CONSTRAINT sessions_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.sessions OWNER TO fvoci_migrator;

-- 서버 관리자(D-112). 운영자가 지정한 OIDC 식별자의 사용자를 시드하며 조직 생성·목록·서버 로그 같은 메타데이터만 다룬다.
CREATE TABLE fvoci.server_admins (
    user_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    detail jsonb DEFAULT '{}'::jsonb NOT NULL
);

CREATE TABLE fvoci.storage_deletions (
    storage_key text NOT NULL,
    storage_upload_id text DEFAULT ''::text NOT NULL,
    queued_at timestamp with time zone DEFAULT now() NOT NULL,
    attempts integer DEFAULT 0 NOT NULL
);


ALTER TABLE fvoci.server_admins OWNER TO fvoci_migrator;

ALTER TABLE fvoci.storage_deletions OWNER TO fvoci_migrator;

CREATE TABLE fvoci.study_units (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    number integer NOT NULL,
    title text NOT NULL,
    chapter text DEFAULT ''::text NOT NULL,
    unit text DEFAULT 'pages'::text NOT NULL,
    amount integer DEFAULT 0 NOT NULL,
    planned_on date,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    CONSTRAINT study_units_amount_check CHECK (((amount >= 0) AND (amount <= 1000000))),
    CONSTRAINT study_units_chapter_check CHECK ((length(chapter) <= 300)),
    CONSTRAINT study_units_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT study_units_number_check CHECK ((number > 0)),
    CONSTRAINT study_units_title_check CHECK (((length(btrim(title)) >= 1) AND (length(btrim(title)) <= 300))),
    CONSTRAINT study_units_unit_check CHECK ((unit = ANY (ARRAY['pages'::text, 'problems'::text, 'minutes'::text, 'items'::text]))),
    CONSTRAINT study_units_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.study_units OWNER TO fvoci_migrator;

CREATE TABLE fvoci.task_assignees (
    task_id uuid NOT NULL,
    user_id uuid NOT NULL,
    assigned_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


ALTER TABLE fvoci.task_assignees OWNER TO fvoci_migrator;

CREATE TABLE fvoci.task_dependencies (
    predecessor_id uuid NOT NULL,
    successor_id uuid NOT NULL,
    item_id uuid NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    kind text DEFAULT 'FS'::text NOT NULL,
    lag_days integer DEFAULT 0 NOT NULL,
    CONSTRAINT task_dependencies_distinct CHECK ((predecessor_id <> successor_id)),
    CONSTRAINT task_dependencies_kind CHECK ((kind = ANY (ARRAY['FS'::text, 'SS'::text, 'FF'::text, 'SF'::text]))),
    CONSTRAINT task_dependencies_lag CHECK (((lag_days >= 0) AND (lag_days <= 3650)))
);


ALTER TABLE fvoci.task_dependencies OWNER TO fvoci_migrator;

CREATE TABLE fvoci.task_series (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid NOT NULL,
    root_task_id uuid NOT NULL,
    rule jsonb,
    timezone text NOT NULL,
    anchor date NOT NULL,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    effective_from date NOT NULL,
    skipped_dates date[] DEFAULT '{}'::date[] NOT NULL,
    CONSTRAINT task_series_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.task_series OWNER TO fvoci_migrator;

-- 항목별 업무 상태 정의(D-116). 범주 todo·doing·done만 집계·알림·간트가 본다. 새 항목은 기본 세 상태로 시작한다.
CREATE TABLE fvoci.task_statuses (
    id uuid NOT NULL,
    item_id uuid NOT NULL,
    name text NOT NULL,
    color text DEFAULT ''::text NOT NULL,
    "position" numeric NOT NULL,
    category text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT task_statuses_category_check CHECK ((category = ANY (ARRAY['todo'::text, 'doing'::text, 'done'::text]))),
    CONSTRAINT task_statuses_color_check CHECK (((color = ''::text) OR (color ~ '^#[0-9a-f]{6}$'::text))),
    CONSTRAINT task_statuses_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT task_statuses_name_check CHECK (((name = btrim(name)) AND (length(name) >= 1) AND (length(name) <= 50)))
);

CREATE TABLE fvoci.tasks (
    id uuid NOT NULL,
    workspace_id uuid NOT NULL,
    item_id uuid NOT NULL,
    parent_id uuid,
    number bigint NOT NULL,
    title text NOT NULL,
    description text DEFAULT ''::text NOT NULL,
    status_id uuid NOT NULL,
    priority text DEFAULT 'none'::text NOT NULL,
    start_date date,
    due_date date,
    estimate_minutes integer,
    tags text[] DEFAULT '{}'::text[] NOT NULL,
    "position" numeric DEFAULT 0 NOT NULL,
    version integer DEFAULT 1 NOT NULL,
    completed_at timestamp with time zone,
    created_by uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_by uuid NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    request_id uuid NOT NULL,
    milestone boolean DEFAULT false NOT NULL,
    series_id uuid,
    occurrence_date date,
    deleted_at timestamp with time zone,
    trash_batch_id uuid,
    move_version integer DEFAULT 0 NOT NULL,
    CONSTRAINT tasks_dates CHECK (((start_date IS NULL) OR (due_date IS NULL) OR (start_date <= due_date))),
    CONSTRAINT tasks_estimate_minutes_check CHECK ((estimate_minutes >= 0)),
    CONSTRAINT tasks_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT tasks_milestone_single_date CHECK (((NOT milestone) OR (start_date IS NULL))),
    CONSTRAINT tasks_priority_check CHECK ((priority = ANY (ARRAY['none'::text, 'low'::text, 'medium'::text, 'high'::text, 'urgent'::text]))),
    CONSTRAINT tasks_series_occurrence CHECK (((series_id IS NULL) = (occurrence_date IS NULL))),
    CONSTRAINT tasks_title_check CHECK ((length(btrim(title)) > 0)),
    CONSTRAINT tasks_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.task_statuses OWNER TO fvoci_migrator;

ALTER TABLE fvoci.tasks OWNER TO fvoci_migrator;

CREATE TABLE fvoci.time_entries (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    started_at timestamp with time zone NOT NULL,
    ended_at timestamp with time zone,
    paused_at timestamp with time zone,
    paused_seconds integer DEFAULT 0 NOT NULL,
    kind text DEFAULT 'work'::text NOT NULL,
    memo text DEFAULT ''::text NOT NULL,
    target_kind text,
    target_id uuid,
    workspace_id uuid,
    version integer DEFAULT 1 NOT NULL,
    request_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    CONSTRAINT time_entries_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT time_entries_kind_check CHECK ((kind = ANY (ARRAY['work'::text, 'study'::text, 'lecture'::text, 'hobby'::text, 'rest'::text, 'other'::text]))),
    CONSTRAINT time_entries_order CHECK (((ended_at IS NULL) OR (ended_at >= started_at))),
    CONSTRAINT time_entries_pause CHECK (((paused_at IS NULL) OR (ended_at IS NULL))),
    CONSTRAINT time_entries_paused_seconds_check CHECK ((paused_seconds >= 0)),
    CONSTRAINT time_entries_target CHECK (((target_kind IS NULL) = (target_id IS NULL))),
    CONSTRAINT time_entries_target_kind_check CHECK ((target_kind = ANY (ARRAY['task'::text, 'document'::text, 'item'::text, 'episode'::text, 'study_unit'::text]))),
    CONSTRAINT time_entries_version_check CHECK ((version > 0))
);


ALTER TABLE fvoci.time_entries OWNER TO fvoci_migrator;

-- 시작·수동 기록·이어가기의 요청을 같은 사용자별 키로 기억한다. 이어가기도 최초 기록을 재시도 결과로 돌려준다.
-- 내부 멱등 대장이며 앱 역할의 직접 접근은 허용하지 않는다.
CREATE TABLE fvoci.time_entry_requests (
    user_id uuid NOT NULL,
    request_id uuid NOT NULL,
    entry_id uuid NOT NULL
);

ALTER TABLE fvoci.time_entry_requests OWNER TO fvoci_migrator;

CREATE TABLE fvoci.time_entry_revisions (
    id uuid NOT NULL,
    entry_id uuid NOT NULL,
    user_id uuid NOT NULL,
    started_at timestamp with time zone NOT NULL,
    ended_at timestamp with time zone,
    paused_seconds integer NOT NULL,
    kind text NOT NULL,
    memo text NOT NULL,
    target_kind text,
    target_id uuid,
    changed_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT time_entry_revisions_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.time_entry_revisions OWNER TO fvoci_migrator;



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

CREATE TABLE fvoci.unit_progress (
    unit_id uuid NOT NULL,
    user_id uuid NOT NULL,
    status text DEFAULT 'not_started'::text NOT NULL,
    done_amount integer DEFAULT 0 NOT NULL,
    planned_on date,
    completed_on date,
    note text DEFAULT ''::text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT unit_progress_done_amount_check CHECK (((done_amount >= 0) AND (done_amount <= 1000000))),
    CONSTRAINT unit_progress_note_check CHECK ((length(note) <= 20000)),
    CONSTRAINT unit_progress_status_check CHECK ((status = ANY (ARRAY['not_started'::text, 'in_progress'::text, 'done'::text])))
);


ALTER TABLE fvoci.unit_progress OWNER TO fvoci_migrator;

CREATE TABLE fvoci.upload_parts (
    upload_id uuid NOT NULL,
    number integer NOT NULL,
    size bigint NOT NULL,
    sha256 text NOT NULL,
    etag text NOT NULL,
    CONSTRAINT upload_parts_number_check CHECK (((number >= 1) AND (number <= 10000))),
    CONSTRAINT upload_parts_sha256_check CHECK ((sha256 ~ '^[0-9a-f]{64}$'::text)),
    CONSTRAINT upload_parts_size_check CHECK ((size > 0))
);


ALTER TABLE fvoci.upload_parts OWNER TO fvoci_migrator;

CREATE TABLE fvoci.uploads (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    request_id uuid NOT NULL,
    file_id uuid,
    target_kind text,
    target_id uuid,
    workspace_id uuid,
    owner_user_id uuid,
    name text NOT NULL,
    note text DEFAULT ''::text NOT NULL,
    content_type text NOT NULL,
    size bigint NOT NULL,
    chunk_bytes bigint NOT NULL,
    storage_key text NOT NULL,
    storage_upload_id text,
    state text DEFAULT 'uploading'::text NOT NULL,
    state_changed_at timestamp with time zone DEFAULT now() NOT NULL,
    failure text,
    version_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    CONSTRAINT uploads_chunk_bytes_check CHECK ((chunk_bytes > 0)),
    CONSTRAINT uploads_id_uuidv7 CHECK ((uuid_extract_version(id) = 7)),
    CONSTRAINT uploads_size_check CHECK ((size >= 0)),
    CONSTRAINT uploads_state_check CHECK ((state = ANY (ARRAY['uploading'::text, 'completing'::text, 'done'::text, 'aborted'::text, 'expired'::text, 'failed'::text]))),
    CONSTRAINT uploads_target CHECK (((file_id IS NULL) = ((target_kind IS NOT NULL) AND (target_id IS NOT NULL)))),
    CONSTRAINT uploads_target_kind_check CHECK ((target_kind = ANY (ARRAY['document'::text, 'item'::text])))
);


ALTER TABLE fvoci.uploads OWNER TO fvoci_migrator;

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

CREATE TABLE fvoci.users (
    id uuid NOT NULL,
    issuer text NOT NULL,
    subject text NOT NULL,
    email text,
    display_name text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_login_at timestamp with time zone DEFAULT now() NOT NULL,
    email_verified boolean DEFAULT false NOT NULL,
    CONSTRAINT users_id_uuidv7 CHECK ((uuid_extract_version(id) = 7))
);


ALTER TABLE fvoci.users OWNER TO fvoci_migrator;

-- 워크스페이스 역할(D-20·D-112): 관리자·멤버·읽기 전용. 읽기 전용은 댓글을 포함해 쓰지 않는다. 소유자는 workspaces.owner_id다.
CREATE TABLE fvoci.workspace_members (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT workspace_members_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'member'::text, 'readonly'::text])))
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

-- 함수. 행 타입은 위에서 준비했고 각 최종 서명·본문을 한 번만 정의한다.

CREATE FUNCTION fvoci.app_abort_upload(p_upload uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_up uploads%ROWTYPE;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_up.state <> 'uploading' THEN
      RAISE EXCEPTION 'the upload is no longer open' USING ERRCODE = '55000';
    END IF;
    UPDATE uploads SET state = 'aborted', state_changed_at = now() WHERE id = p_upload;
    DELETE FROM upload_parts WHERE upload_id = p_upload;
    IF v_up.storage_upload_id IS NOT NULL THEN
      INSERT INTO storage_deletions (storage_key, storage_upload_id) VALUES (v_up.storage_key, v_up.storage_upload_id)
      ON CONFLICT DO NOTHING;
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_abort_upload(p_upload uuid) OWNER TO fvoci_migrator;

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
    UPDATE workspace_members SET role = 'admin' WHERE workspace_id = p_workspace AND user_id = v_user;
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
    -- 역할이 개별 공유의 상한이다(D-20): 읽기 전용 멤버는 수정 공유를 받았어도 읽기까지다.
    IF v_role = 'readonly' AND v_access = 'write' THEN
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

-- 공유·공개 링크·설정 관리는 지금 등급이 owner인 사람만 한다(D-113: 등급 하나). 보관된 항목의 문서는 read로 낮아지지만
-- 워크스페이스 관리자는 보관 중에도 관리할 수 있다(개정 2026-10-26 ⑯, "보관 중 관리자 예외" 복원; 등급 위에 얹는 추가 조건).
CREATE FUNCTION fvoci.app_document_can_manage(p_doc uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
    v_doc    documents%ROWTYPE;
  BEGIN
    -- 문서 관리(공유·링크·설정)는 owner 등급 하나다(D-113·D-115). 404/보관 409/403의 구분은 app_require_document와 같다.
    IF v_access = 'owner' THEN
      RETURN true;
    END IF;
    -- 개정 2026-10-26(⑯, D-113·D-106): 보관된 항목의 문서는 등급이 read로 낮아지지만, 워크스페이스 관리자(보관 전 owner 등급)는
    -- 보관 해제 없이 공유·공개 링크·설정을 바꿀 수 있다. 내용 편집·댓글·상태 변경은 보관 중 409 그대로(app_require_document).
    IF v_access <> 'none' THEN
      SELECT * INTO v_doc FROM documents WHERE id = p_doc;
      IF v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id)
         AND app_workspace_role(v_doc.workspace_id, app_user_id()) = 'admin' THEN
        RETURN true;
      END IF;
    END IF;
    RETURN false;
  END
  $$;

ALTER FUNCTION fvoci.app_document_can_manage(p_doc uuid) OWNER TO fvoci_migrator;

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

-- 개별 공유(D-23·D-49·D-114). 받는 사람은 이메일이 검증된 사용자이자 같은 조직 멤버다. 공간 문서는 그 공간의 조직,
-- 개인 노트는 둘이 함께 속한 조직(둘 이상이면 p_organization으로 고른다)을 공유 행에 적는다. 역할이 상한이다(D-20).
CREATE FUNCTION fvoci.app_share_document(p_doc uuid, p_email text, p_permission text, p_organization uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc     documents%ROWTYPE;
    v_email   text := lower(btrim(p_email));
    v_grantee uuid;
    v_org     uuid;
    v_common  uuid[];
    v_role    text;
    v_id      uuid;
  BEGIN
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    IF p_permission NOT IN ('read', 'write') THEN
      RAISE EXCEPTION 'invalid permission' USING ERRCODE = '22023';
    END IF;
    IF position('@' in v_email) < 2 THEN
      RAISE EXCEPTION 'invalid email' USING ERRCODE = '22023';
    END IF;
    SELECT u.id INTO v_grantee FROM users u WHERE u.email_verified AND lower(u.email) = v_email
    ORDER BY u.created_at LIMIT 1;
    IF v_grantee IS NULL THEN
      RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
    END IF;
    IF v_grantee = app_user_id() THEN
      RAISE EXCEPTION 'you already own this document' USING ERRCODE = '22023';
    END IF;
    IF v_doc.owner_user_id IS NOT NULL THEN
      v_common := ARRAY(SELECT o1.organization_id FROM organization_members o1
                        JOIN organization_members o2 ON o2.organization_id = o1.organization_id AND o2.user_id = v_grantee
                        WHERE o1.user_id = v_doc.owner_user_id
                          AND (p_organization IS NULL OR o1.organization_id = p_organization)
                        ORDER BY o1.organization_id);
      IF cardinality(v_common) = 0 THEN
        RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
      END IF;
      IF cardinality(v_common) > 1 THEN
        RAISE EXCEPTION 'choose the organization for this share' USING ERRCODE = '22023';
      END IF;
      v_org := v_common[1];
    ELSE
      SELECT organization_id INTO v_org FROM workspaces WHERE id = v_doc.workspace_id;
      IF p_organization IS NOT NULL AND p_organization <> v_org THEN
        RAISE EXCEPTION 'the document belongs to another organization' USING ERRCODE = '22023';
      END IF;
      IF NOT EXISTS (SELECT 1 FROM organization_members WHERE organization_id = v_org AND user_id = v_grantee) THEN
        RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
      END IF;
      v_role := app_workspace_role(v_doc.workspace_id, v_grantee);
      IF v_role = 'readonly' AND p_permission = 'write' THEN
        RAISE EXCEPTION 'a read-only member cannot receive write access' USING ERRCODE = '22023';
      END IF;
    END IF;
    -- 조직 탈퇴(조직 행 갱신 잠금 뒤 공유 삭제)와 줄 세운다.
    PERFORM 1 FROM organization_members WHERE organization_id = v_org AND user_id = v_grantee FOR SHARE;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'the recipient must be a member of the organization' USING ERRCODE = '22023';
    END IF;
    INSERT INTO document_shares (id, document_id, organization_id, grantee_user_id, permission, granted_by)
    VALUES (app_id(p_ids, 1), p_doc, v_org, v_grantee, p_permission, app_user_id())
    ON CONFLICT ON CONSTRAINT document_shares_target
      DO UPDATE SET permission = EXCLUDED.permission, granted_by = EXCLUDED.granted_by,
                    organization_id = EXCLUDED.organization_id
    RETURNING id INTO v_id;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'share',
                               jsonb_build_object('user', v_grantee, 'email', v_email, 'permission', p_permission,
                                                  'organization', v_org));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_share_document(p_doc uuid, p_email text, p_permission text, p_organization uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_unshare_document(p_share uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_share document_shares%ROWTYPE;
  BEGIN
    SELECT * INTO v_share FROM document_shares WHERE id = p_share;
    PERFORM 1 FROM documents WHERE id = v_share.document_id FOR UPDATE;
    IF v_share.id IS NULL OR app_access('document', v_share.document_id) = 'none' THEN
      RAISE EXCEPTION 'share not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_document_manager(v_share.document_id);
    DELETE FROM document_shares WHERE id = p_share;
    PERFORM app_audit_document(app_id(p_ids, 1), v_share.document_id, 'unshare',
                               jsonb_build_object('user', v_share.grantee_user_id));
  END
  $$;


ALTER FUNCTION fvoci.app_unshare_document(p_share uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_shared_with_me(p_organization uuid DEFAULT NULL::uuid) RETURNS TABLE(id uuid, title text, permission text, shared_by text, updated_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT d.id, d.title, s.permission, granter.display_name, d.updated_at
    FROM document_shares s
    JOIN documents d ON d.id = s.document_id
    JOIN users granter ON granter.id = s.granted_by
    -- 공유받은 문서는 현재 조직 범위(D-111): p_organization이 NULL이면 전체. 공유 행의 조직은 공유를 준 조직이다(D-114).
    WHERE s.grantee_user_id = app_user_id() AND app_access('document', d.id) <> 'none'
      AND (p_organization IS NULL OR s.organization_id = p_organization)
    ORDER BY d.updated_at DESC
  $$;


ALTER FUNCTION fvoci.app_shared_with_me(p_organization uuid) OWNER TO fvoci_migrator;

-- 문서가 그 자리에 있을 때 볼 수 있는 사람과 등급. 후보는 소유자·공간 멤버·공유받은 사람이다.
CREATE FUNCTION fvoci.app_document_audience(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid) RETURNS TABLE(user_id uuid, grade text)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT c.id, g.grade
    FROM (SELECT p_owner AS id WHERE p_owner IS NOT NULL
          UNION SELECT m.user_id FROM workspace_members m WHERE p_workspace IS NOT NULL AND m.workspace_id = p_workspace
          UNION SELECT s.grantee_user_id FROM document_shares s WHERE s.document_id = p_doc
          UNION SELECT d.owner_user_id FROM documents d WHERE d.id = p_doc AND d.owner_user_id IS NOT NULL) c
    CROSS JOIN LATERAL (SELECT app_grade_document_at(p_doc, p_workspace, p_item, p_owner, c.id) AS grade) g
    WHERE g.grade <> 'none'
  $$;


ALTER FUNCTION fvoci.app_document_audience(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid) OWNER TO fvoci_migrator;

-- 이동 전 확인(D-24·D-114): 사용자 id·등급 단위의 변화. 같은 조직 안의 이동이라 공개 링크·개별 공유는 그대로 유효하다.
CREATE FUNCTION fvoci.app_move_preview(p_doc uuid, p_target uuid, p_item uuid) RETURNS TABLE(change text, user_id uuid, display_name text, from_grade text, to_grade text)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE := app_check_move(p_doc, p_target, p_item);
  BEGIN
    RETURN QUERY
    WITH old_set AS (
      SELECT a.user_id AS uid, a.grade FROM app_document_audience(p_doc, v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id) a
    ), new_set AS (
      SELECT a.user_id AS uid, a.grade FROM app_document_audience(p_doc, p_target, p_item, NULL) a
    )
    SELECT CASE WHEN o.uid IS NULL THEN 'gain' WHEN n.uid IS NULL THEN 'lose' ELSE 'change' END::text,
           coalesce(o.uid, n.uid), u.display_name, o.grade, n.grade
    FROM old_set o FULL JOIN new_set n ON n.uid = o.uid
    JOIN users u ON u.id = coalesce(o.uid, n.uid)
    WHERE o.grade IS DISTINCT FROM n.grade
    ORDER BY 1, 3, 2;
  END
  $$;


ALTER FUNCTION fvoci.app_move_preview(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_move(p_doc uuid, p_target uuid, p_item uuid) RETURNS fvoci.documents
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE;
    v_item items%ROWTYPE;
  BEGIN
    PERFORM app_require_document(p_doc, 'owner');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF p_target IS NULL THEN
      RAISE EXCEPTION 'team documents stay in the workspace; make a copy instead' USING ERRCODE = '55000';
    END IF;
    IF v_doc.workspace_id IS NOT DISTINCT FROM p_target AND v_doc.item_id IS NOT DISTINCT FROM p_item THEN
      RAISE EXCEPTION 'the document is already there' USING ERRCODE = '55000';
    END IF;
    IF app_access('workspace', p_target) = 'none' THEN
      RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('workspace', p_target) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
    END IF;
    -- 공간 간 이동은 같은 조직 안에서만 한다(D-24·D-114). 개인 노트는 소유자가 속한 어느 공간으로든 옮길 수 있다.
    IF v_doc.workspace_id IS NOT NULL
       AND app_target_organization('workspace', v_doc.workspace_id) IS DISTINCT FROM app_target_organization('workspace', p_target) THEN
      RAISE EXCEPTION 'documents move within the organization' USING ERRCODE = '55000';
    END IF;
    IF p_item IS NOT NULL THEN
      SELECT * INTO v_item FROM items WHERE id = p_item;
      IF v_item.id IS NULL OR v_item.workspace_id <> p_target OR app_access('item', p_item) = 'none' THEN
        RAISE EXCEPTION 'target item not found' USING ERRCODE = 'P0002';
      END IF;
      IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
        RAISE EXCEPTION 'write access to the target item is required' USING ERRCODE = '42501';
      END IF;
      IF app_item_archived(v_item.id) THEN
        RAISE EXCEPTION 'the target item is archived' USING ERRCODE = '55000';
      END IF;
    END IF;
    RETURN v_doc;
  END
  $$;


ALTER FUNCTION fvoci.app_check_move(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

-- 복제(D-23·D-115). 소유 권한이 없으면 allow_copy가 켜진 문서만 복제한다. 템플릿에서 새 문서 만들기는 예외다.
-- 공간 간 복제는 같은 조직 안에서만 한다(D-114). 새 id 4: 문서, 원본 감사, 사본 감사, 사본 활동(개인 노트 사본은 3).
CREATE FUNCTION fvoci.app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_access  text := app_require_document(p_doc, 'read');
    v_src     documents%ROWTYPE;
    v_private boolean;
    v_id      uuid;
    v_number  bigint;
  BEGIN
    SELECT * INTO v_src FROM documents WHERE id = p_doc;
    IF v_access <> 'owner' AND NOT v_src.allow_copy AND NOT v_src.is_template THEN
      RAISE EXCEPTION 'copying is not allowed for this document' USING ERRCODE = '42501';
    END IF;
    -- 사본으로 볼 수 있는 사람이 조용히 늘지 않게: 비공개 문서, 개인 노트, 비공개 항목 아래 문서의 사본은 비공개.
    v_private := v_src.visibility = 'private'
      OR EXISTS (SELECT 1 FROM items i WHERE i.id = v_src.item_id AND i.visibility = 'private');

    IF p_target_workspace IS NULL THEN
      SELECT id INTO v_id FROM documents WHERE owner_user_id = v_user AND request_id = p_request_id;
      IF v_id IS NOT NULL THEN RETURN v_id; END IF;
      INSERT INTO documents (id, owner_user_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (app_id(p_ids, 1), v_user, 'personal', 'NOTE', NULL, btrim(p_title), v_src.schema_version, v_src.body,
              1, v_user, v_user, p_request_id, 'private')
      RETURNING id INTO v_id;
    ELSE
      IF app_access('workspace', p_target_workspace) = 'none' THEN
        RAISE EXCEPTION 'target workspace not found' USING ERRCODE = 'P0002';
      END IF;
      IF app_access('workspace', p_target_workspace) NOT IN ('owner', 'write') THEN
        RAISE EXCEPTION 'write access to the target workspace is required' USING ERRCODE = '42501';
      END IF;
      IF v_src.workspace_id IS NOT NULL
         AND app_target_organization('workspace', v_src.workspace_id) IS DISTINCT FROM app_target_organization('workspace', p_target_workspace) THEN
        RAISE EXCEPTION 'copies stay within the organization' USING ERRCODE = '55000';
      END IF;
      SELECT id INTO v_id FROM documents
      WHERE workspace_id = p_target_workspace AND created_by = v_user AND request_id = p_request_id;
      IF v_id IS NOT NULL THEN RETURN v_id; END IF;
      v_id := app_id(p_ids, 1);
      v_number := app_allocate_number(p_target_workspace, p_target_workspace, 'document', v_id);
      INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                             revision, created_by, updated_by, request_id, visibility)
      VALUES (v_id, p_target_workspace, 'wiki', 'WIKI', v_number,
              btrim(p_title), v_src.schema_version, v_src.body, 1, v_user, v_user, p_request_id,
              CASE WHEN v_private THEN 'private' ELSE 'workspace' END);
    END IF;

    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), v_src.schema_version, v_src.body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'copy',
      jsonb_build_object('target', CASE WHEN p_target_workspace IS NULL THEN 'personal' ELSE 'workspace' END));
    PERFORM app_audit_document(app_id(p_ids, 3), v_id, 'create',
      jsonb_build_object('copied', true, 'from', CASE WHEN v_src.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    IF p_target_workspace IS NOT NULL THEN
      PERFORM app_activity(app_id(p_ids, 4), p_target_workspace, NULL, 'document', v_id, 'create',
                           jsonb_build_object('title', btrim(p_title), 'copied_from', p_doc));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-115: allow_copy가 꺼진 문서는 소유 권한 외 누구도(수정 권한 포함) 내보내거나 첨부를 내려받지 못한다.
CREATE FUNCTION fvoci.app_document_can_export(p_doc uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
  BEGIN
    IF v_access = 'none' THEN
      RETURN false;
    END IF;
    IF v_access = 'owner' THEN
      RETURN true;
    END IF;
    RETURN coalesce((SELECT allow_copy FROM documents WHERE id = p_doc AND deleted_at IS NULL), false);
  END
  $$;


ALTER FUNCTION fvoci.app_document_can_export(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_can_download(p_file uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('file', p_file);
    v_doc    uuid;
  BEGIN
    IF v_access = 'none' THEN
      RETURN false;
    END IF;
    IF v_access = 'owner' THEN
      RETURN true;
    END IF;
    SELECT document_id INTO v_doc FROM files WHERE id = p_file;
    -- 항목 첨부에는 문서 플래그가 없다. 문서 첨부는 그 문서의 allow_copy를 따른다.
    IF v_doc IS NULL THEN
      RETURN true;
    END IF;
    RETURN coalesce((SELECT allow_copy FROM documents WHERE id = v_doc), false);
  END
  $$;


ALTER FUNCTION fvoci.app_file_can_download(p_file uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 비로그인 공개 링크(D-20·D-114): 유일한 조직 밖 출구. 조직 관리자가 끄면 새 링크를 만들 수 없고 기존 링크도 응답하지 않는다.
CREATE FUNCTION fvoci.app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamp with time zone, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id uuid;
    v_ws uuid;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_require_document_manager(p_doc);
    IF p_expires_at IS NOT NULL AND p_expires_at <= now() THEN
      RAISE EXCEPTION 'expiry must be in the future' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM documents WHERE id = p_doc;
    IF v_ws IS NOT NULL AND NOT EXISTS (
      SELECT 1 FROM workspaces w JOIN organizations o ON o.id = w.organization_id
      WHERE w.id = v_ws AND o.public_links_enabled
    ) THEN
      RAISE EXCEPTION 'public links are turned off for this organization' USING ERRCODE = '55000';
    END IF;
    INSERT INTO public_links (id, document_id, token_hash, created_by, expires_at)
    VALUES (app_id(p_ids, 1), p_doc, p_token_hash, app_user_id(), p_expires_at)
    RETURNING id INTO v_id;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'create_public_link',
                               jsonb_build_object('link', v_id, 'expires_at', p_expires_at));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamp with time zone, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_revoke_public_link(p_link uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc uuid;
  BEGIN
    SELECT document_id INTO v_doc FROM public_links WHERE id = p_link AND revoked_at IS NULL;
    IF v_doc IS NULL OR app_access('document', v_doc) = 'none' THEN
      RAISE EXCEPTION 'link not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM 1 FROM documents WHERE id = v_doc FOR UPDATE;
    PERFORM app_require_document_manager(v_doc);
    UPDATE public_links SET revoked_at = now() WHERE id = p_link;
    PERFORM app_audit_document(app_id(p_ids, 1), v_doc, 'revoke_public_link', jsonb_build_object('link', p_link));
  END
  $$;


ALTER FUNCTION fvoci.app_revoke_public_link(p_link uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_public_document(p_token_hash bytea) RETURNS TABLE(title text, body jsonb, schema_version integer, updated_at timestamp with time zone)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT d.title, d.body, d.schema_version, d.updated_at
    FROM public_links l JOIN documents d ON d.id = l.document_id
    WHERE l.token_hash = p_token_hash
      AND l.revoked_at IS NULL
      AND (l.expires_at IS NULL OR l.expires_at > now())
      AND d.deleted_at IS NULL
      -- 항목이나 그 모듈이 휴지통에 있으면 열리지 않는다.
      AND NOT EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                      WHERE i.id = d.item_id AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL))
      AND NOT EXISTS (SELECT 1 FROM workspaces w WHERE w.id = d.workspace_id AND w.deleted_at IS NOT NULL)
      -- 조직이 공개 링크를 껐으면 기존 링크도 응답하지 않는다(D-114).
      AND (d.workspace_id IS NULL OR EXISTS (
            SELECT 1 FROM workspaces w JOIN organizations o ON o.id = w.organization_id
            WHERE w.id = d.workspace_id AND o.public_links_enabled))
  $$;


ALTER FUNCTION fvoci.app_public_document(p_token_hash bytea) OWNER TO fvoci_migrator;

-- 본문의 @참조와 언급(D-18·D-114). 연결은 작성자가 볼 수 있고 같은 조직 안에 있는 대상만 남긴다(개인 노트는 조직과 무관하므로
-- 볼 수 있는 대상이면 된다). 언급 알림은 그 사람이 문서를 볼 수 있을 때만 간다.
CREATE FUNCTION fvoci.app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_doc      documents%ROWTYPE;
    v_org      uuid;
    v_previous jsonb;
    v_uid      uuid;
    v_new      integer;
    v_at       integer := 0;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    v_org := app_target_organization('document', p_doc);

    DELETE FROM document_links WHERE document_id = p_doc;
    INSERT INTO document_links (document_id, target_kind, target_id)
    SELECT DISTINCT p_doc, r.kind, r.id FROM app_reference_nodes(v_doc.body) r
    WHERE r.kind IN ('item', 'document', 'task') AND NOT (r.kind = 'document' AND r.id = p_doc)
      AND app_access(r.kind, r.id) <> 'none'
      -- D-114: 같은 조직 안의 대상만. 개인 대상(조직 없음)은 볼 수 있으면 되고, 개인 노트에서는 쓴 사람이 속한 조직의
      -- 대상만 참조한다.
      AND (app_target_organization(r.kind, r.id) IS NULL
           OR app_target_organization(r.kind, r.id) = v_org
           OR (v_org IS NULL AND EXISTS (SELECT 1 FROM organization_members m
                                         WHERE m.organization_id = app_target_organization(r.kind, r.id)
                                           AND m.user_id = v_user)));

    IF NOT p_notify THEN
      RETURN;
    END IF;
    SELECT body INTO v_previous FROM document_revisions
    WHERE document_id = p_doc AND revision = v_doc.revision - 1;
    FOR v_uid IN
      SELECT r.id FROM app_reference_nodes(v_doc.body) r JOIN users u ON u.id = r.id
      WHERE r.kind = 'user' AND r.id <> v_user
        AND NOT EXISTS (SELECT 1 FROM app_reference_nodes(coalesce(v_previous, '{}'::jsonb)) p
                        WHERE p.kind = 'user' AND p.id = r.id)
    LOOP
      IF app_access_as('document', p_doc, v_uid) <> 'none' THEN
        INSERT INTO document_mentions (document_id, user_id, mentioned_by)
        VALUES (p_doc, v_uid, v_user)
        ON CONFLICT DO NOTHING;
        GET DIAGNOSTICS v_new = ROW_COUNT;
        IF v_new > 0 THEN
          v_at := v_at + 1;
          INSERT INTO notifications (id, user_id, organization_id, kind, document_id, actor)
          VALUES (app_id(p_ids, v_at), v_uid, v_org, 'mention', p_doc, v_user);
        END IF;
      END IF;
    END LOOP;
  END
  $$;


ALTER FUNCTION fvoci.app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 리비전 본문이 가리키는 첨부 버전을 고정한다(D-56). 같은 조직 밖의 첨부는 참조로 기록하지 않는다(D-114).
CREATE FUNCTION fvoci.app_record_file_refs() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE;
  BEGIN
    SELECT * INTO v_doc FROM documents WHERE id = NEW.document_id;
    -- 가리키는 버전 행을 참조를 넣기 전에 공유 잠금한다. 범위 밖 참조(다른 조직의 파일)는 기록하지 않는다(D-114).
    PERFORM 1 FROM file_versions v
    JOIN (SELECT DISTINCT r.file_id, r.version FROM app_file_refs(NEW.body) r) r
      ON r.file_id = v.file_id AND r.version = v.version
    JOIN files f ON f.id = v.file_id
    WHERE app_file_ref_in_scope(v_doc.workspace_id, v_doc.owner_user_id, f.workspace_id, f.owner_user_id)
    ORDER BY v.id
    FOR KEY SHARE OF v;
    INSERT INTO document_file_refs (document_id, revision, file_id, version)
    SELECT NEW.document_id, NEW.revision, r.file_id, r.version
    FROM app_file_refs(NEW.body) r JOIN files f ON f.id = r.file_id
    WHERE app_file_ref_in_scope(v_doc.workspace_id, v_doc.owner_user_id, f.workspace_id, f.owner_user_id)
    ON CONFLICT DO NOTHING;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_record_file_refs() OWNER TO fvoci_migrator;

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
        WHERE i.id = p_invitation AND me.email_verified AND lower(me.email) = i.email)
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
    WHERE i.closed_at IS NULL AND i.expires_at > now() AND me.email_verified AND lower(me.email) = i.email
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
      WHERE i.closed_at IS NULL AND i.expires_at > now() AND me.email_verified AND lower(me.email) = i.email
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

-- 타이머 시작(D-32·D-117, 개정 2026-10-26 ⑰). 사용자당 모든 조직·공간에서 실행 중인 기록은 하나다. 기록 중에 다른 대상에서 시작하면 한
-- 트랜잭션에서 진행 중 기록을 멈춰 저장하고 새 기록을 시작한다(stopped_entry_id). 새 대상을 볼 수 없으면 아무것도 바뀌지
-- 않는다(기존 기록은 계속 간다). 같은 request_id의 재시도는 같은 기록을 돌려주고 다시 멈추지 않는다.
CREATE FUNCTION fvoci.app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[], p_mode text DEFAULT NULL::text) RETURNS TABLE(entry_id uuid, stopped_entry_id uuid)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_id      uuid;
    v_ws      uuid;
    v_running time_entries%ROWTYPE;
    v_stopped uuid;
    v_now     timestamp with time zone;
  BEGIN
    IF p_mode IS NOT NULL AND p_mode NOT IN ('continue', 'new') THEN
      RAISE EXCEPTION 'mode must be continue or new' USING ERRCODE = '22023';
    END IF;
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    SELECT r.entry_id INTO v_id FROM time_entry_requests r
    WHERE r.user_id = v_user AND r.request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN QUERY SELECT v_id, NULL::uuid;
      RETURN;
    END IF;
    -- 새 대상을 먼저 확인한다. 못 보는 대상이면 여기서 끝나고 진행 중 기록은 그대로다.
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    -- 같은 사용자의 시작 요청을 줄 세운다(하나만 진행 중이라는 규칙을 고유 색인과 함께 지킨다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || v_user::text, 0));
    -- 전환 시각은 잠금을 얻은 뒤의 실제 시각이다(now()는 트랜잭션 시작 시각이라, 먼저 시작했지만 잠금을 늦게 얻은 요청이
    -- 뒤에 만들어진 기록의 시작보다 이른 시각으로 멈추려다 time_entries_order에 걸린다).
    v_now := clock_timestamp();
    SELECT r.entry_id INTO v_id FROM time_entry_requests r
    WHERE r.user_id = v_user AND r.request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN QUERY SELECT v_id, NULL::uuid;
      RETURN;
    END IF;
    SELECT * INTO v_running FROM time_entries
    WHERE user_id = v_user AND ended_at IS NULL AND deleted_at IS NULL FOR UPDATE;
    -- 개정 2026-10-26(⑰, D-117): 같은 대상으로 기록 중에 다시 시작하면 웹이 "이어가기 / 새 기록"을 묻는다. 플래그 없이는
    -- 55000(→409)으로 거절하고, continue는 기존 기록을 그대로 돌려주며(새 기록 없음), new는 아래처럼 멈춰 저장하고 새로 시작한다.
    -- 다른 대상이면 전처럼 자동으로 전환한다.
    IF v_running.id IS NOT NULL AND v_running.target_kind = p_target_kind
       AND v_running.target_id IS NOT DISTINCT FROM p_target THEN
      IF p_mode = 'continue' THEN
        INSERT INTO time_entry_requests (user_id, request_id, entry_id)
        VALUES (v_user, p_request_id, v_running.id);
        RETURN QUERY SELECT v_running.id, NULL::uuid;
        RETURN;
      ELSIF p_mode IS DISTINCT FROM 'new' THEN
        RAISE EXCEPTION 'a timer for this target is already running' USING ERRCODE = '55000';
      END IF;
    END IF;
    IF v_running.id IS NOT NULL THEN
      -- 멈춤(app_timer_action 'stop')과 같은 계산: 일시정지 중이던 시간은 정지 시간에 더한다.
      UPDATE time_entries SET
        paused_seconds = paused_seconds
          + CASE WHEN paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM v_now - paused_at))::integer END,
        paused_at = NULL, ended_at = v_now, version = version + 1, updated_at = v_now
      WHERE id = v_running.id;
      v_stopped := v_running.id;
    END IF;
    INSERT INTO time_entries (id, user_id, started_at, kind, memo, target_kind, target_id, workspace_id, request_id)
    VALUES (app_id(p_ids, 1), v_user, v_now, p_kind, coalesce(p_memo, ''), p_target_kind, p_target, v_ws,
            p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO time_entry_requests (user_id, request_id, entry_id)
    VALUES (v_user, p_request_id, v_id);
    RETURN QUERY SELECT v_id, v_stopped;
  END
  $$;


ALTER FUNCTION fvoci.app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[], p_mode text) OWNER TO fvoci_migrator;

-- 항목·업무의 내 소요 시간(D-117: 기록은 본인만 보고 선택 공유 집계는 없다).
CREATE FUNCTION fvoci.app_item_time(p_item uuid) RETURNS TABLE(entry_kind text, mine_seconds bigint)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      WITH targets (target_kind, target_id) AS (
        SELECT 'item'::text, p_item
        UNION ALL SELECT 'task', t.id FROM tasks t WHERE t.item_id = p_item
        UNION ALL SELECT 'document', d.id FROM documents d WHERE d.item_id = p_item
        UNION ALL SELECT 'episode', c.id FROM course_episodes c WHERE c.item_id = p_item
        UNION ALL SELECT 'study_unit', u.id FROM study_units u WHERE u.item_id = p_item
      )
      SELECT e.kind, coalesce(sum(app_entry_seconds(e)), 0)::bigint
      FROM time_entries e
      JOIN targets g ON g.target_kind = e.target_kind AND g.target_id = e.target_id
      WHERE e.user_id = app_user_id() AND e.deleted_at IS NULL AND e.ended_at IS NOT NULL
      GROUP BY e.kind
      ORDER BY e.kind;
  END
  $$;


ALTER FUNCTION fvoci.app_item_time(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_time(p_task uuid) RETURNS bigint
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('task', p_task) = 'none' THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN (SELECT coalesce(sum(app_entry_seconds(e)), 0)::bigint
            FROM time_entries e
            WHERE e.target_kind = 'task' AND e.target_id = p_task AND e.user_id = app_user_id()
              AND e.deleted_at IS NULL AND e.ended_at IS NOT NULL);
  END
  $$;


ALTER FUNCTION fvoci.app_task_time(p_task uuid) OWNER TO fvoci_migrator;

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

-- 상태 정의 관리(D-116). 항목 소유 권한이 정의를 만들고 고치고 지운다. 지울 때는 그 상태의 업무를 다른 상태로 옮긴다.
CREATE FUNCTION fvoci.app_task_statuses(p_item uuid) RETURNS TABLE(id uuid, name text, color text, "position" numeric, category text, task_count integer)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN QUERY
      SELECT s.id, s.name, s.color, s.position, s.category,
             (SELECT count(*)::integer FROM tasks t WHERE t.status_id = s.id AND t.deleted_at IS NULL)
      FROM task_statuses s WHERE s.item_id = p_item
      ORDER BY s.position, s.created_at, s.id;
  END
  $$;


ALTER FUNCTION fvoci.app_task_statuses(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_task_status(p_item uuid, p_name text, p_color text, p_category text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item items%ROWTYPE;
    v_id   uuid;
  BEGIN
    -- 항목의 상태 집합 변경(생성·수정·삭제)을 줄 세운다: 개수·이름 중복 검사가 잠금 뒤에 유효하도록.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-statuses:' || p_item::text, 0));
    SELECT * INTO v_item FROM items WHERE id = p_item FOR SHARE;
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    -- 보관된 항목은 볼 수 있는 누구에게나 보관 상태(409)가 먼저다(문서·업무·파일과 같은 규칙).
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    IF p_category NOT IN ('todo', 'doing', 'done') THEN
      RAISE EXCEPTION 'invalid category' USING ERRCODE = '22023';
    END IF;
    IF EXISTS (SELECT 1 FROM task_statuses WHERE item_id = p_item AND name = btrim(coalesce(p_name, ''))) THEN
      RAISE EXCEPTION 'a status with this name exists' USING ERRCODE = '55000';
    END IF;
    IF (SELECT count(*) FROM task_statuses WHERE item_id = p_item) >= 30 THEN
      RAISE EXCEPTION 'too many statuses' USING ERRCODE = '22023';
    END IF;
    INSERT INTO task_statuses (id, item_id, name, color, position, category)
    VALUES (app_id(p_ids, 1), p_item, btrim(p_name), coalesce(p_color, ''),
            (SELECT coalesce(max(position), 0) + 1 FROM task_statuses WHERE item_id = p_item), p_category)
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create_status', 'item', p_item,
                      jsonb_build_object('status', v_id, 'name', btrim(p_name), 'category', p_category));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_task_status(p_item uuid, p_name text, p_color text, p_category text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 이름·색·순서·범주 바꾸기. 범주가 done으로 바뀌거나 done에서 벗어나면 그 상태의 업무 완료 시각을 맞춘다. 이름이 바뀌면 그
-- 상태의 업무를 다시 색인한다(task_statuses_search 트리거).
CREATE FUNCTION fvoci.app_update_task_status(p_status uuid, p_name text, p_color text, p_category text, p_position numeric, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
    v_item   items%ROWTYPE;
    v_name   text;
    v_cat    text;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status;
    IF v_status.id IS NULL OR app_access('item', v_status.item_id) = 'none' THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    -- 항목의 상태 집합 변경을 줄 세운 뒤(생성·삭제와 같은 잠금) 열을 잠그고, 잠금 뒤 등급·보관·존재를 다시 본다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-statuses:' || v_status.item_id::text, 0));
    PERFORM app_lock_lane(p_status);
    SELECT * INTO v_item FROM items WHERE id = v_status.item_id FOR SHARE;
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status FOR UPDATE;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    v_name := coalesce(btrim(p_name), v_status.name);
    v_cat := coalesce(p_category, v_status.category);
    IF v_cat NOT IN ('todo', 'doing', 'done') THEN
      RAISE EXCEPTION 'invalid category' USING ERRCODE = '22023';
    END IF;
    IF v_name <> v_status.name AND EXISTS (SELECT 1 FROM task_statuses WHERE item_id = v_status.item_id AND name = v_name) THEN
      RAISE EXCEPTION 'a status with this name exists' USING ERRCODE = '55000';
    END IF;
    UPDATE task_statuses SET name = v_name, color = coalesce(p_color, color), category = v_cat,
                             position = coalesce(p_position, position), updated_at = now()
    WHERE id = p_status;
    IF v_cat <> v_status.category THEN
      UPDATE tasks SET completed_at = CASE WHEN v_cat = 'done' THEN coalesce(completed_at, now()) END
      WHERE status_id = p_status;
    END IF;
    PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'update_status', 'item', v_status.item_id,
                      jsonb_build_object('status', p_status, 'from', jsonb_build_object('name', v_status.name,
                        'color', v_status.color, 'category', v_status.category, 'position', v_status.position),
                        'to', jsonb_build_object('name', v_name, 'color', coalesce(p_color, v_status.color),
                        'category', v_cat, 'position', coalesce(p_position, v_status.position))));
  END
  $$;


ALTER FUNCTION fvoci.app_update_task_status(p_status uuid, p_name text, p_color text, p_category text, p_position numeric, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 상태 지우기. 그 상태의 업무가 있으면 같은 항목의 p_move_to로 옮긴 뒤 지운다(D-116). 마지막 상태는 지울 수 없다.
CREATE FUNCTION fvoci.app_delete_task_status(p_status uuid, p_move_to uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
    v_to     task_statuses%ROWTYPE;
    v_item   items%ROWTYPE;
    v_lane   uuid;
    v_moved  integer := 0;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status;
    IF v_status.id IS NULL OR app_access('item', v_status.item_id) = 'none' THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    IF p_move_to IS NOT NULL THEN
      SELECT * INTO v_to FROM task_statuses WHERE id = p_move_to;
      IF v_to.id IS NULL OR v_to.item_id <> v_status.item_id OR v_to.id = p_status THEN
        RAISE EXCEPTION 'move the tasks to another status of the same item' USING ERRCODE = '22023';
      END IF;
    END IF;
    -- 항목의 상태 집합 변경을 줄 세운다(생성·수정과 같은 잠금): 마지막 상태 보장이 동시 삭제에서도 지켜지도록 개수 검사는
    -- 이 잠금 뒤에 한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-statuses:' || v_status.item_id::text, 0));
    IF (SELECT count(*) FROM task_statuses WHERE item_id = v_status.item_id) <= 1 THEN
      RAISE EXCEPTION 'an item needs at least one status' USING ERRCODE = '55000';
    END IF;
    -- 두 열을 id 순서로 잠근 뒤(이동·생성과 같은 순서) 항목을 공유 잠금하고, 잠금 뒤 등급·보관·존재를 다시 본다.
    FOR v_lane IN SELECT x FROM unnest(ARRAY[p_status, p_move_to]) x WHERE x IS NOT NULL ORDER BY x LOOP
      PERFORM app_lock_lane(v_lane);
    END LOOP;
    SELECT * INTO v_item FROM items WHERE id = v_status.item_id FOR SHARE;
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status FOR UPDATE;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'status not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_item_archived(v_status.item_id) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF app_access('item', v_status.item_id) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can define statuses' USING ERRCODE = '42501';
    END IF;
    IF EXISTS (SELECT 1 FROM tasks WHERE status_id = p_status) THEN
      IF p_move_to IS NULL THEN
        RAISE EXCEPTION 'choose a status for the tasks in this column' USING ERRCODE = '22023';
      END IF;
      PERFORM 1 FROM tasks WHERE status_id = p_status ORDER BY id FOR NO KEY UPDATE;
      UPDATE tasks t SET
        status_id = p_move_to,
        position = r.pos + (SELECT coalesce(max(position), 0) FROM tasks WHERE status_id = p_move_to),
        version = version + 1, updated_at = now(),
        completed_at = CASE WHEN v_to.category = 'done' THEN coalesce(t.completed_at, now()) END
      FROM (SELECT id, row_number() OVER (ORDER BY position, number) AS pos FROM tasks WHERE status_id = p_status) r
      WHERE t.id = r.id;
      GET DIAGNOSTICS v_moved = ROW_COUNT;
    END IF;
    DELETE FROM task_statuses WHERE id = p_status;
    PERFORM app_audit(app_id(p_ids, 1), v_item.workspace_id, 'delete_status', 'item', v_status.item_id,
                      jsonb_build_object('status', p_status, 'name', v_status.name, 'moved_to', p_move_to, 'moved', v_moved));
    RETURN v_moved;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_task_status(p_status uuid, p_move_to uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 상태 이름·범주가 바뀌면 그 상태의 업무를 다시 색인한다(D-101·D-102의 색인 일치).
CREATE FUNCTION fvoci.app_search_enqueue_status_tasks() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'task', t.id FROM tasks t WHERE t.status_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_status_tasks() OWNER TO fvoci_migrator;

-- 업무 반복 규칙(D-31·D-125). 일정 반복과 같은 기준 판 검사: 지금 규칙이 보낸 규칙과도, 편집을 시작할 때 본 규칙(p_base)과도
-- 다르면 아무것도 바꾸지 않고 conflict를 돌려준다(409). 새 id 2: 계열, 감사.
CREATE FUNCTION fvoci.app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_base jsonb, p_ids uuid[]) RETURNS TABLE(series_id uuid, outcome text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task    tasks%ROWTYPE;
    v_series  task_series%ROWTYPE;
    v_current jsonb;
    v_anchor  date;
    v_today   date;
  BEGIN
    PERFORM app_require_task_write(p_task);
    IF p_rule IS NOT NULL THEN
      PERFORM app_check_recurrence(p_rule);
    END IF;
    PERFORM app_check_timezone(p_timezone);
    PERFORM app_lock_task_rows(ARRAY[p_task], NULL);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    v_today := (now() AT TIME ZONE p_timezone)::date;
    IF v_task.series_id IS NOT NULL THEN
      SELECT * INTO v_series FROM task_series WHERE id = v_task.series_id FOR UPDATE;
      IF v_series.root_task_id <> p_task THEN
        RAISE EXCEPTION 'change the recurrence on the first occurrence' USING ERRCODE = '22023';
      END IF;
    END IF;
    v_current := v_series.rule;
    IF jsonb_strip_nulls(coalesce(v_current, 'null'::jsonb)) IS DISTINCT FROM jsonb_strip_nulls(coalesce(p_rule, 'null'::jsonb))
       AND jsonb_strip_nulls(coalesce(v_current, 'null'::jsonb)) IS DISTINCT FROM jsonb_strip_nulls(coalesce(p_base, 'null'::jsonb)) THEN
      RETURN QUERY SELECT v_series.id, 'conflict'::text;
      RETURN;
    END IF;
    IF v_series.id IS NOT NULL THEN
      IF (p_rule ->> 'until')::date < v_series.anchor THEN
        RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
      END IF;
      IF p_rule IS NOT NULL AND v_task.due_date IS NULL AND v_task.start_date IS NULL THEN
        RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
      END IF;
      IF p_rule IS NOT NULL AND v_task.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'only top-level tasks can repeat' USING ERRCODE = '22023';
      END IF;
      IF jsonb_strip_nulls(coalesce(v_current, 'null'::jsonb)) IS NOT DISTINCT FROM jsonb_strip_nulls(coalesce(p_rule, 'null'::jsonb))
         AND v_series.timezone = p_timezone THEN
        RETURN QUERY SELECT v_series.id, 'updated'::text;
        RETURN;
      END IF;
      UPDATE task_series SET
        rule = p_rule, timezone = p_timezone, updated_at = now(),
        effective_from = CASE WHEN p_rule IS DISTINCT FROM v_series.rule
                              THEN greatest(v_series.anchor, v_today) ELSE effective_from END
      WHERE id = v_series.id;
      PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'set_recurrence', 'task', p_task,
                        jsonb_build_object('from', v_series.rule, 'to', p_rule, 'timezone', p_timezone));
      RETURN QUERY SELECT v_series.id, 'updated'::text;
      RETURN;
    END IF;
    IF p_rule IS NULL THEN
      RETURN QUERY SELECT NULL::uuid, 'updated'::text;
      RETURN;
    END IF;
    IF v_task.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'only top-level tasks can repeat' USING ERRCODE = '22023';
    END IF;
    v_anchor := coalesce(v_task.due_date, v_task.start_date);
    IF v_anchor IS NULL THEN
      RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
    END IF;
    IF (p_rule ->> 'until')::date < v_anchor THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    INSERT INTO task_series (id, workspace_id, item_id, root_task_id, rule, timezone, anchor, effective_from,
                             created_by)
    VALUES (app_id(p_ids, 1), v_task.workspace_id, v_task.item_id, p_task, p_rule, p_timezone, v_anchor, v_anchor,
            app_user_id())
    RETURNING * INTO v_series;
    UPDATE tasks SET series_id = v_series.id, occurrence_date = v_anchor WHERE id = p_task;
    PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'set_recurrence', 'task', p_task,
                      jsonb_build_object('from', NULL, 'to', p_rule, 'timezone', p_timezone));
    RETURN QUERY SELECT v_series.id, 'updated'::text;
  END
  $$;


ALTER FUNCTION fvoci.app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_base jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 조직 전체 저장량(D-124): 조직 안 모든 워크스페이스의 파일 버전과 열린 업로드.
CREATE FUNCTION fvoci.app_storage_used_organization(p_organization uuid, p_except uuid) RETURNS bigint
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT sum(v.size) FROM file_versions v JOIN files f ON f.id = v.file_id
                     JOIN workspaces w ON w.id = f.workspace_id WHERE w.organization_id = p_organization), 0)
         + coalesce((SELECT sum(u.size) FROM uploads u JOIN workspaces w ON w.id = u.workspace_id
                     WHERE w.organization_id = p_organization AND u.state IN ('uploading', 'completing')
                       AND u.id IS DISTINCT FROM p_except), 0)
  $$;


ALTER FUNCTION fvoci.app_storage_used_organization(p_organization uuid, p_except uuid) OWNER TO fvoci_migrator;

-- 업로드 시작(D-52·D-124). 파일 크기·저장량·동시 업로드 수는 워크스페이스·조직·서버 기본값 가운데 가장 작은 한도를 따르고
-- 조직 저장량 상한도 함께 본다. 개인 영역은 서버 기본값만 쓴다.
CREATE FUNCTION fvoci.app_create_upload(p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text, p_type text, p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint, p_max_storage bigint, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user    uuid := app_user_id();
    v_id      uuid;
    v_file    files%ROWTYPE;
    v_place   record;
    v_kind    text := p_kind;
    v_tgt     uuid := p_target;
    v_name    text := btrim(coalesce(p_name, ''));
    v_limit   bigint;
    v_org     uuid;
    v_org_max bigint;
    v_org_conc integer;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'not signed in' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM uploads WHERE user_id = v_user AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_size < 1 THEN
      RAISE EXCEPTION 'empty files cannot be uploaded' USING ERRCODE = '22023';
    END IF;
    IF p_file IS NOT NULL THEN
      SELECT * INTO v_file FROM files WHERE id = p_file;
      IF app_access('file', p_file) = 'none' THEN
        RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
      END IF;
      v_kind := CASE WHEN v_file.document_id IS NOT NULL THEN 'document' ELSE 'item' END;
      v_tgt := coalesce(v_file.document_id, v_file.item_id);
      IF v_name = '' THEN
        v_name := v_file.name;
      END IF;
    END IF;
    SELECT * INTO v_place FROM app_file_place(v_kind, v_tgt);
    v_limit := app_effective_limit(v_place.workspace_id, 'max_file_bytes', p_max_file);
    IF p_size > v_limit THEN
      RAISE EXCEPTION 'the file is larger than the limit (% bytes)', v_limit USING ERRCODE = '22023';
    END IF;
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023';
    END IF;
    -- 같은 영역의 업로드 시작을 줄 세워 저장량·동시성 확인이 겹치지 않게 한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage:' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text, 0));
    v_limit := app_effective_limit(v_place.workspace_id, 'max_storage_bytes', p_max_storage);
    IF app_storage_used(v_place.workspace_id, v_place.owner_user_id, NULL) + p_size > v_limit THEN
      RAISE EXCEPTION 'not enough storage left (limit % bytes)', v_limit USING ERRCODE = '55000';
    END IF;
    IF v_place.workspace_id IS NOT NULL THEN
      SELECT w.organization_id, o.max_storage_bytes, o.max_concurrent_uploads INTO v_org, v_org_max, v_org_conc
      FROM workspaces w JOIN organizations o ON o.id = w.organization_id WHERE w.id = v_place.workspace_id;
      IF v_org_max IS NOT NULL OR v_org_conc IS NOT NULL THEN
        PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage-org:' || v_org::text, 0));
      END IF;
      IF v_org_max IS NOT NULL AND app_storage_used_organization(v_org, NULL) + p_size > v_org_max THEN
        RAISE EXCEPTION 'not enough storage left in the organization (limit % bytes)', v_org_max USING ERRCODE = '55000';
      END IF;
      -- 조직 동시 업로드 한도(D-124)는 조직 안 모든 공간의 진행 중 업로드를 센다(공간 단계는 아래 실효 한도).
      IF v_org_conc IS NOT NULL AND (SELECT count(*) FROM uploads u JOIN workspaces w ON w.id = u.workspace_id
                                     WHERE w.organization_id = v_org
                                       AND u.state IN ('uploading', 'completing')) >= v_org_conc THEN
        RAISE EXCEPTION 'too many uploads in progress in the organization (limit %)', v_org_conc USING ERRCODE = '55000';
      END IF;
      v_limit := app_effective_limit(v_place.workspace_id, 'max_concurrent_uploads', NULL);
      IF v_limit IS NOT NULL AND (SELECT count(*) FROM uploads u
                                  WHERE u.workspace_id = v_place.workspace_id
                                    AND u.state IN ('uploading', 'completing')) >= v_limit THEN
        RAISE EXCEPTION 'too many uploads in progress (limit %)', v_limit USING ERRCODE = '55000';
      END IF;
    END IF;
    v_id := app_id(p_ids, 1);
    INSERT INTO uploads (id, user_id, request_id, file_id, target_kind, target_id, workspace_id, owner_user_id,
                         name, note, content_type, size, chunk_bytes, storage_key, expires_at)
    VALUES (v_id, v_user, p_request, p_file,
            CASE WHEN p_file IS NULL THEN v_kind END, CASE WHEN p_file IS NULL THEN v_tgt END,
            v_place.workspace_id, v_place.owner_user_id, v_name, coalesce(p_note, ''),
            coalesce(nullif(btrim(p_type), ''), 'application/octet-stream'), p_size, p_chunk,
            'v1/' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text || '/' || v_id::text,
            now() + make_interval(secs => p_ttl_secs))
    ON CONFLICT (user_id, request_id) DO NOTHING;
    -- 같은 요청이 동시에 오면 먼저 들어간 쪽을 돌려준다.
    SELECT id INTO v_id FROM uploads WHERE user_id = v_user AND request_id = p_request;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_upload(p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text, p_type text, p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint, p_max_storage bigint, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_finish_upload(p_upload uuid, p_sha256 text, p_max_storage bigint, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_up      uploads%ROWTYPE;
    v_file    uuid;
    v_version integer;
    v_vid     uuid;
    v_place   record;
    v_limit   bigint;
    v_org     uuid;
    v_org_max bigint;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_up.state = 'done' THEN
      RETURN v_up.version_id;
    END IF;
    IF v_up.state <> 'completing' THEN
      RAISE EXCEPTION 'the upload is no longer open' USING ERRCODE = '55000';
    END IF;
    IF v_up.file_id IS NOT NULL THEN
      IF app_access('file', v_up.file_id) = 'none' THEN
        RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
      END IF;
      SELECT * INTO v_place FROM app_file_place(
        (SELECT CASE WHEN document_id IS NOT NULL THEN 'document' ELSE 'item' END FROM files WHERE id = v_up.file_id),
        (SELECT coalesce(document_id, item_id) FROM files WHERE id = v_up.file_id));
    ELSE
      SELECT * INTO v_place FROM app_file_place(v_up.target_kind, v_up.target_id);
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage:' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text, 0));
    -- 저장량 잠금을 기다리는 사이 대상 권한이 바뀌었을 수 있다(잠금 뒤 재검사).
    IF v_up.file_id IS NOT NULL THEN
      IF app_access('file', v_up.file_id) NOT IN ('owner', 'write') THEN
        RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
      END IF;
    ELSIF app_access(v_up.target_kind, v_up.target_id) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    v_limit := app_effective_limit(v_place.workspace_id, 'max_storage_bytes', p_max_storage);
    IF app_storage_used(v_place.workspace_id, v_place.owner_user_id, p_upload) + v_up.size > v_limit THEN
      RAISE EXCEPTION 'not enough storage left (limit % bytes)', v_limit USING ERRCODE = '55000';
    END IF;
    IF v_place.workspace_id IS NOT NULL THEN
      SELECT w.organization_id, o.max_storage_bytes INTO v_org, v_org_max
      FROM workspaces w JOIN organizations o ON o.id = w.organization_id WHERE w.id = v_place.workspace_id;
      IF v_org_max IS NOT NULL THEN
        PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage-org:' || v_org::text, 0));
        IF app_storage_used_organization(v_org, p_upload) + v_up.size > v_org_max THEN
          RAISE EXCEPTION 'not enough storage left in the organization (limit % bytes)', v_org_max USING ERRCODE = '55000';
        END IF;
      END IF;
    END IF;
    IF v_up.file_id IS NULL THEN
      INSERT INTO files (id, workspace_id, owner_user_id, document_id, item_id, name, created_by)
      VALUES (app_id(p_ids, 1), v_place.workspace_id, v_place.owner_user_id,
              CASE WHEN v_up.target_kind = 'document' THEN v_up.target_id END,
              CASE WHEN v_up.target_kind = 'item' THEN v_up.target_id END,
              v_up.name, v_up.user_id)
      RETURNING id INTO v_file;
    ELSE
      v_file := v_up.file_id;
    END IF;
    UPDATE files SET current_version = current_version + 1, updated_at = now()
    WHERE id = v_file RETURNING current_version INTO v_version;
    INSERT INTO file_versions (id, file_id, version, size, sha256, content_type, storage_key, note, uploaded_by)
    VALUES (app_id(p_ids, 2), v_file, v_version, v_up.size, p_sha256, v_up.content_type, v_up.storage_key, v_up.note,
            v_up.user_id)
    RETURNING id INTO v_vid;
    UPDATE uploads SET state = 'done', state_changed_at = now(), version_id = v_vid WHERE id = p_upload;
    DELETE FROM upload_parts WHERE upload_id = p_upload;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 3), v_place.workspace_id, v_place.owner_user_id, v_up.user_id, 'upload', 'file', v_file,
            jsonb_build_object('name', v_up.name, 'version', v_version, 'size', v_up.size));
    RETURN v_vid;
  END
  $$;


ALTER FUNCTION fvoci.app_finish_upload(p_upload uuid, p_sha256 text, p_max_storage bigint, p_ids uuid[]) OWNER TO fvoci_migrator;

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

-- 댓글 대상(문서 또는 업무)의 등급. 댓글 행이 아직 없을 때(만들기) 쓴다.
CREATE FUNCTION fvoci.app_comment_target_access(p_document uuid, p_task uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN p_document IS NOT NULL THEN app_access('document', p_document) ELSE app_access('task', p_task) END
  $$;


ALTER FUNCTION fvoci.app_comment_target_access(p_document uuid, p_task uuid) OWNER TO fvoci_migrator;

-- 서버 관리자·조직 관리자(D-112)는 메타데이터 함수에서만 쓴다. 내용 등급과 무관하다.
CREATE FUNCTION fvoci.app_is_server_admin() RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT EXISTS (SELECT 1 FROM server_admins WHERE user_id = app_user_id()) $$;


ALTER FUNCTION fvoci.app_is_server_admin() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_require_server_admin() RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
  BEGIN
    IF v_user IS NULL OR NOT app_is_server_admin() THEN
      RAISE EXCEPTION 'server admin required' USING ERRCODE = '42501';
    END IF;
    RETURN v_user;
  END
  $$;


ALTER FUNCTION fvoci.app_require_server_admin() OWNER TO fvoci_migrator;

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

-- 업무 상태 정의(D-116). API는 상태를 이름으로 가리키고 이름은 항목 안에서 고유하다. 범주(todo·doing·done)만 집계·알림·
-- 간트·검색 필터가 본다. 새 항목은 기본 세 상태(todo·in_progress·done, 언어 중립 키)로 시작한다.
CREATE FUNCTION fvoci.app_task_status(p_status uuid) RETURNS fvoci.task_statuses
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE id = p_status;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    RETURN v_status;
  END
  $$;


ALTER FUNCTION fvoci.app_task_status(p_status uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_status_by_name(p_item uuid, p_name text) RETURNS fvoci.task_statuses
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_status task_statuses%ROWTYPE;
  BEGIN
    SELECT * INTO v_status FROM task_statuses WHERE item_id = p_item AND name = btrim(coalesce(p_name, ''));
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    RETURN v_status;
  END
  $$;


ALTER FUNCTION fvoci.app_task_status_by_name(p_item uuid, p_name text) OWNER TO fvoci_migrator;

-- 항목의 기본 상태: todo 범주의 첫 상태, 없으면 순서상 첫 상태. 새 업무·반복 회차·초기화 복제가 쓴다.
CREATE FUNCTION fvoci.app_default_task_status(p_item uuid) RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id uuid;
  BEGIN
    SELECT id INTO v_id FROM task_statuses WHERE item_id = p_item
    ORDER BY (category <> 'todo'), position, created_at, id LIMIT 1;
    IF v_id IS NULL THEN
      RAISE EXCEPTION 'the item has no task status' USING ERRCODE = '22023';
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_default_task_status(p_item uuid) OWNER TO fvoci_migrator;

-- 새 항목의 기본 세 상태. 새 id 셋을 차례로 쓴다.
CREATE FUNCTION fvoci.app_seed_task_statuses(p_item uuid, p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO task_statuses (id, item_id, name, color, position, category)
    VALUES (app_id(p_ids, 1), p_item, '할 일', '', 1, 'todo'),
           (app_id(p_ids, 2), p_item, '진행 중', '', 2, 'doing'),
           (app_id(p_ids, 3), p_item, '완료', '', 3, 'done')
  $$;


ALTER FUNCTION fvoci.app_seed_task_statuses(p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_server_admins() RETURNS TABLE(user_id uuid, display_name text, created_at timestamp with time zone)
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_require_server_admin();
    RETURN QUERY
      SELECT a.user_id, u.display_name, a.created_at
      FROM server_admins a JOIN users u ON u.id = a.user_id
      ORDER BY a.created_at, a.user_id;
  END
  $$;


ALTER FUNCTION fvoci.app_server_admins() OWNER TO fvoci_migrator;

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
    SELECT role INTO v_old FROM organization_members WHERE organization_id = p_organization AND user_id = p_user;
    IF v_old = 'admin' AND p_role <> 'admin'
       AND (SELECT count(*) FROM organization_members WHERE organization_id = p_organization AND role = 'admin') = 1 THEN
      RAISE EXCEPTION 'the organization needs at least one admin' USING ERRCODE = '55000';
    END IF;
    INSERT INTO organization_members (organization_id, user_id, role) VALUES (p_organization, p_user, p_role)
    ON CONFLICT (organization_id, user_id) DO UPDATE SET role = EXCLUDED.role;
    IF v_old IS DISTINCT FROM p_role THEN
      INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), NULL, p_organization, v_admin,
              CASE WHEN v_old IS NULL THEN 'add_member' ELSE 'change_role' END, 'organization_member', p_user,
              jsonb_build_object('from', v_old, 'to', p_role));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_set_organization_member(p_organization uuid, p_user uuid, p_role text, p_ids uuid[]) OWNER TO fvoci_migrator;

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
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 본인 또는 조직 관리자만. 조직 전체의 받은 공유(공간 멤버십과 무관)마다 활동 1 + 공간마다 (감사 1 + 담당·리더 해제 2배) + 조직 로그 1.
    SELECT CASE WHEN app_user_id() IS NULL OR app_access('organization', p_organization) = 'none'
                     OR (app_user_id() <> p_user AND app_access('organization', p_organization) <> 'owner') THEN 0 ELSE
      1 + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
           WHERE s.organization_id = p_organization AND s.grantee_user_id = p_user AND d.workspace_id IS NOT NULL)::integer
        + coalesce((SELECT sum(1 + app_unseen_assignees_bound(NULL, m.workspace_id, p_user) * 2)
                    FROM workspace_members m JOIN workspaces w ON w.id = m.workspace_id
                    WHERE w.organization_id = p_organization AND m.user_id = p_user), 0)::integer
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
    IF NOT EXISTS (SELECT 1 FROM organization_members WHERE organization_id = p_organization AND user_id = p_user) THEN
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
    SELECT u.id INTO v_user FROM users u WHERE u.email_verified AND lower(u.email) = v_inv.email
    ORDER BY u.created_at LIMIT 1;
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
  BEGIN
    SELECT * INTO v_ws FROM workspaces WHERE id = p_workspace AND deleted_at IS NULL;
    IF v_ws.id IS NULL OR app_access('organization', v_ws.organization_id) = 'none' THEN
      RAISE EXCEPTION 'workspace not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_organization_admin(v_ws.organization_id);
    -- 조직 행 → 워크스페이스 행 → 멤버 행 순서로 잠그고 관리자인지 다시 본다(강등·탈퇴와 직렬화).
    PERFORM 1 FROM organizations WHERE id = v_ws.organization_id FOR SHARE;
    PERFORM 1 FROM workspaces WHERE id = p_workspace AND deleted_at IS NULL FOR NO KEY UPDATE;
    PERFORM app_require_organization_admin(v_ws.organization_id);
    SELECT role INTO v_old FROM workspace_members WHERE workspace_id = p_workspace AND user_id = v_user FOR UPDATE;
    IF v_old = 'admin' THEN
      RETURN;
    END IF;
    INSERT INTO workspace_members (workspace_id, user_id, role) VALUES (p_workspace, v_user, 'admin')
    ON CONFLICT (workspace_id, user_id) DO UPDATE SET role = 'admin';
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
    INSERT INTO workspace_members (workspace_id, user_id, role) VALUES (v_id, v_user, 'admin');
    INSERT INTO audit_log (id, workspace_id, organization_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), v_id, p_organization, v_user, 'create', 'workspace', v_id,
            jsonb_build_object('name', btrim(p_name))),
           (app_id(p_ids, 3), NULL, p_organization, v_user, 'create', 'workspace', v_id,
            jsonb_build_object('name', btrim(p_name)));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_workspace(p_request_id uuid, p_organization uuid, p_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 워크스페이스 초대(워크스페이스 관리자, D-20·D-118). 받는 사람은 수락할 때 조직 멤버여야 한다(D-114). 발송 단계에서 조직 밖
-- 이메일을 막을지는 사용자 결정 대기다. 새 id 3: 초대, 공간 로그, 알림.
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
      AND me.email_verified AND lower(me.email) = i.email
    ORDER BY i.created_at
  $$;


ALTER FUNCTION fvoci.app_my_invitations() OWNER TO fvoci_migrator;

-- 초대 수락·거절(D-51·D-118). 조직 초대는 조직 멤버가 되고, 워크스페이스 초대는 그 조직의 멤버여야 수락할 수 있다(D-114).
-- 돌려주는 값은 워크스페이스 초대면 워크스페이스 id, 조직 초대면 조직 id다. 알림함의 invite 알림에 결과를 남긴다.
CREATE FUNCTION fvoci.app_respond_invitation(p_invitation uuid, p_accept boolean, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_inv  invitations%ROWTYPE;
    v_ok   boolean;
  BEGIN
    SELECT * INTO v_inv FROM invitations WHERE id = p_invitation;
    SELECT u.email_verified AND lower(u.email) = v_inv.email INTO v_ok FROM users u WHERE u.id = v_user;
    -- 대상이 아니면 초대의 존재를 알리지 않는다.
    IF v_user IS NULL OR v_inv.id IS NULL OR NOT coalesce(v_ok, false) THEN
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
        INSERT INTO workspace_members (workspace_id, user_id, role)
        VALUES (v_inv.workspace_id, v_user, v_inv.role)
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
    IF p_action = 'organization_left' THEN
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
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('workspace', p_workspace) = 'owner'
                THEN 1 + 2 * app_unseen_assignees_bound(NULL, p_workspace, p_member)
                       + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
                          WHERE d.workspace_id = p_workspace AND s.grantee_user_id = p_member)::integer
                ELSE 0 END
  $$;


ALTER FUNCTION fvoci.app_remove_member_ids(p_workspace uuid, p_member uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_leave_workspace_ids(p_workspace uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL AND app_access('workspace', p_workspace) <> 'none'
                THEN 1 + 2 * app_unseen_assignees_bound(NULL, p_workspace, app_user_id())
                       + (SELECT count(*) FROM document_shares s JOIN documents d ON d.id = s.document_id
                          WHERE d.workspace_id = p_workspace AND s.grantee_user_id = app_user_id())::integer
                ELSE 0 END
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
    IF p_to = v_ws.owner_id OR NOT EXISTS (
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

CREATE FUNCTION fvoci.app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task tasks%ROWTYPE;
    v_item jsonb;
    v_next numeric;
    v_at   integer := 0;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task FOR UPDATE;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    SELECT coalesce(max(position), 0) + 1 INTO v_next FROM checklist_items WHERE task_id = p_task;
    FOR v_item IN SELECT * FROM jsonb_array_elements(coalesce(p_items, '[]'::jsonb)) LOOP
      v_at := v_at + 1;
      INSERT INTO checklist_items (id, task_id, text, checked, position, request_id, created_by)
      VALUES (app_id(p_ids, v_at), p_task, btrim(v_item ->> 'text'), coalesce((v_item ->> 'checked')::boolean, false),
              v_next, (v_item ->> 'request_id')::uuid, app_user_id())
      ON CONFLICT ON CONSTRAINT checklist_items_request DO NOTHING;
      IF FOUND THEN
        v_next := v_next + 1;
      END IF;
    END LOOP;
  END
  $$;


ALTER FUNCTION fvoci.app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
    v_at    integer := 0;
  BEGIN
    PERFORM app_require_course_write(p_item);
    -- 항목을 잠근 뒤 다시 본다(기다리는 사이 공개 범위·보관이 바뀌었을 수 있다).
    PERFORM app_lock_item(p_item);
    PERFORM app_require_course_write(p_item);
    IF jsonb_typeof(p_rows) <> 'array' OR jsonb_array_length(p_rows) NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'give 1 to 1000 episodes' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR NO KEY UPDATE;
    SELECT coalesce(max(number), 0) + 1 INTO v_next FROM course_episodes WHERE item_id = p_item;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_rows) LOOP
      IF length(btrim(coalesce(v_row->>'title', ''))) NOT BETWEEN 1 AND 300 THEN
        RAISE EXCEPTION 'every episode needs a title (1-300 characters)' USING ERRCODE = '22023';
      END IF;
      v_at := v_at + 1;
      INSERT INTO course_episodes (id, item_id, workspace_id, number, title, chapter, duration_seconds, url, materials,
                                   planned_on, created_by, request_id)
      VALUES (app_id(p_ids, v_at), p_item, v_ws, coalesce((v_row->>'number')::integer, v_next), btrim(v_row->>'title'),
              btrim(coalesce(v_row->>'chapter', '')), coalesce((v_row->>'duration_seconds')::integer, 0),
              btrim(coalesce(v_row->>'url', '')), coalesce(v_row->>'materials', ''),
              (v_row->>'planned_on')::date, app_user_id(), (v_row->>'request_id')::uuid)
      ON CONFLICT (item_id, created_by, request_id) DO NOTHING;
      IF FOUND THEN
        v_added := v_added + 1;
        v_next := greatest(v_next, coalesce((v_row->>'number')::integer, v_next)) + 1;
      END IF;
    END LOOP;
    IF v_added > 0 THEN
      PERFORM app_audit(app_id(p_ids, v_at + 1), v_ws, 'add_episodes', 'item', p_item,
                        jsonb_build_object('count', v_added));
    END IF;
    RETURN v_added;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow
         OR numeric_value_out_of_range OR check_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws    uuid;
    v_row   jsonb;
    v_next  integer;
    v_added integer := 0;
    v_at    integer := 0;
  BEGIN
    PERFORM app_require_study_write(p_item);
    -- 항목을 잠근 뒤 다시 본다(기다리는 사이 공개 범위·보관이 바뀌었을 수 있다).
    PERFORM app_lock_item(p_item);
    PERFORM app_require_study_write(p_item);
    IF jsonb_typeof(p_rows) <> 'array' OR jsonb_array_length(p_rows) NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'give 1 to 1000 units' USING ERRCODE = '22023';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR NO KEY UPDATE;
    SELECT coalesce(max(number), 0) + 1 INTO v_next FROM study_units WHERE item_id = p_item;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_rows) LOOP
      v_at := v_at + 1;
      INSERT INTO study_units (id, item_id, workspace_id, number, title, chapter, unit, amount, planned_on, created_by,
                               request_id)
      VALUES (app_id(p_ids, v_at), p_item, v_ws, coalesce((v_row->>'number')::integer, v_next), btrim(v_row->>'title'),
              btrim(coalesce(v_row->>'chapter', '')), coalesce(v_row->>'unit', 'pages'),
              coalesce((v_row->>'amount')::integer, 0), (v_row->>'planned_on')::date, app_user_id(),
              (v_row->>'request_id')::uuid)
      ON CONFLICT (item_id, created_by, request_id) DO NOTHING;
      IF FOUND THEN
        v_added := v_added + 1;
        v_next := greatest(v_next, coalesce((v_row->>'number')::integer, v_next)) + 1;
      END IF;
    END LOOP;
    IF v_added > 0 THEN
      PERFORM app_audit(app_id(p_ids, v_at + 1), v_ws, 'add_study_units', 'item', p_item,
                        jsonb_build_object('count', v_added));
    END IF;
    RETURN v_added;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow
         OR numeric_value_out_of_range OR check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_add_time_entry(p_request_id uuid, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_id   uuid;
    v_ws   uuid;
  BEGIN
    IF v_user IS NULL THEN
      RAISE EXCEPTION 'sign in required' USING ERRCODE = '42501';
    END IF;
    -- 타이머 시작·이어가기와 요청 키를 공유한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-timer:' || v_user::text, 0));
    SELECT r.entry_id INTO v_id FROM time_entry_requests r
    WHERE r.user_id = v_user AND r.request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    v_ws := app_time_target_workspace(p_target_kind, p_target);
    INSERT INTO time_entries (id, user_id, started_at, ended_at, paused_seconds, kind, memo, target_kind, target_id,
                              workspace_id, request_id)
    VALUES (app_id(p_ids, 1), v_user, p_started, p_ended, p_paused, p_kind, coalesce(p_memo, ''), p_target_kind,
            p_target, v_ws, p_request_id)
    RETURNING id INTO v_id;
    INSERT INTO time_entry_requests (user_id, request_id, entry_id)
    VALUES (v_user, p_request_id, v_id);
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_add_time_entry(p_request_id uuid, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_batch_documents(p_batch fvoci.trash_batches) RETURNS TABLE(document_id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT d.id FROM documents d
    WHERE (p_batch.root_kind = 'document' AND d.trash_batch_id = p_batch.id)
       OR (p_batch.root_kind = 'item' AND d.item_id = p_batch.root_id)
       OR (p_batch.root_kind = 'module'
           AND d.item_id IN (SELECT i.id FROM items i WHERE i.module_id = p_batch.root_id))
  $$;


ALTER FUNCTION fvoci.app_batch_documents(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_batch_expired(p_batch uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT b.restored_at IS NULL AND b.purged_at IS NULL AND app_trash_expires_at(b) <= now()
                     FROM trash_batches b WHERE b.id = p_batch), false)
  $$;


ALTER FUNCTION fvoci.app_batch_expired(p_batch uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_batch_files(p_batch fvoci.trash_batches) RETURNS TABLE(file_id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT f.id FROM files f WHERE p_batch.root_kind = 'file' AND f.trash_batch_id = p_batch.id
    UNION ALL
    SELECT f.id FROM files f WHERE p_batch.root_kind = 'item' AND f.item_id = p_batch.root_id
    UNION ALL
    SELECT f.id FROM files f JOIN items i ON i.id = f.item_id
    WHERE p_batch.root_kind = 'module' AND i.module_id = p_batch.root_id
    UNION ALL
    SELECT f.id FROM app_batch_documents(p_batch) d JOIN files f ON f.document_id = d.document_id
  $$;


ALTER FUNCTION fvoci.app_batch_files(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_ref_in_scope(p_doc_ws uuid, p_doc_owner uuid, p_file_ws uuid, p_file_owner uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 파일 참조의 유효 범위(D-114): 개인 파일은 같은 소유자의 문서만, 공간 파일은 같은 조직의 공간 문서 또는 그 조직 멤버가
    -- 소유한 개인 노트만. 참조 기록(app_record_file_refs)·버전 보존(app_version_referenced)·휴지통 보존
    -- (app_batch_keeps_referenced)이 같은 규칙을 쓴다.
    SELECT CASE
      WHEN p_file_owner IS NOT NULL THEN p_doc_owner IS NOT DISTINCT FROM p_file_owner
      WHEN p_doc_ws IS NOT NULL THEN
        (SELECT w.organization_id FROM workspaces w WHERE w.id = p_doc_ws)
          = (SELECT w.organization_id FROM workspaces w WHERE w.id = p_file_ws)
      WHEN p_doc_owner IS NOT NULL THEN EXISTS (
        SELECT 1 FROM organization_members m JOIN workspaces w ON w.organization_id = m.organization_id
        WHERE w.id = p_file_ws AND m.user_id = p_doc_owner)
      ELSE false
    END
  $$;


ALTER FUNCTION fvoci.app_file_ref_in_scope(p_doc_ws uuid, p_doc_owner uuid, p_file_ws uuid, p_file_owner uuid) OWNER TO fvoci_migrator;

REVOKE ALL ON FUNCTION fvoci.app_file_ref_in_scope(p_doc_ws uuid, p_doc_owner uuid, p_file_ws uuid, p_file_owner uuid) FROM PUBLIC;

CREATE FUNCTION fvoci.app_batch_keeps_referenced(p_batch fvoci.trash_batches) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH bf AS (SELECT file_id FROM app_batch_files(p_batch)),
         gone AS (SELECT document_id FROM app_batch_documents(p_batch))
    SELECT EXISTS (
             SELECT 1 FROM bf
             JOIN file_versions v ON v.file_id = bf.file_id
             JOIN record_files rf ON rf.version_id = v.id
             JOIN item_records r ON r.id = rf.record_id
             JOIN items i ON i.id = r.item_id
             JOIN modules m ON m.id = i.module_id
             WHERE NOT (p_batch.root_kind = 'item' AND r.item_id = p_batch.root_id)
               AND NOT (p_batch.root_kind = 'module' AND i.module_id = p_batch.root_id)
               AND NOT app_batch_expired(i.trash_batch_id)
               AND NOT app_batch_expired(m.trash_batch_id))
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_file_refs ref ON ref.file_id = f.id
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND app_file_ref_in_scope(d.workspace_id, d.owner_user_id, f.workspace_id, f.owner_user_id))
        -- 리비전 줄과 같은 규칙(휴지통 만료), 낡은 상태는 빼고.
        OR EXISTS (
             SELECT 1 FROM bf
             JOIN files f ON f.id = bf.file_id
             JOIN document_collab_file_refs ref ON ref.file_id = f.id
             JOIN document_collab_states s ON s.document_id = ref.document_id AND s.pending
             JOIN file_versions v ON v.file_id = f.id AND v.version = ref.version
             JOIN documents d ON d.id = ref.document_id
             LEFT JOIN items di ON di.id = d.item_id
             LEFT JOIN modules dm ON dm.id = di.module_id
             WHERE app_collab_state_current(s.document_id, s.revision)
               AND NOT EXISTS (SELECT 1 FROM gone WHERE gone.document_id = d.id)
               AND NOT app_batch_expired(d.trash_batch_id)
               AND NOT app_batch_expired(di.trash_batch_id)
               AND NOT app_batch_expired(dm.trash_batch_id)
               AND app_file_ref_in_scope(d.workspace_id, d.owner_user_id, f.workspace_id, f.owner_user_id))
  $$;


ALTER FUNCTION fvoci.app_batch_keeps_referenced(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_begin_complete(p_upload uuid) RETURNS fvoci.uploads
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_up    uploads%ROWTYPE;
    v_parts integer;
    v_bytes bigint;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_up.state = 'done' THEN
      RETURN v_up;
    END IF;
    IF v_up.state = 'completing' AND v_up.state_changed_at > now() - interval '5 minutes' THEN
      RAISE EXCEPTION 'the upload is being completed; try again shortly' USING ERRCODE = '55000';
    END IF;
    IF v_up.state NOT IN ('uploading', 'completing') OR v_up.expires_at <= now() THEN
      RAISE EXCEPTION 'the upload is no longer open' USING ERRCODE = '55000';
    END IF;
    SELECT count(*), coalesce(sum(size), 0) INTO v_parts, v_bytes FROM upload_parts WHERE upload_id = p_upload;
    IF v_bytes <> v_up.size OR v_parts <> greatest(1, ceil(v_up.size::numeric / v_up.chunk_bytes))::integer
       OR EXISTS (SELECT 1 FROM upload_parts WHERE upload_id = p_upload
                  AND number > greatest(1, ceil(v_up.size::numeric / v_up.chunk_bytes))::integer) THEN
      RAISE EXCEPTION 'some parts are missing' USING ERRCODE = '55000';
    END IF;
    -- 마치는 동안 기한이 지나 닫히지 않게 늘린다.
    UPDATE uploads SET state = 'completing', state_changed_at = now(),
                       expires_at = greatest(expires_at, now() + interval '1 hour')
    WHERE id = p_upload RETURNING * INTO v_up;
    RETURN v_up;
  END
  $$;


ALTER FUNCTION fvoci.app_begin_complete(p_upload uuid) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_check_assignee(p_workspace uuid, p_item uuid, p_user uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_user FOR SHARE;
    IF app_access_as('item', p_item, p_user) = 'none' THEN
      RAISE EXCEPTION 'an assignee cannot see this item' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_assignee(p_workspace uuid, p_item uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_recurrence(p_rule jsonb) RETURNS void
    LANGUAGE plpgsql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_interval integer;
  BEGIN
    IF jsonb_typeof(p_rule) <> 'object' OR p_rule ->> 'freq' NOT IN ('daily', 'weekly', 'monthly', 'yearly') THEN
      RAISE EXCEPTION 'invalid recurrence frequency' USING ERRCODE = '22023';
    END IF;
    v_interval := (p_rule ->> 'interval')::integer;
    IF v_interval IS NULL OR v_interval NOT BETWEEN 1 AND 99 THEN
      RAISE EXCEPTION 'interval must be 1 to 99' USING ERRCODE = '22023';
    END IF;
    IF jsonb_typeof(p_rule -> 'weekdays') <> 'array'
       OR EXISTS (SELECT 1 FROM jsonb_array_elements(p_rule -> 'weekdays') w
                  WHERE jsonb_typeof(w) <> 'number' OR w::integer NOT BETWEEN 1 AND 7)
       OR (p_rule ->> 'freq' <> 'weekly' AND jsonb_array_length(p_rule -> 'weekdays') > 0) THEN
      RAISE EXCEPTION 'weekdays must be ISO weekdays and only for weekly rules' USING ERRCODE = '22023';
    END IF;
    IF p_rule ? 'count' AND p_rule -> 'count' <> 'null'::jsonb
       AND (p_rule ->> 'count')::integer NOT BETWEEN 1 AND 1000 THEN
      RAISE EXCEPTION 'count must be 1 to 1000' USING ERRCODE = '22023';
    END IF;
    IF p_rule ->> 'until' IS NOT NULL AND p_rule ->> 'count' IS NOT NULL THEN
      RAISE EXCEPTION 'use either until or count' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_recurrence(p_rule jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_parent tasks%ROWTYPE;
  BEGIN
    IF p_parent IS NULL THEN
      RETURN;
    END IF;
    IF p_task IS NOT NULL AND p_parent = p_task THEN
      RAISE EXCEPTION 'a task cannot be its own parent' USING ERRCODE = '22023';
    END IF;
    -- 볼 수 없는 업무는 잠그지 않는다(잠금 대기로 존재가 드러나지 않게). 잠근 뒤 다시 확인한다.
    IF app_access('task', p_parent) = 'none' THEN
      RAISE EXCEPTION 'parent task not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_task IS NOT NULL THEN
      -- 두 업무의 열을 먼저, 그다음 행(app_lock_task_rows).
      PERFORM app_lock_task_rows(ARRAY[p_task, p_parent], NULL);
    ELSE
      -- 새 업무: 만드는 쪽은 이미 할 일 열을 잡았고 이 행 뒤로는 다른 업무 행을 기다리지 않는다.
      PERFORM 1 FROM tasks WHERE id = p_parent FOR NO KEY UPDATE;
    END IF;
    SELECT * INTO v_parent FROM tasks WHERE id = p_parent;
    IF v_parent.id IS NULL OR app_access('task', p_parent) = 'none' THEN
      RAISE EXCEPTION 'parent task not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_parent.item_id <> p_item THEN
      RAISE EXCEPTION 'the parent task must be in the same item' USING ERRCODE = '22023';
    END IF;
    IF v_parent.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'subtasks can only be one level deep' USING ERRCODE = '22023';
    END IF;
    IF p_task IS NOT NULL AND EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task) THEN
      RAISE EXCEPTION 'a task with subtasks cannot become a subtask' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_check_timezone(p_tz text) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_tz IS NULL OR p_tz ~ '^(posix|right|SystemV)/' OR p_tz IN ('Factory', 'localtime', 'posixrules')
       OR NOT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = p_tz) THEN
      RAISE EXCEPTION 'unknown time zone' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_check_timezone(p_tz text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_claim_schedule(p_name text, p_every interval) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_last timestamptz;
  BEGIN
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-schedule:' || p_name, 0));
    SELECT last_run_at INTO v_last FROM scheduled_runs WHERE name = p_name;
    IF v_last IS NOT NULL AND v_last + p_every > now() THEN
      RETURN false;
    END IF;
    INSERT INTO scheduled_runs (name, last_run_at) VALUES (p_name, now())
    ON CONFLICT (name) DO UPDATE SET last_run_at = now();
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_claim_schedule(p_name text, p_every interval) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_access(p_doc uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('document', p_doc);
  BEGIN
    IF app_user_id() IS NULL OR v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT app_document_realtime(p_doc) THEN
      RAISE EXCEPTION 'realtime editing is off for this document' USING ERRCODE = '55000';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_collab_access(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_load(p_doc uuid) RETURNS TABLE(doc_revision integer, title text, body jsonb, state bytea, editors uuid[])
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_collab_access(p_doc);
    RETURN QUERY
    SELECT d.revision, d.title, d.body, cs.state, cs.editors
    FROM documents d
    -- 상태가 이어받은 리비전 뒤로 본문이 바뀌지 않았으면(제목만 바꾼 저장 등) 그 상태와 편집자를 그대로 쓴다.
    LEFT JOIN LATERAL (
      SELECT s.state, s.editors FROM document_collab_states s
      WHERE s.document_id = d.id
        AND NOT EXISTS (
          SELECT 1 FROM document_revisions r
          WHERE r.document_id = d.id AND r.revision > s.revision
            AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                         WHERE b.document_id = d.id AND b.revision = s.revision))
    ) cs ON true
    WHERE d.id = p_doc;
  END
  $$;


ALTER FUNCTION fvoci.app_collab_load(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_pending(p_workspace uuid) RETURNS SETOF uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can change realtime editing' USING ERRCODE = '42501';
    END IF;
    RETURN QUERY
      SELECT s.document_id
      FROM document_collab_states s
      JOIN documents d ON d.id = s.document_id
      WHERE s.pending AND d.workspace_id = p_workspace AND d.deleted_at IS NULL
        AND NOT EXISTS (SELECT 1 FROM document_revisions r
                        WHERE r.document_id = s.document_id AND r.revision > s.revision
                          AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                       WHERE b.document_id = s.document_id
                                                         AND b.revision = s.revision))
      ORDER BY s.document_id;
  END
  $$;


ALTER FUNCTION fvoci.app_collab_pending(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_collab_state_current(p_doc uuid, p_revision integer) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT NOT EXISTS (SELECT 1 FROM document_revisions r
                       WHERE r.document_id = p_doc AND r.revision > p_revision
                         AND r.body IS DISTINCT FROM (SELECT b.body FROM document_revisions b
                                                      WHERE b.document_id = p_doc AND b.revision = p_revision))
  $$;


ALTER FUNCTION fvoci.app_collab_state_current(p_doc uuid, p_revision integer) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_course_access(p_item uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
  BEGIN
    IF v_access = 'none' OR NOT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = p_item AND m.kind = 'course') THEN
      RAISE EXCEPTION 'course not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_course_access(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_course_participants(p_item uuid, p_today date) RETURNS TABLE(user_id uuid, display_name text, done integer, in_progress integer, overdue integer, done_seconds bigint)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
      WHERE app_access_as('item', p_item, u.id) <> 'none'
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;


ALTER FUNCTION fvoci.app_course_participants(p_item uuid, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_access text := app_access('item', p_item);
    v_item   items%ROWTYPE;
    v_id     uuid;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT id INTO v_id FROM events WHERE item_id = p_item AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    PERFORM app_check_timezone(p_timezone);
    PERFORM app_lock_item(p_item);
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 잠금 뒤 등급을 다시 본다(기다리는 사이 공개 범위·멤버가 바뀌었을 수 있다).
    IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    INSERT INTO events (id, workspace_id, item_id, kind, title, description, all_day, start_date, end_date,
                        starts_at, ends_at, timezone, created_by, updated_by, request_id)
    VALUES (app_id(p_ids, 1), v_item.workspace_id, p_item, p_kind, btrim(p_title), coalesce(p_description, ''),
            p_all_day, p_start_date, p_end_date, p_starts_at, p_ends_at, p_timezone, v_user, v_user, p_request_id)
    RETURNING id INTO v_id;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'event', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    PERFORM app_activity(app_id(p_ids, 3), v_item.workspace_id, p_item, 'event', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_create_login_state(p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    -- 만료 행은 한 번에 일부만 지운다(요청마다 전체 스캔을 피한다).
    DELETE FROM oidc_login_states
    WHERE secret_hash IN (
      SELECT secret_hash FROM oidc_login_states WHERE expires_at < now() LIMIT 100
    );
    INSERT INTO oidc_login_states (secret_hash, state, pkce_verifier, nonce, return_to, expires_at)
    VALUES (p_secret_hash, p_state, p_verifier, p_nonce, p_return_to,
            now() + make_interval(secs => p_ttl_secs));
  END
  $$;


ALTER FUNCTION fvoci.app_create_login_state(p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_item     items%ROWTYPE;
    v_access   text;
    v_id       uuid;
    v_existing uuid;
    v_number   bigint;
  BEGIN
    v_access := app_access('item', p_item);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    SELECT id, item_id INTO v_id, v_existing FROM documents
    WHERE workspace_id = v_item.workspace_id AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      IF v_existing IS DISTINCT FROM p_item THEN
        RAISE EXCEPTION 'request_id was already used for another document' USING ERRCODE = '55000';
      END IF;
      RETURN v_id;
    END IF;
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(v_item.workspace_id, p_item, NULL);
    END IF;
    PERFORM app_lock_item(p_item);
    IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'document', v_id);
    INSERT INTO documents (id, workspace_id, item_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, 'module', v_item.module_id::text, v_number, btrim(p_title),
            p_schema_version, p_body, 1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    PERFORM app_activity(app_id(p_ids, 3), v_item.workspace_id, p_item, 'document', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 4));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
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
    SELECT id INTO v_id FROM documents WHERE owner_user_id = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(NULL, NULL, v_user);
    END IF;
    INSERT INTO documents (id, owner_user_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id, visibility)
    VALUES (app_id(p_ids, 1), v_user, 'personal', 'NOTE', NULL, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id, 'private')
    RETURNING id INTO v_id;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 3));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id uuid;
    v_ws uuid;
  BEGIN
    SELECT id INTO v_id FROM item_records
    WHERE item_id = p_item AND created_by = app_user_id() AND request_id = p_request;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item FOR KEY SHARE;
    PERFORM app_record_write_check(p_item);
    INSERT INTO item_records (id, item_id, workspace_id, kind, title, occurred_on, body, fields, created_by, request_id)
    VALUES (app_id(p_ids, 1), p_item, v_ws, p_kind, btrim(p_title), p_occurred, coalesce(p_body, ''),
            coalesce(p_fields, '{}'), app_user_id(), p_request)
    RETURNING id INTO v_id;
    PERFORM app_set_record_files(v_id, p_item, p_files);
    PERFORM app_audit(app_id(p_ids, 2), v_ws, 'create_record', 'item', p_item,
                      jsonb_build_object('record', v_id, 'kind', p_kind));
    RETURN v_id;
  EXCEPTION
    WHEN check_violation THEN
      RAISE EXCEPTION 'invalid record: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    -- 로그인 중이던 세션이 있으면 회전한다(이전 토큰 무효).
    IF p_replaces IS NOT NULL THEN
      DELETE FROM sessions WHERE token_hash = p_replaces;
    END IF;
    DELETE FROM sessions
    WHERE id IN (SELECT id FROM sessions WHERE expires_at < now() LIMIT 100);
    INSERT INTO sessions (id, token_hash, user_id, id_token, expires_at)
    VALUES (app_id(p_ids, 1), p_token_hash, p_user, p_id_token, now() + make_interval(secs => p_ttl_secs));
  END
  $$;


ALTER FUNCTION fvoci.app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-105: 업무 한 건의 필드·담당·번호·감사 기록을 같은 트랜잭션으로 만든다. docs/decisions/tasks.md#d-105.
-- 같은 요청 ID는 입력을 덮거나 비교하지 않고 기존 업무를 반환한다. 삭제된 결과는 FV004이며 권한 검사 뒤에만 알린다.
-- 잠금은 열 → 상위 업무 → 항목 공유 → 멤버 → 번호 순서다. D-105: docs/decisions/tasks.md#d-105.
CREATE FUNCTION fvoci.app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_item   items%ROWTYPE;
    v_access text := app_access('item', p_item);
    v_status task_statuses%ROWTYPE;
    v_id     uuid;
    v_number bigint;
    v_uid    uuid;
    v_people uuid[];
    v_at     integer := 3;
    v_deleted boolean;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    SELECT id, deleted_at IS NOT NULL INTO v_id, v_deleted
    FROM tasks WHERE item_id = p_item AND created_by = v_user AND request_id = p_request_id;
    IF v_deleted THEN
      RAISE EXCEPTION 'the task made by this request was deleted' USING ERRCODE = 'FV004';
    END IF;
    IF v_id IS NOT NULL THEN
      RETURN v_id;
    END IF;
    -- 상태는 항목의 상태 정의 이름이다(D-116). 비우면 기본 상태.
    IF p_status IS NULL THEN
      v_status := app_task_status(app_default_task_status(p_item));
    ELSE
      v_status := app_task_status_by_name(p_item, p_status);
    END IF;
    IF p_priority IS NULL OR p_priority NOT IN ('none', 'low', 'medium', 'high', 'urgent') THEN
      RAISE EXCEPTION 'invalid priority' USING ERRCODE = '22023';
    END IF;
    IF p_start IS NOT NULL AND p_due IS NOT NULL AND p_start > p_due THEN
      RAISE EXCEPTION 'the start date must not be after the due date' USING ERRCODE = '22023';
    END IF;
    IF p_milestone AND p_start IS NOT NULL THEN
      RAISE EXCEPTION 'a milestone has only a due date' USING ERRCODE = '22023';
    END IF;
    IF p_estimate IS NOT NULL AND p_estimate < 0 THEN
      RAISE EXCEPTION 'estimate_minutes must not be negative' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item;
    SELECT coalesce(array_agg(u ORDER BY u), '{}') INTO v_people
    FROM (SELECT DISTINCT u FROM unnest(coalesce(p_assignees, '{}')) AS u) s;
    -- 담당자 후보의 멤버 행을 id 순으로 먼저 공유 잠금한다(멤버 행 → 열 → 항목 행: 내보내기·역할 변경과 같은 순서).
    PERFORM 1 FROM workspace_members WHERE workspace_id = v_item.workspace_id AND user_id = ANY (v_people)
    ORDER BY user_id FOR SHARE;
    PERFORM app_lock_lane(v_status.id);
    PERFORM app_check_task_parent(NULL, p_parent, p_item);
    PERFORM app_lock_item(p_item);
    -- 잠금 뒤의 상태로 다시 본다: 권한·휴지통·보관·상태 정의(상태 지우기는 그 열을 잠근 뒤 지운다).
    IF app_access('item', p_item) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 열 잠금 뒤 상태 정의를 다시 읽는다(기다리는 사이 이름·범주가 바뀌었거나 지워졌을 수 있다).
    SELECT * INTO v_status FROM task_statuses WHERE id = v_status.id;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    FOREACH v_uid IN ARRAY v_people LOOP
      PERFORM app_check_assignee(v_item.workspace_id, p_item, v_uid);
    END LOOP;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(v_item.workspace_id, v_item.module_id, 'task', v_id);
    INSERT INTO tasks (id, workspace_id, item_id, parent_id, number, title, description, status_id, priority,
                       start_date, due_date, estimate_minutes, tags, milestone, completed_at, position, created_by,
                       updated_by, request_id)
    VALUES (v_id, v_item.workspace_id, p_item, p_parent, v_number, btrim(p_title), coalesce(p_description, ''),
            v_status.id, p_priority, p_start, p_due, p_estimate, coalesce(p_tags, '{}'), coalesce(p_milestone, false),
            CASE WHEN v_status.category = 'done' THEN now() END, app_task_lane_end(v_status.id), v_user, v_user,
            p_request_id);
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'create', 'task', v_id,
                      jsonb_build_object('title', btrim(p_title)));
    PERFORM app_activity(app_id(p_ids, 3), v_item.workspace_id, p_item, 'task', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    FOREACH v_uid IN ARRAY v_people LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_item.workspace_id, 'assign', 'task', v_id,
                        jsonb_build_object('user', v_uid));
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_item.workspace_id, p_item, 'task', v_id, 'assign',
                           jsonb_build_object('user', v_uid));
    END LOOP;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) RETURNS TABLE(view_id uuid, outcome text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_view saved_views%ROWTYPE;
    v_id   uuid;
  BEGIN
    SELECT * INTO v_view FROM saved_views
    WHERE user_id = app_user_id() AND (request_id = p_request OR updated_request_id = p_request);
    IF v_view.id IS NOT NULL THEN
      IF v_view.request_id <> p_request OR v_view.item_id <> p_item OR v_view.name <> btrim(p_name) THEN
        RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
      END IF;
      RETURN QUERY SELECT v_view.id, 'created'::text;
      RETURN;
    END IF;
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO saved_views (id, user_id, item_id, name, config, request_id)
    VALUES (app_id(p_ids, 1), app_user_id(), p_item, btrim(p_name), p_config, p_request)
    ON CONFLICT (user_id, item_id, name) DO NOTHING
    RETURNING id INTO v_id;
    IF v_id IS NULL THEN
      RETURN QUERY SELECT id, 'exists'::text FROM saved_views
      WHERE user_id = app_user_id() AND item_id = p_item AND name = btrim(p_name);
      RETURN;
    END IF;
    RETURN QUERY SELECT v_id, 'created'::text;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user      uuid := app_user_id();
    v_id        uuid;
    v_placement text;
    v_number    bigint;
  BEGIN
    IF v_user IS NULL OR app_access('workspace', p_workspace) = 'none'
       OR app_access('workspace', p_workspace) NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'not allowed' USING ERRCODE = '42501';
    END IF;
    SELECT id, placement INTO v_id, v_placement FROM documents
    WHERE workspace_id = p_workspace AND created_by = v_user AND request_id = p_request_id;
    IF v_id IS NOT NULL THEN
      IF v_placement <> 'wiki' THEN
        RAISE EXCEPTION 'request_id was already used for another document' USING ERRCODE = '55000';
      END IF;
      RETURN v_id;
    END IF;
    IF p_parent IS NOT NULL THEN
      PERFORM app_lock_tree(p_workspace, NULL, NULL);
    END IF;
    v_id := app_id(p_ids, 1);
    v_number := app_allocate_number(p_workspace, p_workspace, 'document', v_id);
    INSERT INTO documents (id, workspace_id, placement, scope, number, title, schema_version, body,
                           revision, created_by, updated_by, request_id)
    VALUES (v_id, p_workspace, 'wiki', 'WIKI', v_number, btrim(p_title), p_schema_version, p_body,
            1, v_user, v_user, p_request_id);
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id)
    VALUES (v_id, 1, btrim(p_title), p_schema_version, p_body, v_user, p_request_id);
    PERFORM app_audit_document(app_id(p_ids, 2), v_id, 'create', jsonb_build_object('title', btrim(p_title)));
    PERFORM app_activity(app_id(p_ids, 3), p_workspace, NULL, 'document', v_id, 'create',
                         jsonb_build_object('title', btrim(p_title)));
    IF p_parent IS NOT NULL THEN
      PERFORM app_place_document(v_id, p_parent, NULL, p_max_depth, app_ids_from(p_ids, 4));
    END IF;
    RETURN v_id;
  END
  $$;


ALTER FUNCTION fvoci.app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_delete_checklist_item(p_id uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_access('task', v_task) = 'none' THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    PERFORM app_lock_item((SELECT item_id FROM tasks WHERE id = v_task));
    PERFORM app_require_task_write(v_task);
    DELETE FROM checklist_items WHERE id = p_id;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_checklist_item(p_id uuid) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_delete_episode(p_episode uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ep course_episodes%ROWTYPE;
  BEGIN
    SELECT * INTO v_ep FROM course_episodes WHERE id = p_episode;
    IF v_ep.id IS NULL OR app_access('item', v_ep.item_id) = 'none' THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_ep.item_id);
    DELETE FROM course_episodes WHERE id = p_episode;
    PERFORM app_audit(app_id(p_ids, 1), v_ep.workspace_id, 'delete_episode', 'item', v_ep.item_id,
                      jsonb_build_object('number', v_ep.number, 'title', v_ep.title));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_episode(p_episode uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_event(p_event uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_event events%ROWTYPE;
    v_batch uuid;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_event.workspace_id, 'event', p_event, v_event.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE events SET deleted_at = now(), deleted_by = app_user_id(), trash_batch_id = v_batch WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 2), v_event.workspace_id, 'delete', 'event', p_event,
                      jsonb_build_object('title', v_event.title, 'batch', v_batch));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_event(p_event uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_file_rows(p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    -- 파일 자체를 영구 삭제할 때는 연결이 이름·버전·SHA-256만 남긴다.
    PERFORM set_config('fvoci.purging_files', 'on', true);
    UPDATE uploads SET version_id = NULL
    WHERE version_id IN (SELECT id FROM file_versions WHERE file_id = ANY (p_ids));
    DELETE FROM file_versions WHERE file_id = ANY (p_ids);
    -- 파일만 담은 휴지통 묶음은 함께 끝난다(되살릴 것이 없으므로).
    UPDATE trash_batches SET purged_at = now()
    WHERE root_kind = 'file' AND root_id = ANY (p_ids) AND purged_at IS NULL AND restored_at IS NULL;
    DELETE FROM files WHERE id = ANY (p_ids);
    PERFORM set_config('fvoci.purging_files', 'off', true);
  END
  $$;


ALTER FUNCTION fvoci.app_delete_file_rows(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_file_version(p_file uuid, p_version integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_file   files%ROWTYPE;
    v_id     uuid;
  BEGIN
    PERFORM app_require_file_write(p_file);
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF app_file_archived(p_file) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF p_version = v_file.current_version THEN
      RAISE EXCEPTION 'the current version cannot be deleted; upload a new one or delete the file'
        USING ERRCODE = '55000';
    END IF;
    SELECT id INTO v_id FROM file_versions WHERE file_id = p_file AND version = p_version;
    IF v_id IS NULL THEN
      RAISE EXCEPTION 'version not found' USING ERRCODE = 'P0002';
    END IF;
    UPDATE uploads SET version_id = NULL WHERE version_id = v_id;
    DELETE FROM file_versions WHERE id = v_id;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete_version', 'file',
            p_file, jsonb_build_object('name', v_file.name, 'version', p_version));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_file_version(p_file uuid, p_version integer, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_delete_record(p_record uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_rec    item_records%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    v_access := app_access('item', v_rec.item_id);
    IF v_rec.id IS NULL OR v_access = 'none' THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.created_by <> app_user_id() AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the author or the item owner can delete a record' USING ERRCODE = '42501';
    END IF;
    DELETE FROM item_records WHERE id = p_record;
    PERFORM app_audit(app_id(p_ids, 1), v_rec.workspace_id, 'delete_record', 'item', v_rec.item_id,
                      jsonb_build_object('record', p_record, 'kind', v_rec.kind, 'title', v_rec.title));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_record(p_record uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_study_unit(p_unit uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_unit study_units%ROWTYPE;
  BEGIN
    SELECT * INTO v_unit FROM study_units WHERE id = p_unit;
    IF v_unit.id IS NULL OR app_access('item', v_unit.item_id) = 'none' THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_unit.item_id);
    DELETE FROM study_units WHERE id = p_unit;
    PERFORM app_audit(app_id(p_ids, 1), v_unit.workspace_id, 'delete_study_unit', 'item', v_unit.item_id,
                      jsonb_build_object('number', v_unit.number, 'title', v_unit.title));
  END
  $$;


ALTER FUNCTION fvoci.app_delete_study_unit(p_unit uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_time_entry(p_entry uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
  BEGIN
    UPDATE time_entries SET deleted_at = now(), version = version + 1 WHERE id = v_entry.id;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_time_entry(p_entry uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_delete_view(p_view uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    DELETE FROM saved_views WHERE id = p_view AND user_id = app_user_id();
    IF NOT FOUND THEN
      RAISE EXCEPTION 'view not found' USING ERRCODE = 'P0002';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_delete_view(p_view uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_depth(p_doc uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH RECURSIVE up(id, parent_id, depth) AS (
      SELECT id, parent_id, 1 FROM documents WHERE id = p_doc
      UNION ALL
      SELECT d.id, d.parent_id, up.depth + 1 FROM documents d JOIN up ON d.id = up.parent_id
      WHERE up.depth < 1000
    )
    SELECT max(depth) FROM up
  $$;


ALTER FUNCTION fvoci.app_document_depth(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_height(p_doc uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH RECURSIVE down(id, height) AS (
      SELECT id, 1 FROM documents WHERE id = p_doc
      UNION ALL
      SELECT d.id, down.height + 1 FROM documents d JOIN down ON d.parent_id = down.id
      WHERE down.height < 1000
    )
    SELECT max(height) FROM down
  $$;


ALTER FUNCTION fvoci.app_document_height(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_realtime(p_doc uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT w.realtime_enabled FROM documents d JOIN workspaces w ON w.id = d.workspace_id
                     WHERE d.id = p_doc), false)
  $$;


ALTER FUNCTION fvoci.app_document_realtime(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_realtime_locked(p_doc uuid) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_enabled boolean;
  BEGIN
    IF app_access('document', p_doc) = 'none' THEN
      RETURN false;
    END IF;
    SELECT w.realtime_enabled INTO v_enabled
    FROM documents d JOIN workspaces w ON w.id = d.workspace_id
    WHERE d.id = p_doc
    FOR SHARE OF w;
    RETURN coalesce(v_enabled, false);
  END
  $$;


ALTER FUNCTION fvoci.app_document_realtime_locked(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_document_reference(p_doc uuid) RETURNS text
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE
      WHEN d.placement = 'wiki' THEN 'WIKI-' || d.number
      WHEN d.placement = 'module' THEN (
        SELECT k.key || '-' || d.number FROM items i
        JOIN module_keys k ON k.module_id = i.module_id AND k.retired_at IS NULL
        WHERE i.id = d.item_id)
    END
    FROM documents d WHERE d.id = p_doc
  $$;


ALTER FUNCTION fvoci.app_document_reference(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row record;
    v_ws  uuid := coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item));
    v_at  integer := 0;
  BEGIN
    -- 같은 워크스페이스의 정리를 직렬화한다. 비공개 전환과 역할 강등이 동시에 일어나도 나중 쪽이 앞의 변경을
    -- 보고 정리한다(이 잠금 뒤의 조회는 앞 트랜잭션이 끝난 뒤의 상태를 본다).
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-assignees:' || v_ws::text, 0));
    FOR v_row IN
      SELECT a.task_id, a.user_id, t.workspace_id, t.item_id
      FROM task_assignees a JOIN tasks t ON t.id = a.task_id
      WHERE (p_item IS NULL OR t.item_id = p_item)
        AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
        AND (p_user IS NULL OR a.user_id = p_user)
    LOOP
      -- 휴지통이라 지금 볼 수 없는 것이 아니라 정말 권한을 잃었을 때만 뺀다.
      IF app_grade_item(v_row.item_id, v_row.user_id, true) = 'none' THEN
        DELETE FROM task_assignees WHERE task_id = v_row.task_id AND user_id = v_row.user_id;
        IF FOUND THEN
          v_at := v_at + 1;
          PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unassign', 'task', v_row.task_id,
                            jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
          -- 공간 제거·역할 변경에 따른 담당 해제는 활동 피드에도 남긴다(D-114·D-119).
          v_at := v_at + 1;
          PERFORM app_activity(app_id(p_ids, v_at), v_row.workspace_id, v_row.item_id, 'task', v_row.task_id, 'unassign',
                               jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
        END IF;
      END IF;
    END LOOP;
    -- 리더도 그 항목을 볼 수 있는 사람이어야 한다(휴지통 항목도 같은 규칙이며 휴지통이라는 이유로는 빼지 않는다).
    FOR v_row IN
      SELECT i.id, i.workspace_id, i.leader_id FROM items i
      WHERE i.leader_id IS NOT NULL AND i.workspace_id = v_ws
        AND (p_item IS NULL OR i.id = p_item)
        AND (p_user IS NULL OR i.leader_id = p_user)
      FOR UPDATE
    LOOP
      IF app_grade_item(v_row.id, v_row.leader_id, true) = 'none' THEN
        UPDATE items SET leader_id = NULL WHERE id = v_row.id;
        v_at := v_at + 1;
        PERFORM app_audit(app_id(p_ids, v_at), v_row.workspace_id, 'unset_leader', 'item', v_row.id,
                          jsonb_build_object('user', v_row.leader_id, 'reason', p_reason));
      END IF;
    END LOOP;
    RETURN v_at;
  END
  $$;


ALTER FUNCTION fvoci.app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_edit_comment(p_comment uuid, p_body text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_body   text := btrim(coalesce(p_body, ''));
    v_access text;
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    IF v_c.id IS NULL OR v_c.deleted_at IS NOT NULL OR app_comment_target_access(v_c.document_id, v_c.task_id) = 'none' THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.created_by <> app_user_id() THEN
      RAISE EXCEPTION 'only the author can edit a comment' USING ERRCODE = '42501';
    END IF;
    -- 보관과 직렬화(#86): 항목을 공유 잠금한 뒤 등급·보관을 다시 본다. 본인 댓글이라도 쓰기 등급이 필요하다(D-20).
    PERFORM app_lock_item(coalesce((SELECT d.item_id FROM documents d WHERE d.id = v_c.document_id),
                                   (SELECT t.item_id FROM tasks t WHERE t.id = v_c.task_id)));
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

CREATE FUNCTION fvoci.app_end_session(p_token_hash bytea) RETURNS text
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ DELETE FROM sessions WHERE token_hash = p_token_hash RETURNING id_token $$;


ALTER FUNCTION fvoci.app_end_session(p_token_hash bytea) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_entry_seconds(e fvoci.time_entries) RETURNS bigint
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT greatest(0, floor(extract(epoch FROM coalesce(e.ended_at, now()) - e.started_at))::bigint
      - e.paused_seconds
      - CASE WHEN e.paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM now() - e.paused_at))::bigint END)
  $$;


ALTER FUNCTION fvoci.app_entry_seconds(e fvoci.time_entries) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_event_occurrences(p_workspace uuid, p_from timestamp with time zone, p_to timestamp with time zone, p_from_date date, p_to_date date) RETURNS TABLE(event_id uuid, occurrence date, start_date date, end_date date, starts_at timestamp with time zone, ends_at timestamp with time zone, skipped boolean)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH visible AS (
      SELECT e.*, coalesce(e.start_date, (e.starts_at AT TIME ZONE e.timezone)::date) AS anchor
      FROM events e
      WHERE e.workspace_id = p_workspace AND e.deleted_at IS NULL AND app_access('event', e.id) <> 'none'
    ),
    shaped AS (
      SELECT v.id, v.anchor AS occ, v.start_date AS sd, v.end_date AS ed, v.starts_at AS sa, v.ends_at AS ea,
             false AS skipped
      FROM visible v WHERE v.recurrence IS NULL
      UNION ALL
      SELECT v.id, o.occurrence,
             CASE WHEN v.all_day THEN v.start_date + (o.occurrence - v.anchor) END,
             CASE WHEN v.all_day THEN v.end_date + (o.occurrence - v.anchor) END,
             l.at,
             l.at + (v.ends_at - v.starts_at),
             NOT v.all_day AND l.at IS NULL
      FROM visible v
      CROSS JOIN LATERAL app_recurrence_dates(v.recurrence, v.anchor, p_from_date - 400, p_to_date + 2) o
      CROSS JOIN LATERAL (
        SELECT CASE WHEN NOT v.all_day
                    THEN app_local_instant(o.occurrence + (v.starts_at AT TIME ZONE v.timezone)::time, v.timezone)
               END AS at
      ) l
      WHERE v.recurrence IS NOT NULL AND NOT (o.occurrence = ANY (v.excluded_dates))
    )
    SELECT id, occ, sd, ed, sa, ea, skipped FROM shaped
    WHERE (sd IS NOT NULL AND sd <= p_to_date AND ed >= p_from_date)
       OR (sa IS NOT NULL AND sa < p_to AND (ea > p_from OR sa >= p_from))
       OR (skipped AND occ BETWEEN p_from_date AND p_to_date)
    ORDER BY coalesce(sa, sd::timestamp AT TIME ZONE 'UTC', occ::timestamp AT TIME ZONE 'UTC'), id
  $$;


ALTER FUNCTION fvoci.app_event_occurrences(p_workspace uuid, p_from timestamp with time zone, p_to timestamp with time zone, p_from_date date, p_to_date date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF v_event.recurrence IS NULL THEN
      RAISE EXCEPTION 'the event does not repeat' USING ERRCODE = '22023';
    END IF;
    IF p_date = ANY (v_event.excluded_dates) THEN
      RETURN;
    END IF;
    UPDATE events SET excluded_dates = array_append(excluded_dates, p_date), version = version + 1,
                      updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'exclude_occurrence', 'event', p_event,
                      jsonb_build_object('date', p_date));
  END
  $$;


ALTER FUNCTION fvoci.app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_fail_upload(p_upload uuid, p_reason text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_up uploads%ROWTYPE;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL OR v_up.state NOT IN ('uploading', 'completing') THEN
      RETURN;
    END IF;
    UPDATE uploads SET state = 'failed', state_changed_at = now(), failure = p_reason WHERE id = p_upload;
    DELETE FROM upload_parts WHERE upload_id = p_upload;
    INSERT INTO storage_deletions (storage_key, storage_upload_id) VALUES (v_up.storage_key, '')
    ON CONFLICT DO NOTHING;
    IF v_up.storage_upload_id IS NOT NULL THEN
      INSERT INTO storage_deletions (storage_key, storage_upload_id) VALUES (v_up.storage_key, v_up.storage_upload_id)
      ON CONFLICT DO NOTHING;
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_fail_upload(p_upload uuid, p_reason text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_archived(p_file uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce(app_item_archived(coalesce(f.item_id, d.item_id)), false)
    FROM files f LEFT JOIN documents d ON d.id = f.document_id
    WHERE f.id = p_file
  $$;


ALTER FUNCTION fvoci.app_file_archived(p_file uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_place(p_kind text, p_id uuid, OUT workspace_id uuid, OUT owner_user_id uuid) RETURNS record
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text;
    v_item   uuid;
  BEGIN
    IF p_kind = 'document' THEN
      v_access := app_access('document', p_id);
      SELECT d.workspace_id, d.owner_user_id, d.item_id INTO workspace_id, owner_user_id, v_item
      FROM documents d WHERE d.id = p_id;
    ELSIF p_kind = 'item' THEN
      v_access := app_access('item', p_id);
      SELECT i.workspace_id, NULL::uuid, i.id INTO workspace_id, owner_user_id, v_item FROM items i WHERE i.id = p_id;
    ELSE
      RAISE EXCEPTION 'unknown target' USING ERRCODE = '22023';
    END IF;
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF v_item IS NOT NULL AND app_item_archived(v_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_file_place(p_kind text, p_id uuid, OUT workspace_id uuid, OUT owner_user_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_file_refs(p_body jsonb) RETURNS TABLE(file_id uuid, version integer)
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $_$
    SELECT DISTINCT lower(replace(m[1], '-', ''))::uuid, (substring(m[2] FROM '(?:^|&)version=0*([0-9]{1,9})(?:&|$)'))::integer
    FROM regexp_matches(p_body::text, '/api/files/([0-9A-Fa-f-]{32,36})/content\?([^"]*)', 'g') AS m
    WHERE lower(replace(m[1], '-', '')) ~ '^[0-9a-f]{32}$'
      AND substring(m[2] FROM '(?:^|&)version=0*([0-9]{1,9})(?:&|$)') IS NOT NULL
  $_$;


ALTER FUNCTION fvoci.app_file_refs(p_body jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_guard_pinned_version() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF current_setting('fvoci.purging_files', true) = 'on' THEN
      RETURN OLD;
    END IF;
    IF app_version_referenced(OLD.file_id, OLD.version, OLD.id) THEN
      RAISE EXCEPTION 'this version is referenced by a record or a document revision' USING ERRCODE = '55000';
    END IF;
    RETURN OLD;
  END
  $$;


ALTER FUNCTION fvoci.app_guard_pinned_version() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_guard_series_root() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF EXISTS (SELECT 1 FROM task_series s WHERE s.root_task_id = NEW.id AND s.rule IS NOT NULL) THEN
      IF NEW.parent_id IS NOT NULL THEN
        RAISE EXCEPTION 'a repeating task cannot become a subtask' USING ERRCODE = '22023';
      END IF;
      IF NEW.due_date IS NULL AND NEW.start_date IS NULL THEN
        RAISE EXCEPTION 'a repeating task needs a date' USING ERRCODE = '22023';
      END IF;
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_guard_series_root() OWNER TO fvoci_migrator;

-- D-88: 앱이 공급한 UUIDv7만 소비한다. DB 생성이나 부족한 ID의 대체는 없다. docs/decisions/database.md#d-88.
-- FV001은 개수 조회 뒤 대상이 늘어난 경합으로 재시도할 수 있는 부족, FV002는 잘못된 ID·위치다.
-- HTTP는 부족을 409로 돌려주고 작업 프로세스는 개수 조회부터 트랜잭션 전체를 재시도한다.
CREATE FUNCTION fvoci.app_id(p_ids uuid[], p_at integer) RETURNS uuid
    LANGUAGE plpgsql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_at IS NULL OR p_at < 1 THEN
      RAISE EXCEPTION 'invalid new id position %', p_at USING ERRCODE = 'FV002';
    END IF;
    IF p_at > coalesce(cardinality(p_ids), 0) THEN
      RAISE EXCEPTION 'new id #% was not supplied by the application', p_at USING ERRCODE = 'FV001';
    END IF;
    IF p_ids[p_at] IS NULL OR uuid_extract_version(p_ids[p_at]) IS DISTINCT FROM 7 THEN
      RAISE EXCEPTION 'new ids must be UUIDv7' USING ERRCODE = 'FV002';
    END IF;
    RETURN p_ids[p_at];
  END
  $$;


ALTER FUNCTION fvoci.app_id(p_ids uuid[], p_at integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_ids_from(p_ids uuid[], p_from integer) RETURNS uuid[]
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT coalesce(p_ids[p_from:], '{}'::uuid[]) $$;


ALTER FUNCTION fvoci.app_ids_from(p_ids uuid[], p_from integer) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_local_instant(p_local timestamp without time zone, p_zone text) RETURNS timestamp with time zone
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT min(c) FROM (
      SELECT (p_local - ((t AT TIME ZONE p_zone) - (t AT TIME ZONE 'UTC'))) AT TIME ZONE 'UTC' AS c
      FROM (VALUES ((p_local - interval '1 day') AT TIME ZONE 'UTC'),
                   ((p_local + interval '1 day') AT TIME ZONE 'UTC')) AS around (t)
    ) candidates
    WHERE (c AT TIME ZONE p_zone) = p_local
  $$;


ALTER FUNCTION fvoci.app_local_instant(p_local timestamp without time zone, p_zone text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_dependencies(p_item uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT pg_advisory_xact_lock(hashtextextended('fvoci-deps:' || p_item::text, 0))
  $$;


ALTER FUNCTION fvoci.app_lock_dependencies(p_item uuid) OWNER TO fvoci_migrator;

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

-- 칸반 열 잠금. 열은 상태 정의 하나(status_id)이고 상태는 항목에 속하므로 항목 id는 따로 넣지 않는다.
CREATE FUNCTION fvoci.app_lock_lane(p_status uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT pg_advisory_xact_lock(hashtextextended('fvoci-lane:' || p_status::text, 0))
  $$;


ALTER FUNCTION fvoci.app_lock_lane(p_status uuid) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_lock_move(p_doc uuid, p_target uuid, p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc  documents%ROWTYPE;
    v_from text;
    v_to   text;
  BEGIN
    IF app_access('document', p_doc) = 'none' THEN
      RETURN;
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    v_from := coalesce(v_doc.workspace_id::text, '') || ':' || coalesce(v_doc.item_id::text, '') || ':'
              || coalesce(v_doc.owner_user_id::text, '');
    v_to := coalesce(p_target::text, '') || ':' || coalesce(p_item::text, '') || ':';
    IF v_from <= v_to THEN
      PERFORM app_lock_tree(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id);
      PERFORM app_lock_tree(p_target, p_item, NULL);
    ELSE
      PERFORM app_lock_tree(p_target, p_item, NULL);
      PERFORM app_lock_tree(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id);
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_lock_move(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_own_entry(p_entry uuid) RETURNS fvoci.time_entries
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_entry time_entries%ROWTYPE;
  BEGIN
    SELECT * INTO v_entry FROM time_entries
    WHERE id = p_entry AND user_id = app_user_id() AND deleted_at IS NULL FOR UPDATE;
    IF v_entry.id IS NULL THEN
      RAISE EXCEPTION 'time entry not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_entry;
  END
  $$;


ALTER FUNCTION fvoci.app_lock_own_entry(p_entry uuid) OWNER TO fvoci_migrator;

-- 교착을 피하려고 항목·상태 순서로 열을 잠근 뒤 업무 행을 ID 순서로 잠근다. 호출자는 잠근 뒤 권한·관계를 다시 확인한다.
-- 업무 키는 바꾸지 않으므로 FOR NO KEY UPDATE로 외래 키 검사의 KEY SHARE와 불필요하게 충돌하지 않게 한다.
-- 업무 행을 잠그기 전에 관련 열(지금 열과 옮겨 갈 열)을 열 id 순서로 먼저 잠근다(다시 매기기·반복 생성과 같은 순서).
CREATE FUNCTION fvoci.app_lock_task_rows(p_ids uuid[], p_extra_status uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_lane  uuid;
    v_lanes uuid[] := '{}';
  BEGIN
    FOR v_lane IN
      SELECT DISTINCT l.status_id FROM (
        SELECT status_id FROM tasks WHERE id = ANY (p_ids)
        UNION
        SELECT p_extra_status WHERE p_extra_status IS NOT NULL
      ) l
      ORDER BY l.status_id
    LOOP
      PERFORM app_lock_lane(v_lane);
      v_lanes := array_append(v_lanes, v_lane);
    END LOOP;
    PERFORM 1 FROM tasks WHERE id = ANY (p_ids) ORDER BY id FOR NO KEY UPDATE;
    IF EXISTS (SELECT 1 FROM tasks WHERE id = ANY (p_ids) AND NOT (status_id = ANY (v_lanes))) THEN
      RAISE EXCEPTION 'a task moved while it was being changed; try again' USING ERRCODE = '40001';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_lock_task_rows(p_ids uuid[], p_extra_status uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_lock_tree(p_workspace uuid, p_item uuid, p_owner uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT pg_advisory_xact_lock(hashtextextended(
      'fvoci-tree:' || coalesce(p_workspace::text, '') || ':' || coalesce(p_item::text, '') || ':'
        || coalesce(p_owner::text, ''), 0))
  $$;


ALTER FUNCTION fvoci.app_lock_tree(p_workspace uuid, p_item uuid, p_owner uuid) OWNER TO fvoci_migrator;

-- 멤버십 정책에서 같은 테이블을 RLS로 다시 읽으면 재귀하므로 소유자 권한으로 현재 사용자의 역할만 확인한다.
-- 현재 사용자의 워크스페이스 역할(관리자·멤버·읽기 전용, 표시용). 권한 판정은 역할이 아니라 app_access('workspace', …)로 한다.
CREATE FUNCTION fvoci.app_member_role(ws uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_workspace_role(ws, app_user_id()) $$;


ALTER FUNCTION fvoci.app_member_role(ws uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[]) RETURNS TABLE(workspace_id uuid, reference text)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_seen   documents%ROWTYPE;
    v_doc    documents%ROWTYPE;
    v_ref    text;
    v_scope  uuid;
    v_number bigint;
    v_next   numeric;
    v_child  record;
    v_at     integer := 1;
  BEGIN
    SELECT * INTO v_seen FROM documents d WHERE d.id = p_doc;
    -- 업로드 마치기와 같은 순서(업로드 행 → 문서)로 먼저 잠근다.
    PERFORM 1 FROM uploads u
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc))
    ORDER BY u.id
    FOR NO KEY UPDATE;
    PERFORM app_lock_move(p_doc, p_target, p_item);
    PERFORM 1 FROM documents d WHERE d.id = p_doc FOR UPDATE;
    PERFORM app_lock_item((SELECT d.item_id FROM documents d WHERE d.id = p_doc));
    PERFORM app_lock_item(p_item);
    v_doc := app_check_move(p_doc, p_target, p_item);
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    v_ref := app_document_reference(p_doc);
    PERFORM app_audit_document(app_id(p_ids, v_at), p_doc, 'move_out', jsonb_build_object('reference', v_ref));
    v_scope := CASE WHEN p_item IS NULL THEN p_target ELSE (SELECT i.module_id FROM items i WHERE i.id = p_item) END;
    IF app_move_keeps_number(p_doc, p_target, p_item) THEN
      v_number := v_doc.number;
    ELSE
      v_number := app_allocate_number(p_target, v_scope, 'document', p_doc);
    END IF;

    -- 자식: 원래 부모 아래, 이동한 문서가 있던 위치와 그다음 형제 사이에 원래 순서대로.
    SELECT min(s.position) INTO v_next
    FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, v_doc.parent_id) s
    WHERE s.id <> p_doc AND s.position > v_doc.position;
    v_next := coalesce(v_next, v_doc.position + 1);
    FOR v_child IN
      SELECT c.id, row_number() OVER (ORDER BY c.position, c.number NULLS LAST, c.created_at, c.id) AS k,
             count(*) OVER () AS n
      FROM documents c WHERE c.parent_id = p_doc
    LOOP
      UPDATE documents d
      SET parent_id = v_doc.parent_id,
          position = v_doc.position + (v_next - v_doc.position) * v_child.k / (v_child.n + 1)
      WHERE d.id = v_child.id;
      v_at := v_at + 1;
      PERFORM app_audit_document(app_id(p_ids, v_at), v_child.id, 'reparent',
                                 jsonb_build_object('parent', v_doc.parent_id, 'reason', 'parent_moved'));
    END LOOP;

    UPDATE documents d
    SET workspace_id = p_target, owner_user_id = NULL, item_id = p_item,
        placement = CASE WHEN p_item IS NULL THEN 'wiki' ELSE 'module' END,
        scope = CASE WHEN p_item IS NULL THEN 'WIKI' ELSE v_scope::text END,
        number = v_number, parent_id = NULL,
        position = (SELECT coalesce(max(s.position), 0) + 1 FROM app_tree_siblings(p_target, p_item, NULL, NULL) s)
    WHERE d.id = p_doc;
    -- 첨부(휴지통에 있는 것 포함)·그 휴지통 묶음·열린 업로드도 새 자리로. 권한·휴지통·감사·저장 용량·
    -- 내보내기가 모두 이 자리를 본다.
    UPDATE files f SET workspace_id = p_target, owner_user_id = NULL WHERE f.document_id = p_doc;
    UPDATE trash_batches b SET workspace_id = p_target, owner_user_id = NULL
    WHERE b.root_kind = 'file' AND b.restored_at IS NULL AND b.purged_at IS NULL
      AND b.root_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc);
    UPDATE uploads u SET workspace_id = p_target, owner_user_id = NULL
    WHERE u.state IN ('uploading', 'completing')
      AND ((u.target_kind = 'document' AND u.target_id = p_doc)
           OR u.file_id IN (SELECT f.id FROM files f WHERE f.document_id = p_doc));
    -- D-114: 옮긴 자리의 조직 밖 공유는 철회한다(개인 노트를 공간으로 옮길 때 다른 조직에서 준 공유).
    FOR v_child IN
      SELECT s.id, s.grantee_user_id FROM document_shares s
      WHERE s.document_id = p_doc
        AND s.organization_id <> (SELECT w.organization_id FROM workspaces w WHERE w.id = p_target)
      ORDER BY s.id
    LOOP
      DELETE FROM document_shares WHERE id = v_child.id;
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), p_target, p_item, 'document', p_doc, 'unshare',
                           jsonb_build_object('user', v_child.grantee_user_id, 'reason', 'moved'));
    END LOOP;
    v_at := v_at + 1;
    PERFORM app_audit_document(app_id(p_ids, v_at), p_doc, 'move_in',
      jsonb_build_object('from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END));
    v_at := v_at + 1;
    PERFORM app_activity(app_id(p_ids, v_at), p_target, p_item, 'document', p_doc, 'move',
      jsonb_build_object('from_workspace', v_doc.workspace_id, 'from_item', v_doc.item_id,
                         'from', CASE WHEN v_doc.owner_user_id IS NULL THEN 'workspace' ELSE 'personal' END,
                         'reference', v_ref));
    RETURN QUERY SELECT p_target, (SELECT
      CASE WHEN p_item IS NULL THEN 'WIKI-' || v_number
           ELSE (SELECT k.key FROM module_keys k WHERE k.module_id = v_scope AND k.retired_at IS NULL) || '-' || v_number
      END);
  END
  $$;


ALTER FUNCTION fvoci.app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_move_document_ids(p_doc uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- move_out·move_in 감사 2 + 활동 1 + 원래 자리에 남기는 자식마다 1 + 철회할 수 있는 공유마다 1(상한).
    SELECT CASE WHEN app_access('document', p_doc) = 'none' THEN 0
                ELSE 3 + (SELECT count(*) FROM documents c WHERE c.parent_id = p_doc)::integer
                       + (SELECT count(*) FROM document_shares s WHERE s.document_id = p_doc)::integer END
  $$;


ALTER FUNCTION fvoci.app_move_document_ids(p_doc uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) RETURNS boolean
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc documents%ROWTYPE := app_check_move(p_doc, p_target, p_item);
  BEGIN
    RETURN v_doc.number IS NOT NULL
      AND v_doc.workspace_id = p_target
      AND v_doc.scope = CASE WHEN p_item IS NULL THEN 'WIKI'
                             ELSE (SELECT module_id::text FROM items WHERE id = p_item) END;
  END
  $$;


ALTER FUNCTION fvoci.app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) OWNER TO fvoci_migrator;

-- 칸반 이동(D-79·D-89). 열은 항목의 상태 정의 이름이다(D-116). 활동 피드에 kanban_move를 남긴다(D-119).
CREATE FUNCTION fvoci.app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task     tasks%ROWTYPE;
    v_before   tasks%ROWTYPE;
    v_status   task_statuses%ROWTYPE;
    v_old      task_statuses%ROWTYPE;
    v_lower    numeric;
    v_position numeric;
  BEGIN
    PERFORM app_require_task_write(p_task);
    v_status := app_task_status_by_name((SELECT item_id FROM tasks WHERE id = p_task), p_status);
    -- 지금 열과 옮겨 갈 열을 먼저, 그다음 행(다시 매기기와의 교착 방지).
    PERFORM app_lock_task_rows(ARRAY[p_task], v_status.id);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- D-89: 읽은 뒤 그사이 이 업무의 열·위치가 바뀌었으면(이동 판이 다르면) 아무것도 바꾸지 않는다. 행을 잠근 뒤라
    -- 비교와 옮기기 사이에 다른 이동이 끼지 못한다.
    IF p_base_move_version IS NOT NULL AND v_task.move_version IS DISTINCT FROM p_base_move_version THEN
      RAISE EXCEPTION 'the task moved since it was read' USING ERRCODE = 'FV003';
    END IF;
    -- 열 잠금 뒤 상태 정의를 다시 읽는다(기다리는 사이 이름·범주가 바뀌었거나 지워졌을 수 있다).
    SELECT * INTO v_status FROM task_statuses WHERE id = v_status.id;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    v_old := app_task_status(v_task.status_id);
    IF p_before IS NOT NULL THEN
      SELECT * INTO v_before FROM tasks WHERE id = p_before;
      IF v_before.id IS NULL OR v_before.id = p_task OR v_before.item_id <> v_task.item_id
         OR v_before.status_id <> v_status.id OR app_access('task', p_before) = 'none' THEN
        RAISE EXCEPTION 'before must be a task in the same column' USING ERRCODE = '22023';
      END IF;
      SELECT max(position) INTO v_lower FROM tasks
      WHERE status_id = v_status.id AND id <> p_task AND position < v_before.position;
      v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
      IF scale(v_position) > 40 THEN
        -- 같은 틈에 너무 많이 끼워 자릿수가 길어졌다. 열을 다시 매긴 뒤 계산한다.
        PERFORM app_renumber_lane(v_status.id);
        SELECT position INTO v_before.position FROM tasks WHERE id = p_before;
        SELECT max(position) INTO v_lower FROM tasks
        WHERE status_id = v_status.id AND id <> p_task AND position < v_before.position;
        v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
      END IF;
    ELSE
      SELECT coalesce(max(position), 0) + 1 INTO v_position FROM tasks
      WHERE status_id = v_status.id AND id <> p_task;
    END IF;
    UPDATE tasks SET
      status_id = v_status.id, position = v_position,
      version = CASE WHEN v_status.id <> v_task.status_id THEN version + 1 ELSE version END,
      completed_at = CASE WHEN v_status.category = 'done' AND v_old.category <> 'done' THEN now()
                          WHEN v_status.category <> 'done' THEN NULL ELSE completed_at END,
      updated_by = CASE WHEN v_status.id <> v_task.status_id THEN app_user_id() ELSE updated_by END,
      updated_at = CASE WHEN v_status.id <> v_task.status_id THEN now() ELSE updated_at END
    WHERE id = p_task;
    PERFORM app_activity(app_id(p_ids, 1), v_task.workspace_id, v_task.item_id, 'task', p_task, 'kanban_move',
                         jsonb_build_object('from', v_old.name, 'to', v_status.name, 'before', p_before));
  END
  $$;


ALTER FUNCTION fvoci.app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_my_upload(p_upload uuid) RETURNS fvoci.uploads
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_up uploads%ROWTYPE;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id();
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_up;
  END
  $$;


ALTER FUNCTION fvoci.app_my_upload(p_upload uuid) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_normalize_key(p_key text) RETURNS text
    LANGUAGE plpgsql IMMUTABLE
    AS $_$
  DECLARE
    v text := upper(btrim(p_key));
  BEGIN
    IF v !~ '^[A-Z][A-Z0-9]{1,9}$' THEN
      RAISE EXCEPTION 'key must be 2-10 letters or digits starting with a letter' USING ERRCODE = '22023';
    END IF;
    IF v = 'WIKI' THEN
      RAISE EXCEPTION 'WIKI is reserved' USING ERRCODE = '22023';
    END IF;
    RETURN v;
  END
  $_$;


ALTER FUNCTION fvoci.app_normalize_key(p_key text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_doc      documents%ROWTYPE;
    v_parent   documents%ROWTYPE;
    v_before   documents%ROWTYPE;
    v_ancestor uuid;
    v_lower    numeric;
    v_position numeric;
  BEGIN
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF p_parent IS NOT NULL THEN
      IF app_access('document', p_parent) = 'none' THEN
        RAISE EXCEPTION 'parent not found' USING ERRCODE = 'P0002';
      END IF;
      SELECT * INTO v_parent FROM documents WHERE id = p_parent;
      IF v_parent.workspace_id IS DISTINCT FROM v_doc.workspace_id
         OR v_parent.item_id IS DISTINCT FROM v_doc.item_id
         OR v_parent.owner_user_id IS DISTINCT FROM v_doc.owner_user_id THEN
        RAISE EXCEPTION 'the parent must be in the same place' USING ERRCODE = '22023';
      END IF;
      -- 자기 자신이나 자기 하위 문서 아래로는 옮길 수 없다(자리 잠금 아래에서 조상 경로를 본다).
      v_ancestor := p_parent;
      WHILE v_ancestor IS NOT NULL LOOP
        IF v_ancestor = p_doc THEN
          RAISE EXCEPTION 'a document cannot be placed under itself' USING ERRCODE = '22023';
        END IF;
        SELECT parent_id INTO v_ancestor FROM documents WHERE id = v_ancestor;
      END LOOP;
      IF app_document_depth(p_parent) + app_document_height(p_doc) > p_max_depth THEN
        RAISE EXCEPTION 'the tree would be deeper than % levels', p_max_depth USING ERRCODE = '22023';
      END IF;
    END IF;

    IF p_before IS NOT NULL THEN
      SELECT * INTO v_before FROM documents WHERE id = p_before;
      IF v_before.id IS NULL OR v_before.id = p_doc OR app_access('document', p_before) = 'none'
         -- 형제 판단은 호출한 사람이 보는 부모 기준(볼 수 없는 부모 아래 문서는 최상위로 보인다).
         OR (CASE WHEN v_before.parent_id IS NOT NULL AND app_access('document', v_before.parent_id) <> 'none'
                  THEN v_before.parent_id END) IS DISTINCT FROM p_parent
         OR v_before.workspace_id IS DISTINCT FROM v_doc.workspace_id
         OR v_before.item_id IS DISTINCT FROM v_doc.item_id
         OR v_before.owner_user_id IS DISTINCT FROM v_doc.owner_user_id THEN
        RAISE EXCEPTION 'before must be a sibling under the new parent' USING ERRCODE = '22023';
      END IF;
      SELECT max(s.position) INTO v_lower
      FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, p_parent) s
      WHERE s.id <> p_doc AND s.position < v_before.position;
      v_position := (coalesce(v_lower, v_before.position - 1) + v_before.position) * 0.5;
    ELSE
      IF v_doc.parent_id IS NOT DISTINCT FROM p_parent AND NOT EXISTS (
           SELECT 1 FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, p_parent) s
           WHERE s.id <> p_doc AND s.position > v_doc.position) THEN
        RETURN;  -- 이미 그 부모의 맨 끝: 바꿀 것이 없다.
      END IF;
      SELECT coalesce(max(s.position), 0) + 1 INTO v_position
      FROM app_tree_siblings(v_doc.workspace_id, v_doc.item_id, v_doc.owner_user_id, p_parent) s
      WHERE s.id <> p_doc;
    END IF;

    UPDATE documents SET parent_id = p_parent, position = v_position WHERE id = p_doc;
    IF v_doc.parent_id IS DISTINCT FROM p_parent THEN
      PERFORM app_audit_document(app_id(p_ids, 1), p_doc, 'reparent', jsonb_build_object('parent', p_parent));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_plan_units(p_item uuid, p_entries jsonb) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row   jsonb;
    v_count integer := 0;
  BEGIN
    PERFORM app_study_access(p_item);
    IF app_access('item', p_item) = 'read' THEN
      RAISE EXCEPTION 'read-only members cannot record progress' USING ERRCODE = '42501';
    END IF;
    IF jsonb_typeof(p_entries) <> 'array' OR jsonb_array_length(p_entries) > 1000 THEN
      RAISE EXCEPTION 'give at most 1000 entries' USING ERRCODE = '22023';
    END IF;
    FOR v_row IN SELECT * FROM jsonb_array_elements(p_entries) LOOP
      IF NOT EXISTS (SELECT 1 FROM study_units WHERE id = (v_row->>'unit_id')::uuid AND item_id = p_item) THEN
        RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
      END IF;
      INSERT INTO unit_progress (unit_id, user_id, planned_on)
      VALUES ((v_row->>'unit_id')::uuid, app_user_id(), (v_row->>'planned_on')::date)
      ON CONFLICT (unit_id, user_id) DO UPDATE SET planned_on = EXCLUDED.planned_on, updated_at = now();
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  EXCEPTION
    WHEN invalid_text_representation OR invalid_datetime_format OR datetime_field_overflow THEN
      RAISE EXCEPTION 'invalid plan: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_plan_units(p_item uuid, p_entries jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date) RETURNS TABLE(series_id uuid, root_task_id uuid, item_id uuid, title text, milestone boolean, occurrence date, start_date date, due_date date)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT s.id, r.id, r.item_id, r.title, r.milestone, o.occurrence,
           r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)
    FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    CROSS JOIN LATERAL app_recurrence_dates(s.rule, s.anchor, greatest(s.effective_from, p_from - 400), p_to) o
    WHERE s.rule IS NOT NULL
      AND (p_workspace IS NULL OR s.workspace_id = p_workspace)
      AND (p_item IS NULL OR s.item_id = p_item)
      AND app_access('task', r.id) <> 'none'
      AND NOT app_item_archived(r.item_id)
      AND coalesce(r.due_date + (o.occurrence - s.anchor), r.start_date + (o.occurrence - s.anchor)) >= p_from
      AND coalesce(r.start_date + (o.occurrence - s.anchor), r.due_date + (o.occurrence - s.anchor)) <= p_to
      AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = s.id AND t.occurrence_date = o.occurrence)
      AND NOT (o.occurrence = ANY (s.skipped_dates))
  $$;


ALTER FUNCTION fvoci.app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_batch(p_batch uuid, p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_purge_batch_as(p_batch, false, p_ids) $$;


ALTER FUNCTION fvoci.app_purge_batch(p_batch uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 자동 정리는 참조를 먼저 확인하고 실제로 지울 버전만 잠근다. 다른 저장이 잡은 버전은 SKIP LOCKED로 미룬다.
-- 참조 기록은 버전 행을 먼저 공유 잠금하므로, 잠금 뒤의 재검사가 저장 중인 참조를 잃지 않게 한다(D-56).
CREATE FUNCTION fvoci.app_purge_batch_as(p_batch uuid, p_system boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_batch  trash_batches%ROWTYPE;
    v_item   uuid;
    v_total  bigint;
    v_locked bigint;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR v_batch.restored_at IS NOT NULL OR v_batch.purged_at IS NOT NULL
       OR (NOT p_system AND app_trash_role(v_batch) IS NULL) THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system AND app_trash_role(v_batch) <> 'admin' THEN
      RAISE EXCEPTION 'only an admin can delete permanently' USING ERRCODE = '42501';
    END IF;
    -- 작업 프로세스는 만료된 묶음만 지운다.
    IF p_system AND app_trash_expires_at(v_batch) > now() THEN
      RETURN;
    END IF;
    -- 자동 정리는 살아 있는 기록·문서가 참조하는 버전을 지우지 않는다(D-56). 잠금 없이 먼저 보고, 지울 때만 잠근 뒤 다시 본다.
    IF p_system THEN
      IF app_batch_keeps_referenced(v_batch) THEN
        RETURN;
      END IF;
      SELECT count(*) INTO v_total FROM file_versions
      WHERE file_id IN (SELECT file_id FROM app_batch_files(v_batch));
      SELECT count(*) INTO v_locked FROM (
        SELECT 1 FROM file_versions
        WHERE file_id IN (SELECT file_id FROM app_batch_files(v_batch))
        ORDER BY id
        FOR UPDATE SKIP LOCKED) l;
      IF v_locked < v_total OR app_batch_keeps_referenced(v_batch) THEN
        RETURN;
      END IF;
    END IF;
    IF v_batch.root_kind = 'document' THEN
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'event' THEN
      DELETE FROM events WHERE trash_batch_id = p_batch;
    ELSIF v_batch.root_kind = 'file' THEN
      PERFORM app_delete_file_rows(ARRAY(SELECT id FROM files WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'module' THEN
      -- 모듈의 모든 항목(따로 휴지통에 있던 것 포함)을 지운 뒤 모듈을 지운다. 그 항목들의 묶음도 끝난다.
      FOR v_item IN SELECT id FROM items WHERE module_id = v_batch.root_id ORDER BY id LOOP
        PERFORM app_purge_item_rows(v_item);
      END LOOP;
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND root_kind = 'item' AND NOT EXISTS (SELECT 1 FROM items WHERE id = trash_batches.root_id);
      DELETE FROM numbered_targets WHERE scope_id = v_batch.root_id;
      -- 키 행을 지우지 않고 예약으로 남긴다(주인을 비우고 과거 키로).
      UPDATE module_keys SET module_id = NULL, retired_at = coalesce(retired_at, now())
      WHERE module_id = v_batch.root_id;
      DELETE FROM modules WHERE id = v_batch.root_id;
    ELSE
      PERFORM app_purge_item_rows(v_batch.root_id);
    END IF;
    IF v_batch.root_kind IN ('item', 'module') THEN
      -- 지운 항목 안의 것이 다른 휴지통 묶음에 있었으면 그 묶음도 함께 끝난다.
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND NOT EXISTS (SELECT 1 FROM documents WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM tasks WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM events WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM files WHERE trash_batch_id = trash_batches.id)
        AND root_kind NOT IN ('item', 'module');
    END IF;
    UPDATE trash_batches SET purged_at = now() WHERE id = p_batch;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_batch.workspace_id, v_batch.owner_user_id,
            CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
  END
  $$;


ALTER FUNCTION fvoci.app_purge_batch_as(p_batch uuid, p_system boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_documents(p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE documents SET parent_id = NULL WHERE parent_id = ANY (p_ids) AND NOT (id = ANY (p_ids));
    UPDATE documents SET parent_id = NULL WHERE id = ANY (p_ids);
    DELETE FROM notifications WHERE document_id = ANY (p_ids);
    DELETE FROM document_mentions WHERE document_id = ANY (p_ids);
    DELETE FROM document_links WHERE document_id = ANY (p_ids);
    DELETE FROM document_shares WHERE document_id = ANY (p_ids);
    DELETE FROM public_links WHERE document_id = ANY (p_ids);
    DELETE FROM document_revisions WHERE document_id = ANY (p_ids);
    DELETE FROM user_marks WHERE target_kind = 'document' AND target_id = ANY (p_ids);
    DELETE FROM documents WHERE id = ANY (p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_purge_documents(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_expired(p_workspace uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_id   uuid;
    v_made integer := 0;
  BEGIN
    IF p_workspace IS NOT NULL AND app_access('workspace', p_workspace) <> 'owner' THEN
      RAISE EXCEPTION 'only an admin can empty the trash' USING ERRCODE = '42501';
    END IF;
    FOR v_id IN
      SELECT b.id FROM trash_batches b
      WHERE b.restored_at IS NULL AND b.purged_at IS NULL
        AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
        AND app_trash_expires_at(b) <= now()
      ORDER BY b.deleted_at
    LOOP
      v_made := v_made + 1;
      PERFORM app_purge_batch(v_id, ARRAY[app_id(p_ids, v_made)]);
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_purge_expired(p_workspace uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_expired_ids(p_workspace uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN p_workspace IS NOT NULL AND app_access('workspace', p_workspace) <> 'owner' THEN 0 ELSE
      (SELECT count(*) FROM trash_batches b
       WHERE b.restored_at IS NULL AND b.purged_at IS NULL
         AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
         AND app_trash_expires_at(b) <= now())::integer
    END
  $$;


ALTER FUNCTION fvoci.app_purge_expired_ids(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_item_rows(p_item uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE item_id = p_item));
    DELETE FROM task_dependencies WHERE item_id = p_item;
    DELETE FROM task_series WHERE item_id = p_item;
    PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE item_id = p_item));
    DELETE FROM events WHERE item_id = p_item;
    DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = p_item;
    DELETE FROM items WHERE id = p_item;
  END
  $$;


ALTER FUNCTION fvoci.app_purge_item_rows(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_tasks(p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE tasks SET parent_id = NULL WHERE parent_id = ANY (p_ids) AND NOT (id = ANY (p_ids));
    -- 영구 삭제한 반복 회차는 다시 만들지 않는다(반복에 건너뛸 날짜로 남긴다).
    UPDATE task_series s SET skipped_dates = (
      SELECT ARRAY(SELECT DISTINCT d FROM unnest(s.skipped_dates || ARRAY(
        SELECT t.occurrence_date FROM tasks t
        WHERE t.id = ANY (p_ids) AND t.series_id = s.id AND t.id <> s.root_task_id)) d ORDER BY d))
    WHERE s.id IN (SELECT series_id FROM tasks WHERE id = ANY (p_ids) AND series_id IS NOT NULL);
    UPDATE tasks SET series_id = NULL, occurrence_date = NULL
    WHERE series_id IN (SELECT id FROM task_series WHERE root_task_id = ANY (p_ids));
    DELETE FROM task_series WHERE root_task_id = ANY (p_ids);
    UPDATE tasks SET series_id = NULL, occurrence_date = NULL WHERE id = ANY (p_ids);
    DELETE FROM task_assignees WHERE task_id = ANY (p_ids);
    DELETE FROM checklist_items WHERE task_id = ANY (p_ids);
    DELETE FROM user_marks WHERE target_kind = 'task' AND target_id = ANY (p_ids);
    DELETE FROM tasks WHERE id = ANY (p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_purge_tasks(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_workspace(p_ws uuid, p_ids uuid[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_purge_workspace_as(p_ws, false, p_ids) $$;


ALTER FUNCTION fvoci.app_purge_workspace(p_ws uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_purge_workspace_as(p_ws uuid, p_system boolean, p_ids uuid[]) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_ws    workspaces%ROWTYPE;
    v_items uuid[];
  BEGIN
    -- 새 내용이 이 워크스페이스를 가리키며 들어오지 못하게 행을 잠근다(외래 키 공유 잠금과 충돌).
    SELECT * INTO v_ws FROM workspaces WHERE id = p_ws FOR UPDATE;
    IF v_ws.id IS NULL OR v_ws.deleted_at IS NULL
       OR (NOT p_system AND v_ws.owner_id IS DISTINCT FROM app_user_id()
           AND app_access('organization', v_ws.organization_id) <> 'owner') THEN
      RAISE EXCEPTION 'deleted workspace not found' USING ERRCODE = 'P0002';
    END IF;
    IF p_system AND v_ws.deleted_at + make_interval(days => v_ws.trash_retention_days) > now() THEN
      RETURN false;
    END IF;
    v_items := ARRAY(SELECT id FROM items WHERE workspace_id = p_ws);
    PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE workspace_id = p_ws));
    DELETE FROM task_dependencies WHERE item_id = ANY (v_items);
    DELETE FROM task_series WHERE workspace_id = p_ws;
    PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE workspace_id = p_ws));
    DELETE FROM events WHERE workspace_id = p_ws;
    DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = ANY (v_items);
    DELETE FROM items WHERE workspace_id = p_ws;
    DELETE FROM module_keys WHERE workspace_id = p_ws;
    DELETE FROM modules WHERE workspace_id = p_ws;
    -- 묶음을 가리키는 모듈을 지운 뒤에 묶음을 지운다.
    DELETE FROM trash_batches WHERE workspace_id = p_ws;
    DELETE FROM numbered_targets WHERE workspace_id = p_ws;
    DELETE FROM reference_counters WHERE workspace_id = p_ws;
    DELETE FROM groups WHERE workspace_id = p_ws;
    DELETE FROM invitations WHERE workspace_id = p_ws;
    DELETE FROM ownership_offers WHERE workspace_id = p_ws;
    -- 개인 시간 기록은 남는다(대상 이름만 사라진다).
    UPDATE time_entries SET workspace_id = NULL WHERE workspace_id = p_ws;
    -- 공간 감사 기록은 소유자 영역으로 옮겨 남기고(조직 로그는 그대로), 활동 피드는 워크스페이스와 함께 지운다(FK).
    UPDATE audit_log SET workspace_id = NULL, organization_id = NULL,
                         owner_user_id = coalesce(owner_user_id, v_ws.owner_id)
    WHERE workspace_id = p_ws;
    DELETE FROM workspace_members WHERE workspace_id = p_ws;
    DELETE FROM workspaces WHERE id = p_ws;
    INSERT INTO audit_log (id, workspace_id, organization_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), NULL, NULL, v_ws.owner_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END),
           (app_id(p_ids, 2), NULL, v_ws.organization_id, NULL, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            'workspace', p_ws,
            jsonb_build_object('name', v_ws.name, 'deleted_at', v_ws.deleted_at)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_purge_workspace_as(p_ws uuid, p_system boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_queue_version_deletion() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO storage_deletions (storage_key) VALUES (OLD.storage_key) ON CONFLICT DO NOTHING;
    RETURN OLD;
  END
  $$;


ALTER FUNCTION fvoci.app_queue_version_deletion() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_record_document_references_ids(p_doc uuid, p_notify boolean) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN NOT p_notify OR app_access('document', p_doc) = 'none' THEN 0
                ELSE (SELECT count(*) FROM documents d, app_reference_nodes(d.body) r
                      WHERE d.id = p_doc AND r.kind = 'user')::integer END
  $$;


ALTER FUNCTION fvoci.app_record_document_references_ids(p_doc uuid, p_notify boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_record_part(p_upload uuid, p_number integer, p_size bigint, p_sha256 text, p_etag text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_up uploads%ROWTYPE;
  BEGIN
    SELECT * INTO v_up FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE;
    IF v_up.id IS NULL THEN
      RAISE EXCEPTION 'upload not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_up.state <> 'uploading' OR v_up.expires_at <= now() THEN
      RAISE EXCEPTION 'the upload is no longer open' USING ERRCODE = '55000';
    END IF;
    INSERT INTO upload_parts (upload_id, number, size, sha256, etag)
    VALUES (p_upload, p_number, p_size, p_sha256, p_etag)
    ON CONFLICT (upload_id, number) DO UPDATE SET size = EXCLUDED.size, sha256 = EXCLUDED.sha256, etag = EXCLUDED.etag;
  END
  $$;


ALTER FUNCTION fvoci.app_record_part(p_upload uuid, p_number integer, p_size bigint, p_sha256 text, p_etag text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_record_write_check(p_item uuid) RETURNS void
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_record_write_check(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date) RETURNS TABLE(occurrence date)
    LANGUAGE plpgsql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_freq     text := p_rule ->> 'freq';
    v_step     integer := (p_rule ->> 'interval')::integer;
    v_until    date := (p_rule ->> 'until')::date;
    v_count    integer := (p_rule ->> 'count')::integer;
    v_days     integer[];
    v_last     date := least(p_to, coalesce(v_until, p_to));
    v_monday   date := p_anchor - (extract(isodow FROM p_anchor)::integer - 1);
    v_k0       integer := 0;
    v_k1       integer;
  BEGIN
    IF v_last < p_anchor OR p_to < p_from THEN
      RETURN;
    END IF;
    SELECT coalesce(array_agg(w::integer), '{}') INTO v_days FROM jsonb_array_elements(p_rule -> 'weekdays') w;
    IF cardinality(v_days) = 0 THEN
      v_days := ARRAY[extract(isodow FROM p_anchor)::integer];
    END IF;
    -- 회차 번호 k의 범위. 횟수가 없으면 p_from 바로 앞에서 시작한다(앞의 회차는 셀 필요가 없다).
    IF v_freq = 'daily' THEN
      IF v_count IS NULL THEN v_k0 := greatest(0, (p_from - p_anchor) / v_step); END IF;
      v_k1 := (v_last - p_anchor) / v_step;
      IF v_count IS NOT NULL THEN v_k1 := least(v_k1, v_count); END IF;
      RETURN QUERY
        WITH c AS (SELECT p_anchor + k * v_step AS d FROM generate_series(v_k0, v_k1) k),
             n AS (SELECT d, row_number() OVER (ORDER BY d) AS i FROM c)
        SELECT d FROM n WHERE (v_count IS NULL OR i <= v_count) AND d BETWEEN greatest(p_from, p_anchor) AND v_last
        ORDER BY d;
      RETURN;
    END IF;
    IF v_freq = 'weekly' THEN
      IF v_count IS NULL THEN v_k0 := greatest(0, (p_from - v_monday) / (7 * v_step)); END IF;
      v_k1 := (v_last - v_monday) / (7 * v_step);
      IF v_count IS NOT NULL THEN v_k1 := least(v_k1, v_count); END IF;
      RETURN QUERY
        WITH c AS (
          SELECT p_anchor AS d
          UNION
          SELECT v_monday + 7 * v_step * k + (wd - 1)
          FROM generate_series(v_k0, v_k1) k, unnest(v_days) wd
        ),
        n AS (SELECT d, row_number() OVER (ORDER BY d) AS i FROM c WHERE d >= p_anchor)
        SELECT d FROM n
        WHERE (v_count IS NULL OR i <= v_count) AND d BETWEEN greatest(p_from, p_anchor) AND v_last
        ORDER BY d;
      RETURN;
    END IF;
    -- 매달·매년: 없는 날(31일·2월 29일)은 건너뛴다. 횟수가 있으면 건너뛸 수 있는 몫까지 넉넉히(×8) 센다.
    DECLARE
      v_unit   integer := CASE v_freq WHEN 'monthly' THEN 1 ELSE 12 END;
      v_months integer := ((extract(year FROM v_last) - extract(year FROM p_anchor)) * 12
                           + extract(month FROM v_last) - extract(month FROM p_anchor))::integer;
      v_from_m integer := ((extract(year FROM p_from) - extract(year FROM p_anchor)) * 12
                           + extract(month FROM p_from) - extract(month FROM p_anchor))::integer;
      v_start  date := date_trunc('month', p_anchor)::date;
      v_day    integer := extract(day FROM p_anchor)::integer;
    BEGIN
      IF v_count IS NULL THEN v_k0 := greatest(0, v_from_m / (v_step * v_unit)); END IF;
      v_k1 := v_months / (v_step * v_unit);
      IF v_count IS NOT NULL THEN v_k1 := least(v_k1, v_count * 8); END IF;
      RETURN QUERY
        WITH m AS (
          SELECT (v_start + make_interval(months => k * v_step * v_unit))::date AS first_day
          FROM generate_series(v_k0, v_k1) k
        ),
        c AS (
          SELECT first_day + (v_day - 1) AS d, first_day FROM m
        ),
        n AS (
          SELECT d, row_number() OVER (ORDER BY d) AS i FROM c
          WHERE date_trunc('month', d)::date = first_day
        )
        SELECT d FROM n
        WHERE (v_count IS NULL OR i <= v_count) AND d BETWEEN greatest(p_from, p_anchor) AND v_last
        ORDER BY d;
    END;
  END
  $$;


ALTER FUNCTION fvoci.app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_reference_nodes(p_body jsonb) RETURNS TABLE(kind text, id uuid)
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $_$
    SELECT DISTINCT r.kind, r.raw_id::uuid
    FROM (
      SELECT n -> 'attrs' ->> 'kind' AS kind, n -> 'attrs' ->> 'id' AS raw_id
      FROM jsonb_path_query(p_body, 'lax $.**') AS n
      WHERE jsonb_typeof(n) = 'object' AND n ->> 'type' = 'reference'
    ) r
    WHERE r.kind IN ('item', 'document', 'task', 'user')
      AND r.raw_id ~ '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
  $_$;


ALTER FUNCTION fvoci.app_reference_nodes(p_body jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_release_upload_claim(p_upload uuid, p_claim timestamp with time zone) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE uploads SET state_changed_at = now() - interval '5 minutes' - interval '1 second'
    WHERE id = p_upload AND user_id = app_user_id() AND state = 'completing' AND state_changed_at = p_claim
  $$;


ALTER FUNCTION fvoci.app_release_upload_claim(p_upload uuid, p_claim timestamp with time zone) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_succ tasks%ROWTYPE;
  BEGIN
    PERFORM app_require_task_write(p_successor);
    SELECT * INTO v_succ FROM tasks WHERE id = p_successor;
    PERFORM app_lock_item(v_succ.item_id);
    PERFORM app_lock_dependencies(v_succ.item_id);
    PERFORM app_require_task_write(p_successor);
    DELETE FROM task_dependencies WHERE predecessor_id = p_predecessor AND successor_id = p_successor;
    IF FOUND THEN
      PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'unlink_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor));
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

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

REVOKE ALL ON FUNCTION fvoci.app_require_file_write(p_file uuid) FROM PUBLIC;

CREATE FUNCTION fvoci.app_rename_file(p_file uuid, p_name text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_file   files%ROWTYPE;
    v_name   text := btrim(coalesce(p_name, ''));
  BEGIN
    PERFORM app_require_file_write(p_file);
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF app_file_archived(p_file) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    UPDATE files SET name = v_name, updated_at = now() WHERE id = p_file;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 1), v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'rename', 'file', p_file,
            jsonb_build_object('from', v_file.name, 'to', v_name));
  END
  $$;


ALTER FUNCTION fvoci.app_rename_file(p_file uuid, p_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_renumber_lane(p_status uuid) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE tasks t SET position = r.pos
    FROM (SELECT id, row_number() OVER (ORDER BY position, number) AS pos
          FROM tasks WHERE status_id = p_status) r
    WHERE t.id = r.id AND t.status_id = p_status
  $$;


ALTER FUNCTION fvoci.app_renumber_lane(p_status uuid) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_reset_upload(p_upload uuid) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    UPDATE uploads SET state = 'uploading', state_changed_at = now()
    WHERE id = p_upload AND user_id = app_user_id() AND state = 'completing';
    IF FOUND THEN
      DELETE FROM upload_parts WHERE upload_id = p_upload;
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_reset_upload(p_upload uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_resolve_comment(p_comment uuid, p_resolved boolean) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_c      comments%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_c FROM comments WHERE id = p_comment FOR NO KEY UPDATE;
    v_access := app_comment_target_access(v_c.document_id, v_c.task_id);
    IF v_c.id IS NULL OR v_access = 'none' THEN
      RAISE EXCEPTION 'comment not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_c.parent_id IS NOT NULL THEN
      RAISE EXCEPTION 'resolve the first comment of the thread' USING ERRCODE = '22023';
    END IF;
    -- 보관과 직렬화(#86): 항목을 공유 잠금한 뒤 등급·보관을 다시 본다. 스레드 해결은 쓰기라 쓰기 등급이 필요하다(D-20).
    PERFORM app_lock_item(coalesce((SELECT d.item_id FROM documents d WHERE d.id = v_c.document_id),
                                   (SELECT t.item_id FROM tasks t WHERE t.id = v_c.task_id)));
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

CREATE FUNCTION fvoci.app_restore_batch(p_batch uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_doc   documents%ROWTYPE;
    v_item  uuid;
    v_ids   uuid[];
    v_file_doc uuid;
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch;
    IF v_batch.id IS NULL OR app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    -- 첨부 묶음은 붙은 문서 행을 묶음보다 먼저 잠근다. 문서 이동은 문서 행 → 첨부 → 묶음 순서로 잡으므로 묶음을 먼저 잡으면
    -- 서로 기다릴 수 있다(교착 방지).
    IF v_batch.root_kind = 'file' THEN
      PERFORM 1 FROM documents WHERE id = (SELECT document_id FROM files WHERE id = v_batch.root_id) FOR SHARE;
    END IF;
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_trash_expires_at(v_batch) <= now() AND NOT app_batch_keeps_referenced(v_batch) THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
    END IF;
    IF v_batch.root_kind = 'module' THEN
      -- 모듈 관리는 관리자 일이다. 지운 뒤 관리자가 아니게 된 사람은 되살리지 못한다.
      IF app_trash_role(v_batch) <> 'admin' THEN
        RAISE EXCEPTION 'only an admin can restore a module' USING ERRCODE = '42501';
      END IF;
      PERFORM 1 FROM modules WHERE id = v_batch.root_id FOR UPDATE;
      UPDATE modules SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'module', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
      UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
      RETURN;
    END IF;
    -- 지우기와 같은 잠금 순서(문서: 트리 → 문서 행 → 항목, 업무: 열 → 업무 행 → 항목).
    IF v_batch.root_kind = 'document' THEN
      SELECT * INTO v_doc FROM documents WHERE id = v_batch.root_id;
      PERFORM app_lock_tree(CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END,
                            v_doc.item_id, v_doc.owner_user_id);
      PERFORM 1 FROM documents WHERE trash_batch_id = p_batch ORDER BY id FOR NO KEY UPDATE;
      v_item := v_doc.item_id;
    ELSIF v_batch.root_kind = 'task' THEN
      v_ids := ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch);
      PERFORM app_lock_task_rows(v_ids, NULL);
      SELECT item_id INTO v_item FROM tasks WHERE id = v_batch.root_id;
    ELSIF v_batch.root_kind = 'event' THEN
      SELECT item_id INTO v_item FROM events WHERE id = v_batch.root_id FOR NO KEY UPDATE;
    ELSIF v_batch.root_kind = 'file' THEN
      -- 붙은 문서 행을 먼저 공유 잠금한다(#87): 공유 철회·공개 링크 철회·문서 휴지통 이동은 문서 행을 FOR UPDATE로 잡으므로
      -- 복원은 그 뒤에 지금 권한·휴지통 상태를 본다. 문서 → 파일 순서는 지우기(트리 → 문서 행 → 항목)와 같은 방향이다.
      PERFORM 1 FROM documents WHERE id = (SELECT document_id FROM files WHERE id = v_batch.root_id) FOR SHARE;
      -- 항목 문서에 붙은 파일은 그 항목의 권한·보관을 본다.
      SELECT coalesce(f.item_id, d.item_id), f.document_id INTO v_item, v_file_doc
      FROM files f LEFT JOIN documents d ON d.id = f.document_id
      WHERE f.id = v_batch.root_id FOR NO KEY UPDATE OF f;
      -- 붙은 문서가 휴지통에 있으면 먼저 문서를 되살려야 한다.
      IF EXISTS (SELECT 1 FROM files f JOIN documents d ON d.id = f.document_id
                 WHERE f.id = v_batch.root_id AND d.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the document first' USING ERRCODE = '55000';
      END IF;
    ELSE
      PERFORM 1 FROM items WHERE id = v_batch.root_id FOR NO KEY UPDATE;
      IF EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                 WHERE i.id = v_batch.root_id AND m.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the module first' USING ERRCODE = '55000';
      END IF;
      -- 보관된 모듈 아래에서는 항목도 복구하지 못한다. 모듈 보관을 푼 뒤 복구한다(D-113·D-125).
      IF EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                 WHERE i.id = v_batch.root_id AND m.archived_at IS NOT NULL) THEN
        RAISE EXCEPTION 'the module is archived' USING ERRCODE = '55000';
      END IF;
    END IF;
    IF v_item IS NOT NULL THEN
      PERFORM app_lock_item(v_item);
      IF EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id
                 WHERE i.id = v_item AND m.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the module first' USING ERRCODE = '55000';
      END IF;
      -- 항목이 휴지통에 있으면 먼저 항목을 되살려야 한다. 보관된 항목에는 되살리지 않는다(지우기와 같은 규칙).
      IF EXISTS (SELECT 1 FROM items WHERE id = v_item AND deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the item first' USING ERRCODE = '55000';
      END IF;
      IF app_item_archived(v_item) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      -- 지운 사람은 지금도 그 항목에 쓸 수 있어야 한다(볼 수 없게 됐으면 없는 것과 같다).
      IF app_trash_role(v_batch) = 'deleter' AND app_access('item', v_item) <> 'owner'
         AND app_access('item', v_item) <> 'write' THEN
        RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
      END IF;
    END IF;
    -- 잠금 뒤 다시 본다: 기다리는 사이 역할을 잃었으면(강등·제거·공유 철회) 없는 것과 같다.
    IF app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    -- 문서에 붙은 첨부는 지운 사람이 지금도 그 문서에 쓸 수 있어야 되살린다(항목 없는 위키 문서·공유 철회 포함).
    IF v_batch.root_kind = 'file' AND v_file_doc IS NOT NULL AND app_trash_role(v_batch) = 'deleter'
       AND app_access('document', v_file_doc) <> 'owner'
       AND app_access('document', v_file_doc) <> 'write' THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_batch.root_kind = 'document' THEN
      UPDATE documents SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE documents d SET parent_id = NULL
      WHERE d.id = v_batch.root_id AND d.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM documents p WHERE p.id = d.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit_document(app_id(p_ids, 1), v_batch.root_id, 'restore', jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      UPDATE tasks SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE tasks t SET parent_id = NULL
      WHERE t.id = v_batch.root_id AND t.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM tasks p WHERE p.id = t.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'task', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'item' THEN
      UPDATE items SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'item', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'file' THEN
      UPDATE files SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, 1), v_batch.workspace_id, v_batch.owner_user_id, app_user_id(), 'restore', 'file',
              v_batch.root_id, jsonb_build_object('batch', p_batch));
    ELSE
      UPDATE events SET deleted_at = NULL, deleted_by = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(app_id(p_ids, 1), v_batch.workspace_id, 'restore', 'event', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    END IF;
    UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_restore_batch(p_batch uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user     uuid := app_user_id();
    v_doc      documents%ROWTYPE;
    v_source   document_revisions%ROWTYPE;
    v_replay   document_revisions%ROWTYPE;
    v_next     integer;
  BEGIN
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    PERFORM app_lock_item(v_doc.item_id);
    PERFORM app_require_document(p_doc, 'write');

    -- 같은 사람의 같은 요청 재시도: 같은 리비전의 복원이고 그 결과가 아직 최신일 때만 성공.
    SELECT * INTO v_replay FROM document_revisions
    WHERE document_id = p_doc AND author = v_user AND request_id = p_request_id;
    IF v_replay.document_id IS NOT NULL THEN
      IF v_replay.restored_from IS DISTINCT FROM p_revision THEN
        RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
      END IF;
      RETURN CASE WHEN v_replay.revision = v_doc.revision THEN 'restored' ELSE 'conflict' END;
    END IF;
    IF v_doc.revision <> p_base_revision THEN
      RETURN 'conflict';
    END IF;

    SELECT * INTO v_source FROM document_revisions WHERE document_id = p_doc AND revision = p_revision;
    IF v_source.document_id IS NULL THEN
      RAISE EXCEPTION 'revision not found' USING ERRCODE = 'P0002';
    END IF;
    -- 옛 저장 형식으로 문서를 되돌리지 않는다(형식 변환이 생기면 변환 뒤 복원).
    IF v_source.schema_version <> p_schema_version THEN
      RAISE EXCEPTION 'this revision uses an older document format' USING ERRCODE = '55000';
    END IF;
    IF v_source.title = v_doc.title AND v_source.body = v_doc.body
       AND v_source.schema_version = v_doc.schema_version THEN
      RETURN 'unchanged';
    END IF;

    v_next := v_doc.revision + 1;
    UPDATE documents
    SET title = v_source.title, body = v_source.body, schema_version = v_source.schema_version,
        revision = v_next, updated_by = v_user, updated_at = now()
    WHERE id = p_doc;
    INSERT INTO document_revisions (document_id, revision, title, schema_version, body, author, request_id,
                                    restored_from)
    VALUES (p_doc, v_next, v_source.title, v_source.schema_version, v_source.body, v_user, p_request_id,
            p_revision);
    -- 앱이 공급한 UUID로 제목 변경 감사를 리비전 기록 뒤에 남긴다(D-88).
    PERFORM app_audit_document_rename(app_id(p_ids, 1), p_doc, v_next);
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'restore_revision',
                               jsonb_build_object('from', p_revision, 'to', v_next));
    PERFORM app_record_document_references(p_doc, false, '{}'::uuid[]);
    RETURN 'restored';
  END
  $$;


ALTER FUNCTION fvoci.app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_kind text := TG_ARGV[0];
    v_id   uuid := CASE WHEN TG_OP = 'DELETE' THEN OLD.id ELSE NEW.id END;
  BEGIN
    INSERT INTO search_queue (kind, target_id) VALUES (v_kind, v_id)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_assignee() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    VALUES ('task', CASE WHEN TG_OP = 'DELETE' THEN OLD.task_id ELSE NEW.task_id END)
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_assignee() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_document_files() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'file', id FROM files WHERE document_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_document_files() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_item_children() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE item_id = NEW.id
    UNION ALL SELECT 'task', id FROM tasks WHERE item_id = NEW.id
    UNION ALL SELECT 'event', id FROM events WHERE item_id = NEW.id
    UNION ALL SELECT 'file', id FROM files WHERE item_id = NEW.id
    UNION ALL SELECT 'file', f.id FROM files f JOIN documents d ON d.id = f.document_id WHERE d.item_id = NEW.id
    UNION ALL SELECT 'record', id FROM item_records WHERE item_id = NEW.id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_item_children() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_enqueue_module() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    INSERT INTO search_queue (kind, target_id)
    SELECT 'item', i.id FROM items i WHERE i.module_id = NEW.module_id
    UNION ALL SELECT 'task', t.id FROM tasks t JOIN items i ON i.id = t.item_id WHERE i.module_id = NEW.module_id
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_search_enqueue_module() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_search_visible(p_kinds text[], p_ids uuid[]) RETURNS TABLE(kind text, id uuid)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT w.kind, w.id FROM unnest(p_kinds, p_ids) AS w(kind, id)
    WHERE CASE w.kind
      WHEN 'event' THEN app_access('event', w.id) <> 'none'
      WHEN 'file' THEN app_access('file', w.id) <> 'none'
      WHEN 'record' THEN EXISTS (SELECT 1 FROM item_records r
                                 WHERE r.id = w.id AND app_access('item', r.item_id) <> 'none')
      ELSE app_access(w.kind, w.id) <> 'none'
    END
  $$;


ALTER FUNCTION fvoci.app_search_visible(p_kinds text[], p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_session_user(p_token_hash bytea) RETURNS uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT user_id FROM sessions WHERE token_hash = p_token_hash AND expires_at > now() $$;


ALTER FUNCTION fvoci.app_session_user(p_token_hash bytea) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
    PERFORM app_audit(app_id(p_ids, 1), (SELECT workspace_id FROM items WHERE id = p_item), 'update_course', 'item',
                      p_item, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_seen documents%ROWTYPE;
    v_doc  documents%ROWTYPE;
  BEGIN
    -- 권한을 먼저 확인한 뒤에만 잠근다. 자리 잠금 → 행 잠금 순서. 잠근 뒤 자리가 바뀌었으면(그사이 이동) 다시 하게 한다.
    PERFORM app_require_document(p_doc, 'write');
    SELECT * INTO v_seen FROM documents WHERE id = p_doc;
    PERFORM app_lock_tree(v_seen.workspace_id, v_seen.item_id, v_seen.owner_user_id);
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_item(v_doc.item_id);
    PERFORM app_require_document(p_doc, 'write');
    PERFORM app_place_document(p_doc, p_parent, p_before, p_max_depth, p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text;
  BEGIN
    PERFORM 1 FROM documents WHERE id = p_doc FOR NO KEY UPDATE;
    v_access := app_access('document', p_doc);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the owner can mark a template' USING ERRCODE = '42501';
    END IF;
    UPDATE documents SET is_template = p_template WHERE id = p_doc;
    PERFORM app_audit_document(app_id(p_ids, 1), p_doc,
                               CASE WHEN p_template THEN 'mark_template' ELSE 'unmark_template' END, '{}'::jsonb);
  END
  $$;


ALTER FUNCTION fvoci.app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_episode_progress(p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item  uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_today date := coalesce(p_today, current_date);
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    -- 개정 2026-10-26(⑧, D-20): 읽기 전용은 본인 진도도 남기지 못한다.
    IF app_access('item', v_item) = 'read' THEN
      RAISE EXCEPTION 'read-only members cannot record progress' USING ERRCODE = '42501';
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


ALTER FUNCTION fvoci.app_set_episode_progress(p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) OWNER TO fvoci_migrator;

-- 현재 규칙이 요청과 이미 같으면 재시도 성공으로 끝낸다. 다른 경우에만 잠근 행의 규칙과 읽은 기준을 비교한다.
-- 기준이 지난 요청은 규칙·판·감사를 덮지 않는다. D-94·D-125: docs/decisions/tasks.md#d-94.
CREATE FUNCTION fvoci.app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_event events%ROWTYPE;
  BEGIN
    PERFORM app_require_event_write(p_event);
    IF p_rule IS NOT NULL THEN
      PERFORM app_check_recurrence(p_rule);
    END IF;
    SELECT * INTO v_event FROM events WHERE id = p_event FOR UPDATE;
    PERFORM app_lock_item(v_event.item_id);
    PERFORM app_require_event_write(p_event);
    IF jsonb_strip_nulls(v_event.recurrence) IS DISTINCT FROM jsonb_strip_nulls(p_rule)
       AND jsonb_strip_nulls(v_event.recurrence) IS DISTINCT FROM jsonb_strip_nulls(p_base) THEN
      RETURN 'conflict';
    END IF;
    IF jsonb_strip_nulls(v_event.recurrence) IS NOT DISTINCT FROM jsonb_strip_nulls(p_rule) THEN
      RETURN 'updated';
    END IF;
    IF (p_rule ->> 'until')::date < coalesce(v_event.start_date, (v_event.starts_at AT TIME ZONE v_event.timezone)::date)
    THEN
      RAISE EXCEPTION 'until must not be before the first occurrence' USING ERRCODE = '22023';
    END IF;
    UPDATE events SET recurrence = p_rule, version = version + 1, updated_by = app_user_id(), updated_at = now()
    WHERE id = p_event;
    PERFORM app_audit(app_id(p_ids, 1), v_event.workspace_id, 'set_recurrence', 'event', p_event,
                      jsonb_build_object('from', v_event.recurrence, 'to', p_rule));
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old text;
  BEGIN
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
    UPDATE workspace_members SET role = p_role WHERE workspace_id = p_workspace AND user_id = p_member;
    PERFORM app_audit(app_id(p_ids, 1), p_workspace, 'change_role', 'member', p_member,
                      jsonb_build_object('from', v_old, 'to', p_role));
    PERFORM app_drop_unseen_assignees(NULL, p_workspace, p_member, 'role_change', app_ids_from(p_ids, 2));
  END
  $$;


ALTER FUNCTION fvoci.app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_member_role_ids(p_workspace uuid, p_member uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('workspace', p_workspace) = 'owner'
                THEN 1 + 2 * app_unseen_assignees_bound(NULL, p_workspace, p_member) ELSE 0 END
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

CREATE FUNCTION fvoci.app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row  jsonb;
    v_pos  integer := 0;
    v_file files%ROWTYPE;
    v_ver  file_versions%ROWTYPE;
    v_had  record_files%ROWTYPE;
    v_before record_files[];
    v_used integer[] := '{}';
  BEGIN
    IF jsonb_typeof(coalesce(p_files, '[]'::jsonb)) <> 'array' OR jsonb_array_length(coalesce(p_files, '[]')) > 50 THEN
      RAISE EXCEPTION 'give at most 50 linked files' USING ERRCODE = '22023';
    END IF;
    -- 지난 연결을 기억해 두고, 다시 보낸 것은 그대로 쓰고, 파일이 영구 삭제된 연결은 끝에 그대로 둔다.
    SELECT coalesce(array_agg(rf ORDER BY rf.position), '{}') INTO v_before
    FROM record_files rf WHERE rf.record_id = p_record;
    DELETE FROM record_files WHERE record_id = p_record;
    FOR v_row IN SELECT * FROM jsonb_array_elements(coalesce(p_files, '[]'::jsonb)) LOOP
      v_had := NULL;
      SELECT * INTO v_had FROM unnest(v_before) b
      WHERE b.file_id = (v_row->>'file_id')::uuid AND b.version = (v_row->>'version')::integer
        AND b.position <> ALL (v_used)
      ORDER BY b.position LIMIT 1;
      IF v_had.record_id IS NOT NULL THEN
        -- 이미 연결된 파일·버전: 파일이 휴지통에 있어도 연결을 그대로 쓴다(되살릴 수 있고, 연결이 지킨다, D-56).
        INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
        VALUES (p_record, v_pos, coalesce(v_row->>'role', v_had.role), v_had.file_id, v_had.version_id,
                v_had.file_name, v_had.version, v_had.sha256, v_had.size);
        v_used := v_used || v_had.position;
        v_pos := v_pos + 1;
        CONTINUE;
      END IF;
      SELECT * INTO v_file FROM files
      WHERE id = (v_row->>'file_id')::uuid AND item_id = p_item AND deleted_at IS NULL FOR KEY SHARE;
      IF v_file.id IS NULL THEN
        RAISE EXCEPTION 'linked files must be attachments of this item' USING ERRCODE = '22023';
      END IF;
      SELECT * INTO v_ver FROM file_versions
      WHERE file_id = v_file.id AND version = coalesce((v_row->>'version')::integer, v_file.current_version)
      FOR KEY SHARE;
      IF v_ver.id IS NULL THEN
        RAISE EXCEPTION 'file version not found' USING ERRCODE = '22023';
      END IF;
      INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
      VALUES (p_record, v_pos, coalesce(v_row->>'role', 'other'), v_file.id, v_ver.id, v_file.name, v_ver.version,
              v_ver.sha256, v_ver.size);
      v_pos := v_pos + 1;
    END LOOP;
    -- 파일이 영구 삭제되어 이름·버전·SHA-256만 남은 연결은 화면이 다시 보낼 수 없으므로 그대로 둔다(D-35).
    INSERT INTO record_files (record_id, position, role, file_id, version_id, file_name, version, sha256, size)
    SELECT p_record, v_pos + row_number() OVER (ORDER BY b.position) - 1, b.role, NULL, NULL,
           b.file_name, b.version, b.sha256, b.size
    FROM unnest(v_before) b WHERE b.file_id IS NULL;
  EXCEPTION
    WHEN invalid_text_representation OR check_violation OR numeric_value_out_of_range THEN
      RAISE EXCEPTION 'invalid linked file: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_storage_upload(p_upload uuid, p_storage_upload text) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE uploads SET storage_upload_id = p_storage_upload
    WHERE id = p_upload AND user_id = app_user_id() AND storage_upload_id IS NULL
  $$;


ALTER FUNCTION fvoci.app_set_storage_upload(p_upload uuid, p_storage_upload text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer, p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[]) RETURNS TABLE(outcome text, old_kind text, old_lag integer)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_succ  tasks%ROWTYPE;
    v_pred  tasks%ROWTYPE;
    v_other tasks%ROWTYPE;
    v_old   task_dependencies%ROWTYPE;
    v_shift jsonb;
    v_ids   uuid[];
    v_state text;
    v_stale boolean := false;
    v_lock  uuid[];
    v_start date;
    v_due   date;
    v_at    integer := 1;
  BEGIN
    IF p_kind IS NULL OR p_kind NOT IN ('FS', 'SS', 'FF', 'SF') THEN
      RAISE EXCEPTION 'kind must be FS, SS, FF or SF' USING ERRCODE = '22023';
    END IF;
    IF p_lag IS NULL OR p_lag < 0 OR p_lag > 3650 THEN
      RAISE EXCEPTION 'lag_days must be between 0 and 3650' USING ERRCODE = '22023';
    END IF;
    IF p_shifts IS NULL OR jsonb_typeof(p_shifts) <> 'array' THEN
      RAISE EXCEPTION 'shifts must be an array' USING ERRCODE = '22023';
    END IF;
    PERFORM app_require_task_write(p_successor);
    IF app_access('task', p_predecessor) = 'none' THEN
      RAISE EXCEPTION 'task not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_succ FROM tasks WHERE id = p_successor;
    SELECT * INTO v_pred FROM tasks WHERE id = p_predecessor;
    IF v_pred.item_id <> v_succ.item_id THEN
      RAISE EXCEPTION 'dependencies must stay within one item' USING ERRCODE = '22023';
    END IF;
    IF p_predecessor = p_successor THEN
      RAISE EXCEPTION 'a task cannot depend on itself' USING ERRCODE = '22023';
    END IF;
    SELECT array_agg(DISTINCT (s ->> 'task_id')::uuid) INTO v_ids FROM jsonb_array_elements(p_shifts) s;
    IF coalesce(cardinality(v_ids), 0) <> jsonb_array_length(p_shifts) THEN
      RAISE EXCEPTION 'each shifted task must appear once' USING ERRCODE = '22023';
    END IF;
    -- 후속 업무와 옮길 업무(같은 항목만)를 열 → 행 순으로 잠그고, 항목 공유 → 관계 잠금(app_update_task와 같은 순서).
    SELECT array_agg(id) INTO v_lock FROM tasks
    WHERE id = ANY (coalesce(v_ids, '{}')) AND id <> p_successor AND item_id = v_succ.item_id;
    PERFORM app_lock_task_rows(array_prepend(p_successor, coalesce(v_lock, '{}')), NULL);
    PERFORM app_lock_item(v_succ.item_id);
    PERFORM app_lock_dependencies(v_succ.item_id);
    PERFORM app_require_task_write(p_successor);
    SELECT * INTO v_old FROM task_dependencies WHERE predecessor_id = p_predecessor AND successor_id = p_successor;
    -- 화면이 편집을 시작한 관계(새 관계면 없음)와 잠근 뒤의 관계가 다르면 아무것도 바꾸지 않는다. 지금 관계가
    -- 이미 보낸 종류·지연일과 같으면(응답을 잃은 같은 요청의 재시도 등) 충돌로 보지 않는다.
    IF NOT (v_old.predecessor_id IS NOT NULL AND v_old.kind = p_kind AND v_old.lag_days = p_lag)
       AND ((v_old.predecessor_id IS NULL AND p_base_kind IS NOT NULL)
            OR (v_old.predecessor_id IS NOT NULL
                AND (p_base_kind IS NULL OR v_old.kind IS DISTINCT FROM p_base_kind
                     OR v_old.lag_days IS DISTINCT FROM p_base_lag))) THEN
      RETURN QUERY SELECT 'link_conflict', v_old.kind, v_old.lag_days;
      RETURN;
    END IF;
    -- 새 관계: 후속에서 출발해 선행에 닿으면 순환이다.
    IF v_old.predecessor_id IS NULL AND EXISTS (
      WITH RECURSIVE reach(id) AS (
        SELECT p_successor
        UNION
        SELECT d.successor_id FROM task_dependencies d JOIN reach r ON d.predecessor_id = r.id
      )
      SELECT 1 FROM reach WHERE id = p_predecessor
    ) THEN
      RAISE EXCEPTION 'the dependency would create a cycle' USING ERRCODE = '22023';
    END IF;

    -- 승인한 재배치: 관계를 고친 후속 업무 자신이거나 그 뒤 업무여야 하고, 기준 버전이 모두 맞아야 한다. 같은
    -- 항목인데 지금은 이어지지 않는 업무(미리본 뒤 관계가 지워짐)는 오래된 계획이라 `shift_conflict`(아무것도 바꾸지
    -- 않음), 처음부터 옮길 수 없는 업무(없음·다른 항목·관계의 선행 업무)는 400 그대로다(지금 선행 쪽인 업무도 오래된 계획).
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_state := CASE WHEN v_other.id = p_predecessor THEN 'invalid'
                      ELSE app_shift_target(p_successor, v_other.id, true) END;
      IF v_state = 'invalid' THEN
        RAISE EXCEPTION 'a shifted task must be the successor or come after it' USING ERRCODE = '22023';
      END IF;
      -- 오래된 항목은 날짜 모양을 보기 전에 거른다.
      IF v_state = 'stale' OR v_other.version IS DISTINCT FROM (v_shift ->> 'base_version')::integer THEN
        v_stale := true;
        CONTINUE;
      END IF;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      IF (v_start IS NULL) <> (v_other.start_date IS NULL) OR (v_due IS NULL) <> (v_other.due_date IS NULL)
         OR (v_start IS NOT NULL AND v_due IS NOT NULL AND v_start > v_due) THEN
        RAISE EXCEPTION 'a shift moves existing dates only' USING ERRCODE = '22023';
      END IF;
    END LOOP;
    IF v_stale THEN
      RETURN QUERY SELECT 'shift_conflict', v_old.kind, v_old.lag_days;
      RETURN;
    END IF;

    IF v_old.predecessor_id IS NULL THEN
      INSERT INTO task_dependencies (predecessor_id, successor_id, item_id, created_by, kind, lag_days)
      VALUES (p_predecessor, p_successor, v_succ.item_id, v_user, p_kind, p_lag);
      PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'link_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor, 'kind', p_kind, 'lag_days', p_lag));
    ELSIF v_old.kind <> p_kind OR v_old.lag_days <> p_lag THEN
      UPDATE task_dependencies SET kind = p_kind, lag_days = p_lag
      WHERE predecessor_id = p_predecessor AND successor_id = p_successor;
      PERFORM app_audit(app_id(p_ids, 1), v_succ.workspace_id, 'change_predecessor', 'task', p_successor,
                        jsonb_build_object('predecessor', p_predecessor,
                                           'from', jsonb_build_object('kind', v_old.kind, 'lag_days', v_old.lag_days),
                                           'to', jsonb_build_object('kind', p_kind, 'lag_days', p_lag)));
    ELSIF jsonb_array_length(p_shifts) = 0 THEN
      RETURN QUERY SELECT 'unchanged', v_old.kind, v_old.lag_days;
      RETURN;
    END IF;

    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      UPDATE tasks SET start_date = v_start, due_date = v_due, version = version + 1,
                       updated_by = v_user, updated_at = now()
      WHERE id = v_other.id;
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_succ.workspace_id, 'reschedule', 'task', v_other.id,
                        jsonb_build_object('reason', 'dependency_changed', 'predecessor', p_predecessor,
                                           'successor', p_successor,
                                           'from', jsonb_build_array(v_other.start_date, v_other.due_date),
                                           'to', jsonb_build_array(v_start, v_due)));
    END LOOP;
    RETURN QUERY SELECT CASE WHEN v_old.predecessor_id IS NULL THEN 'created' ELSE 'updated' END,
                        v_old.kind, v_old.lag_days;
  END
  $$;


ALTER FUNCTION fvoci.app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer, p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task tasks%ROWTYPE;
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_check_task_parent(p_task, p_parent, v_task.item_id);
    IF p_parent IS NULL THEN
      PERFORM app_lock_task_rows(ARRAY[p_task], NULL);
    END IF;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    IF v_task.parent_id IS NOT DISTINCT FROM p_parent THEN
      RETURN;
    END IF;
    UPDATE tasks SET parent_id = p_parent, updated_by = app_user_id(), updated_at = now() WHERE id = p_task;
    PERFORM app_audit(app_id(p_ids, 1), v_task.workspace_id, 'reparent', 'task', p_task,
                      jsonb_build_object('parent', p_parent));
  END
  $$;


ALTER FUNCTION fvoci.app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_set_unit_progress(p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item  uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
    v_today date := coalesce(p_today, current_date);
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    -- 개정 2026-10-26(⑧, D-20): 읽기 전용은 본인 진도도 남기지 못한다.
    IF app_access('item', v_item) = 'read' THEN
      RAISE EXCEPTION 'read-only members cannot record progress' USING ERRCODE = '42501';
    END IF;
    IF p_status NOT IN ('not_started', 'in_progress', 'done') THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    IF coalesce(p_done, 0) NOT BETWEEN 0 AND 1000000 OR length(coalesce(p_note, '')) > 20000 THEN
      RAISE EXCEPTION 'invalid progress' USING ERRCODE = '22023';
    END IF;
    IF abs(v_today - current_date) > 1 THEN
      RAISE EXCEPTION 'today is too far from the server date' USING ERRCODE = '22023';
    END IF;
    INSERT INTO unit_progress (unit_id, user_id, status, done_amount, planned_on, completed_on, note, updated_at)
    VALUES (p_unit, app_user_id(), p_status, coalesce(p_done, 0), p_planned,
            CASE WHEN p_status = 'done' THEN v_today END, coalesce(p_note, ''), now())
    ON CONFLICT (unit_id, user_id) DO UPDATE SET
      status = EXCLUDED.status,
      done_amount = EXCLUDED.done_amount,
      planned_on = EXCLUDED.planned_on,
      completed_on = CASE WHEN EXCLUDED.status <> 'done' THEN NULL
                          WHEN unit_progress.status = 'done' THEN unit_progress.completed_on
                          ELSE EXCLUDED.completed_on END,
      note = EXCLUDED.note,
      updated_at = now();
  END
  $$;


ALTER FUNCTION fvoci.app_set_unit_progress(p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_shift_excluded_dates() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_old date := coalesce(OLD.start_date, (OLD.starts_at AT TIME ZONE OLD.timezone)::date);
    v_new date := coalesce(NEW.start_date, (NEW.starts_at AT TIME ZONE NEW.timezone)::date);
  BEGIN
    IF cardinality(NEW.excluded_dates) > 0 AND v_old IS DISTINCT FROM v_new AND NEW.recurrence IS NOT NULL
       AND (NEW.recurrence ->> 'freq' = 'daily'
            OR (NEW.recurrence ->> 'freq' = 'weekly' AND coalesce(jsonb_array_length(NEW.recurrence -> 'weekdays'), 0) = 0)) THEN
      NEW.excluded_dates := ARRAY(SELECT d + (v_new - v_old) FROM unnest(NEW.excluded_dates) d);
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_shift_excluded_dates() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_shift_target(p_from uuid, p_task uuid, p_include_from boolean) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE
      WHEN t.id IS NULL OR t.item_id <> f.item_id THEN 'invalid'
      WHEN t.id = f.id THEN CASE WHEN p_include_from THEN 'successor' ELSE 'invalid' END
      WHEN EXISTS (
        WITH RECURSIVE down(id) AS (
          SELECT d.successor_id FROM task_dependencies d WHERE d.predecessor_id = f.id
          UNION
          SELECT d.successor_id FROM task_dependencies d JOIN down r ON d.predecessor_id = r.id
        )
        SELECT 1 FROM down WHERE id = t.id
      ) THEN 'successor'
      ELSE 'stale'
    END
    FROM tasks f LEFT JOIN tasks t ON t.id = p_task
    WHERE f.id = p_from
  $$;


ALTER FUNCTION fvoci.app_shift_target(p_from uuid, p_task uuid, p_include_from boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_storage_used(p_workspace uuid, p_owner uuid, p_except uuid) RETURNS bigint
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT sum(v.size) FROM file_versions v JOIN files f ON f.id = v.file_id
                     WHERE f.workspace_id IS NOT DISTINCT FROM p_workspace
                       AND f.owner_user_id IS NOT DISTINCT FROM p_owner), 0)
         + coalesce((SELECT sum(u.size) FROM uploads u
                     WHERE u.state IN ('uploading', 'completing') AND u.id IS DISTINCT FROM p_except
                       AND u.workspace_id IS NOT DISTINCT FROM p_workspace
                       AND u.owner_user_id IS NOT DISTINCT FROM p_owner), 0)
  $$;


ALTER FUNCTION fvoci.app_storage_used(p_workspace uuid, p_owner uuid, p_except uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_study_access(p_item uuid) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
  BEGIN
    IF v_access = 'none' OR NOT EXISTS (
      SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id WHERE i.id = p_item AND m.kind = 'study') THEN
      RAISE EXCEPTION 'study not found' USING ERRCODE = 'P0002';
    END IF;
    RETURN v_access;
  END
  $$;


ALTER FUNCTION fvoci.app_study_access(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_study_participants(p_item uuid, p_today date) RETURNS TABLE(user_id uuid, display_name text, done integer, in_progress integer, overdue integer, done_amount bigint)
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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
      WHERE app_access_as('item', p_item, u.id) <> 'none'
      GROUP BY u.id, u.display_name
      ORDER BY u.display_name, u.id;
  END
  $$;


ALTER FUNCTION fvoci.app_study_participants(p_item uuid, p_today date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_item_series(p_item uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_access text := app_access('item', p_item);
    v_id     uuid;
    v_made   integer := 0;
    v_at     integer := 0;
    v_need   integer;
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'write access is required' USING ERRCODE = '42501';
    END IF;
    IF app_item_archived(p_item) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    -- 항목의 모든 열을 사용자 요청과 같은 순서(열 id 순)로 먼저 잠근다.
    FOR v_id IN SELECT id FROM task_statuses WHERE item_id = p_item ORDER BY id LOOP
      PERFORM app_lock_lane(v_id);
    END LOOP;
    FOR v_id IN
      SELECT s.id FROM task_series s JOIN tasks r ON r.id = s.root_task_id
      WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL
      ORDER BY s.id
    LOOP
      v_need := app_task_series_bound(v_id);
      v_made := v_made + app_sync_task_series(v_id, coalesce(p_ids[v_at + 1:v_at + v_need], '{}'::uuid[]));
      v_at := v_at + v_need;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_sync_item_series(p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_item_series_ids(p_item uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('item', p_item) = 'none' THEN 0 ELSE
      coalesce((SELECT sum(app_task_series_bound(s.id)) FROM task_series s JOIN tasks r ON r.id = s.root_task_id
                WHERE s.item_id = p_item AND s.rule IS NOT NULL AND r.deleted_at IS NULL), 0)::integer
    END
  $$;


ALTER FUNCTION fvoci.app_sync_item_series_ids(p_item uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_task_series(p_series uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT app_sync_task_series_as(p_series, false, p_ids) $$;


ALTER FUNCTION fvoci.app_sync_task_series(p_series uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_task_series_as(p_series uuid, p_system boolean, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_series task_series%ROWTYPE;
    v_root   tasks%ROWTYPE;
    v_today  date;
    v_date   date;
    v_id     uuid;
    v_made   integer := 0;
    v_user   uuid := CASE WHEN p_system THEN NULL ELSE app_user_id() END;
    v_module uuid;
    v_uid    uuid;
    v_at     integer := 0;
    v_count  integer;
    v_todo   uuid;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL THEN
      RAISE EXCEPTION 'series not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    -- 새 회차는 항목의 기본 상태(todo 범주 첫 상태, D-116) 열에 들어간다.
    v_todo := app_default_task_status((SELECT item_id FROM tasks WHERE id = v_series.root_task_id));
    PERFORM app_lock_task_rows(ARRAY[v_series.root_task_id], v_todo);
    SELECT * INTO v_root FROM tasks WHERE id = v_series.root_task_id;
    PERFORM app_lock_item(v_root.item_id);
    IF NOT p_system THEN
      PERFORM app_require_task_write(v_series.root_task_id);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM task_statuses WHERE id = v_todo) THEN
      RAISE EXCEPTION 'the default status changed while syncing; try again' USING ERRCODE = '40001';
    END IF;
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    -- 작업 프로세스는 만든 사람 이름으로 회차를 만들고 감사 기록에는 system으로 남긴다.
    v_user := coalesce(v_user, v_series.created_by);
    -- 잠근 뒤의 상태로 다시 본다: 규칙·날짜·휴지통(루트 업무·항목·모듈·워크스페이스)·보관.
    IF v_series.rule IS NULL OR (v_root.due_date IS NULL AND v_root.start_date IS NULL)
       OR v_root.deleted_at IS NOT NULL OR app_item_archived(v_root.item_id)
       OR EXISTS (SELECT 1 FROM items i JOIN modules m ON m.id = i.module_id JOIN workspaces w ON w.id = i.workspace_id
                  WHERE i.id = v_root.item_id
                    AND (i.deleted_at IS NOT NULL OR m.deleted_at IS NOT NULL OR w.deleted_at IS NOT NULL)) THEN
      RETURN 0;
    END IF;
    v_today := (now() AT TIME ZONE v_series.timezone)::date;
    SELECT module_id INTO v_module FROM items WHERE id = v_root.item_id;
    FOR v_date IN
      WITH wanted AS (
        SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_series.effective_from, v_today)
        UNION
        (SELECT occurrence
         FROM app_recurrence_dates(v_series.rule, v_series.anchor, greatest(v_today + 1, v_series.effective_from),
                                   greatest(v_today + 1, v_series.effective_from) + 3660)
         ORDER BY occurrence LIMIT 1)
      )
      SELECT w.occurrence FROM wanted w
      WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = p_series AND t.occurrence_date = w.occurrence)
        AND NOT (w.occurrence = ANY (v_series.skipped_dates))
      ORDER BY w.occurrence
      LIMIT 50
    LOOP
      v_id := app_id(p_ids, v_at + 1);
      INSERT INTO tasks (id, workspace_id, item_id, number, title, description, status_id, priority, start_date,
                         due_date, estimate_minutes, tags, milestone, position, completed_at, created_by, updated_by,
                         request_id, series_id, occurrence_date)
      VALUES (v_id, v_root.workspace_id, v_root.item_id,
              app_allocate_number(v_root.workspace_id, v_module, 'task', v_id),
              v_root.title, v_root.description, v_todo, v_root.priority,
              v_root.start_date + (v_date - v_series.anchor), v_root.due_date + (v_date - v_series.anchor),
              v_root.estimate_minutes, v_root.tags, v_root.milestone, app_task_lane_end(v_todo),
              CASE WHEN (SELECT category FROM task_statuses WHERE id = v_todo) = 'done' THEN now() END,
              v_user, v_user, app_id(p_ids, v_at + 2), p_series, v_date)
      ON CONFLICT (series_id, occurrence_date) DO NOTHING;
      v_at := v_at + 2;
      IF NOT FOUND THEN
        CONTINUE;
      END IF;
      INSERT INTO checklist_items (id, task_id, text, checked, position, request_id, created_by)
      SELECT app_id(p_ids, v_at + 2 * c.n - 1), v_id, c.text, false, c.position, app_id(p_ids, v_at + 2 * c.n), v_user
      FROM (SELECT c.*, (row_number() OVER (ORDER BY c.position, c.id))::integer AS n
            FROM checklist_items c WHERE c.task_id = v_root.id) c;
      GET DIAGNOSTICS v_count = ROW_COUNT;
      v_at := v_at + 2 * v_count;
      FOR v_uid IN SELECT a.user_id FROM task_assignees a WHERE a.task_id = v_root.id ORDER BY a.user_id LOOP
        IF EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = v_root.workspace_id AND m.user_id = v_uid)
           AND app_access_as('item', v_root.item_id, v_uid) <> 'none' THEN
          INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (v_id, v_uid, v_user);
        END IF;
      END LOOP;
      v_at := v_at + 1;
      INSERT INTO audit_log (id, workspace_id, actor, action, target_type, target_id, detail)
      VALUES (app_id(p_ids, v_at), v_root.workspace_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END,
              'create', 'task', v_id,
              jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'series', p_series,
                                 'occurrence', v_date)
                || CASE WHEN p_system THEN jsonb_build_object('system', true) ELSE '{}'::jsonb END);
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_root.workspace_id, v_root.item_id, 'task', v_id, 'create',
                           jsonb_build_object('title', v_root.title, 'reason', 'recurrence', 'occurrence', v_date));
      v_made := v_made + 1;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_sync_task_series_as(p_series uuid, p_system boolean, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_sync_task_series_ids(p_series uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_access('task', s.root_task_id) = 'none' THEN 0 ELSE app_task_series_bound(s.id) END
    FROM task_series s WHERE s.id = p_series
  $$;


ALTER FUNCTION fvoci.app_sync_task_series_ids(p_series uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[]) RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RAISE EXCEPTION 'system function' USING ERRCODE = '42501';
    END IF;
    -- 지운(휴지통) 문서는 저장만 하지 않고 참조는 그대로 둔다. 낡은 상태만 참조를 지운다.
    IF NOT EXISTS (SELECT 1 FROM documents WHERE id = p_doc AND deleted_at IS NULL) THEN
      RETURN false;
    END IF;
    IF NOT app_collab_state_current(p_doc, p_revision) THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      RETURN false;
    END IF;
    -- 편집자를 함께 적는다. 리비전으로 남은 상태에는 남길 편집이 없으므로 비운다.
    INSERT INTO document_collab_states (document_id, revision, state, pending, editors, updated_at)
    VALUES (p_doc, p_revision, p_state, p_pending,
            CASE WHEN p_pending THEN coalesce(p_editors, '{}') ELSE '{}' END, now())
    ON CONFLICT (document_id) DO UPDATE
      SET revision = excluded.revision, state = excluded.state, pending = excluded.pending,
          editors = excluded.editors, updated_at = now();
    IF NOT p_pending THEN
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
    ELSIF p_body IS NOT NULL THEN
      PERFORM 1 FROM file_versions v
      JOIN (SELECT DISTINCT r.file_id, r.version FROM app_file_refs(p_body) r) r
        ON r.file_id = v.file_id AND r.version = v.version
      ORDER BY v.id
      FOR KEY SHARE OF v;
      DELETE FROM document_collab_file_refs WHERE document_id = p_doc;
      INSERT INTO document_collab_file_refs (document_id, file_id, version)
      SELECT DISTINCT p_doc, r.file_id, r.version FROM app_file_refs(p_body) r
      ON CONFLICT DO NOTHING;
    END IF;
    RETURN true;
  END
  $$;


ALTER FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_system_expire_uploads() RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    WITH gone AS (
      UPDATE uploads SET state = 'expired', state_changed_at = now()
      WHERE expires_at <= now()
        AND (state = 'uploading'
             OR (state = 'completing' AND state_changed_at < now() - interval '10 minutes'))
      RETURNING id, storage_key, storage_upload_id
    ), parts AS (
      DELETE FROM upload_parts WHERE upload_id IN (SELECT id FROM gone)
    )
    INSERT INTO storage_deletions (storage_key, storage_upload_id)
    SELECT storage_key, coalesce(storage_upload_id, '') FROM gone
    UNION SELECT storage_key, '' FROM gone
    ON CONFLICT DO NOTHING;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_expire_uploads() OWNER TO fvoci_migrator;

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
                                                          THEN (r.fields->>'revision_due')::date END)) AS x(key, on_date)
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

CREATE FUNCTION fvoci.app_system_purge_expired(p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_ws    uuid;
    v_made  integer := 0;
    v_at    integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_ws IN
      SELECT id FROM workspaces
      WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now()
      ORDER BY deleted_at
    LOOP
      -- 워크스페이스마다 새 id 둘(소유자 로그·조직 로그).
      v_at := v_at + 2;
      BEGIN
        IF app_purge_workspace_as(v_ws, true, ARRAY[app_id(p_ids, v_at - 1), app_id(p_ids, v_at)]) THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION
        WHEN SQLSTATE 'FV001' OR SQLSTATE 'FV002' THEN
          -- 새 id 계약 위반(D-88)은 묶음 실패가 아니라 호출 전체를 되돌려 다시 하게 한다.
          RAISE;
        WHEN OTHERS THEN
          RAISE WARNING 'purge of deleted workspace % failed with SQLSTATE %', v_ws, SQLSTATE;
      END;
    END LOOP;
    FOR v_batch IN
      SELECT * FROM trash_batches WHERE restored_at IS NULL AND purged_at IS NULL ORDER BY deleted_at
    LOOP
      CONTINUE WHEN app_trash_expires_at(v_batch) > now();
      v_at := v_at + 1;
      BEGIN
        PERFORM app_purge_batch_as(v_batch.id, true, ARRAY[app_id(p_ids, v_at)]);
        IF (SELECT purged_at FROM trash_batches WHERE id = v_batch.id) IS NOT NULL THEN
          v_made := v_made + 1;
        END IF;
      EXCEPTION
        WHEN SQLSTATE 'FV001' OR SQLSTATE 'FV002' THEN
          RAISE;
        WHEN OTHERS THEN
          RAISE WARNING 'purge of trash batch % failed with SQLSTATE %', v_batch.id, SQLSTATE;
      END;
    END LOOP;
    RETURN v_made;
  END
  $$;


ALTER FUNCTION fvoci.app_system_purge_expired(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_purge_expired_ids() RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE
      2 * (SELECT count(*) FROM workspaces
       WHERE deleted_at IS NOT NULL AND deleted_at + make_interval(days => trash_retention_days) <= now())::integer
      + (SELECT count(*) FROM trash_batches b
         WHERE b.restored_at IS NULL AND b.purged_at IS NULL AND app_trash_expires_at(b) <= now())::integer
    END
  $$;


ALTER FUNCTION fvoci.app_system_purge_expired_ids() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_purge_old_versions() RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_row   record;
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_row IN
      SELECT v.id, v.file_id, v.version
      FROM file_versions v
      JOIN files f ON f.id = v.file_id
      JOIN workspaces w ON w.id = f.workspace_id
      WHERE w.version_retention_days IS NOT NULL
        AND v.version <> f.current_version
        AND (SELECT min(n.created_at) FROM file_versions n WHERE n.file_id = v.file_id AND n.version > v.version)
            < now() - make_interval(days => w.version_retention_days)
        AND NOT app_version_referenced(v.file_id, v.version, v.id)
      ORDER BY v.created_at
      LIMIT 500
    LOOP
      PERFORM 1 FROM file_versions WHERE id = v_row.id FOR UPDATE SKIP LOCKED;
      CONTINUE WHEN NOT FOUND OR app_version_referenced(v_row.file_id, v_row.version, v_row.id);
      UPDATE uploads SET version_id = NULL WHERE version_id = v_row.id;
      DELETE FROM file_versions WHERE id = v_row.id;
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_purge_old_versions() OWNER TO fvoci_migrator;

-- 반복 회차 만들기(D-31, #95). 작업 프로세스는 계열마다 트랜잭션 하나로 app_system_sync_task_series를 부른다. 계열 하나의
-- 실패가 다른 계열의 커밋을 막지 않고, 처리한 계열의 잠금을 작업이 끝날 때까지 쥐지 않는다. 사용자 요청의 동기화
-- (app_sync_item_series)는 그대로 둔다.
CREATE FUNCTION fvoci.app_system_recurrence_due() RETURNS SETOF uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT s.id FROM task_series s
    JOIN tasks r ON r.id = s.root_task_id
    JOIN items i ON i.id = s.item_id
    JOIN modules m ON m.id = i.module_id
    JOIN workspaces w ON w.id = s.workspace_id
    WHERE app_user_id() IS NULL AND s.rule IS NOT NULL AND r.deleted_at IS NULL AND i.deleted_at IS NULL
      AND m.deleted_at IS NULL AND w.deleted_at IS NULL
    ORDER BY s.item_id, s.id
  $$;


ALTER FUNCTION fvoci.app_system_recurrence_due() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_sync_task_series(p_series uuid, p_ids uuid[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    RETURN app_sync_task_series_as(p_series, true, p_ids);
  END
  $$;


ALTER FUNCTION fvoci.app_system_sync_task_series(p_series uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_sync_task_series_ids(p_series uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE WHEN app_user_id() IS NOT NULL THEN 0 ELSE app_task_series_bound(p_series) END
  $$;


ALTER FUNCTION fvoci.app_system_sync_task_series_ids(p_series uuid) OWNER TO fvoci_migrator;

-- 반복은 항목·ID 순으로 처리하고 항목의 세 열은 done → in_progress → todo 순서로 먼저 잠근다.
-- 사용자 이동·항목 동기화와 순서를 맞추며 잠금은 트랜잭션 끝까지 유지한다.
-- 실패한 항목·반복의 ID 몫도 소비해 다음 반복의 몫을 지킨다. 항목별 부분 커밋은 하지 않는다(P-35 보류).
CREATE FUNCTION fvoci.app_system_search_claim(p_limit integer) RETURNS TABLE(kind text, target_id uuid, queued_at timestamp with time zone, attempts integer)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes', attempts = q.attempts + 1
    WHERE (q.kind, q.target_id) IN (
      SELECT s.kind, s.target_id FROM search_queue s
      WHERE app_user_id() IS NULL
        AND (s.claimed_until IS NULL OR s.claimed_until < now())
        AND s.attempts < 10
        -- 보낸 작업을 기다리는 행은 그 작업이 끝날 때까지 꺼내지 않는다.
        AND s.task_uid IS NULL
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING q.kind, q.target_id, q.queued_at, q.attempts
  $$;


ALTER FUNCTION fvoci.app_system_search_claim(p_limit integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[]) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    DELETE FROM search_queue q
    USING unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE search_queue q SET claimed_until = NULL
    FROM unnest(p_kinds, p_ids) AS d(kind, id)
    WHERE q.kind = d.kind AND q.target_id = d.id;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_enqueue_all() RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    INSERT INTO search_queue (kind, target_id)
    SELECT 'document', id FROM documents WHERE deleted_at IS NULL
    UNION ALL SELECT 'task', id FROM tasks WHERE deleted_at IS NULL
    UNION ALL SELECT 'item', id FROM items WHERE deleted_at IS NULL
    UNION ALL SELECT 'event', id FROM events WHERE deleted_at IS NULL
    UNION ALL SELECT 'file', id FROM files WHERE deleted_at IS NULL
    UNION ALL SELECT 'record', id FROM item_records
    ON CONFLICT (kind, target_id) DO UPDATE SET queued_at = now(), attempts = 0;
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_enqueue_all() OWNER TO fvoci_migrator;

-- 검색 색인과 대체 SQL의 작성자·날짜 의미를 맞춘다. D-101·D-102: docs/decisions/notify-audit.md#d-101.
-- 시각은 사용자 시간대의 하루 경계로, 날짜 전용 값은 날짜 그대로 비교한다. 최종 접근 권한은 서버가 다시 검사한다.
CREATE FUNCTION fvoci.app_system_search_payload(p_kinds text[], p_ids uuid[]) RETURNS TABLE(kind text, id uuid, live boolean, workspace_id uuid, owner_user_id uuid, item_id uuid, title text, reference text, body jsonb, description text, created_by uuid, module_id uuid, status text, status_category text, assignee_ids uuid[], at timestamp with time zone, on_day date, file_key text, file_type text, file_size bigint)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH wanted AS (SELECT * FROM unnest(p_kinds, p_ids) AS w(kind, id)),
    rows AS (
      SELECT w.kind, w.id,
        CASE w.kind
          WHEN 'document' THEN d.id IS NOT NULL AND d.deleted_at IS NULL
            AND (d.item_id IS NULL OR EXISTS (SELECT 1 FROM items i WHERE i.id = d.item_id AND i.deleted_at IS NULL))
          WHEN 'task' THEN t.id IS NOT NULL AND t.deleted_at IS NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = t.item_id AND i.deleted_at IS NULL)
          WHEN 'item' THEN it.id IS NOT NULL AND it.deleted_at IS NULL
          WHEN 'event' THEN e.id IS NOT NULL AND e.deleted_at IS NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = e.item_id AND i.deleted_at IS NULL)
          WHEN 'file' THEN fl.id IS NOT NULL AND fl.deleted_at IS NULL
            AND (fl.document_id IS NULL OR EXISTS (SELECT 1 FROM documents fd WHERE fd.id = fl.document_id AND fd.deleted_at IS NULL))
            AND NOT EXISTS (SELECT 1 FROM items i WHERE i.id = coalesce(fl.item_id, (SELECT item_id FROM documents WHERE id = fl.document_id)) AND i.deleted_at IS NOT NULL)
          WHEN 'record' THEN rc.id IS NOT NULL
            AND EXISTS (SELECT 1 FROM items i WHERE i.id = rc.item_id AND i.deleted_at IS NULL)
        END AS live,
        coalesce(d.workspace_id, t.workspace_id, it.workspace_id, e.workspace_id, fl.workspace_id, rc.workspace_id)
          AS workspace_id,
        coalesce(d.owner_user_id, fl.owner_user_id) AS owner_user_id,
        coalesce(d.item_id, t.item_id, e.item_id, fl.item_id, rc.item_id,
                 (SELECT item_id FROM documents WHERE id = fl.document_id))
          AS item_id,
        coalesce(d.title, t.title, it.title, e.title, fl.name, rc.title) AS title,
        CASE w.kind
          WHEN 'task' THEN app_task_reference(t.id)
          WHEN 'item' THEN app_item_reference(it.id)
        END AS reference,
        d.body,
        coalesce(t.description, it.description, e.description,
          rc.body || ' ' || coalesce((SELECT string_agg(f.value, ' ') FROM jsonb_each_text(rc.fields) f), ''))
          AS description,
        coalesce(d.created_by, t.created_by, it.created_by, e.created_by, fl.created_by, rc.created_by) AS created_by,
        it.module_id AS item_module,
        (SELECT s.name FROM task_statuses s WHERE s.id = t.status_id) AS status,
        (SELECT s.category FROM task_statuses s WHERE s.id = t.status_id) AS status_category,
        CASE WHEN w.kind = 'task' THEN ARRAY(SELECT a.user_id FROM task_assignees a WHERE a.task_id = t.id ORDER BY 1) END
          AS assignee_ids,
        -- 검색 API의 대체 경로(`search_api.rs` `fallback`)와 같은 규칙.
        CASE w.kind
          WHEN 'task' THEN NULL
          WHEN 'event' THEN e.starts_at
          WHEN 'record' THEN CASE WHEN rc.occurred_on IS NULL THEN rc.updated_at END
          ELSE coalesce(d.updated_at, it.updated_at, fl.updated_at)
        END AS at,
        CASE w.kind
          WHEN 'task' THEN coalesce(t.due_date, t.start_date)
          WHEN 'event' THEN e.start_date
          WHEN 'record' THEN rc.occurred_on
        END AS on_day,
        v.storage_key AS file_key, v.content_type AS file_type, v.size AS file_size
      FROM wanted w
      LEFT JOIN documents d ON w.kind = 'document' AND d.id = w.id
      LEFT JOIN tasks t ON w.kind = 'task' AND t.id = w.id
      LEFT JOIN items it ON w.kind = 'item' AND it.id = w.id
      LEFT JOIN events e ON w.kind = 'event' AND e.id = w.id
      LEFT JOIN files fl ON w.kind = 'file' AND fl.id = w.id
      LEFT JOIN item_records rc ON w.kind = 'record' AND rc.id = w.id
      LEFT JOIN file_versions v ON v.file_id = fl.id AND v.version = fl.current_version
      WHERE app_user_id() IS NULL
    )
    SELECT r.kind, r.id, r.live, r.workspace_id, r.owner_user_id, r.item_id, r.title, r.reference, r.body,
           r.description, r.created_by,
           coalesce(r.item_module, (SELECT module_id FROM items WHERE id = r.item_id)),
           r.status, r.status_category, r.assignee_ids, r.at, r.on_day, r.file_key, r.file_type, r.file_size
    FROM rows r
  $$;


ALTER FUNCTION fvoci.app_system_search_payload(p_kinds text[], p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_pending(p_limit integer) RETURNS TABLE(task_uid bigint)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH picked AS (
      UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
      WHERE (q.kind, q.target_id) IN (
        SELECT s.kind, s.target_id FROM search_queue s
        WHERE app_user_id() IS NULL AND s.task_uid IS NOT NULL
          AND (s.claimed_until IS NULL OR s.claimed_until < now())
        ORDER BY s.queued_at
        LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
      RETURNING q.task_uid)
    SELECT DISTINCT picked.task_uid FROM picked
  $$;


ALTER FUNCTION fvoci.app_system_search_pending(p_limit integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) RETURNS TABLE(kind text, target_id uuid)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE search_queue q SET claimed_until = now() + interval '2 minutes'
    FROM unnest(p_kinds, p_ids, p_queued, p_attempts) AS d(kind, id, queued_at, attempts)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.queued_at = d.queued_at AND q.attempts = d.attempts
      AND q.task_uid IS NULL
    RETURNING q.kind, q.target_id
  $$;


ALTER FUNCTION fvoci.app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE search_queue q SET claimed_until = NULL
    FROM unnest(p_kinds, p_ids, p_queued, p_attempts) AS d(kind, id, queued_at, attempts)
    WHERE app_user_id() IS NULL
      AND q.kind = d.kind AND q.target_id = d.id AND q.task_uid IS NULL
      AND ((q.queued_at = d.queued_at AND q.attempts = d.attempts) OR q.attempts = 0)
  $$;


ALTER FUNCTION fvoci.app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_settled(p_task bigint, p_outcome text) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    IF p_outcome NOT IN ('succeeded', 'failed', 'pending') THEN
      RAISE EXCEPTION 'unknown search task outcome' USING ERRCODE = '22023';
    END IF;
    IF p_outcome = 'pending' THEN
      UPDATE search_queue SET claimed_until = NULL WHERE task_uid = p_task;
      RETURN 0;
    END IF;
    IF p_outcome = 'succeeded' THEN
      DELETE FROM search_queue WHERE task_uid = p_task AND queued_at = task_queued_at;
      GET DIAGNOSTICS v_count = ROW_COUNT;
    END IF;
    UPDATE search_queue SET task_uid = NULL, task_queued_at = NULL, claimed_until = NULL WHERE task_uid = p_task;
    RETURN v_count;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_settled(p_task bigint, p_outcome text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_task bigint) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN;
    END IF;
    UPDATE search_queue q SET task_uid = p_task, task_queued_at = d.queued_at, claimed_until = NULL
    FROM unnest(p_kinds, p_ids, p_queued) AS d(kind, id, queued_at)
    WHERE q.kind = d.kind AND q.target_id = d.id;
  END
  $$;


ALTER FUNCTION fvoci.app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_task bigint) OWNER TO fvoci_migrator;

-- MATERIALIZED로 선택을 한 번만 실행한다. DELETE 중 하위 질의를 다시 실행해 p_limit보다 많이 꺼내지 않게 한다.
CREATE FUNCTION fvoci.app_system_storage_claim(p_limit integer) RETURNS TABLE(storage_key text, storage_upload_id text, attempts integer)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    WITH picked AS MATERIALIZED (
      SELECT s.storage_key, s.storage_upload_id FROM storage_deletions s
      WHERE app_user_id() IS NULL AND s.attempts < 10 AND s.queued_at <= now()
      ORDER BY s.queued_at
      LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    DELETE FROM storage_deletions d USING picked p
    WHERE d.storage_key = p.storage_key AND d.storage_upload_id = p.storage_upload_id
    RETURNING d.storage_key, d.storage_upload_id, d.attempts
  $$;


ALTER FUNCTION fvoci.app_system_storage_claim(p_limit integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_system_storage_retry(p_key text, p_upload text, p_attempts integer) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO storage_deletions (storage_key, storage_upload_id, attempts, queued_at)
    SELECT p_key, p_upload, p_attempts + 1, now() + make_interval(mins => p_attempts + 1)
    WHERE app_user_id() IS NULL
    ON CONFLICT DO NOTHING
  $$;


ALTER FUNCTION fvoci.app_system_storage_retry(p_key text, p_upload text, p_attempts integer) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_take_login_state(p_secret_hash bytea, p_state text) RETURNS TABLE(pkce_verifier text, nonce text, return_to text)
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    DELETE FROM oidc_login_states
    WHERE secret_hash = p_secret_hash AND state = p_state AND expires_at > now()
    RETURNING pkce_verifier, nonce, return_to
  $$;


ALTER FUNCTION fvoci.app_take_login_state(p_secret_hash bytea, p_state text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_checklist_counts(p_task uuid) RETURNS TABLE(done bigint, total bigint)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT count(*) FILTER (WHERE checked), count(*) FROM checklist_items
    WHERE task_id = p_task AND app_access('task', p_task) <> 'none'
  $$;


ALTER FUNCTION fvoci.app_task_checklist_counts(p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_lane_end(p_status uuid) RETURNS numeric
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce(max(position), 0) + 1 FROM tasks WHERE status_id = p_status
  $$;


ALTER FUNCTION fvoci.app_task_lane_end(p_status uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_reference(p_task uuid) RETURNS text
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT k.key || '-' || n.number
    FROM numbered_targets n
    JOIN module_keys k ON k.module_id = n.scope_id AND k.retired_at IS NULL
    WHERE n.target_kind = 'task' AND n.target_id = p_task
  $$;


ALTER FUNCTION fvoci.app_task_reference(p_task uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_task_series_bound(p_series uuid) RETURNS integer
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_series  task_series%ROWTYPE;
    v_today   date;
    v_missing integer;
    v_lines   integer;
  BEGIN
    SELECT * INTO v_series FROM task_series WHERE id = p_series;
    IF v_series.id IS NULL OR v_series.rule IS NULL THEN
      RETURN 0;
    END IF;
    v_today := (now() AT TIME ZONE v_series.timezone)::date;
    SELECT count(*) INTO v_missing FROM (
      WITH wanted AS (
        SELECT occurrence FROM app_recurrence_dates(v_series.rule, v_series.anchor, v_series.effective_from, v_today)
        UNION
        (SELECT occurrence
         FROM app_recurrence_dates(v_series.rule, v_series.anchor, greatest(v_today + 1, v_series.effective_from),
                                   greatest(v_today + 1, v_series.effective_from) + 3660)
         ORDER BY occurrence LIMIT 1)
      )
      SELECT 1 FROM wanted w
      WHERE NOT EXISTS (SELECT 1 FROM tasks t WHERE t.series_id = p_series AND t.occurrence_date = w.occurrence)
        AND NOT (w.occurrence = ANY (v_series.skipped_dates))
      LIMIT 50) x;
    SELECT count(*) INTO v_lines FROM checklist_items WHERE task_id = v_series.root_task_id;
    RETURN v_missing * (4 + 2 * v_lines);
  END
  $$;


ALTER FUNCTION fvoci.app_task_series_bound(p_series uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_tasks_move_version() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF NEW.status_id IS DISTINCT FROM OLD.status_id OR NEW.position IS DISTINCT FROM OLD.position THEN
      NEW.move_version := OLD.move_version + 1;
    ELSE
      NEW.move_version := OLD.move_version;
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.app_tasks_move_version() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_time_target_label(p_kind text, p_id uuid) RETURNS text
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT CASE
      WHEN p_kind = 'task' AND app_access('task', p_id) <> 'none' THEN
        (SELECT concat_ws(' ', app_task_reference(id), title) FROM tasks WHERE id = p_id)
      WHEN p_kind = 'document' AND app_access('document', p_id) <> 'none' THEN
        (SELECT concat_ws(' ', app_document_reference(id), title) FROM documents WHERE id = p_id)
      WHEN p_kind = 'item' AND app_access('item', p_id) <> 'none' THEN
        (SELECT concat_ws(' ', app_item_reference(id), title) FROM items WHERE id = p_id)
      WHEN p_kind = 'episode'
           AND app_access('item', (SELECT item_id FROM course_episodes WHERE id = p_id)) <> 'none' THEN
        (SELECT concat_ws(' ', app_item_reference(e.item_id), e.number || '회', e.title)
         FROM course_episodes e WHERE e.id = p_id)
      WHEN p_kind = 'study_unit'
           AND app_access('item', (SELECT item_id FROM study_units WHERE id = p_id)) <> 'none' THEN
        (SELECT concat_ws(' ', app_item_reference(s.item_id), s.number || '.', s.title)
         FROM study_units s WHERE s.id = p_id)
    END
  $$;


ALTER FUNCTION fvoci.app_time_target_label(p_kind text, p_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_time_target_workspace(p_kind text, p_id uuid) RETURNS uuid
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_kind IS NULL THEN
      RETURN NULL;
    END IF;
    IF p_kind = 'task' AND app_access('task', p_id) <> 'none' THEN
      RETURN (SELECT workspace_id FROM tasks WHERE id = p_id);
    ELSIF p_kind = 'document' AND app_access('document', p_id) <> 'none' THEN
      RETURN (SELECT workspace_id FROM documents WHERE id = p_id);
    ELSIF p_kind = 'item' AND app_access('item', p_id) <> 'none' THEN
      RETURN (SELECT workspace_id FROM items WHERE id = p_id);
    ELSIF p_kind = 'episode'
          AND app_access('item', (SELECT item_id FROM course_episodes WHERE id = p_id)) <> 'none' THEN
      RETURN (SELECT workspace_id FROM course_episodes WHERE id = p_id);
    ELSIF p_kind = 'study_unit'
          AND app_access('item', (SELECT item_id FROM study_units WHERE id = p_id)) <> 'none' THEN
      RETURN (SELECT workspace_id FROM study_units WHERE id = p_id);
    END IF;
    RAISE EXCEPTION 'target not found' USING ERRCODE = 'P0002';
  END
  $$;


ALTER FUNCTION fvoci.app_time_target_workspace(p_kind text, p_id uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_timer_action(p_entry uuid, p_action text) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
  BEGIN
    IF v_entry.ended_at IS NOT NULL THEN
      IF p_action = 'stop' THEN
        RETURN;
      END IF;
      RAISE EXCEPTION 'the timer has stopped' USING ERRCODE = '55000';
    END IF;
    IF p_action = 'pause' THEN
      IF v_entry.paused_at IS NULL THEN
        UPDATE time_entries SET paused_at = now(), version = version + 1, updated_at = now() WHERE id = p_entry;
      END IF;
    ELSIF p_action = 'resume' THEN
      IF v_entry.paused_at IS NOT NULL THEN
        UPDATE time_entries SET
          paused_seconds = paused_seconds + floor(extract(epoch FROM now() - paused_at))::integer,
          paused_at = NULL, version = version + 1, updated_at = now()
        WHERE id = p_entry;
      END IF;
    ELSIF p_action = 'stop' THEN
      UPDATE time_entries SET
        paused_seconds = paused_seconds
          + CASE WHEN paused_at IS NULL THEN 0 ELSE floor(extract(epoch FROM now() - paused_at))::integer END,
        paused_at = NULL, ended_at = now(), version = version + 1, updated_at = now()
      WHERE id = p_entry;
    ELSE
      RAISE EXCEPTION 'unknown timer action' USING ERRCODE = '22023';
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_timer_action(p_entry uuid, p_action text) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_document(p_doc uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_doc    documents%ROWTYPE;
    v_seen   documents%ROWTYPE;
    v_access text := app_access('document', p_doc);
    v_batch  uuid;
    v_count  integer;
    v_tree   uuid[];
  BEGIN
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc;
    IF NOT (v_access = 'owner' OR (v_access = 'write' AND v_doc.created_by = v_user)) THEN
      IF v_doc.item_id IS NOT NULL AND app_item_archived(v_doc.item_id)
         AND app_access('document', p_doc) <> 'none' THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      RAISE EXCEPTION 'only the owner or the author can delete this document' USING ERRCODE = '42501';
    END IF;
    -- 트리 잠금 → 문서 행 → 항목 공유(문서 잠금 순서).
    PERFORM app_lock_tree(CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END,
                          v_doc.item_id, v_doc.owner_user_id);
    -- 잠근 뒤 자리를 다시 본다. 기다리는 사이 다른 워크스페이스·항목·개인 노트로 옮겨졌으면 옛 자리 기준으로
    -- 묶음을 만들지 않고 다시 하게 한다(자리 바꾸기와 같은 규칙). 권한·작성자 규칙도 잠근 뒤의 값으로 다시 본다.
    v_seen := v_doc;
    SELECT * INTO v_doc FROM documents WHERE id = p_doc FOR UPDATE;
    IF v_doc.workspace_id IS DISTINCT FROM v_seen.workspace_id
       OR v_doc.item_id IS DISTINCT FROM v_seen.item_id
       OR v_doc.owner_user_id IS DISTINCT FROM v_seen.owner_user_id THEN
      RAISE EXCEPTION 'the document was moved; reload and try again' USING ERRCODE = '55000';
    END IF;
    PERFORM app_lock_item(v_doc.item_id);
    v_access := app_access('document', p_doc);
    IF v_access = 'none' THEN
      RAISE EXCEPTION 'document not found' USING ERRCODE = 'P0002';
    END IF;
    IF NOT (v_access = 'owner' OR (v_access = 'write' AND v_doc.created_by = v_user)) THEN
      RAISE EXCEPTION 'only the owner or the author can delete this document' USING ERRCODE = '42501';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), CASE WHEN v_doc.owner_user_id IS NULL THEN v_doc.workspace_id END, v_doc.owner_user_id,
            'document', p_doc, v_doc.title, 0, v_user)
    RETURNING id INTO v_batch;
    -- 지우는 사람이 지울 수 있는 하위 문서만 함께 지운다. 볼 수 없거나 지울 수 없는 하위 문서는 이 문서의
    -- 부모 아래로 옮겨 남긴다(남의 문서를 몰래 휴지통에 넣지 않게).
    v_tree := ARRAY(
    WITH RECURSIVE tree(id) AS (
      SELECT p_doc
      UNION
      SELECT d.id FROM documents d JOIN tree t ON d.parent_id = t.id
      WHERE d.deleted_at IS NULL
        AND (app_access('document', d.id) = 'owner'
             OR (app_access('document', d.id) = 'write' AND d.created_by = v_user))
    )
    SELECT id FROM tree);
    UPDATE documents SET parent_id = v_doc.parent_id
    WHERE parent_id = ANY (v_tree) AND NOT (id = ANY (v_tree)) AND deleted_at IS NULL;
    UPDATE documents SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_tree);
    GET DIAGNOSTICS v_count = ROW_COUNT;
    UPDATE trash_batches SET member_count = v_count WHERE id = v_batch;
    PERFORM app_audit_document(app_id(p_ids, 2), p_doc, 'delete', jsonb_build_object('batch', v_batch, 'count', v_count));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_document(p_doc uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_expires_at(p_batch fvoci.trash_batches) RETURNS timestamp with time zone
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT p_batch.deleted_at + make_interval(days => coalesce(
      (SELECT trash_retention_days FROM workspaces WHERE id = p_batch.workspace_id), 30))
  $$;


ALTER FUNCTION fvoci.app_trash_expires_at(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_file(p_file uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_file   files%ROWTYPE;
    v_batch  uuid;
  BEGIN
    PERFORM app_require_file_write(p_file);
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF app_file_archived(p_file) THEN
      RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
    END IF;
    IF v_file.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_file.workspace_id, v_file.owner_user_id, 'file', p_file, v_file.name, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE files SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_file;
    INSERT INTO audit_log (id, workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (app_id(p_ids, 2), v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete', 'file', p_file,
            jsonb_build_object('name', v_file.name, 'batch', v_batch));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_file(p_file uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_item(p_item uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item  items%ROWTYPE;
    v_batch uuid;
  BEGIN
    IF app_access('item', p_item) = 'none' THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_access('item', p_item) <> 'owner' THEN
      RAISE EXCEPTION 'only the item owner can delete it' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_item FROM items WHERE id = p_item FOR UPDATE;
    IF v_item.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_item.workspace_id, 'item', p_item, v_item.title, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE items SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_item;
    PERFORM app_audit(app_id(p_ids, 2), v_item.workspace_id, 'delete', 'item', p_item,
                      jsonb_build_object('batch', v_batch));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_item(p_item uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_kept_for_references(p_ids uuid[]) RETURNS SETOF uuid
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT b.id FROM trash_batches b
    WHERE b.id = ANY (p_ids) AND b.purged_at IS NULL AND b.restored_at IS NULL
      AND app_trash_role(b) IS NOT NULL AND app_batch_keeps_referenced(b)
  $$;


ALTER FUNCTION fvoci.app_trash_kept_for_references(p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_list(p_workspace uuid) RETURNS TABLE(id uuid, root_kind text, root_id uuid, title text, member_count integer, deleted_by uuid, deleted_by_name text, deleted_at timestamp with time zone, expires_at timestamp with time zone, can_purge boolean)
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT b.id, b.root_kind, b.root_id, b.title, b.member_count, b.deleted_by, u.display_name, b.deleted_at,
           app_trash_expires_at(b), app_trash_role(b) = 'admin'
    FROM trash_batches b LEFT JOIN users u ON u.id = b.deleted_by
    WHERE b.restored_at IS NULL AND b.purged_at IS NULL
      AND ((p_workspace IS NULL AND b.owner_user_id = app_user_id()) OR b.workspace_id = p_workspace)
      AND app_trash_role(b) IS NOT NULL
    ORDER BY b.deleted_at DESC, b.id
  $$;


ALTER FUNCTION fvoci.app_trash_list(p_workspace uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_module(p_module uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_module modules%ROWTYPE;
    v_batch  uuid;
    v_count  integer;
  BEGIN
    SELECT * INTO v_module FROM modules WHERE id = p_module FOR UPDATE;
    IF v_module.id IS NULL OR v_module.deleted_at IS NOT NULL OR app_access('workspace', v_module.workspace_id) = 'none' THEN
      RAISE EXCEPTION 'module not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_module.workspace_id);
    -- 휴지통에 없는 항목 수(목록 표시용). 따로 지운 항목은 자기 묶음에 있다.
    SELECT count(*) INTO v_count FROM items WHERE module_id = p_module AND deleted_at IS NULL;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_module.workspace_id, 'module', p_module, v_module.name, v_count, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE modules SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_module;
    PERFORM app_audit(app_id(p_ids, 2), v_module.workspace_id, 'delete', 'module', p_module,
                      jsonb_build_object('batch', v_batch, 'items', v_count));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_module(p_module uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_role(p_batch fvoci.trash_batches) RETURNS text
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user uuid := app_user_id();
    v_role text;
  BEGIN
    IF v_user IS NULL OR p_batch.restored_at IS NOT NULL OR p_batch.purged_at IS NOT NULL THEN
      RETURN NULL;
    END IF;
    IF p_batch.owner_user_id IS NOT NULL THEN
      RETURN CASE WHEN p_batch.owner_user_id = v_user THEN 'admin' END;
    END IF;
    v_role := app_access('workspace', p_batch.workspace_id);
    IF v_role = 'owner' THEN
      RETURN 'admin';
    END IF;
    IF v_role = 'write' AND p_batch.deleted_by = v_user THEN
      RETURN 'deleter';
    END IF;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_role(p_batch fvoci.trash_batches) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_trash_task(p_task uuid, p_ids uuid[]) RETURNS uuid
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task  tasks%ROWTYPE;
    v_batch uuid;
    v_ids   uuid[];
  BEGIN
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(id) INTO v_ids FROM tasks WHERE (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    PERFORM app_lock_task_rows(v_ids, NULL);
    -- 잠그는 사이 새로 생긴 하위 업무가 있으면 다시 잠그지 않는다(이미 쥔 열 뒤에 더 낮은 열을 잡으면 순서가 뒤집혀 교착할
    -- 수 있다). 드문 경우이므로 다시 하게 한다(409).
    IF EXISTS (SELECT 1 FROM tasks WHERE parent_id = p_task AND deleted_at IS NULL AND NOT (id = ANY (v_ids))) THEN
      RAISE EXCEPTION 'subtasks changed while deleting; try again' USING ERRCODE = '55000';
    END IF;
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    -- 잠근 뒤 다시 고른다. 기다리는 사이 다른 지우기가 끝낸 하위 업무는 이미 자기 묶음에 있고, 다른 트랜잭션이 떼어 낸
    -- 하위 업무(최상위로 올리거나 다른 상위로 옮김)는 더는 이 업무의 하위가 아니다.
    SELECT array_agg(id) INTO v_ids FROM tasks
    WHERE id = ANY (v_ids) AND (id = p_task OR parent_id = p_task) AND deleted_at IS NULL;
    INSERT INTO trash_batches (id, workspace_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (app_id(p_ids, 1), v_task.workspace_id, 'task', p_task, v_task.title, cardinality(v_ids), app_user_id())
    RETURNING id INTO v_batch;
    UPDATE tasks SET deleted_at = now(), trash_batch_id = v_batch WHERE id = ANY (v_ids);
    PERFORM app_audit(app_id(p_ids, 2), v_task.workspace_id, 'delete', 'task', p_task,
                      jsonb_build_object('batch', v_batch, 'count', cardinality(v_ids)));
    RETURN v_batch;
  END
  $$;


ALTER FUNCTION fvoci.app_trash_task(p_task uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_tree_siblings(p_workspace uuid, p_item uuid, p_owner uuid, p_parent uuid) RETURNS SETOF fvoci.documents
    LANGUAGE plpgsql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF p_parent IS NOT NULL THEN
      RETURN QUERY SELECT * FROM documents WHERE parent_id = p_parent;
    ELSIF p_owner IS NOT NULL THEN
      RETURN QUERY SELECT * FROM documents WHERE owner_user_id = p_owner AND parent_id IS NULL;
    ELSIF p_item IS NOT NULL THEN
      RETURN QUERY SELECT * FROM documents WHERE item_id = p_item AND parent_id IS NULL;
    ELSE
      RETURN QUERY SELECT * FROM documents
      WHERE workspace_id = p_workspace AND item_id IS NULL AND owner_user_id IS NULL AND parent_id IS NULL;
    END IF;
  END
  $$;


ALTER FUNCTION fvoci.app_tree_siblings(p_workspace uuid, p_item uuid, p_owner uuid, p_parent uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT (SELECT count(*) FROM task_assignees a JOIN tasks t ON t.id = a.task_id
            WHERE (p_item IS NULL OR t.item_id = p_item)
              AND (p_workspace IS NULL OR t.workspace_id = p_workspace)
              AND (p_user IS NULL OR a.user_id = p_user))::integer
         + (SELECT count(*) FROM items i
            WHERE i.leader_id IS NOT NULL
              AND i.workspace_id = coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item))
              AND (p_item IS NULL OR i.id = p_item)
              AND (p_user IS NULL OR i.leader_id = p_user))::integer
  $$;


ALTER FUNCTION fvoci.app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) RETURNS void
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_task uuid;
  BEGIN
    SELECT task_id INTO v_task FROM checklist_items WHERE id = p_id;
    IF v_task IS NULL OR app_access('task', v_task) = 'none' THEN
      RAISE EXCEPTION 'checklist item not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_task_write(v_task);
    PERFORM app_lock_item((SELECT item_id FROM tasks WHERE id = v_task));
    PERFORM app_require_task_write(v_task);
    UPDATE checklist_items SET text = coalesce(btrim(p_text), text), checked = coalesce(p_checked, checked),
      updated_at = now()
    WHERE id = p_id;
  END
  $$;


ALTER FUNCTION fvoci.app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_episode(p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text, p_materials text, p_planned date) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM course_episodes WHERE id = p_episode);
    v_version integer;
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'episode not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_course_write(v_item);
    UPDATE course_episodes SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      duration_seconds = p_duration, url = btrim(coalesce(p_url, '')), materials = coalesce(p_materials, ''),
      planned_on = p_planned, version = version + 1, updated_at = now()
    WHERE id = p_episode AND version = p_version
    RETURNING version INTO v_version;
    IF v_version IS NULL THEN
      RAISE EXCEPTION 'the episode changed; reload it' USING ERRCODE = '55000';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid episode: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_episode(p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text, p_materials text, p_planned date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
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


ALTER FUNCTION fvoci.app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_update_record(p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_rec item_records%ROWTYPE;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    IF v_rec.id IS NULL OR app_access('item', v_rec.item_id) = 'none' THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.version <> p_version THEN
      RAISE EXCEPTION 'the record changed; reload it' USING ERRCODE = '55000';
    END IF;
    UPDATE item_records SET title = btrim(p_title), occurred_on = p_occurred, body = coalesce(p_body, ''),
                            fields = coalesce(p_fields, '{}'), updated_by = app_user_id(), updated_at = now(),
                            version = version + 1
    WHERE id = p_record;
    PERFORM app_set_record_files(p_record, v_rec.item_id, p_files);
    RETURN v_rec.version + 1;
  EXCEPTION
    WHEN check_violation THEN
      RAISE EXCEPTION 'invalid record: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_record(p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_study_unit(p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date) RETURNS integer
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_item uuid := (SELECT item_id FROM study_units WHERE id = p_unit);
    v_version integer;
  BEGIN
    IF v_item IS NULL OR app_access('item', v_item) = 'none' THEN
      RAISE EXCEPTION 'unit not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_study_write(v_item);
    UPDATE study_units SET number = p_number, title = btrim(p_title), chapter = btrim(coalesce(p_chapter, '')),
      unit = p_kind, amount = p_amount, planned_on = p_planned, version = version + 1, updated_at = now()
    WHERE id = p_unit AND version = p_version
    RETURNING version INTO v_version;
    IF v_version IS NULL THEN
      RAISE EXCEPTION 'the unit changed; reload it' USING ERRCODE = '55000';
    END IF;
    RETURN v_version;
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid unit: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_study_unit(p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_user   uuid := app_user_id();
    v_task   tasks%ROWTYPE;
    v_other  tasks%ROWTYPE;
    v_uid    uuid;
    v_shift  jsonb;
    v_ids    uuid[];
    v_start  date;
    v_due    date;
    v_new    uuid[];
    v_lock   uuid[];
    v_at     integer := 1;
    v_state  text;
    v_stale  boolean := false;
    v_status task_statuses%ROWTYPE;
    v_old    task_statuses%ROWTYPE;
  BEGIN
    IF p_shifts IS NULL OR jsonb_typeof(p_shifts) <> 'array' THEN
      RAISE EXCEPTION 'shifts must be an array' USING ERRCODE = '22023';
    END IF;
    PERFORM app_require_task_write(p_task);
    SELECT array_agg(DISTINCT (s ->> 'task_id')::uuid) INTO v_ids FROM jsonb_array_elements(p_shifts) s;
    IF coalesce(cardinality(v_ids), 0) <> jsonb_array_length(p_shifts) THEN
      RAISE EXCEPTION 'each shifted task must appear once' USING ERRCODE = '22023';
    END IF;
    IF p_task = ANY (coalesce(v_ids, '{}')) THEN
      RAISE EXCEPTION 'the edited task cannot also be a shifted successor' USING ERRCODE = '22023';
    END IF;
    -- 상태는 항목의 상태 정의 이름이다(D-116).
    v_status := app_task_status_by_name((SELECT item_id FROM tasks WHERE id = p_task), p_status);
    -- 담당자 후보의 멤버 행을 id 순으로 먼저 공유 잠금한다(멤버 행 → 열 → 항목 행: 내보내기·역할 변경과 같은 순서).
    PERFORM 1 FROM workspace_members
    WHERE workspace_id = (SELECT workspace_id FROM tasks WHERE id = p_task) AND user_id = ANY (coalesce(p_assignees, '{}'))
    ORDER BY user_id FOR SHARE;
    -- 이 업무와 옮길 후속 업무(같은 항목만, 다른 항목 id는 잠그지 않고 아래에서 거부)를 열 → 행 순으로 잠근다.
    SELECT array_agg(id) INTO v_lock FROM tasks
    WHERE id = ANY (coalesce(v_ids, '{}')) AND item_id = (SELECT item_id FROM tasks WHERE id = p_task);
    PERFORM app_lock_task_rows(array_prepend(p_task, coalesce(v_lock, '{}')), v_status.id);
    SELECT * INTO v_task FROM tasks WHERE id = p_task;
    PERFORM app_lock_item(v_task.item_id);
    PERFORM app_require_task_write(p_task);
    IF jsonb_array_length(p_shifts) > 0 THEN
      -- 후속 여부 확인과 기록 사이에 선행 관계가 바뀌지 않게(항목 공유 → 관계 잠금 순서).
      PERFORM app_lock_dependencies(v_task.item_id);
    END IF;
    IF v_task.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    -- 열 잠금 뒤 상태 정의를 다시 읽는다(기다리는 사이 이름·범주가 바뀌었거나 지워졌을 수 있다).
    SELECT * INTO v_status FROM task_statuses WHERE id = v_status.id;
    IF v_status.id IS NULL THEN
      RAISE EXCEPTION 'invalid status' USING ERRCODE = '22023';
    END IF;
    v_old := app_task_status(v_task.status_id);
    IF p_priority NOT IN ('none', 'low', 'medium', 'high', 'urgent') THEN
      RAISE EXCEPTION 'invalid priority' USING ERRCODE = '22023';
    END IF;
    IF p_start IS NOT NULL AND p_due IS NOT NULL AND p_start > p_due THEN
      RAISE EXCEPTION 'the start date must not be after the due date' USING ERRCODE = '22023';
    END IF;
    IF p_milestone AND p_start IS NOT NULL THEN
      RAISE EXCEPTION 'a milestone has only a due date' USING ERRCODE = '22023';
    END IF;

    -- 승인한 후속 업무: 이 업무에서 이어지는 후속이어야 하고, 기준 버전이 모두 맞아야 한다. 같은 항목인데 지금은
    -- 이어지지 않는 업무(미리본 뒤 관계가 지워짐)는 오래된 계획이라 `shift_conflict`(아무것도 바꾸지 않음), 처음부터 옮길
    -- 수 없는 업무(없음·다른 항목)는 400 그대로다(지금 선행 쪽인 업무도 오래된 계획).
    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_state := app_shift_target(p_task, v_other.id, false);
      IF v_state = 'invalid' THEN
        RAISE EXCEPTION 'a shifted task must be a successor of the edited task' USING ERRCODE = '22023';
      END IF;
      -- 오래된 항목은 날짜 모양을 보기 전에 거른다(미리본 뒤 시작일을 지운 후속도 400이 아니라 409).
      IF v_state = 'stale' OR v_other.version IS DISTINCT FROM (v_shift ->> 'base_version')::integer THEN
        v_stale := true;
        CONTINUE;
      END IF;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      IF (v_start IS NULL) <> (v_other.start_date IS NULL) OR (v_due IS NULL) <> (v_other.due_date IS NULL)
         OR (v_start IS NOT NULL AND v_due IS NOT NULL AND v_start > v_due) THEN
        RAISE EXCEPTION 'a shift moves existing dates only' USING ERRCODE = '22023';
      END IF;
    END LOOP;
    IF v_stale THEN
      RETURN 'shift_conflict';
    END IF;

    -- 새로 넣는 담당자만 확인한다(이미 있는 담당 때문에 다른 필드 고치기가 막히지 않게). 새 담당자 목록은 여기서
    -- 한 번 정하고 그 목록만 넣는다(그사이 다른 트랜잭션이 지운 기존 담당을 확인 없이 되살리지 않게).
    -- 멤버 행 공유 잠금은 사용자 id 순으로 잡는다.
    SELECT coalesce(array_agg(u ORDER BY u), '{}') INTO v_new
    FROM (SELECT DISTINCT u FROM unnest(coalesce(p_assignees, '{}')) AS u
          WHERE NOT EXISTS (SELECT 1 FROM task_assignees a WHERE a.task_id = p_task AND a.user_id = u)) s;
    FOREACH v_uid IN ARRAY v_new LOOP
      PERFORM app_check_assignee(v_task.workspace_id, v_task.item_id, v_uid);
    END LOOP;

    UPDATE tasks SET
      title = btrim(p_title), description = coalesce(p_description, ''), status_id = v_status.id,
      priority = p_priority, start_date = p_start, due_date = p_due, estimate_minutes = p_estimate,
      tags = coalesce(p_tags, '{}'), milestone = p_milestone, version = version + 1, updated_by = v_user,
      updated_at = now(),
      completed_at = CASE WHEN v_status.category = 'done' AND v_old.category <> 'done' THEN now()
                          WHEN v_status.category <> 'done' THEN NULL ELSE completed_at END,
      position = CASE WHEN v_status.id <> v_task.status_id THEN app_task_lane_end(v_status.id)
                      ELSE position END
    WHERE id = p_task;

    IF v_task.title <> btrim(p_title) THEN
      PERFORM app_audit(app_id(p_ids, 1), v_task.workspace_id, 'rename', 'task', p_task,
                        jsonb_build_object('from', v_task.title, 'to', btrim(p_title)));
    END IF;
    IF v_status.id <> v_task.status_id THEN
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_task.workspace_id, v_task.item_id, 'task', p_task, 'status',
                           jsonb_build_object('from', v_old.name, 'to', v_status.name,
                                              'from_category', v_old.category, 'to_category', v_status.category));
    END IF;
    FOR v_uid IN
      SELECT a.user_id FROM task_assignees a
      WHERE a.task_id = p_task AND NOT (a.user_id = ANY (coalesce(p_assignees, '{}')))
    LOOP
      DELETE FROM task_assignees WHERE task_id = p_task AND user_id = v_uid;
      IF FOUND THEN
        v_at := v_at + 1;
        PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'unassign', 'task', p_task,
                          jsonb_build_object('user', v_uid));
        v_at := v_at + 1;
        PERFORM app_activity(app_id(p_ids, v_at), v_task.workspace_id, v_task.item_id, 'task', p_task, 'unassign',
                             jsonb_build_object('user', v_uid));
      END IF;
    END LOOP;
    FOREACH v_uid IN ARRAY v_new LOOP
      INSERT INTO task_assignees (task_id, user_id, assigned_by) VALUES (p_task, v_uid, v_user);
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'assign', 'task', p_task,
                        jsonb_build_object('user', v_uid));
      v_at := v_at + 1;
      PERFORM app_activity(app_id(p_ids, v_at), v_task.workspace_id, v_task.item_id, 'task', p_task, 'assign',
                           jsonb_build_object('user', v_uid));
    END LOOP;

    FOR v_shift IN SELECT * FROM jsonb_array_elements(p_shifts) LOOP
      SELECT * INTO v_other FROM tasks WHERE id = (v_shift ->> 'task_id')::uuid;
      v_start := (v_shift ->> 'start_date')::date;
      v_due := (v_shift ->> 'due_date')::date;
      UPDATE tasks SET start_date = v_start, due_date = v_due, version = version + 1,
                       updated_by = v_user, updated_at = now()
      WHERE id = v_other.id;
      v_at := v_at + 1;
      PERFORM app_audit(app_id(p_ids, v_at), v_task.workspace_id, 'reschedule', 'task', v_other.id,
                        jsonb_build_object('reason', 'predecessor_moved', 'predecessor', p_task,
                                           'from', jsonb_build_array(v_other.start_date, v_other.due_date),
                                           'to', jsonb_build_array(v_start, v_due)));
    END LOOP;
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb, p_ids uuid[]) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) RETURNS integer
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    -- 이름 바꾸기 1 + 상태 변경 활동 1 + 담당 해제·지정마다 감사와 활동 2 + 밀린 후속마다 1.
    SELECT CASE WHEN app_access('task', p_task) = 'none' THEN 0 ELSE
      2 + 2 * (SELECT count(*) FROM task_assignees WHERE task_id = p_task)::integer
        + 2 * coalesce(cardinality(p_assignees), 0)
        + CASE WHEN jsonb_typeof(p_shifts) = 'array' THEN jsonb_array_length(p_shifts) ELSE 0 END
    END
  $$;


ALTER FUNCTION fvoci.app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) OWNER TO fvoci_migrator;

-- 구간을 바꿀 때만 멈춤 누계와 범위를 검사한다. 이미 연결된 대상을 볼 수 없어도 메모 등 다른 필드는 고칠 수 있다.
-- 길이 상한 추가나 기존 긴 기록 보정은 하지 않는다(P-29 보류).
CREATE FUNCTION fvoci.app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_entry time_entries%ROWTYPE := app_lock_own_entry(p_entry);
    v_ws    uuid;
  BEGIN
    IF v_entry.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    IF v_entry.ended_at IS NULL THEN
      IF p_ended IS NOT NULL OR p_paused IS DISTINCT FROM v_entry.paused_seconds
         OR p_started > coalesce(v_entry.paused_at, now()) THEN
        RAISE EXCEPTION 'a running timer only changes its start, kind, memo and target' USING ERRCODE = '22023';
      END IF;
      IF p_started IS DISTINCT FROM v_entry.started_at
         AND v_entry.paused_seconds > extract(epoch FROM coalesce(v_entry.paused_at, now()) - p_started) THEN
        RAISE EXCEPTION 'the start leaves less time than the timer was paused' USING ERRCODE = '22023';
      END IF;
    ELSIF p_ended IS NULL THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    ELSIF (p_started, p_ended, p_paused) IS DISTINCT FROM (v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds)
          AND (p_ended < p_started OR p_paused < 0 OR p_paused > extract(epoch FROM p_ended - p_started)) THEN
      RAISE EXCEPTION 'invalid time range' USING ERRCODE = '22023';
    END IF;
    -- 대상을 바꿀 때만 지금 볼 수 있는지 확인한다(이미 연결된 대상은 볼 수 없게 되어도 그대로 둘 수 있다).
    IF (p_target_kind, p_target) IS DISTINCT FROM (v_entry.target_kind, v_entry.target_id) THEN
      v_ws := app_time_target_workspace(p_target_kind, p_target);
    ELSE
      v_ws := v_entry.workspace_id;
    END IF;
    INSERT INTO time_entry_revisions (id, entry_id, user_id, started_at, ended_at, paused_seconds, kind, memo,
                                      target_kind, target_id)
    VALUES (app_id(p_ids, 1), p_entry, v_entry.user_id, v_entry.started_at, v_entry.ended_at, v_entry.paused_seconds,
            v_entry.kind, v_entry.memo, v_entry.target_kind, v_entry.target_id);
    UPDATE time_entries SET
      started_at = p_started, ended_at = p_ended, paused_seconds = p_paused, kind = p_kind,
      memo = coalesce(p_memo, ''), target_kind = p_target_kind, target_id = p_target, workspace_id = v_ws,
      version = version + 1, updated_at = now()
    WHERE id = p_entry;
    RETURN 'updated';
  END
  $$;


ALTER FUNCTION fvoci.app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) OWNER TO fvoci_migrator;

-- D-103: 마지막 수정 요청 ID만 재시도 성공으로 인정한다. 이후 저장이 있으면 옛 기준 판으로 되돌릴 수 없다.
-- 판 확인과 갱신은 같은 행 잠금 안에서 한다. D-89: docs/decisions/tasks.md#d-89.
CREATE FUNCTION fvoci.app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb) RETURNS text
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_view saved_views%ROWTYPE;
  BEGIN
    SELECT * INTO v_view FROM saved_views WHERE id = p_view AND user_id = app_user_id() FOR UPDATE;
    IF v_view.id IS NULL OR app_access('item', v_view.item_id) = 'none' THEN
      RAISE EXCEPTION 'view not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_view.updated_request_id = p_request THEN
      RETURN 'replayed';
    END IF;
    IF EXISTS (SELECT 1 FROM saved_views
               WHERE user_id = app_user_id() AND (request_id = p_request OR updated_request_id = p_request)) THEN
      RAISE EXCEPTION 'request_id was already used for another change' USING ERRCODE = '22023';
    END IF;
    IF v_view.version IS DISTINCT FROM p_base_version THEN
      RETURN 'conflict';
    END IF;
    UPDATE saved_views SET config = p_config, version = version + 1, updated_request_id = p_request
    WHERE id = p_view;
    RETURN 'updated';
  EXCEPTION
    WHEN check_violation OR not_null_violation THEN
      RAISE EXCEPTION 'invalid view: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;


ALTER FUNCTION fvoci.app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_upload_claim_held(p_upload uuid, p_claim timestamp with time zone) RETURNS boolean
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT state = 'done' OR (state = 'completing' AND state_changed_at = p_claim)
                     FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE), false)
  $$;


ALTER FUNCTION fvoci.app_upload_claim_held(p_upload uuid, p_claim timestamp with time zone) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text, p_ids uuid[]) RETURNS uuid
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    INSERT INTO users (id, issuer, subject, email, email_verified, display_name)
    VALUES (app_id(p_ids, 1), p_issuer, p_subject, p_email, coalesce(p_email_verified, false), p_display_name)
    ON CONFLICT ON CONSTRAINT users_identity
      DO UPDATE SET email = EXCLUDED.email,
                    email_verified = EXCLUDED.email_verified,
                    display_name = EXCLUDED.display_name,
                    last_login_at = now()
    RETURNING id
  $$;


ALTER FUNCTION fvoci.app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text, p_ids uuid[]) OWNER TO fvoci_migrator;

-- 요청 사용자는 트랜잭션의 app.user_id다. 미설정·빈 값은 NULL이며 사용자 RLS·권한 검사는 거부한다.
-- app_system_*의 명시적인 사용자 없음 경로는 서버 작업용이다. docs/decisions/access.md#d-08.
CREATE FUNCTION fvoci.app_user_id() RETURNS uuid
    LANGUAGE sql STABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$ SELECT nullif(current_setting('app.user_id', true), '')::uuid $$;


ALTER FUNCTION fvoci.app_user_id() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_utc_text(p_at timestamp with time zone) RETURNS text
    LANGUAGE sql IMMUTABLE
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT to_char(p_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
  $$;


ALTER FUNCTION fvoci.app_utc_text(p_at timestamp with time zone) OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.app_version_referenced(p_file uuid, p_version integer, p_version_id uuid) RETURNS boolean
    LANGUAGE sql STABLE SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT EXISTS (SELECT 1 FROM record_files WHERE version_id = p_version_id)
        OR EXISTS (SELECT 1 FROM document_file_refs r
                   JOIN documents d ON d.id = r.document_id
                   JOIN files f ON f.id = r.file_id
                   WHERE r.file_id = p_file AND r.version = p_version
                     AND app_file_ref_in_scope(d.workspace_id, d.owner_user_id, f.workspace_id, f.owner_user_id))
        -- 휴지통의 문서도 리비전과 같게 보고, 낡은 상태는 빼고.
        OR EXISTS (SELECT 1 FROM document_collab_file_refs r
                   JOIN document_collab_states s ON s.document_id = r.document_id AND s.pending
                   JOIN documents d ON d.id = r.document_id
                   JOIN files f ON f.id = r.file_id
                   WHERE r.file_id = p_file AND r.version = p_version
                     AND app_collab_state_current(s.document_id, s.revision)
                     AND app_file_ref_in_scope(d.workspace_id, d.owner_user_id, f.workspace_id, f.owner_user_id))
  $$;


ALTER FUNCTION fvoci.app_version_referenced(p_file uuid, p_version integer, p_version_id uuid) OWNER TO fvoci_migrator;

-- 호출자 권한으로 문서를 읽는다. 소유자 권한으로 RLS를 건너뛰면 다른 공간 문서의 존재가 오류로 드러날 수 있다.
CREATE FUNCTION fvoci.document_revisions_match() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    IF NEW.revision <> (SELECT revision FROM documents WHERE id = NEW.document_id) THEN
      RAISE EXCEPTION 'revision row must match the document revision' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.document_revisions_match() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.documents_initial_position() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  BEGIN
    SELECT coalesce(max(s.position), 0) + 1 INTO NEW.position
    FROM app_tree_siblings(NEW.workspace_id, NEW.item_id, NEW.owner_user_id, NEW.parent_id) s;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.documents_initial_position() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.documents_revision_step() RETURNS trigger
    LANGUAGE plpgsql
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_content_changed boolean :=
      NEW.title IS DISTINCT FROM OLD.title
      OR NEW.body IS DISTINCT FROM OLD.body
      OR NEW.schema_version IS DISTINCT FROM OLD.schema_version;
  BEGIN
    IF v_content_changed AND NEW.revision <> OLD.revision + 1 THEN
      RAISE EXCEPTION 'content change must advance the revision by one' USING ERRCODE = '23514';
    END IF;
    -- 내용 없이 리비전만 올리면 실제로 없던 리비전 기록을 끼워 넣을 수 있다. 막는다.
    IF NOT v_content_changed AND NEW.revision <> OLD.revision THEN
      RAISE EXCEPTION 'revision changes only with content' USING ERRCODE = '23514';
    END IF;
    RETURN NEW;
  END
  $$;


ALTER FUNCTION fvoci.documents_revision_step() OWNER TO fvoci_migrator;

CREATE FUNCTION fvoci.documents_tree_check() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
  DECLARE
    v_parent documents%ROWTYPE;
  BEGIN
    IF NEW.parent_id IS NOT NULL THEN
      IF NEW.parent_id = NEW.id THEN
        RAISE EXCEPTION 'a document cannot be its own parent' USING ERRCODE = '23514';
      END IF;
      SELECT * INTO v_parent FROM documents WHERE id = NEW.parent_id;
      IF v_parent.workspace_id IS DISTINCT FROM NEW.workspace_id
         OR v_parent.item_id IS DISTINCT FROM NEW.item_id
         OR v_parent.owner_user_id IS DISTINCT FROM NEW.owner_user_id THEN
        RAISE EXCEPTION 'the parent must be in the same place' USING ERRCODE = '23514';
      END IF;
    END IF;
    IF TG_OP = 'UPDATE'
       AND (NEW.workspace_id IS DISTINCT FROM OLD.workspace_id
            OR NEW.item_id IS DISTINCT FROM OLD.item_id
            OR NEW.owner_user_id IS DISTINCT FROM OLD.owner_user_id)
       AND EXISTS (SELECT 1 FROM documents c WHERE c.parent_id = NEW.id) THEN
      RAISE EXCEPTION 'children must be detached before moving a document' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
  END
  $$;


ALTER FUNCTION fvoci.documents_tree_check() OWNER TO fvoci_migrator;

-- 기본키·고유 제약.

ALTER TABLE ONLY fvoci.activity
    ADD CONSTRAINT activity_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.audit_log
    ADD CONSTRAINT audit_log_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_request UNIQUE (task_id, request_id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.comments
    ADD CONSTRAINT comments_request UNIQUE (created_by, request_id);

ALTER TABLE ONLY fvoci.course_details
    ADD CONSTRAINT course_details_pkey PRIMARY KEY (item_id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.document_collab_file_refs
    ADD CONSTRAINT document_collab_file_refs_pkey PRIMARY KEY (document_id, file_id, version);

ALTER TABLE ONLY fvoci.document_collab_states
    ADD CONSTRAINT document_collab_states_pkey PRIMARY KEY (document_id);

ALTER TABLE ONLY fvoci.document_file_refs
    ADD CONSTRAINT document_file_refs_pkey PRIMARY KEY (document_id, revision, file_id, version);

ALTER TABLE ONLY fvoci.document_links
    ADD CONSTRAINT document_links_pkey PRIMARY KEY (document_id, target_kind, target_id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_pkey PRIMARY KEY (document_id, user_id);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_pkey PRIMARY KEY (document_id, revision);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_request UNIQUE (document_id, author, request_id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_target UNIQUE (document_id, grantee_user_id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_number UNIQUE (workspace_id, scope, number);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_request UNIQUE (workspace_id, created_by, request_id);

ALTER TABLE ONLY fvoci.episode_progress
    ADD CONSTRAINT episode_progress_pkey PRIMARY KEY (episode_id, user_id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_file_id_version_key UNIQUE (file_id, version);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_storage_key_key UNIQUE (storage_key);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.group_members
    ADD CONSTRAINT group_members_pkey PRIMARY KEY (group_id, user_id);

ALTER TABLE ONLY fvoci.groups
    ADD CONSTRAINT groups_name UNIQUE (workspace_id, name);

ALTER TABLE ONLY fvoci.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.instance_state
    ADD CONSTRAINT instance_state_pkey PRIMARY KEY (key);

ALTER TABLE ONLY fvoci.invitations
    ADD CONSTRAINT invitations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_request UNIQUE (item_id, created_by, request_id);

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

ALTER TABLE ONLY fvoci.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.numbered_targets
    ADD CONSTRAINT numbered_targets_pkey PRIMARY KEY (scope_id, number);

ALTER TABLE ONLY fvoci.numbered_targets
    ADD CONSTRAINT numbered_targets_target UNIQUE (target_kind, target_id);

ALTER TABLE ONLY fvoci.oidc_login_states
    ADD CONSTRAINT oidc_login_states_pkey PRIMARY KEY (secret_hash);

ALTER TABLE ONLY fvoci.organization_members
    ADD CONSTRAINT organization_members_pkey PRIMARY KEY (organization_id, user_id);

ALTER TABLE ONLY fvoci.organizations
    ADD CONSTRAINT organizations_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.organizations
    ADD CONSTRAINT organizations_request UNIQUE (created_by, request_id);

ALTER TABLE ONLY fvoci.ownership_offers
    ADD CONSTRAINT ownership_offers_pkey PRIMARY KEY (workspace_id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_token_hash_key UNIQUE (token_hash);

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_pkey PRIMARY KEY (record_id, "position");

ALTER TABLE ONLY fvoci.reference_counters
    ADD CONSTRAINT reference_counters_pkey PRIMARY KEY (workspace_id, scope);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_name UNIQUE (user_id, item_id, name);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_request UNIQUE (user_id, request_id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_updated_request UNIQUE (user_id, updated_request_id);

ALTER TABLE ONLY fvoci.scheduled_runs
    ADD CONSTRAINT scheduled_runs_pkey PRIMARY KEY (name);

ALTER TABLE ONLY fvoci.search_queue
    ADD CONSTRAINT search_queue_pkey PRIMARY KEY (kind, target_id);

ALTER TABLE ONLY fvoci.sessions
    ADD CONSTRAINT sessions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.sessions
    ADD CONSTRAINT sessions_token_hash_key UNIQUE (token_hash);

ALTER TABLE ONLY fvoci.server_admins
    ADD CONSTRAINT server_admins_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY fvoci.storage_deletions
    ADD CONSTRAINT storage_deletions_pkey PRIMARY KEY (storage_key, storage_upload_id);

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_pkey PRIMARY KEY (task_id, user_id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_pkey PRIMARY KEY (predecessor_id, successor_id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_root_task_id_key UNIQUE (root_task_id);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_item UNIQUE (id, item_id);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_item_name UNIQUE (item_id, name);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_occurrence UNIQUE (series_id, occurrence_date);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_request UNIQUE (item_id, created_by, request_id);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_request UNIQUE (user_id, request_id);

ALTER TABLE ONLY fvoci.time_entry_requests
    ADD CONSTRAINT time_entry_requests_pkey PRIMARY KEY (user_id, request_id);

ALTER TABLE ONLY fvoci.time_entry_revisions
    ADD CONSTRAINT time_entry_revisions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.unit_progress
    ADD CONSTRAINT unit_progress_pkey PRIMARY KEY (unit_id, user_id);

ALTER TABLE ONLY fvoci.upload_parts
    ADD CONSTRAINT upload_parts_pkey PRIMARY KEY (upload_id, number);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_request UNIQUE (user_id, request_id);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_storage_key_key UNIQUE (storage_key);

ALTER TABLE ONLY fvoci.user_marks
    ADD CONSTRAINT user_marks_pkey PRIMARY KEY (user_id, mark, target_kind, target_id);

ALTER TABLE ONLY fvoci.user_settings
    ADD CONSTRAINT user_settings_pkey PRIMARY KEY (user_id);

ALTER TABLE ONLY fvoci.users
    ADD CONSTRAINT users_identity UNIQUE (issuer, subject);

ALTER TABLE ONLY fvoci.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.workspace_members
    ADD CONSTRAINT workspace_members_pkey PRIMARY KEY (workspace_id, user_id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.workspaces
    ADD CONSTRAINT workspaces_request UNIQUE (created_by, request_id);

-- 조회·멱등성·중복 방지 인덱스.

CREATE UNIQUE INDEX audit_log_document_rename ON fvoci.audit_log USING btree (target_id, ((detail ->> 'revision'::text))) WHERE ((action = 'rename'::text) AND (target_type = 'document'::text) AND (detail ? 'revision'::text));

CREATE INDEX activity_item ON fvoci.activity USING btree (item_id, created_at DESC) WHERE (item_id IS NOT NULL);

CREATE INDEX activity_target ON fvoci.activity USING btree (target_kind, target_id, created_at DESC);

CREATE INDEX activity_workspace ON fvoci.activity USING btree (workspace_id, created_at DESC);

CREATE INDEX audit_log_organization ON fvoci.audit_log USING btree (organization_id, created_at) WHERE (organization_id IS NOT NULL);

CREATE INDEX audit_log_workspace ON fvoci.audit_log USING btree (workspace_id, created_at);

CREATE INDEX checklist_items_task ON fvoci.checklist_items USING btree (task_id, "position");

CREATE INDEX comments_document ON fvoci.comments USING btree (document_id, created_at) WHERE (document_id IS NOT NULL);

CREATE INDEX comments_parent ON fvoci.comments USING btree (parent_id) WHERE (parent_id IS NOT NULL);

CREATE INDEX comments_task ON fvoci.comments USING btree (task_id, created_at) WHERE (task_id IS NOT NULL);

CREATE INDEX course_episodes_item ON fvoci.course_episodes USING btree (item_id, number);

CREATE INDEX document_collab_file_refs_version ON fvoci.document_collab_file_refs USING btree (file_id, version);

CREATE INDEX document_file_refs_file ON fvoci.document_file_refs USING btree (file_id, version);

CREATE INDEX document_links_target ON fvoci.document_links USING btree (target_kind, target_id);

CREATE INDEX document_shares_grantee ON fvoci.document_shares USING btree (grantee_user_id);

CREATE INDEX document_shares_organization ON fvoci.document_shares USING btree (organization_id);

CREATE INDEX documents_item ON fvoci.documents USING btree (item_id);

CREATE INDEX documents_item_roots ON fvoci.documents USING btree (item_id, "position") WHERE (parent_id IS NULL);

CREATE INDEX documents_note_roots ON fvoci.documents USING btree (owner_user_id, "position") WHERE (parent_id IS NULL);

CREATE INDEX documents_owner_updated ON fvoci.documents USING btree (owner_user_id, updated_at DESC) WHERE (owner_user_id IS NOT NULL);

CREATE INDEX documents_parent ON fvoci.documents USING btree (parent_id, "position");

CREATE UNIQUE INDEX documents_personal_request ON fvoci.documents USING btree (owner_user_id, request_id) WHERE (owner_user_id IS NOT NULL);

CREATE INDEX documents_templates ON fvoci.documents USING btree (workspace_id) WHERE (is_template AND (deleted_at IS NULL));

CREATE INDEX documents_trash ON fvoci.documents USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX documents_updated_by ON fvoci.documents USING btree (updated_by, updated_at DESC);

CREATE INDEX documents_wiki_roots ON fvoci.documents USING btree (workspace_id, "position") WHERE ((item_id IS NULL) AND (owner_user_id IS NULL) AND (parent_id IS NULL));

CREATE INDEX documents_workspace_updated ON fvoci.documents USING btree (workspace_id, updated_at DESC);

CREATE INDEX episode_progress_user ON fvoci.episode_progress USING btree (user_id);

CREATE INDEX events_item ON fvoci.events USING btree (item_id) WHERE (deleted_at IS NULL);

CREATE INDEX events_trash ON fvoci.events USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX events_workspace_dates ON fvoci.events USING btree (workspace_id, start_date, end_date) WHERE (deleted_at IS NULL);

CREATE INDEX events_workspace_times ON fvoci.events USING btree (workspace_id, starts_at, ends_at) WHERE (deleted_at IS NULL);

CREATE INDEX files_document ON fvoci.files USING btree (document_id) WHERE (document_id IS NOT NULL);

CREATE INDEX files_item ON fvoci.files USING btree (item_id) WHERE (item_id IS NOT NULL);

CREATE INDEX files_owner ON fvoci.files USING btree (owner_user_id) WHERE (owner_user_id IS NOT NULL);

CREATE INDEX files_trash ON fvoci.files USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX files_workspace ON fvoci.files USING btree (workspace_id);

CREATE UNIQUE INDEX invitations_open_organization ON fvoci.invitations USING btree (organization_id, email) WHERE ((closed_at IS NULL) AND (workspace_id IS NULL));

CREATE UNIQUE INDEX invitations_open_workspace ON fvoci.invitations USING btree (workspace_id, email) WHERE ((closed_at IS NULL) AND (workspace_id IS NOT NULL));

CREATE INDEX item_members_by_user ON fvoci.item_members USING btree (user_id) WHERE (user_id IS NOT NULL);

CREATE UNIQUE INDEX item_members_group ON fvoci.item_members USING btree (item_id, group_id) WHERE (group_id IS NOT NULL);

CREATE UNIQUE INDEX item_members_user ON fvoci.item_members USING btree (item_id, user_id) WHERE (user_id IS NOT NULL);

CREATE INDEX item_records_item ON fvoci.item_records USING btree (item_id, kind, occurred_on);

CREATE INDEX items_module ON fvoci.items USING btree (module_id);

CREATE INDEX items_trash ON fvoci.items USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX items_workspace_updated ON fvoci.items USING btree (workspace_id, updated_at DESC);

CREATE UNIQUE INDEX module_keys_current ON fvoci.module_keys USING btree (module_id) WHERE (retired_at IS NULL);

CREATE UNIQUE INDEX notifications_due_once ON fvoci.notifications USING btree (user_id, task_id, ((detail ->> 'date'::text))) WHERE (kind = 'due'::text);

CREATE INDEX notifications_item ON fvoci.notifications USING btree (item_id) WHERE (item_id IS NOT NULL);

CREATE UNIQUE INDEX notifications_reminder_once ON fvoci.notifications USING btree (user_id, item_id, ((detail ->> 'key'::text)), ((detail ->> 'date'::text))) WHERE (kind = ANY (ARRAY['expiry'::text, 'review'::text, 'deadline'::text]));

CREATE INDEX notifications_invitation ON fvoci.notifications USING btree (invitation_id) WHERE (invitation_id IS NOT NULL);

CREATE INDEX notifications_user ON fvoci.notifications USING btree (user_id, created_at DESC, id DESC);

CREATE INDEX notifications_user_unread ON fvoci.notifications USING btree (user_id, organization_id) WHERE (read_at IS NULL);

CREATE INDEX organization_members_user ON fvoci.organization_members USING btree (user_id);

CREATE INDEX oidc_login_states_expires ON fvoci.oidc_login_states USING btree (expires_at);

CREATE INDEX public_links_document ON fvoci.public_links USING btree (document_id);

CREATE INDEX record_files_version ON fvoci.record_files USING btree (version_id) WHERE (version_id IS NOT NULL);

CREATE INDEX saved_views_item ON fvoci.saved_views USING btree (user_id, item_id);

CREATE INDEX search_queue_order ON fvoci.search_queue USING btree (queued_at);

CREATE INDEX search_queue_task ON fvoci.search_queue USING btree (task_uid) WHERE (task_uid IS NOT NULL);

CREATE INDEX sessions_expires ON fvoci.sessions USING btree (expires_at);

CREATE INDEX sessions_user ON fvoci.sessions USING btree (user_id);

CREATE INDEX study_units_item ON fvoci.study_units USING btree (item_id, number);

CREATE INDEX task_assignees_user ON fvoci.task_assignees USING btree (user_id);

CREATE INDEX task_dependencies_item ON fvoci.task_dependencies USING btree (item_id);

CREATE INDEX task_dependencies_successor ON fvoci.task_dependencies USING btree (successor_id);

CREATE INDEX task_series_item ON fvoci.task_series USING btree (item_id);

CREATE INDEX task_statuses_item_position ON fvoci.task_statuses USING btree (item_id, "position");

CREATE INDEX tasks_due ON fvoci.tasks USING btree (due_date) WHERE (due_date IS NOT NULL);

CREATE INDEX tasks_item_updated ON fvoci.tasks USING btree (item_id, updated_at DESC);

CREATE INDEX tasks_lane ON fvoci.tasks USING btree (status_id, "position");

CREATE INDEX tasks_parent ON fvoci.tasks USING btree (parent_id);

CREATE INDEX tasks_trash ON fvoci.tasks USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX tasks_updated_by ON fvoci.tasks USING btree (updated_by, updated_at DESC);

CREATE INDEX tasks_workspace_updated ON fvoci.tasks USING btree (workspace_id, updated_at DESC);

CREATE UNIQUE INDEX time_entries_one_active ON fvoci.time_entries USING btree (user_id) WHERE ((ended_at IS NULL) AND (deleted_at IS NULL));

CREATE INDEX time_entries_target ON fvoci.time_entries USING btree (target_kind, target_id) WHERE (deleted_at IS NULL);

CREATE INDEX time_entries_user_started ON fvoci.time_entries USING btree (user_id, started_at) WHERE (deleted_at IS NULL);

CREATE INDEX time_entry_revisions_entry ON fvoci.time_entry_revisions USING btree (entry_id, changed_at);

CREATE INDEX trash_batches_owner ON fvoci.trash_batches USING btree (owner_user_id, deleted_at) WHERE ((restored_at IS NULL) AND (purged_at IS NULL));

CREATE INDEX trash_batches_workspace ON fvoci.trash_batches USING btree (workspace_id, deleted_at) WHERE ((restored_at IS NULL) AND (purged_at IS NULL));

CREATE INDEX unit_progress_user ON fvoci.unit_progress USING btree (user_id);

CREATE INDEX uploads_open ON fvoci.uploads USING btree (expires_at) WHERE (state = ANY (ARRAY['uploading'::text, 'completing'::text]));

CREATE INDEX uploads_place ON fvoci.uploads USING btree (workspace_id, owner_user_id) WHERE (state = ANY (ARRAY['uploading'::text, 'completing'::text]));

CREATE INDEX workspace_members_user ON fvoci.workspace_members USING btree (user_id);

CREATE INDEX workspaces_deleted ON fvoci.workspaces USING btree (deleted_at) WHERE (deleted_at IS NOT NULL);

CREATE INDEX workspaces_organization ON fvoci.workspaces USING btree (organization_id, created_at);

-- 불변식·감사·검색 대기열 트리거.

CREATE TRIGGER document_revisions_file_refs AFTER INSERT ON fvoci.document_revisions FOR EACH ROW EXECUTE FUNCTION fvoci.app_record_file_refs();

CREATE TRIGGER document_revisions_match BEFORE INSERT ON fvoci.document_revisions FOR EACH ROW EXECUTE FUNCTION fvoci.document_revisions_match();

CREATE TRIGGER documents_files BEFORE DELETE ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.app_delete_attached_files('document');

CREATE TRIGGER documents_initial_position BEFORE INSERT ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.documents_initial_position();

CREATE TRIGGER documents_revision_step BEFORE UPDATE ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.documents_revision_step();

CREATE TRIGGER documents_search AFTER INSERT OR DELETE OR UPDATE OF title, body, deleted_at, visibility, item_id, workspace_id, owner_user_id ON fvoci.documents FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('document');

CREATE TRIGGER documents_search_files AFTER UPDATE OF deleted_at ON fvoci.documents FOR EACH ROW WHEN ((old.deleted_at IS DISTINCT FROM new.deleted_at)) EXECUTE FUNCTION fvoci.app_search_enqueue_document_files();

CREATE CONSTRAINT TRIGGER documents_tree_check AFTER INSERT OR UPDATE OF parent_id, workspace_id, item_id, owner_user_id ON fvoci.documents NOT DEFERRABLE INITIALLY IMMEDIATE FOR EACH ROW EXECUTE FUNCTION fvoci.documents_tree_check();

CREATE TRIGGER events_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, item_id, start_date, starts_at ON fvoci.events FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('event');

CREATE TRIGGER events_shift_excluded BEFORE UPDATE OF start_date, starts_at, timezone ON fvoci.events FOR EACH ROW EXECUTE FUNCTION fvoci.app_shift_excluded_dates();

CREATE TRIGGER file_versions_pinned BEFORE DELETE ON fvoci.file_versions FOR EACH ROW EXECUTE FUNCTION fvoci.app_guard_pinned_version();

CREATE TRIGGER file_versions_storage AFTER DELETE ON fvoci.file_versions FOR EACH ROW EXECUTE FUNCTION fvoci.app_queue_version_deletion();

CREATE TRIGGER files_search AFTER INSERT OR DELETE OR UPDATE OF name, deleted_at, current_version, workspace_id, owner_user_id ON fvoci.files FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('file');

CREATE TRIGGER item_records_search AFTER INSERT OR DELETE OR UPDATE OF title, body, fields, occurred_on ON fvoci.item_records FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('record');

CREATE TRIGGER items_course BEFORE DELETE ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_delete_item_course_rows();

CREATE TRIGGER items_files BEFORE DELETE ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_delete_attached_files('item');

CREATE TRIGGER items_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, visibility, archived_at, updated_at ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('item');

CREATE TRIGGER items_search_children AFTER UPDATE OF deleted_at ON fvoci.items FOR EACH ROW WHEN ((old.deleted_at IS DISTINCT FROM new.deleted_at)) EXECUTE FUNCTION fvoci.app_search_enqueue_item_children();

CREATE TRIGGER items_versions BEFORE UPDATE ON fvoci.items FOR EACH ROW EXECUTE FUNCTION fvoci.app_items_versions();

CREATE TRIGGER module_keys_search AFTER INSERT OR UPDATE ON fvoci.module_keys FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue_module();

CREATE TRIGGER task_assignees_search AFTER INSERT OR DELETE ON fvoci.task_assignees FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue_assignee();

CREATE TRIGGER tasks_move_version BEFORE UPDATE ON fvoci.tasks FOR EACH ROW EXECUTE FUNCTION fvoci.app_tasks_move_version();

CREATE TRIGGER tasks_search AFTER INSERT OR DELETE OR UPDATE OF title, description, deleted_at, status_id, item_id, due_date, start_date ON fvoci.tasks FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('task');

CREATE TRIGGER task_statuses_search AFTER UPDATE OF name, category ON fvoci.task_statuses FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue_status_tasks();

CREATE TRIGGER tasks_series_root_guard BEFORE UPDATE OF parent_id, start_date, due_date ON fvoci.tasks FOR EACH ROW EXECUTE FUNCTION fvoci.app_guard_series_root();


-- 외래 키. 참조 대상 테이블과 키를 준비한 뒤 연결한다.

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

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.checklist_items
    ADD CONSTRAINT checklist_items_task_id_fkey FOREIGN KEY (task_id) REFERENCES fvoci.tasks(id);

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

ALTER TABLE ONLY fvoci.course_details
    ADD CONSTRAINT course_details_item FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.course_details
    ADD CONSTRAINT course_details_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.course_episodes
    ADD CONSTRAINT course_episodes_item FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.document_collab_file_refs
    ADD CONSTRAINT document_collab_file_refs_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.document_collab_states(document_id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.document_collab_states
    ADD CONSTRAINT document_collab_states_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.document_file_refs
    ADD CONSTRAINT document_file_refs_document_id_revision_fkey FOREIGN KEY (document_id, revision) REFERENCES fvoci.document_revisions(document_id, revision) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.document_links
    ADD CONSTRAINT document_links_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_mentioned_by_fkey FOREIGN KEY (mentioned_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.document_mentions
    ADD CONSTRAINT document_mentions_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_author_fkey FOREIGN KEY (author) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.document_revisions
    ADD CONSTRAINT document_revisions_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_granted_by_fkey FOREIGN KEY (granted_by) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_grantee_user_id_fkey FOREIGN KEY (grantee_user_id) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.document_shares
    ADD CONSTRAINT document_shares_organization_id_fkey FOREIGN KEY (organization_id) REFERENCES fvoci.organizations(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.documents
    ADD CONSTRAINT documents_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.episode_progress
    ADD CONSTRAINT episode_progress_episode_id_fkey FOREIGN KEY (episode_id) REFERENCES fvoci.course_episodes(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.episode_progress
    ADD CONSTRAINT episode_progress_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_deleted_by_fkey FOREIGN KEY (deleted_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.events
    ADD CONSTRAINT events_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_file_id_fkey FOREIGN KEY (file_id) REFERENCES fvoci.files(id);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_uploaded_by_fkey FOREIGN KEY (uploaded_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

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

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.item_records
    ADD CONSTRAINT item_records_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

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

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.public_links
    ADD CONSTRAINT public_links_document_id_fkey FOREIGN KEY (document_id) REFERENCES fvoci.documents(id);

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_file_id_fkey FOREIGN KEY (file_id) REFERENCES fvoci.files(id) ON DELETE SET NULL;

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_record_id_fkey FOREIGN KEY (record_id) REFERENCES fvoci.item_records(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.record_files
    ADD CONSTRAINT record_files_version_id_fkey FOREIGN KEY (version_id) REFERENCES fvoci.file_versions(id) ON DELETE SET NULL;

ALTER TABLE ONLY fvoci.reference_counters
    ADD CONSTRAINT reference_counters_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.saved_views
    ADD CONSTRAINT saved_views_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.server_admins
    ADD CONSTRAINT server_admins_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);
ALTER TABLE ONLY fvoci.sessions
    ADD CONSTRAINT sessions_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.study_units
    ADD CONSTRAINT study_units_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_assigned_by_fkey FOREIGN KEY (assigned_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_task_id_fkey FOREIGN KEY (task_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.task_assignees
    ADD CONSTRAINT task_assignees_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id);

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_predecessor_id_fkey FOREIGN KEY (predecessor_id) REFERENCES fvoci.tasks(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.task_dependencies
    ADD CONSTRAINT task_dependencies_successor_id_fkey FOREIGN KEY (successor_id) REFERENCES fvoci.tasks(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_root_task_id_fkey FOREIGN KEY (root_task_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.task_series
    ADD CONSTRAINT task_series_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_created_by_fkey FOREIGN KEY (created_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES fvoci.items(id, workspace_id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_parent_id_fkey FOREIGN KEY (parent_id) REFERENCES fvoci.tasks(id);

ALTER TABLE ONLY fvoci.task_statuses
    ADD CONSTRAINT task_statuses_item_id_fkey FOREIGN KEY (item_id) REFERENCES fvoci.items(id) ON DELETE CASCADE;
ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_series_id_fkey FOREIGN KEY (series_id) REFERENCES fvoci.task_series(id);
ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_status_id_fkey FOREIGN KEY (status_id) REFERENCES fvoci.task_statuses(id);
ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_status_item FOREIGN KEY (status_id, item_id) REFERENCES fvoci.task_statuses(id, item_id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_trash_batch_id_fkey FOREIGN KEY (trash_batch_id) REFERENCES fvoci.trash_batches(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.tasks
    ADD CONSTRAINT tasks_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.time_entries
    ADD CONSTRAINT time_entries_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.time_entry_requests
    ADD CONSTRAINT time_entry_requests_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.time_entry_requests
    ADD CONSTRAINT time_entry_requests_entry_id_fkey FOREIGN KEY (entry_id) REFERENCES fvoci.time_entries(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.time_entry_revisions
    ADD CONSTRAINT time_entry_revisions_entry_id_fkey FOREIGN KEY (entry_id) REFERENCES fvoci.time_entries(id);

ALTER TABLE ONLY fvoci.time_entry_revisions
    ADD CONSTRAINT time_entry_revisions_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);


ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_deleted_by_fkey FOREIGN KEY (deleted_by) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.trash_batches
    ADD CONSTRAINT trash_batches_workspace_id_fkey FOREIGN KEY (workspace_id) REFERENCES fvoci.workspaces(id);

ALTER TABLE ONLY fvoci.unit_progress
    ADD CONSTRAINT unit_progress_unit_id_fkey FOREIGN KEY (unit_id) REFERENCES fvoci.study_units(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.unit_progress
    ADD CONSTRAINT unit_progress_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.upload_parts
    ADD CONSTRAINT upload_parts_upload_id_fkey FOREIGN KEY (upload_id) REFERENCES fvoci.uploads(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_version_id_fkey FOREIGN KEY (version_id) REFERENCES fvoci.file_versions(id);

ALTER TABLE ONLY fvoci.user_marks
    ADD CONSTRAINT user_marks_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

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

-- RLS·정책. 앱은 소유자가 아니며 사용자 문맥이 없으면 사용자 데이터에 접근할 수 없다.
-- 소유자 함수가 권한을 확인하고 변경하므로 FORCE ROW LEVEL SECURITY를 추가하지 않는다(D-08).
-- 읽기 범위는 모두 app_access(kind, id) 하나에서 나온다(D-113). 조직 경계(D-111)는 그 안에 있다.

ALTER TABLE fvoci.activity ENABLE ROW LEVEL SECURITY;

CREATE POLICY activity_select ON fvoci.activity FOR SELECT USING ((fvoci.app_access(target_kind, target_id) <> 'none'::text));

ALTER TABLE fvoci.audit_log ENABLE ROW LEVEL SECURITY;

CREATE POLICY audit_log_insert ON fvoci.audit_log FOR INSERT WITH CHECK (((actor = fvoci.app_user_id()) AND ((workspace_id IS NULL) OR (fvoci.app_access('workspace'::text, workspace_id) <> 'none'::text))));

-- 공간 로그는 공간 관리자, 조직 로그는 조직 관리자, 서버 로그는 서버 관리자, 개인 로그는 소유자가 본다(D-57·D-112).
CREATE POLICY audit_log_select ON fvoci.audit_log FOR SELECT USING ((((workspace_id IS NOT NULL) AND (fvoci.app_access('workspace'::text, workspace_id) = 'owner'::text)) OR ((workspace_id IS NULL) AND (organization_id IS NOT NULL) AND (fvoci.app_access('organization'::text, organization_id) = 'owner'::text)) OR (owner_user_id = fvoci.app_user_id()) OR ((workspace_id IS NULL) AND (organization_id IS NULL) AND (owner_user_id IS NULL) AND fvoci.app_is_server_admin())));

ALTER TABLE fvoci.checklist_items ENABLE ROW LEVEL SECURITY;

CREATE POLICY checklist_items_select ON fvoci.checklist_items FOR SELECT USING ((fvoci.app_access('task'::text, task_id) <> 'none'::text));

ALTER TABLE fvoci.comments ENABLE ROW LEVEL SECURITY;

CREATE POLICY comments_select ON fvoci.comments FOR SELECT USING ((fvoci.app_access('comment'::text, id) <> 'none'::text));

ALTER TABLE fvoci.course_details ENABLE ROW LEVEL SECURITY;

CREATE POLICY course_details_select ON fvoci.course_details FOR SELECT USING ((fvoci.app_access('item'::text, item_id) <> 'none'::text));

ALTER TABLE fvoci.course_episodes ENABLE ROW LEVEL SECURITY;

CREATE POLICY course_episodes_select ON fvoci.course_episodes FOR SELECT USING ((fvoci.app_access('item'::text, item_id) <> 'none'::text));

ALTER TABLE fvoci.document_collab_file_refs ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.document_collab_states ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.document_file_refs ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.document_links ENABLE ROW LEVEL SECURITY;

CREATE POLICY document_links_select ON fvoci.document_links FOR SELECT USING ((fvoci.app_access('document'::text, document_id) <> 'none'::text));

ALTER TABLE fvoci.document_mentions ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.document_revisions ENABLE ROW LEVEL SECURITY;

CREATE POLICY document_revisions_insert ON fvoci.document_revisions FOR INSERT WITH CHECK (((author = fvoci.app_user_id()) AND (fvoci.app_access('document'::text, document_id) = ANY (ARRAY['owner'::text, 'write'::text]))));

CREATE POLICY document_revisions_select ON fvoci.document_revisions FOR SELECT USING ((EXISTS ( SELECT 1
   FROM fvoci.documents d
  WHERE (d.id = document_revisions.document_id))));

ALTER TABLE fvoci.document_shares ENABLE ROW LEVEL SECURITY;

CREATE POLICY document_shares_select ON fvoci.document_shares FOR SELECT USING (fvoci.app_document_can_manage(document_id));

ALTER TABLE fvoci.documents ENABLE ROW LEVEL SECURITY;

CREATE POLICY documents_select ON fvoci.documents FOR SELECT USING ((fvoci.app_access('document'::text, id) <> 'none'::text));

CREATE POLICY documents_update ON fvoci.documents FOR UPDATE USING ((fvoci.app_access('document'::text, id) = ANY (ARRAY['owner'::text, 'write'::text]))) WITH CHECK (((fvoci.app_access('document'::text, id) = ANY (ARRAY['owner'::text, 'write'::text])) AND (updated_by = fvoci.app_user_id())));

ALTER TABLE fvoci.episode_progress ENABLE ROW LEVEL SECURITY;

-- 개인 진도 원시 행은 본인만 읽는다(D-37·D-78). 공유되는 것은 app_course_participants의 집계뿐이다.
CREATE POLICY episode_progress_select ON fvoci.episode_progress FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.events ENABLE ROW LEVEL SECURITY;

CREATE POLICY events_select ON fvoci.events FOR SELECT USING ((fvoci.app_access('event'::text, id) <> 'none'::text));

ALTER TABLE fvoci.file_versions ENABLE ROW LEVEL SECURITY;

CREATE POLICY file_versions_select ON fvoci.file_versions FOR SELECT USING ((fvoci.app_access('file'::text, file_id) <> 'none'::text));

ALTER TABLE fvoci.files ENABLE ROW LEVEL SECURITY;

CREATE POLICY files_select ON fvoci.files FOR SELECT USING ((fvoci.app_access('file'::text, id) <> 'none'::text));

ALTER TABLE fvoci.group_members ENABLE ROW LEVEL SECURITY;

CREATE POLICY group_members_select ON fvoci.group_members FOR SELECT USING ((EXISTS ( SELECT 1
   FROM fvoci.groups g
  WHERE (g.id = group_members.group_id))));

ALTER TABLE fvoci.groups ENABLE ROW LEVEL SECURITY;

CREATE POLICY groups_select ON fvoci.groups FOR SELECT USING ((fvoci.app_access('workspace'::text, workspace_id) <> 'none'::text));

ALTER TABLE fvoci.instance_state ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.invitations ENABLE ROW LEVEL SECURITY;

CREATE POLICY invitations_select ON fvoci.invitations FOR SELECT USING ((((workspace_id IS NOT NULL) AND (fvoci.app_access('workspace'::text, workspace_id) = 'owner'::text)) OR ((workspace_id IS NULL) AND (fvoci.app_access('organization'::text, organization_id) = 'owner'::text))));

ALTER TABLE fvoci.item_members ENABLE ROW LEVEL SECURITY;

CREATE POLICY item_members_select ON fvoci.item_members FOR SELECT USING ((fvoci.app_access('item'::text, item_id) <> 'none'::text));

ALTER TABLE fvoci.item_records ENABLE ROW LEVEL SECURITY;

CREATE POLICY item_records_select ON fvoci.item_records FOR SELECT USING ((fvoci.app_access('item'::text, item_id) <> 'none'::text));

ALTER TABLE fvoci.items ENABLE ROW LEVEL SECURITY;

CREATE POLICY items_select ON fvoci.items FOR SELECT USING ((fvoci.app_access('item'::text, id) <> 'none'::text));

ALTER TABLE fvoci.module_keys ENABLE ROW LEVEL SECURITY;

CREATE POLICY module_keys_select ON fvoci.module_keys FOR SELECT USING ((fvoci.app_access('workspace'::text, workspace_id) <> 'none'::text));

ALTER TABLE fvoci.modules ENABLE ROW LEVEL SECURITY;

CREATE POLICY modules_select ON fvoci.modules FOR SELECT USING (((fvoci.app_access('workspace'::text, workspace_id) <> 'none'::text) AND (deleted_at IS NULL)));

ALTER TABLE fvoci.notifications ENABLE ROW LEVEL SECURITY;

CREATE POLICY notifications_select ON fvoci.notifications FOR SELECT USING (((user_id = fvoci.app_user_id()) AND fvoci.app_notification_visible(document_id, task_id, item_id, invitation_id)));

ALTER TABLE fvoci.numbered_targets ENABLE ROW LEVEL SECURITY;

CREATE POLICY numbered_targets_select ON fvoci.numbered_targets FOR SELECT USING (
CASE target_kind
    WHEN 'item'::text THEN (fvoci.app_access('item'::text, target_id) <> 'none'::text)
    WHEN 'document'::text THEN (fvoci.app_access('document'::text, target_id) <> 'none'::text)
    WHEN 'task'::text THEN (fvoci.app_access('task'::text, target_id) <> 'none'::text)
    ELSE false
END);

ALTER TABLE fvoci.oidc_login_states ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.organization_members ENABLE ROW LEVEL SECURITY;

CREATE POLICY organization_members_select ON fvoci.organization_members FOR SELECT USING ((fvoci.app_access('organization'::text, organization_id) <> 'none'::text));

ALTER TABLE fvoci.organizations ENABLE ROW LEVEL SECURITY;

-- 조직 행은 그 조직의 멤버와 서버 관리자(메타데이터만, D-112)가 본다.
CREATE POLICY organizations_select ON fvoci.organizations FOR SELECT USING (((fvoci.app_access('organization'::text, id) <> 'none'::text) OR fvoci.app_is_server_admin()));

ALTER TABLE fvoci.ownership_offers ENABLE ROW LEVEL SECURITY;

CREATE POLICY ownership_offers_select ON fvoci.ownership_offers FOR SELECT USING ((fvoci.app_access('workspace'::text, workspace_id) <> 'none'::text));

ALTER TABLE fvoci.public_links ENABLE ROW LEVEL SECURITY;

CREATE POLICY public_links_select ON fvoci.public_links FOR SELECT USING (fvoci.app_document_can_manage(document_id));

ALTER TABLE fvoci.record_files ENABLE ROW LEVEL SECURITY;

CREATE POLICY record_files_select ON fvoci.record_files FOR SELECT USING ((EXISTS ( SELECT 1
   FROM fvoci.item_records r
  WHERE (r.id = record_files.record_id))));

ALTER TABLE fvoci.reference_counters ENABLE ROW LEVEL SECURITY;

CREATE POLICY reference_counters_all ON fvoci.reference_counters USING ((fvoci.app_access('workspace'::text, workspace_id) = ANY (ARRAY['owner'::text, 'write'::text]))) WITH CHECK ((fvoci.app_access('workspace'::text, workspace_id) = ANY (ARRAY['owner'::text, 'write'::text])));

ALTER TABLE fvoci.saved_views ENABLE ROW LEVEL SECURITY;

CREATE POLICY saved_views_select ON fvoci.saved_views FOR SELECT USING (((user_id = fvoci.app_user_id()) AND (fvoci.app_access('item'::text, item_id) <> 'none'::text)));

ALTER TABLE fvoci.scheduled_runs ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.search_queue ENABLE ROW LEVEL SECURITY;

CREATE POLICY search_queue_none ON fvoci.search_queue FOR SELECT USING (false);

ALTER TABLE fvoci.server_admins ENABLE ROW LEVEL SECURITY;

CREATE POLICY server_admins_select ON fvoci.server_admins FOR SELECT USING (fvoci.app_is_server_admin());

ALTER TABLE fvoci.sessions ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.storage_deletions ENABLE ROW LEVEL SECURITY;

CREATE POLICY storage_deletions_none ON fvoci.storage_deletions FOR SELECT USING (false);

ALTER TABLE fvoci.study_units ENABLE ROW LEVEL SECURITY;

CREATE POLICY study_units_select ON fvoci.study_units FOR SELECT USING ((fvoci.app_access('item'::text, item_id) <> 'none'::text));

ALTER TABLE fvoci.task_assignees ENABLE ROW LEVEL SECURITY;

CREATE POLICY task_assignees_select ON fvoci.task_assignees FOR SELECT USING ((fvoci.app_access('task'::text, task_id) <> 'none'::text));

ALTER TABLE fvoci.task_dependencies ENABLE ROW LEVEL SECURITY;

CREATE POLICY task_dependencies_select ON fvoci.task_dependencies FOR SELECT USING (((fvoci.app_access('task'::text, predecessor_id) <> 'none'::text) AND (fvoci.app_access('task'::text, successor_id) <> 'none'::text)));

ALTER TABLE fvoci.task_series ENABLE ROW LEVEL SECURITY;

CREATE POLICY task_series_select ON fvoci.task_series FOR SELECT USING ((fvoci.app_access('task'::text, root_task_id) <> 'none'::text));

ALTER TABLE fvoci.task_statuses ENABLE ROW LEVEL SECURITY;

CREATE POLICY task_statuses_select ON fvoci.task_statuses FOR SELECT USING ((fvoci.app_access('item'::text, item_id) <> 'none'::text));

ALTER TABLE fvoci.tasks ENABLE ROW LEVEL SECURITY;

CREATE POLICY tasks_select ON fvoci.tasks FOR SELECT USING ((fvoci.app_access('task'::text, id) <> 'none'::text));

ALTER TABLE fvoci.time_entries ENABLE ROW LEVEL SECURITY;

-- 시간 기록은 본인만 본다(D-117).
CREATE POLICY time_entries_select ON fvoci.time_entries FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.time_entry_requests ENABLE ROW LEVEL SECURITY;

ALTER TABLE fvoci.time_entry_revisions ENABLE ROW LEVEL SECURITY;

CREATE POLICY time_entry_revisions_select ON fvoci.time_entry_revisions FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.trash_batches ENABLE ROW LEVEL SECURITY;

CREATE POLICY trash_batches_none ON fvoci.trash_batches FOR SELECT USING (false);

ALTER TABLE fvoci.unit_progress ENABLE ROW LEVEL SECURITY;

CREATE POLICY unit_progress_select ON fvoci.unit_progress FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.upload_parts ENABLE ROW LEVEL SECURITY;

CREATE POLICY upload_parts_select ON fvoci.upload_parts FOR SELECT USING ((EXISTS ( SELECT 1
   FROM fvoci.uploads u
  WHERE ((u.id = upload_parts.upload_id) AND (u.user_id = fvoci.app_user_id())))));

ALTER TABLE fvoci.uploads ENABLE ROW LEVEL SECURITY;

CREATE POLICY uploads_select ON fvoci.uploads FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.user_marks ENABLE ROW LEVEL SECURITY;

CREATE POLICY user_marks_select ON fvoci.user_marks FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.user_settings ENABLE ROW LEVEL SECURITY;

CREATE POLICY user_settings_select ON fvoci.user_settings FOR SELECT USING ((user_id = fvoci.app_user_id()));

ALTER TABLE fvoci.users ENABLE ROW LEVEL SECURITY;

-- 사용자 행은 본인과 같은 조직 사람만 본다(D-114).
CREATE POLICY users_select ON fvoci.users FOR SELECT USING (((id = fvoci.app_user_id()) OR fvoci.app_shares_organization(id)));

ALTER TABLE fvoci.workspace_members ENABLE ROW LEVEL SECURITY;

CREATE POLICY workspace_members_select ON fvoci.workspace_members FOR SELECT USING ((fvoci.app_access('workspace'::text, workspace_id) <> 'none'::text));

ALTER TABLE fvoci.workspaces ENABLE ROW LEVEL SECURITY;

-- 워크스페이스 행(이름 같은 메타데이터)은 멤버와 조직 관리자가 본다. 내용은 멤버만 본다(D-112).
CREATE POLICY workspaces_select ON fvoci.workspaces FOR SELECT USING (((fvoci.app_access('workspace'::text, id) <> 'none'::text) OR (fvoci.app_access('organization'::text, organization_id) = 'owner'::text)));

CREATE POLICY workspaces_update ON fvoci.workspaces FOR UPDATE USING ((fvoci.app_access('workspace'::text, id) = 'owner'::text)) WITH CHECK ((fvoci.app_access('workspace'::text, id) = 'owner'::text));

-- 실효 권한. PUBLIC 실행을 회수하고 각 앱 진입점·읽기 범위만 부여한다.
-- 기본 권한은 bootstrap과 PostgreSQL 기본값을 유지한다. 일괄 권한 확대는 하지 않는다.

GRANT USAGE ON SCHEMA fvoci TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_abort_upload(p_upload uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_abort_upload(p_upload uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_accept_ownership(p_workspace uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_accept_ownership(p_workspace uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_add_checklist_items(p_task uuid, p_items jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_add_comment(p_request uuid, p_document uuid, p_task uuid, p_parent uuid, p_body text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_add_comment_ids(p_document uuid, p_task uuid, p_parent uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_add_comment_ids(p_document uuid, p_task uuid, p_parent uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_add_episodes(p_item uuid, p_rows jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_add_study_units(p_item uuid, p_rows jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_add_time_entry(p_request_id uuid, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_add_time_entry(p_request_id uuid, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_allocate_number(p_workspace uuid, p_scope uuid, p_kind text, p_target uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_archive_item(p_item uuid, p_archived boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_archive_item(p_item uuid, p_archived boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_archive_module(p_module uuid, p_archived boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_archive_module(p_module uuid, p_archived boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_audit(p_id uuid, p_workspace uuid, p_action text, p_target_type text, p_target uuid, p_detail jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_audit_document(p_id uuid, p_doc uuid, p_action text, p_detail jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_audit_document_export(p_doc uuid, p_revision integer, p_format text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_audit_document_export(p_doc uuid, p_revision integer, p_format text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_audit_document_rename(p_id uuid, p_doc uuid, p_revision integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_audit_document_rename(p_id uuid, p_doc uuid, p_revision integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_audit_personal_export(p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_audit_personal_export(p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_audit_workspace_export(p_workspace uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_audit_workspace_export(p_workspace uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_batch_documents(p_batch fvoci.trash_batches) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_batch_expired(p_batch uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_batch_files(p_batch fvoci.trash_batches) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_batch_keeps_referenced(p_batch fvoci.trash_batches) FROM PUBLIC;

GRANT SELECT ON TABLE fvoci.uploads TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_begin_complete(p_upload uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_begin_complete(p_upload uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_cancel_ownership_offer(p_workspace uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_cancel_ownership_offer(p_workspace uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_check_assignee(p_workspace uuid, p_item uuid, p_user uuid) FROM PUBLIC;

GRANT SELECT ON TABLE fvoci.documents TO fvoci_app;

GRANT UPDATE(title) ON TABLE fvoci.documents TO fvoci_app;

GRANT UPDATE(schema_version) ON TABLE fvoci.documents TO fvoci_app;

GRANT UPDATE(body) ON TABLE fvoci.documents TO fvoci_app;

GRANT UPDATE(revision) ON TABLE fvoci.documents TO fvoci_app;

GRANT UPDATE(updated_by) ON TABLE fvoci.documents TO fvoci_app;

GRANT UPDATE(updated_at) ON TABLE fvoci.documents TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_check_move(p_doc uuid, p_target uuid, p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_check_recurrence(p_rule jsonb) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_check_recurrence(p_rule jsonb) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_check_task_parent(p_task uuid, p_parent uuid, p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_check_timezone(p_tz text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_check_timezone(p_tz text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_claim_schedule(p_name text, p_every interval) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_claim_schedule(p_name text, p_every interval) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_collab_access(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_collab_access(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_collab_load(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_collab_load(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_collab_pending(p_workspace uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_collab_pending(p_workspace uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_collab_state_current(p_doc uuid, p_revision integer) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_comment_target_access(p_document uuid, p_task uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_comment_target_access(p_document uuid, p_task uuid) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_comment_target_archived(p_document uuid, p_task uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_comment_target_state(p_document uuid, p_task uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_comment_target_state(p_document uuid, p_task uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_copy_document(p_doc uuid, p_request_id uuid, p_target_workspace uuid, p_title text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text, p_reset boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_copy_item(p_item uuid, p_module uuid, p_request uuid, p_title text, p_reset boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_copy_item_ids(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_copy_item_ids(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_course_access(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_course_access(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_course_participants(p_item uuid, p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_course_participants(p_item uuid, p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_event(p_item uuid, p_request_id uuid, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_group(p_workspace uuid, p_name text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_group(p_workspace uuid, p_name text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_item(p_module uuid, p_request_id uuid, p_title text, p_description text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_login_state(p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_login_state(p_secret_hash bytea, p_state text, p_verifier text, p_nonce text, p_return_to text, p_ttl_secs integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_module(p_workspace uuid, p_request_id uuid, p_kind text, p_name text, p_key text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_module(p_workspace uuid, p_request_id uuid, p_kind text, p_name text, p_key text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_module_document(p_item uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamp with time zone, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_public_link(p_doc uuid, p_token_hash bytea, p_expires_at timestamp with time zone, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_record(p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_session(p_token_hash bytea, p_user uuid, p_id_token text, p_ttl_secs bigint, p_replaces bytea, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_task(p_item uuid, p_request_id uuid, p_title text, p_parent uuid, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_upload(p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text, p_type text, p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint, p_max_storage bigint, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_upload(p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text, p_type text, p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint, p_max_storage bigint, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_view(p_request uuid, p_item uuid, p_name text, p_config jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_wiki_document(p_workspace uuid, p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[]) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_delete_attached_files() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_delete_checklist_item(p_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_checklist_item(p_id uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_comment(p_comment uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_comment(p_comment uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_episode(p_episode uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_episode(p_episode uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_event(p_event uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_event(p_event uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_file_rows(p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_delete_file_version(p_file uuid, p_version integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_file_version(p_file uuid, p_version integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_group(p_group uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_group(p_group uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_group_ids(p_group uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_group_ids(p_group uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_item_course_rows() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_delete_record(p_record uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_record(p_record uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_study_unit(p_unit uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_study_unit(p_unit uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_time_entry(p_entry uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_time_entry(p_entry uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_view(p_view uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_view(p_view uuid) TO fvoci_app;






REVOKE ALL ON FUNCTION fvoci.app_document_can_export(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_can_export(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_document_can_manage(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_can_manage(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_document_depth(p_doc uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_document_height(p_doc uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_document_realtime(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_realtime(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_document_realtime_locked(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_realtime_locked(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_document_reference(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_reference(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_edit_comment(p_comment uuid, p_body text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_edit_comment(p_comment uuid, p_body text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_end_session(p_token_hash bytea) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_end_session(p_token_hash bytea) TO fvoci_app;

GRANT SELECT ON TABLE fvoci.time_entries TO fvoci_app;

REVOKE ALL ON TABLE fvoci.time_entry_requests FROM PUBLIC, fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_entry_seconds(e fvoci.time_entries) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_entry_seconds(e fvoci.time_entries) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_event_occurrences(p_workspace uuid, p_from timestamp with time zone, p_to timestamp with time zone, p_from_date date, p_to_date date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_event_occurrences(p_workspace uuid, p_from timestamp with time zone, p_to timestamp with time zone, p_from_date date, p_to_date date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_exclude_event_occurrence(p_event uuid, p_date date, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_fail_upload(p_upload uuid, p_reason text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_fail_upload(p_upload uuid, p_reason text) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_file_archived(p_file uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_file_can_download(p_file uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_file_can_download(p_file uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_file_place(p_kind text, p_id uuid, OUT workspace_id uuid, OUT owner_user_id uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_file_refs(p_body jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_finish_upload(p_upload uuid, p_sha256 text, p_max_storage bigint, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_finish_upload(p_upload uuid, p_sha256 text, p_max_storage bigint, p_ids uuid[]) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_guard_pinned_version() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_guard_series_root() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_id(p_ids uuid[], p_at integer) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_ids_from(p_ids uuid[], p_from integer) FROM PUBLIC;


REVOKE ALL ON FUNCTION fvoci.app_is_item_member(p_item uuid, p_user uuid) FROM PUBLIC;




REVOKE ALL ON FUNCTION fvoci.app_item_archived(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_item_assignable(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_item_assignable(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_item_reference(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_item_reference(p_item uuid) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_items_versions() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_leave_workspace(p_workspace uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_leave_workspace(p_workspace uuid, p_ids uuid[]) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_lock_dependencies(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_lock_item(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_lock_item(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_lock_lane(p_status uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_lock_live_workspace(p_ws uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_lock_move(p_doc uuid, p_target uuid, p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_lock_move(p_doc uuid, p_target uuid, p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_lock_own_entry(p_entry uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_lock_task_rows(p_ids uuid[], p_extra_status uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_lock_tree(p_workspace uuid, p_item uuid, p_owner uuid) FROM PUBLIC;


REVOKE ALL ON FUNCTION fvoci.app_member_role(ws uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_member_role(ws uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_move_document(p_doc uuid, p_target uuid, p_item uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_move_document_ids(p_doc uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_move_document_ids(p_doc uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_move_keeps_number(p_doc uuid, p_target uuid, p_item uuid) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_move_task(p_task uuid, p_status text, p_before uuid, p_base_move_version integer, p_ids uuid[]) TO fvoci_app;




REVOKE ALL ON FUNCTION fvoci.app_my_upload(p_upload uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_upload(p_upload uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_workspace_ids(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_workspace_ids(p_organization uuid) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_notify(p_id uuid, p_user uuid, p_kind text, p_document uuid, p_task uuid, p_comment uuid, p_actor uuid, p_detail jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_notify_item(p_id uuid, p_user uuid, p_kind text, p_item uuid, p_record uuid, p_detail jsonb) FROM PUBLIC;


REVOKE ALL ON FUNCTION fvoci.app_place_document(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_plan_units(p_item uuid, p_entries jsonb) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_plan_units(p_item uuid, p_entries jsonb) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_projected_occurrences(p_workspace uuid, p_item uuid, p_from date, p_to date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_public_document(p_token_hash bytea) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_public_document(p_token_hash bytea) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_purge_batch(p_batch uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_purge_batch(p_batch uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_purge_batch_as(p_batch uuid, p_system boolean, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_purge_documents(p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_purge_expired(p_workspace uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_purge_expired(p_workspace uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_purge_expired_ids(p_workspace uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_purge_expired_ids(p_workspace uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_purge_item_rows(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_purge_tasks(p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_purge_workspace(p_ws uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_purge_workspace(p_ws uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_purge_workspace_as(p_ws uuid, p_system boolean, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_queue_version_deletion() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_record_document_references(p_doc uuid, p_notify boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_record_document_references_ids(p_doc uuid, p_notify boolean) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_record_document_references_ids(p_doc uuid, p_notify boolean) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_record_file_refs() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_record_part(p_upload uuid, p_number integer, p_size bigint, p_sha256 text, p_etag text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_record_part(p_upload uuid, p_number integer, p_size bigint, p_sha256 text, p_etag text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_record_write_check(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_recurrence_dates(p_rule jsonb, p_anchor date, p_from date, p_to date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_reference_nodes(p_body jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_release_upload_claim(p_upload uuid, p_claim timestamp with time zone) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_release_upload_claim(p_upload uuid, p_claim timestamp with time zone) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_member(p_workspace uuid, p_member uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_member(p_workspace uuid, p_member uuid, p_ids uuid[]) TO fvoci_app;



REVOKE ALL ON FUNCTION fvoci.app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_task_dependency(p_predecessor uuid, p_successor uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_rename_file(p_file uuid, p_name text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_rename_file(p_file uuid, p_name text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_renumber_lane(p_status uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_admin(p_workspace uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_course_write(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_document(p_doc uuid, p_need text) FROM PUBLIC;
-- 서버가 쓸 수 없는 이유(403/409)를 묻는 데 직접 부른다(documents::save).
GRANT ALL ON FUNCTION fvoci.app_require_document(p_doc uuid, p_need text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_require_document_manager(p_doc uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_event_write(p_event uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_study_write(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_task_write(p_task uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_reset_upload(p_upload uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_reset_upload(p_upload uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_resolve_comment(p_comment uuid, p_resolved boolean) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_resolve_comment(p_comment uuid, p_resolved boolean) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_restore_batch(p_batch uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_restore_batch(p_batch uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_restore_document_revision(p_doc uuid, p_revision integer, p_request_id uuid, p_base_revision integer, p_schema_version integer, p_ids uuid[]) TO fvoci_app;



REVOKE ALL ON FUNCTION fvoci.app_revoke_public_link(p_link uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_revoke_public_link(p_link uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_search_enqueue() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_search_enqueue_assignee() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_search_enqueue_document_files() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_search_enqueue_item_children() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_search_enqueue_module() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_search_visible(p_kinds text[], p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_search_visible(p_kinds text[], p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_session_user(p_token_hash bytea) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_session_user(p_token_hash bytea) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_course_details(p_item uuid, p_subject text, p_instructor text, p_platform text, p_url text, p_textbook text, p_starts date, p_expires date, p_target date, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_display_settings(p_zone text, p_locale text, p_week_start text, p_hour_cycle text, p_theme text) TO fvoci_app;
REVOKE ALL ON FUNCTION fvoci.app_set_last_organization(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_last_organization(p_organization uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_document_parent(p_doc uuid, p_parent uuid, p_before uuid, p_max_depth integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_document_settings(p_doc uuid, p_visibility text, p_allow_copy boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_document_template(p_doc uuid, p_template boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_episode_progress(p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_episode_progress(p_episode uuid, p_status text, p_planned date, p_position integer, p_note text, p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_event_recurrence(p_event uuid, p_rule jsonb, p_base jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_group_member(p_group uuid, p_member uuid, p_present boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_group_member(p_group uuid, p_member uuid, p_present boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_group_member_ids(p_group uuid, p_member uuid, p_present boolean) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_group_member_ids(p_group uuid, p_member uuid, p_present boolean) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[], p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_item_members(p_item uuid, p_users uuid[], p_groups uuid[], p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_item_members_ids(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_item_members_ids(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_item_profile(p_item uuid, p_base_version integer, p_leader uuid, p_goal text, p_scope text, p_starts date, p_ends date, p_status text, p_details jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_item_profile(p_item uuid, p_base_version integer, p_leader uuid, p_goal text, p_scope text, p_starts date, p_ends date, p_status text, p_details jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_item_template(p_item uuid, p_template boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_item_template(p_item uuid, p_template boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_mark(p_mark text, p_kind text, p_id uuid, p_on boolean) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_mark(p_mark text, p_kind text, p_id uuid, p_on boolean) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_member_role(p_workspace uuid, p_member uuid, p_role text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_member_role_ids(p_workspace uuid, p_member uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_member_role_ids(p_workspace uuid, p_member uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_module_description(p_module uuid, p_text text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_module_description(p_module uuid, p_text text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_module_icon(p_module uuid, p_icon text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_module_icon(p_module uuid, p_icon text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_set_storage_upload(p_upload uuid, p_storage_upload text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_storage_upload(p_upload uuid, p_storage_upload text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer, p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_task_dependency(p_predecessor uuid, p_successor uuid, p_kind text, p_lag integer, p_base_kind text, p_base_lag integer, p_shifts jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_task_parent(p_task uuid, p_parent uuid, p_ids uuid[]) TO fvoci_app;



REVOKE ALL ON FUNCTION fvoci.app_set_trash_retention(p_workspace uuid, p_days integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_trash_retention(p_workspace uuid, p_days integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_unit_progress(p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_unit_progress(p_unit uuid, p_status text, p_done integer, p_planned date, p_note text, p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_version_retention(p_workspace uuid, p_days integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_version_retention(p_workspace uuid, p_days integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_workspace_realtime(p_workspace uuid, p_enabled boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_workspace_realtime(p_workspace uuid, p_enabled boolean, p_ids uuid[]) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_shared_with_me(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_shared_with_me(p_organization uuid) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_shift_excluded_dates() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_shift_target(p_from uuid, p_task uuid, p_include_from boolean) FROM PUBLIC;


REVOKE ALL ON FUNCTION fvoci.app_storage_used(p_workspace uuid, p_owner uuid, p_except uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_study_access(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_study_access(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_study_participants(p_item uuid, p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_study_participants(p_item uuid, p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_sync_item_series(p_item uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_sync_item_series(p_item uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_sync_item_series_ids(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_sync_item_series_ids(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_sync_task_series(p_series uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_sync_task_series(p_series uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_sync_task_series_as(p_series uuid, p_system boolean, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_sync_task_series_ids(p_series uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_sync_task_series_ids(p_series uuid) TO fvoci_app;


REVOKE ALL ON FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_due_notifications(p_today date, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_due_notifications(p_today date, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_due_notifications_ids(p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_due_notifications_ids(p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_expire_uploads() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_expire_uploads() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_item_reminders(p_today date, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_item_reminders(p_today date, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_item_reminders_ids(p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_item_reminders_ids(p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_purge_expired(p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_purge_expired(p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_purge_expired_ids() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_purge_expired_ids() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_purge_old_versions() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_purge_old_versions() TO fvoci_app;



REVOKE ALL ON FUNCTION fvoci.app_system_search_claim(p_limit integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_claim(p_limit integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_done(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_enqueue_all() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_enqueue_all() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_payload(p_kinds text[], p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_payload(p_kinds text[], p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_pending(p_limit integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_pending(p_limit integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_recheck(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_release_stale(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_attempts integer[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_settled(p_task bigint, p_outcome text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_settled(p_task bigint, p_outcome text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_task bigint) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_search_submitted(p_kinds text[], p_ids uuid[], p_queued timestamp with time zone[], p_task bigint) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_storage_claim(p_limit integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_storage_claim(p_limit integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_storage_retry(p_key text, p_upload text, p_attempts integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_storage_retry(p_key text, p_upload text, p_attempts integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_take_login_state(p_secret_hash bytea, p_state text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_take_login_state(p_secret_hash bytea, p_state text) TO fvoci_app;



REVOKE ALL ON FUNCTION fvoci.app_task_checklist_counts(p_task uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_task_checklist_counts(p_task uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_task_lane_end(p_status uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_task_reference(p_task uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_task_reference(p_task uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_task_series_bound(p_series uuid) FROM PUBLIC;


REVOKE ALL ON FUNCTION fvoci.app_tasks_move_version() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_time_target_label(p_kind text, p_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_time_target_label(p_kind text, p_id uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_time_target_workspace(p_kind text, p_id uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_timer_action(p_entry uuid, p_action text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_timer_action(p_entry uuid, p_action text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_document(p_doc uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_document(p_doc uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_expires_at(p_batch fvoci.trash_batches) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_trash_file(p_file uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_file(p_file uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_item(p_item uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_item(p_item uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_kept_for_references(p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_kept_for_references(p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_list(p_workspace uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_list(p_workspace uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_module(p_module uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_module(p_module uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_trash_role(p_batch fvoci.trash_batches) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_trash_task(p_task uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_trash_task(p_task uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_tree_siblings(p_workspace uuid, p_item uuid, p_owner uuid, p_parent uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_unseen_assignees_bound(p_item uuid, p_workspace uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_unshare_document(p_share uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_unshare_document(p_share uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_checklist_item(p_id uuid, p_text text, p_checked boolean) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_episode(p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text, p_materials text, p_planned date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_episode(p_episode uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_duration integer, p_url text, p_materials text, p_planned date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_event(p_event uuid, p_base_version integer, p_kind text, p_title text, p_description text, p_all_day boolean, p_start_date date, p_end_date date, p_starts_at timestamp with time zone, p_ends_at timestamp with time zone, p_timezone text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_item(p_item uuid, p_base_version integer, p_title text, p_description text, p_visibility text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_item(p_item uuid, p_base_version integer, p_title text, p_description text, p_visibility text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_item_ids(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_item_ids(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_module(p_module uuid, p_name text, p_key text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_module(p_module uuid, p_name text, p_key text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_record(p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_record(p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_study_unit(p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_study_unit(p_unit uuid, p_version integer, p_number integer, p_title text, p_chapter text, p_kind text, p_amount integer, p_planned date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_task(p_task uuid, p_base_version integer, p_title text, p_description text, p_status text, p_priority text, p_start date, p_due date, p_estimate integer, p_tags text[], p_assignees uuid[], p_milestone boolean, p_shifts jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_task_ids(p_task uuid, p_assignees uuid[], p_shifts jsonb) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_time_entry(p_entry uuid, p_base_version integer, p_started timestamp with time zone, p_ended timestamp with time zone, p_paused integer, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_view(p_request uuid, p_view uuid, p_base_version integer, p_config jsonb) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_upload_claim_held(p_upload uuid, p_claim timestamp with time zone) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_upload_claim_held(p_upload uuid, p_claim timestamp with time zone) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_upsert_user(p_issuer text, p_subject text, p_email text, p_email_verified boolean, p_display_name text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_user_id() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_user_id() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_utc_text(p_at timestamp with time zone) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_version_referenced(p_file uuid, p_version integer, p_version_id uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_web_url_ok(p_url text) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.document_revisions_match() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.documents_initial_position() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.documents_revision_step() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.documents_tree_check() FROM PUBLIC;

GRANT SELECT ON TABLE fvoci.audit_log TO fvoci_app;

GRANT SELECT ON TABLE fvoci.checklist_items TO fvoci_app;

GRANT SELECT ON TABLE fvoci.comments TO fvoci_app;

GRANT SELECT ON TABLE fvoci.course_details TO fvoci_app;

GRANT SELECT ON TABLE fvoci.course_episodes TO fvoci_app;

GRANT SELECT ON TABLE fvoci.document_links TO fvoci_app;

GRANT SELECT,INSERT ON TABLE fvoci.document_revisions TO fvoci_app;

GRANT SELECT ON TABLE fvoci.document_shares TO fvoci_app;

GRANT SELECT ON TABLE fvoci.episode_progress TO fvoci_app;

GRANT SELECT ON TABLE fvoci.events TO fvoci_app;

GRANT SELECT ON TABLE fvoci.file_versions TO fvoci_app;

GRANT SELECT ON TABLE fvoci.files TO fvoci_app;

GRANT SELECT ON TABLE fvoci.group_members TO fvoci_app;

GRANT SELECT ON TABLE fvoci.groups TO fvoci_app;

GRANT SELECT ON TABLE fvoci.invitations TO fvoci_app;

GRANT SELECT ON TABLE fvoci.item_members TO fvoci_app;

GRANT SELECT ON TABLE fvoci.item_records TO fvoci_app;

GRANT SELECT ON TABLE fvoci.items TO fvoci_app;

GRANT SELECT ON TABLE fvoci.module_keys TO fvoci_app;

GRANT SELECT ON TABLE fvoci.modules TO fvoci_app;

GRANT SELECT ON TABLE fvoci.notifications TO fvoci_app;

GRANT SELECT ON TABLE fvoci.numbered_targets TO fvoci_app;

GRANT SELECT ON TABLE fvoci.ownership_offers TO fvoci_app;

GRANT SELECT ON TABLE fvoci.public_links TO fvoci_app;

GRANT SELECT ON TABLE fvoci.record_files TO fvoci_app;

GRANT SELECT ON TABLE fvoci.saved_views TO fvoci_app;

GRANT SELECT ON TABLE fvoci.study_units TO fvoci_app;

GRANT SELECT ON TABLE fvoci.task_assignees TO fvoci_app;

GRANT SELECT ON TABLE fvoci.task_dependencies TO fvoci_app;

GRANT SELECT ON TABLE fvoci.task_series TO fvoci_app;

GRANT SELECT ON TABLE fvoci.tasks TO fvoci_app;

GRANT SELECT ON TABLE fvoci.time_entry_revisions TO fvoci_app;

GRANT SELECT ON TABLE fvoci.unit_progress TO fvoci_app;

GRANT SELECT ON TABLE fvoci.upload_parts TO fvoci_app;

GRANT SELECT ON TABLE fvoci.user_marks TO fvoci_app;

GRANT SELECT ON TABLE fvoci.user_settings TO fvoci_app;

GRANT SELECT ON TABLE fvoci.users TO fvoci_app;

GRANT SELECT ON TABLE fvoci.workspace_members TO fvoci_app;

GRANT SELECT ON TABLE fvoci.workspaces TO fvoci_app;

GRANT UPDATE(name) ON TABLE fvoci.workspaces TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_access(p_kind text, p_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_access(p_kind text, p_id uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_access_as(p_kind text, p_id uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_workspace_role(p_workspace uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_organization(p_organization uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_workspace(p_workspace uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_module(p_module uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_item(p_item uuid, p_user uuid, p_ignore_trash boolean) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_under_item(p_item uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_document_at(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_document(p_doc uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_target_organization(p_kind text, p_id uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_share_document(p_doc uuid, p_email text, p_permission text, p_organization uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_share_document(p_doc uuid, p_email text, p_permission text, p_organization uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_document_audience(p_doc uuid, p_workspace uuid, p_item uuid, p_owner uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_move_preview(p_doc uuid, p_target uuid, p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_move_preview(p_doc uuid, p_target uuid, p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_notification_visible(p_document uuid, p_task uuid, p_item uuid, p_invitation uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_notification_visible(p_document uuid, p_task uuid, p_item uuid, p_invitation uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_notifications(p_organization uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_notifications(p_organization uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_notification_unread_count(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_notification_unread_count(p_organization uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_mark_notifications_read(p_id uuid, p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_mark_notifications_read(p_id uuid, p_organization uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_pending_invitations_count() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_pending_invitations_count() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_notify_pending_invitations(p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_notify_pending_invitations(p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[], p_mode text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_start_timer(p_request_id uuid, p_kind text, p_memo text, p_target_kind text, p_target uuid, p_ids uuid[], p_mode text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_item_time(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_item_time(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_task_time(p_task uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_task_time(p_task uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_tasks(p_organization uuid, p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_tasks(p_organization uuid, p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_task_counts(p_organization uuid, p_today date) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_task_counts(p_organization uuid, p_today date) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_target_activity(p_kind text, p_id uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_target_activity(p_kind text, p_id uuid, p_limit integer, p_before timestamp with time zone, p_before_id uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_recent_activity(p_organization uuid, p_limit integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_recent_activity(p_organization uuid, p_limit integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_purge_audit(p_default_days integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_purge_audit(p_default_days integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_purge_activity(p_default_days integer) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_purge_activity(p_default_days integer) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_task_statuses(p_item uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_task_statuses(p_item uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_task_status(p_item uuid, p_name text, p_color text, p_category text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_task_status(p_item uuid, p_name text, p_color text, p_category text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_update_task_status(p_status uuid, p_name text, p_color text, p_category text, p_position numeric, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_update_task_status(p_status uuid, p_name text, p_color text, p_category text, p_position numeric, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_task_status(p_status uuid, p_move_to uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_task_status(p_status uuid, p_move_to uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_search_enqueue_status_tasks() FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_base jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_task_recurrence(p_task uuid, p_rule jsonb, p_timezone text, p_base jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_storage_used_organization(p_organization uuid, p_except uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_file(p_file uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_grade_comment(p_comment uuid, p_user uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_is_server_admin() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_is_server_admin() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_require_server_admin() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_require_server_admin() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_require_organization_admin(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_require_organization_admin(p_organization uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_shares_organization(other uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_shares_organization(other uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_activity(p_id uuid, p_workspace uuid, p_item uuid, p_target_kind text, p_target uuid, p_kind text, p_detail jsonb) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_task_status(p_status uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_task_status_by_name(p_item uuid, p_name text) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_default_task_status(p_item uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_seed_task_statuses(p_item uuid, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_create_organization(p_request_id uuid, p_name text, p_first_admin uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_organization(p_request_id uuid, p_name text, p_first_admin uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_organizations() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_organizations() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_server_organizations() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_server_organizations() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_server_admins() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_server_admins() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_organization_members(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_organization_members(p_organization uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_organization_workspaces(p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_organization_workspaces(p_organization uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_organization_member(p_organization uuid, p_user uuid, p_role text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_organization_member(p_organization uuid, p_user uuid, p_role text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_organization_member_preview(p_organization uuid, p_user uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_organization_member_preview(p_organization uuid, p_user uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_organization_member_ids(p_organization uuid, p_user uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_organization_member_ids(p_organization uuid, p_user uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_organization_member(p_organization uuid, p_user uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_organization_member(p_organization uuid, p_user uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_invite_organization(p_organization uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_invite_organization(p_organization uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_notify_invitation(p_id uuid, p_invitation uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_set_organization_settings(p_organization uuid, p_patch jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_organization_settings(p_organization uuid, p_patch jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_set_workspace_limits(p_workspace uuid, p_patch jsonb, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_workspace_limits(p_workspace uuid, p_patch jsonb, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_effective_limit(p_workspace uuid, p_name text, p_default bigint) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_effective_limit(p_workspace uuid, p_name text, p_default bigint) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_retention_days(p_workspace uuid, p_organization uuid, p_name text, p_default integer) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_join_workspace_as_admin(p_workspace uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_join_workspace_as_admin(p_workspace uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_claim_initial_admin(p_user uuid, p_name text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_claim_initial_admin(p_user uuid, p_name text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_create_workspace(p_request_id uuid, p_organization uuid, p_name text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_workspace(p_request_id uuid, p_organization uuid, p_name text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_invite(p_workspace uuid, p_email text, p_role text, p_ttl_secs integer, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_invitations() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_invitations() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_respond_invitation(p_invitation uuid, p_accept boolean, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_respond_invitation(p_invitation uuid, p_accept boolean, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_revoke_invitation(p_invitation uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_revoke_invitation(p_invitation uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text, p_ids uuid[]) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_remove_member_preview(p_workspace uuid, p_member uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_member_preview(p_workspace uuid, p_member uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_member_ids(p_workspace uuid, p_member uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_member_ids(p_workspace uuid, p_member uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_leave_workspace_ids(p_workspace uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_leave_workspace_ids(p_workspace uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_delete_workspace(p_ws uuid, p_name text, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_delete_workspace(p_ws uuid, p_name text, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_restore_workspace(p_ws uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_restore_workspace(p_ws uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_my_deleted_workspaces() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_my_deleted_workspaces() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_offer_ownership(p_workspace uuid, p_to uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_offer_ownership(p_workspace uuid, p_to uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_recurrence_due() FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_recurrence_due() TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_sync_task_series(p_series uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_sync_task_series(p_series uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_system_sync_task_series_ids(p_series uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_sync_task_series_ids(p_series uuid) TO fvoci_app;

GRANT SELECT ON TABLE fvoci.activity TO fvoci_app;

GRANT SELECT ON TABLE fvoci.organization_members TO fvoci_app;

GRANT SELECT ON TABLE fvoci.organizations TO fvoci_app;

GRANT SELECT ON TABLE fvoci.server_admins TO fvoci_app;

GRANT SELECT ON TABLE fvoci.task_statuses TO fvoci_app;

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;

-- 앱은 원장 SELECT 권한 없이 카탈로그 주석으로 기준 세대를 확인한다.
-- 실제 버전·체크섬 검증은 migrator의 SQLx 검사를 그대로 사용한다.
COMMENT ON TABLE fvoci._sqlx_migrations IS 'fvoci baseline 202610250001';
