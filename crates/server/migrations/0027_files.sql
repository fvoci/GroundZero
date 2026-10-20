-- 파일 첨부·버전·업로드 (D-06, D-35, D-39, D-52, D-56).
--
-- 파일은 문서나 항목에 붙고 그 권한을 그대로 따른다(문서를 공유받은 사람은 첨부도 읽고, 비공개 항목의 첨부는
-- 항목을 볼 수 있는 사람만). 개인 노트에 붙은 파일은 소유자만. 같은 파일에 새 내용을 올리면 v2, v3…으로 쌓이고
-- 이전 버전은 남는다(업로더·시각·설명). 업로드는 여러 조각으로 나눠 보내고 끊기면 이어 올린다. 마칠 때 서버가
-- 저장된 내용 전체를 다시 읽어 SHA-256과 크기를 확인한다. 같은 요청 id로 다시 만들면 같은 업로드를 돌려준다.
-- 파일 내용은 DB가 아니라 저장소(로컬·S3)에 있으므로, 행을 지우면 키를 지울 목록(storage_deletions)에 올리고
-- 백그라운드 작업이 지운다. 분석 기록이 아직 없으므로 지금은 모든 버전을 휴지통 만료·영구 삭제로 지운다
-- (D-56의 "참조 중인 버전 보호"는 분석 기록 조각에서 더한다).

CREATE TABLE files (
  id              uuid PRIMARY KEY,
  workspace_id    uuid REFERENCES workspaces (id),
  owner_user_id   uuid REFERENCES users (id),
  document_id     uuid REFERENCES documents (id),
  item_id         uuid,
  name            text NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 255 AND name !~ '[\x00-\x1f/\\]'),
  current_version integer NOT NULL DEFAULT 0,
  created_by      uuid NOT NULL REFERENCES users (id),
  created_at      timestamptz NOT NULL DEFAULT now(),
  updated_at      timestamptz NOT NULL DEFAULT now(),
  deleted_at      timestamptz,
  trash_batch_id  uuid REFERENCES trash_batches (id),
  CONSTRAINT files_place CHECK ((workspace_id IS NULL) <> (owner_user_id IS NULL)),
  CONSTRAINT files_target CHECK ((document_id IS NULL) <> (item_id IS NULL)),
  CONSTRAINT files_item_workspace FOREIGN KEY (item_id, workspace_id) REFERENCES items (id, workspace_id)
);
CREATE INDEX files_document ON files (document_id) WHERE document_id IS NOT NULL;
CREATE INDEX files_item ON files (item_id) WHERE item_id IS NOT NULL;
CREATE INDEX files_workspace ON files (workspace_id);
CREATE INDEX files_owner ON files (owner_user_id) WHERE owner_user_id IS NOT NULL;
CREATE INDEX files_trash ON files (trash_batch_id) WHERE trash_batch_id IS NOT NULL;

CREATE TABLE file_versions (
  id           uuid PRIMARY KEY,
  file_id      uuid NOT NULL REFERENCES files (id),
  version      integer NOT NULL CHECK (version > 0),
  size         bigint NOT NULL CHECK (size >= 0),
  sha256       text NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
  content_type text NOT NULL,
  storage_key  text NOT NULL UNIQUE,
  note         text NOT NULL DEFAULT '',
  uploaded_by  uuid NOT NULL REFERENCES users (id),
  created_at   timestamptz NOT NULL DEFAULT now(),
  UNIQUE (file_id, version)
);

-- 업로드 세션. 대상(새 파일이면 문서·항목, 새 버전이면 파일)은 외래 키 없이 기록하고 마칠 때 다시 확인한다.
CREATE TABLE uploads (
  id                uuid PRIMARY KEY,
  user_id           uuid NOT NULL REFERENCES users (id),
  request_id        uuid NOT NULL,
  file_id           uuid,
  target_kind       text CHECK (target_kind IN ('document', 'item')),
  target_id         uuid,
  workspace_id      uuid,
  owner_user_id     uuid,
  name              text NOT NULL,
  note              text NOT NULL DEFAULT '',
  content_type      text NOT NULL,
  size              bigint NOT NULL CHECK (size >= 0),
  chunk_bytes       bigint NOT NULL CHECK (chunk_bytes > 0),
  storage_key       text NOT NULL UNIQUE,
  storage_upload_id text,
  state             text NOT NULL DEFAULT 'uploading'
                    CHECK (state IN ('uploading', 'completing', 'done', 'aborted', 'expired', 'failed')),
  state_changed_at  timestamptz NOT NULL DEFAULT now(),
  failure           text,
  version_id        uuid REFERENCES file_versions (id),
  created_at        timestamptz NOT NULL DEFAULT now(),
  expires_at        timestamptz NOT NULL,
  CONSTRAINT uploads_request UNIQUE (user_id, request_id),
  CONSTRAINT uploads_target CHECK ((file_id IS NULL) = (target_kind IS NOT NULL AND target_id IS NOT NULL))
);
CREATE INDEX uploads_open ON uploads (expires_at) WHERE state IN ('uploading', 'completing');
CREATE INDEX uploads_place ON uploads (workspace_id, owner_user_id) WHERE state IN ('uploading', 'completing');

CREATE TABLE upload_parts (
  upload_id uuid NOT NULL REFERENCES uploads (id) ON DELETE CASCADE,
  number    integer NOT NULL CHECK (number BETWEEN 1 AND 10000),
  size      bigint NOT NULL CHECK (size > 0),
  sha256    text NOT NULL CHECK (sha256 ~ '^[0-9a-f]{64}$'),
  etag      text NOT NULL,
  PRIMARY KEY (upload_id, number)
);

-- 저장소에서 지울 것(완성본 키, 또는 마치지 않은 업로드). 백그라운드 작업이 비운다.
CREATE TABLE storage_deletions (
  storage_key       text NOT NULL,
  storage_upload_id text NOT NULL DEFAULT '',
  queued_at         timestamptz NOT NULL DEFAULT now(),
  attempts          integer NOT NULL DEFAULT 0,
  PRIMARY KEY (storage_key, storage_upload_id)
);

ALTER TABLE trash_batches DROP CONSTRAINT trash_batches_root_kind_check;
ALTER TABLE trash_batches ADD CONSTRAINT trash_batches_root_kind_check
  CHECK (root_kind IN ('item', 'document', 'task', 'event', 'file'));

------------------------------------------------------------------------------
-- 권한
------------------------------------------------------------------------------

-- 파일 권한: 붙은 문서·항목의 권한('owner'·'write'·'read'). 지웠거나 볼 수 없으면 NULL.
CREATE FUNCTION app_file_access(p_file uuid) RETURNS text
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_file files%ROWTYPE;
  BEGIN
    SELECT * INTO v_file FROM files WHERE id = p_file;
    IF v_file.id IS NULL OR v_file.deleted_at IS NOT NULL THEN
      RETURN NULL;
    END IF;
    IF v_file.document_id IS NOT NULL THEN
      RETURN app_document_access(v_file.document_id);
    END IF;
    RETURN app_item_access(v_file.item_id);
  END
  $$;

-- 올릴 곳 확인: 쓸 수 있어야 하고 보관된 항목에는 올릴 수 없다. (workspace_id, owner_user_id)를 돌려준다.
CREATE FUNCTION app_file_place(p_kind text, p_id uuid, OUT workspace_id uuid, OUT owner_user_id uuid)
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text;
    v_item   uuid;
  BEGIN
    IF p_kind = 'document' THEN
      v_access := app_document_access(p_id);
      SELECT d.workspace_id, d.owner_user_id, d.item_id INTO workspace_id, owner_user_id, v_item
      FROM documents d WHERE d.id = p_id;
    ELSIF p_kind = 'item' THEN
      v_access := app_item_access(p_id);
      SELECT i.workspace_id, NULL::uuid, i.id INTO workspace_id, owner_user_id, v_item FROM items i WHERE i.id = p_id;
    ELSE
      RAISE EXCEPTION 'unknown target' USING ERRCODE = '22023';
    END IF;
    IF v_access IS NULL THEN
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

-- 사용 중인 저장량(워크스페이스, 또는 개인 영역): 모든 버전(휴지통 포함) + 진행 중인 업로드.
CREATE FUNCTION app_storage_used(p_workspace uuid, p_owner uuid, p_except uuid) RETURNS bigint
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT coalesce((SELECT sum(v.size) FROM file_versions v JOIN files f ON f.id = v.file_id
                     WHERE f.workspace_id IS NOT DISTINCT FROM p_workspace
                       AND f.owner_user_id IS NOT DISTINCT FROM p_owner), 0)
         + coalesce((SELECT sum(u.size) FROM uploads u
                     WHERE u.state IN ('uploading', 'completing') AND u.id IS DISTINCT FROM p_except
                       AND u.workspace_id IS NOT DISTINCT FROM p_workspace
                       AND u.owner_user_id IS NOT DISTINCT FROM p_owner), 0)
  $$;

------------------------------------------------------------------------------
-- 업로드
------------------------------------------------------------------------------

-- 업로드 시작. p_file이 있으면 그 파일의 새 버전, 없으면 p_kind·p_target에 새 파일. 같은 요청 id면 같은 업로드.
CREATE FUNCTION app_create_upload(
  p_request uuid, p_file uuid, p_kind text, p_target uuid, p_name text, p_note text, p_type text,
  p_size bigint, p_chunk bigint, p_ttl_secs integer, p_max_file bigint, p_max_storage bigint)
  RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_user  uuid := app_user_id();
    v_id    uuid;
    v_file  files%ROWTYPE;
    v_place record;
    v_kind  text := p_kind;
    v_tgt   uuid := p_target;
    v_name  text := btrim(coalesce(p_name, ''));
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
    IF p_size > p_max_file THEN
      RAISE EXCEPTION 'the file is larger than the limit (% bytes)', p_max_file USING ERRCODE = '22023';
    END IF;
    IF p_file IS NOT NULL THEN
      SELECT * INTO v_file FROM files WHERE id = p_file;
      IF app_file_access(p_file) IS NULL THEN
        RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
      END IF;
      v_kind := CASE WHEN v_file.document_id IS NOT NULL THEN 'document' ELSE 'item' END;
      v_tgt := coalesce(v_file.document_id, v_file.item_id);
      IF v_name = '' THEN
        v_name := v_file.name;
      END IF;
    END IF;
    SELECT * INTO v_place FROM app_file_place(v_kind, v_tgt);
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023';
    END IF;
    -- 같은 영역의 업로드 시작을 줄 세워 저장량 확인이 겹치지 않게 한다.
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage:' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text, 0));
    IF app_storage_used(v_place.workspace_id, v_place.owner_user_id, NULL) + p_size > p_max_storage THEN
      RAISE EXCEPTION 'not enough storage left (limit % bytes)', p_max_storage USING ERRCODE = '55000';
    END IF;
    v_id := uuidv7();
    INSERT INTO uploads (id, user_id, request_id, file_id, target_kind, target_id, workspace_id, owner_user_id,
                         name, note, content_type, size, chunk_bytes, storage_key, expires_at)
    VALUES (v_id, v_user, p_request, p_file,
            CASE WHEN p_file IS NULL THEN v_kind END, CASE WHEN p_file IS NULL THEN v_tgt END,
            v_place.workspace_id, v_place.owner_user_id, v_name, coalesce(p_note, ''),
            coalesce(nullif(btrim(p_type), ''), 'application/octet-stream'), p_size, p_chunk,
            'v1/' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text || '/' || v_id::text,
            now() + make_interval(secs => p_ttl_secs));
    RETURN v_id;
  END
  $$;

-- 내 업로드(진행 중인 것을 이어 올릴 때). 다른 사람의 업로드는 없는 것과 같다.
CREATE FUNCTION app_my_upload(p_upload uuid) RETURNS uploads
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

-- 저장소 업로드 id 기록(처음 한 번).
CREATE FUNCTION app_set_storage_upload(p_upload uuid, p_storage_upload text) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    UPDATE uploads SET storage_upload_id = p_storage_upload
    WHERE id = p_upload AND user_id = app_user_id() AND storage_upload_id IS NULL
  $$;

-- 받은 조각 기록. 같은 번호는 바꾼다.
CREATE FUNCTION app_record_part(p_upload uuid, p_number integer, p_size bigint, p_sha256 text, p_etag text)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

-- 마치기 시작: 'uploading' → 'completing'. 이미 끝났으면 그대로 돌려준다(같은 요청을 두 번 보내도 한 번만).
-- 5분 넘게 'completing'이면(중간에 멈춤) 다시 맡는다.
CREATE FUNCTION app_begin_complete(p_upload uuid) RETURNS uploads
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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
    UPDATE uploads SET state = 'completing', state_changed_at = now() WHERE id = p_upload RETURNING * INTO v_up;
    RETURN v_up;
  END
  $$;

-- 마치기 끝: 확인한 SHA-256으로 파일·버전을 만든다. 대상 권한·저장량은 다시 본다.
CREATE FUNCTION app_finish_upload(p_upload uuid, p_sha256 text, p_max_storage bigint) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_up      uploads%ROWTYPE;
    v_file    uuid;
    v_version integer;
    v_vid     uuid;
    v_place   record;
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
      IF app_file_access(v_up.file_id) IS NULL THEN
        RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
      END IF;
      SELECT * INTO v_place FROM app_file_place(
        (SELECT CASE WHEN document_id IS NOT NULL THEN 'document' ELSE 'item' END FROM files WHERE id = v_up.file_id),
        (SELECT coalesce(document_id, item_id) FROM files WHERE id = v_up.file_id));
    ELSE
      SELECT * INTO v_place FROM app_file_place(v_up.target_kind, v_up.target_id);
    END IF;
    PERFORM pg_advisory_xact_lock(hashtextextended('fvoci-storage:' || coalesce(v_place.workspace_id, v_place.owner_user_id)::text, 0));
    IF app_storage_used(v_place.workspace_id, v_place.owner_user_id, p_upload) + v_up.size > p_max_storage THEN
      RAISE EXCEPTION 'not enough storage left (limit % bytes)', p_max_storage USING ERRCODE = '55000';
    END IF;
    IF v_up.file_id IS NULL THEN
      INSERT INTO files (workspace_id, owner_user_id, document_id, item_id, name, created_by)
      VALUES (v_place.workspace_id, v_place.owner_user_id,
              CASE WHEN v_up.target_kind = 'document' THEN v_up.target_id END,
              CASE WHEN v_up.target_kind = 'item' THEN v_up.target_id END,
              v_up.name, v_up.user_id)
      RETURNING id INTO v_file;
    ELSE
      v_file := v_up.file_id;
    END IF;
    UPDATE files SET current_version = current_version + 1, updated_at = now()
    WHERE id = v_file RETURNING current_version INTO v_version;
    INSERT INTO file_versions (file_id, version, size, sha256, content_type, storage_key, note, uploaded_by)
    VALUES (v_file, v_version, v_up.size, p_sha256, v_up.content_type, v_up.storage_key, v_up.note, v_up.user_id)
    RETURNING id INTO v_vid;
    UPDATE uploads SET state = 'done', state_changed_at = now(), version_id = v_vid WHERE id = p_upload;
    DELETE FROM upload_parts WHERE upload_id = p_upload;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_place.workspace_id, v_place.owner_user_id, v_up.user_id, 'upload', 'file', v_file,
            jsonb_build_object('name', v_up.name, 'version', v_version, 'size', v_up.size));
    RETURN v_vid;
  END
  $$;

-- 마치기 실패(내용이 맞지 않음 등): 업로드를 닫고 저장소의 내용은 지울 목록에 올린다.
CREATE FUNCTION app_fail_upload(p_upload uuid, p_reason text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

-- 그만두기(올린 사람). 받은 조각은 지울 목록에 올린다.
CREATE FUNCTION app_abort_upload(p_upload uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

------------------------------------------------------------------------------
-- 파일 다루기
------------------------------------------------------------------------------

CREATE FUNCTION app_rename_file(p_file uuid, p_name text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_file_access(p_file);
    v_file   files%ROWTYPE;
    v_name   text := btrim(coalesce(p_name, ''));
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    IF length(v_name) NOT BETWEEN 1 AND 255 OR v_name ~ '[\x00-\x1f/\\]' THEN
      RAISE EXCEPTION 'invalid file name' USING ERRCODE = '22023';
    END IF;
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    UPDATE files SET name = v_name, updated_at = now() WHERE id = p_file;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'rename', 'file', p_file,
            jsonb_build_object('from', v_file.name, 'to', v_name));
  END
  $$;

-- 휴지통으로(쓸 수 있는 사람).
CREATE FUNCTION app_trash_file(p_file uuid) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_file_access(p_file);
    v_file   files%ROWTYPE;
    v_batch  uuid;
  BEGIN
    IF v_access IS NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    IF v_access NOT IN ('owner', 'write') THEN
      RAISE EXCEPTION 'read-only access' USING ERRCODE = '42501';
    END IF;
    SELECT * INTO v_file FROM files WHERE id = p_file FOR NO KEY UPDATE;
    IF v_file.deleted_at IS NOT NULL THEN
      RAISE EXCEPTION 'file not found' USING ERRCODE = 'P0002';
    END IF;
    INSERT INTO trash_batches (workspace_id, owner_user_id, root_kind, root_id, title, member_count, deleted_by)
    VALUES (v_file.workspace_id, v_file.owner_user_id, 'file', p_file, v_file.name, 1, app_user_id())
    RETURNING id INTO v_batch;
    UPDATE files SET deleted_at = now(), trash_batch_id = v_batch WHERE id = p_file;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_file.workspace_id, v_file.owner_user_id, app_user_id(), 'delete', 'file', p_file,
            jsonb_build_object('name', v_file.name, 'batch', v_batch));
    RETURN v_batch;
  END
  $$;

------------------------------------------------------------------------------
-- 영구 삭제와 저장소 정리
------------------------------------------------------------------------------

-- 버전 행이 지워지면 저장소의 내용도 지울 목록에.
CREATE FUNCTION app_queue_version_deletion() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO storage_deletions (storage_key) VALUES (OLD.storage_key) ON CONFLICT DO NOTHING;
    RETURN OLD;
  END
  $$;
CREATE TRIGGER file_versions_storage AFTER DELETE ON file_versions
  FOR EACH ROW EXECUTE FUNCTION app_queue_version_deletion();

-- 파일 행을 지우면 버전도(업로드 기록의 버전 연결은 끊는다).
CREATE FUNCTION app_delete_file_rows(p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF coalesce(cardinality(p_ids), 0) = 0 THEN
      RETURN;
    END IF;
    UPDATE uploads SET version_id = NULL
    WHERE version_id IN (SELECT id FROM file_versions WHERE file_id = ANY (p_ids));
    DELETE FROM file_versions WHERE file_id = ANY (p_ids);
    DELETE FROM files WHERE id = ANY (p_ids);
  END
  $$;

-- 문서·항목을 영구 삭제하면 붙은 파일도(휴지통·워크스페이스 영구 삭제가 모두 이 길을 지난다).
CREATE FUNCTION app_delete_attached_files() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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
CREATE TRIGGER documents_files BEFORE DELETE ON documents
  FOR EACH ROW EXECUTE FUNCTION app_delete_attached_files('document');
CREATE TRIGGER items_files BEFORE DELETE ON items
  FOR EACH ROW EXECUTE FUNCTION app_delete_attached_files('item');

-- 작업: 저장소에서 지울 것을 꺼낸다(지우고 나서 성공하면 그대로, 실패하면 다시 넣는다).
CREATE FUNCTION app_system_storage_claim(p_limit integer)
  RETURNS TABLE (storage_key text, storage_upload_id text, attempts integer)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    DELETE FROM storage_deletions d
    WHERE (d.storage_key, d.storage_upload_id) IN (
      SELECT s.storage_key, s.storage_upload_id FROM storage_deletions s
      WHERE app_user_id() IS NULL AND s.attempts < 10
      ORDER BY s.queued_at LIMIT greatest(1, least(p_limit, 500)) FOR UPDATE SKIP LOCKED)
    RETURNING d.storage_key, d.storage_upload_id, d.attempts
  $$;

CREATE FUNCTION app_system_storage_retry(p_key text, p_upload text, p_attempts integer) RETURNS void
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    INSERT INTO storage_deletions (storage_key, storage_upload_id, attempts, queued_at)
    SELECT p_key, p_upload, p_attempts + 1, now() WHERE app_user_id() IS NULL
    ON CONFLICT DO NOTHING
  $$;

-- 작업: 기한이 지난 업로드를 닫고 받은 조각을 지울 목록에 올린다.
CREATE FUNCTION app_system_expire_uploads() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_count integer;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    WITH gone AS (
      UPDATE uploads SET state = 'expired', state_changed_at = now()
      WHERE state IN ('uploading', 'completing') AND expires_at <= now()
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

------------------------------------------------------------------------------
-- 휴지통 묶음에 파일 더하기
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_restore_batch(p_batch uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_doc   documents%ROWTYPE;
    v_item  uuid;
    v_ids   uuid[];
  BEGIN
    SELECT * INTO v_batch FROM trash_batches WHERE id = p_batch FOR UPDATE;
    IF v_batch.id IS NULL OR app_trash_role(v_batch) IS NULL THEN
      RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
    END IF;
    IF app_trash_expires_at(v_batch) <= now() THEN
      RAISE EXCEPTION 'the retention period has passed' USING ERRCODE = '55000';
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
      SELECT item_id INTO v_item FROM files WHERE id = v_batch.root_id FOR NO KEY UPDATE;
      -- 붙은 문서가 휴지통에 있으면 먼저 문서를 되살려야 한다.
      IF EXISTS (SELECT 1 FROM files f JOIN documents d ON d.id = f.document_id
                 WHERE f.id = v_batch.root_id AND d.deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the document first' USING ERRCODE = '55000';
      END IF;
    ELSE
      PERFORM 1 FROM items WHERE id = v_batch.root_id FOR NO KEY UPDATE;
    END IF;
    IF v_item IS NOT NULL THEN
      PERFORM app_lock_item(v_item);
      -- 항목이 휴지통에 있으면 먼저 항목을 되살려야 한다. 보관된 항목에는 되살리지 않는다(지우기와 같은 규칙).
      IF EXISTS (SELECT 1 FROM items WHERE id = v_item AND deleted_at IS NOT NULL) THEN
        RAISE EXCEPTION 'restore the item first' USING ERRCODE = '55000';
      END IF;
      IF app_item_archived(v_item) THEN
        RAISE EXCEPTION 'the item is archived' USING ERRCODE = '55000';
      END IF;
      -- 지운 사람은 지금도 그 항목에 쓸 수 있어야 한다(볼 수 없게 됐으면 없는 것과 같다).
      IF app_trash_role(v_batch) = 'deleter' AND app_item_access(v_item) IS DISTINCT FROM 'owner'
         AND app_item_access(v_item) IS DISTINCT FROM 'write' THEN
        RAISE EXCEPTION 'trash entry not found' USING ERRCODE = 'P0002';
      END IF;
    END IF;
    IF v_batch.root_kind = 'document' THEN
      UPDATE documents SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE documents d SET parent_id = NULL
      WHERE d.id = v_batch.root_id AND d.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM documents p WHERE p.id = d.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit_document(v_batch.root_id, 'restore', jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      UPDATE tasks SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      UPDATE tasks t SET parent_id = NULL
      WHERE t.id = v_batch.root_id AND t.parent_id IS NOT NULL
        AND EXISTS (SELECT 1 FROM tasks p WHERE p.id = t.parent_id AND p.deleted_at IS NOT NULL);
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'task', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'item' THEN
      UPDATE items SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'item', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    ELSIF v_batch.root_kind = 'file' THEN
      UPDATE files SET deleted_at = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
      VALUES (v_batch.workspace_id, v_batch.owner_user_id, app_user_id(), 'restore', 'file', v_batch.root_id,
              jsonb_build_object('batch', p_batch));
    ELSE
      UPDATE events SET deleted_at = NULL, deleted_by = NULL, trash_batch_id = NULL WHERE trash_batch_id = p_batch;
      PERFORM app_audit(v_batch.workspace_id, 'restore', 'event', v_batch.root_id,
                        jsonb_build_object('batch', p_batch));
    END IF;
    UPDATE trash_batches SET restored_at = now() WHERE id = p_batch;
  END
  $$;

CREATE OR REPLACE FUNCTION app_purge_batch_as(p_batch uuid, p_system boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_batch trash_batches%ROWTYPE;
    v_item  uuid;
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
    IF v_batch.root_kind = 'document' THEN
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'task' THEN
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE trash_batch_id = p_batch));
    ELSIF v_batch.root_kind = 'event' THEN
      DELETE FROM events WHERE trash_batch_id = p_batch;
    ELSIF v_batch.root_kind = 'file' THEN
      PERFORM app_delete_file_rows(ARRAY(SELECT id FROM files WHERE trash_batch_id = p_batch));
    ELSE
      v_item := v_batch.root_id;
      PERFORM app_purge_tasks(ARRAY(SELECT id FROM tasks WHERE item_id = v_item));
      DELETE FROM task_dependencies WHERE item_id = v_item;
      DELETE FROM task_series WHERE item_id = v_item;
      PERFORM app_purge_documents(ARRAY(SELECT id FROM documents WHERE item_id = v_item));
      DELETE FROM events WHERE item_id = v_item;
      DELETE FROM user_marks WHERE target_kind = 'item' AND target_id = v_item;
      -- 이 항목 안의 것이 다른 휴지통 묶음에 있었으면 그 묶음도 함께 끝난다.
      UPDATE trash_batches SET purged_at = now()
      WHERE id <> p_batch AND purged_at IS NULL AND restored_at IS NULL AND workspace_id = v_batch.workspace_id
        AND NOT EXISTS (SELECT 1 FROM documents WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM tasks WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM events WHERE trash_batch_id = trash_batches.id)
        AND NOT EXISTS (SELECT 1 FROM files WHERE trash_batch_id = trash_batches.id)
        AND root_kind <> 'item';
      DELETE FROM items WHERE id = v_item;
    END IF;
    UPDATE trash_batches SET purged_at = now() WHERE id = p_batch;
    INSERT INTO audit_log (workspace_id, owner_user_id, actor, action, target_type, target_id, detail)
    VALUES (v_batch.workspace_id, v_batch.owner_user_id, CASE WHEN p_system THEN NULL ELSE app_user_id() END, 'purge',
            v_batch.root_kind, v_batch.root_id,
            jsonb_build_object('batch', p_batch, 'title', v_batch.title, 'count', v_batch.member_count)
              || CASE WHEN p_system THEN jsonb_build_object('system', true, 'reason', 'retention')
                      ELSE '{}'::jsonb END);
  END
  $$;

------------------------------------------------------------------------------
-- 행 보안·권한
------------------------------------------------------------------------------

ALTER TABLE files ENABLE ROW LEVEL SECURITY;
CREATE POLICY files_select ON files FOR SELECT USING (app_file_access(id) IS NOT NULL);
ALTER TABLE file_versions ENABLE ROW LEVEL SECURITY;
CREATE POLICY file_versions_select ON file_versions FOR SELECT USING (app_file_access(file_id) IS NOT NULL);
ALTER TABLE uploads ENABLE ROW LEVEL SECURITY;
CREATE POLICY uploads_select ON uploads FOR SELECT USING (user_id = app_user_id());
ALTER TABLE upload_parts ENABLE ROW LEVEL SECURITY;
CREATE POLICY upload_parts_select ON upload_parts FOR SELECT
  USING (EXISTS (SELECT 1 FROM uploads u WHERE u.id = upload_id AND u.user_id = app_user_id()));
ALTER TABLE storage_deletions ENABLE ROW LEVEL SECURITY;
CREATE POLICY storage_deletions_none ON storage_deletions FOR SELECT USING (false);

GRANT SELECT ON files, file_versions, uploads, upload_parts TO fvoci_app;

REVOKE ALL ON FUNCTION app_file_access(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_file_place(text, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_storage_used(uuid, uuid, uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_upload(uuid, uuid, text, uuid, text, text, text, bigint, bigint, integer, bigint, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_my_upload(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_storage_upload(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_record_part(uuid, integer, bigint, text, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_begin_complete(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_finish_upload(uuid, text, bigint) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_fail_upload(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_abort_upload(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_rename_file(uuid, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_trash_file(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_queue_version_deletion() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_file_rows(uuid[]) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_attached_files() FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_storage_claim(integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_storage_retry(text, text, integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_system_expire_uploads() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_file_access(uuid),
  app_create_upload(uuid, uuid, text, uuid, text, text, text, bigint, bigint, integer, bigint, bigint),
  app_my_upload(uuid),
  app_set_storage_upload(uuid, text),
  app_record_part(uuid, integer, bigint, text, text),
  app_begin_complete(uuid),
  app_finish_upload(uuid, text, bigint),
  app_fail_upload(uuid, text),
  app_abort_upload(uuid),
  app_rename_file(uuid, text),
  app_trash_file(uuid),
  app_system_storage_claim(integer),
  app_system_storage_retry(text, text, integer),
  app_system_expire_uploads()
TO fvoci_app;
