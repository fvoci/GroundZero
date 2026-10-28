-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

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

CREATE TABLE fvoci.storage_deletions (
    storage_key text NOT NULL,
    storage_upload_id text DEFAULT ''::text NOT NULL,
    queued_at timestamp with time zone DEFAULT now() NOT NULL,
    attempts integer DEFAULT 0 NOT NULL
);

ALTER TABLE fvoci.storage_deletions OWNER TO fvoci_migrator;

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
      RAISE EXCEPTION 'empty files cannot be uploaded' USING ERRCODE = '22023',
        DETAIL = jsonb_build_object('field','size','reason','out_of_range','limit',NULL)::text;
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
      RAISE EXCEPTION 'the file is larger than the limit (% bytes)', v_limit USING ERRCODE = '22023',
        DETAIL = jsonb_build_object('field','size','reason','out_of_range','limit',v_limit)::text;
    END IF;
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023',
        DETAIL = jsonb_build_object('field','name','reason',
          CASE WHEN length(v_name) = 0 THEN 'required'
               WHEN length(v_name) > 255 THEN 'too_long'
               WHEN v_name ~ '[\x00-\x1f]' THEN 'control_characters'
               ELSE 'invalid_format' END,
          'limit', CASE WHEN length(v_name) > 255 THEN 255 ELSE NULL END)::text;
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

CREATE FUNCTION fvoci.app_release_upload_claim(p_upload uuid, p_claim timestamp with time zone) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE uploads SET state_changed_at = now() - interval '5 minutes' - interval '1 second'
    WHERE id = p_upload AND user_id = app_user_id() AND state = 'completing' AND state_changed_at = p_claim
  $$;


ALTER FUNCTION fvoci.app_release_upload_claim(p_upload uuid, p_claim timestamp with time zone) OWNER TO fvoci_migrator;

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
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023',
        DETAIL = jsonb_build_object('field','name','reason',
          CASE WHEN length(v_name) = 0 THEN 'required'
               WHEN length(v_name) > 255 THEN 'too_long'
               WHEN v_name ~ '[\x00-\x1f]' THEN 'control_characters'
               ELSE 'invalid_format' END,
          'limit', CASE WHEN length(v_name) > 255 THEN 255 ELSE NULL END)::text;
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

CREATE FUNCTION fvoci.app_set_storage_upload(p_upload uuid, p_storage_upload text) RETURNS void
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    UPDATE uploads SET storage_upload_id = p_storage_upload
    WHERE id = p_upload AND user_id = app_user_id() AND storage_upload_id IS NULL
  $$;


ALTER FUNCTION fvoci.app_set_storage_upload(p_upload uuid, p_storage_upload text) OWNER TO fvoci_migrator;

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

CREATE FUNCTION fvoci.app_upload_claim_held(p_upload uuid, p_claim timestamp with time zone) RETURNS boolean
    LANGUAGE sql SECURITY DEFINER
    SET search_path TO 'fvoci', 'pg_temp'
    AS $$
    SELECT coalesce((SELECT state = 'done' OR (state = 'completing' AND state_changed_at = p_claim)
                     FROM uploads WHERE id = p_upload AND user_id = app_user_id() FOR NO KEY UPDATE), false)
  $$;


ALTER FUNCTION fvoci.app_upload_claim_held(p_upload uuid, p_claim timestamp with time zone) OWNER TO fvoci_migrator;

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

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_file_id_version_key UNIQUE (file_id, version);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.file_versions
    ADD CONSTRAINT file_versions_storage_key_key UNIQUE (storage_key);

ALTER TABLE ONLY fvoci.files
    ADD CONSTRAINT files_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.storage_deletions
    ADD CONSTRAINT storage_deletions_pkey PRIMARY KEY (storage_key, storage_upload_id);

ALTER TABLE ONLY fvoci.upload_parts
    ADD CONSTRAINT upload_parts_pkey PRIMARY KEY (upload_id, number);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_pkey PRIMARY KEY (id);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_request UNIQUE (user_id, request_id);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_storage_key_key UNIQUE (storage_key);

CREATE INDEX files_document ON fvoci.files USING btree (document_id) WHERE (document_id IS NOT NULL);

CREATE INDEX files_item ON fvoci.files USING btree (item_id) WHERE (item_id IS NOT NULL);

CREATE INDEX files_owner ON fvoci.files USING btree (owner_user_id) WHERE (owner_user_id IS NOT NULL);

CREATE INDEX files_trash ON fvoci.files USING btree (trash_batch_id) WHERE (trash_batch_id IS NOT NULL);

CREATE INDEX files_workspace ON fvoci.files USING btree (workspace_id);

CREATE INDEX uploads_open ON fvoci.uploads USING btree (expires_at) WHERE (state = ANY (ARRAY['uploading'::text, 'completing'::text]));

CREATE INDEX uploads_place ON fvoci.uploads USING btree (workspace_id, owner_user_id) WHERE (state = ANY (ARRAY['uploading'::text, 'completing'::text]));

CREATE TRIGGER file_versions_pinned BEFORE DELETE ON fvoci.file_versions FOR EACH ROW EXECUTE FUNCTION fvoci.app_guard_pinned_version();

CREATE TRIGGER file_versions_storage AFTER DELETE ON fvoci.file_versions FOR EACH ROW EXECUTE FUNCTION fvoci.app_queue_version_deletion();

CREATE TRIGGER files_search AFTER INSERT OR DELETE OR UPDATE OF name, deleted_at, current_version, workspace_id, owner_user_id ON fvoci.files FOR EACH ROW EXECUTE FUNCTION fvoci.app_search_enqueue('file');

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

ALTER TABLE ONLY fvoci.upload_parts
    ADD CONSTRAINT upload_parts_upload_id_fkey FOREIGN KEY (upload_id) REFERENCES fvoci.uploads(id) ON DELETE CASCADE;

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_user_id_fkey FOREIGN KEY (user_id) REFERENCES fvoci.users(id);

ALTER TABLE ONLY fvoci.uploads
    ADD CONSTRAINT uploads_version_id_fkey FOREIGN KEY (version_id) REFERENCES fvoci.file_versions(id);

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;
