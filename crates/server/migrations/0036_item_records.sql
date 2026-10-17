-- 항목 기록 (D-33 산출물·위험·문제·회고, D-34 문헌·연구 기록·데이터·변수 사전·분석·투고, D-35 분석 당시 버전 고정,
-- D-36 자료·모임·시험·복습, D-38 재료·장비·활동·장비 사용·결과물).
--
-- 모든 기록은 그 항목의 권한을 그대로 따른다: 항목을 볼 수 있으면 읽고, 쓸 수 있으면 만들고 고친다. 지우기는
-- 만든 사람이나 항목 소유 권한자. 종류와 칸의 형식은 서버(`features/records.rs`)가 한곳에서 정하고 확인한다.
-- 기록에는 그 항목에 첨부한 파일의 특정 버전을 연결한다(분석의 입력 데이터·코드·결과 등). 연결된 버전은 하나만
-- 따로 지울 수 없고(409), 파일 자체가 영구 삭제되면 연결은 이름·버전·SHA-256만 남는다.

CREATE TABLE item_records (
  id          uuid PRIMARY KEY DEFAULT uuidv7(),
  item_id     uuid NOT NULL REFERENCES items (id) ON DELETE CASCADE,
  workspace_id uuid NOT NULL REFERENCES workspaces (id),
  kind        text NOT NULL CHECK (kind ~ '^[a-z_]{2,32}$'),
  title       text NOT NULL CHECK (length(btrim(title)) BETWEEN 1 AND 300),
  occurred_on date,
  body        text NOT NULL DEFAULT '' CHECK (length(body) <= 20000),
  fields      jsonb NOT NULL DEFAULT '{}'::jsonb CHECK (jsonb_typeof(fields) = 'object'),
  created_by  uuid NOT NULL REFERENCES users (id),
  updated_by  uuid REFERENCES users (id),
  created_at  timestamptz NOT NULL DEFAULT now(),
  updated_at  timestamptz NOT NULL DEFAULT now(),
  version     integer NOT NULL DEFAULT 1,
  request_id  uuid NOT NULL,
  CONSTRAINT item_records_request UNIQUE (item_id, created_by, request_id)
);
CREATE INDEX item_records_item ON item_records (item_id, kind, occurred_on);

CREATE TABLE record_files (
  record_id   uuid NOT NULL REFERENCES item_records (id) ON DELETE CASCADE,
  position    integer NOT NULL CHECK (position >= 0),
  role        text NOT NULL CHECK (role IN ('input', 'code', 'result', 'reference', 'other')),
  file_id     uuid REFERENCES files (id) ON DELETE SET NULL,
  version_id  uuid REFERENCES file_versions (id) ON DELETE SET NULL,
  file_name   text NOT NULL,
  version     integer NOT NULL,
  sha256      text NOT NULL,
  size        bigint NOT NULL,
  PRIMARY KEY (record_id, position)
);
CREATE INDEX record_files_version ON record_files (version_id) WHERE version_id IS NOT NULL;

-- 기록 만들기·고치기 공통: 항목 쓰기 권한, 보관 안 됨. p_files: [{file_id, version, role}].
CREATE FUNCTION app_record_write_check(p_item uuid) RETURNS void
  LANGUAGE plpgsql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_access text := app_item_access(p_item);
  BEGIN
    IF v_access IS NULL THEN
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

-- 연결할 파일 버전을 확인하고 기록에 붙인다(이전 연결은 바꾼다). 파일은 같은 항목의 첨부여야 한다.
CREATE FUNCTION app_set_record_files(p_record uuid, p_item uuid, p_files jsonb) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row  jsonb;
    v_pos  integer := 0;
    v_file files%ROWTYPE;
    v_ver  file_versions%ROWTYPE;
  BEGIN
    IF jsonb_typeof(coalesce(p_files, '[]'::jsonb)) <> 'array' OR jsonb_array_length(coalesce(p_files, '[]')) > 50 THEN
      RAISE EXCEPTION 'give at most 50 linked files' USING ERRCODE = '22023';
    END IF;
    DELETE FROM record_files WHERE record_id = p_record;
    FOR v_row IN SELECT * FROM jsonb_array_elements(coalesce(p_files, '[]'::jsonb)) LOOP
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
  EXCEPTION
    WHEN invalid_text_representation OR check_violation OR numeric_value_out_of_range THEN
      RAISE EXCEPTION 'invalid linked file: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- 기록 만들기(항목에 쓸 수 있는 사람). 같은 요청 id면 같은 기록. 칸 형식은 서버가 이미 확인했다.
CREATE FUNCTION app_create_record(
  p_request uuid, p_item uuid, p_kind text, p_title text, p_occurred date, p_body text, p_fields jsonb,
  p_files jsonb) RETURNS uuid
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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
    INSERT INTO item_records (item_id, workspace_id, kind, title, occurred_on, body, fields, created_by, request_id)
    VALUES (p_item, v_ws, p_kind, btrim(p_title), p_occurred, coalesce(p_body, ''), coalesce(p_fields, '{}'),
            app_user_id(), p_request)
    RETURNING id INTO v_id;
    PERFORM app_set_record_files(v_id, p_item, p_files);
    PERFORM app_audit(v_ws, 'create_record', 'item', p_item, jsonb_build_object('record', v_id, 'kind', p_kind));
    RETURN v_id;
  EXCEPTION
    WHEN check_violation THEN
      RAISE EXCEPTION 'invalid record: %', SQLERRM USING ERRCODE = '22023';
  END
  $$;

-- 기록 고치기(항목에 쓸 수 있는 사람). 판(version)이 다르면 409.
CREATE FUNCTION app_update_record(
  p_record uuid, p_version integer, p_title text, p_occurred date, p_body text, p_fields jsonb, p_files jsonb)
  RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_rec item_records%ROWTYPE;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    IF v_rec.id IS NULL OR app_item_access(v_rec.item_id) IS NULL THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.version <> p_version THEN
      RAISE EXCEPTION 'the record changed; reload it' USING ERRCODE = '40001';
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

-- 기록 지우기(만든 사람 또는 항목 소유 권한자).
CREATE FUNCTION app_delete_record(p_record uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_rec    item_records%ROWTYPE;
    v_access text;
  BEGIN
    SELECT * INTO v_rec FROM item_records WHERE id = p_record FOR UPDATE;
    v_access := app_item_access(v_rec.item_id);
    IF v_rec.id IS NULL OR v_access IS NULL THEN
      RAISE EXCEPTION 'record not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_record_write_check(v_rec.item_id);
    IF v_rec.created_by <> app_user_id() AND v_access <> 'owner' THEN
      RAISE EXCEPTION 'only the author or the item owner can delete a record' USING ERRCODE = '42501';
    END IF;
    DELETE FROM item_records WHERE id = p_record;
    PERFORM app_audit(v_rec.workspace_id, 'delete_record', 'item', v_rec.item_id,
                      jsonb_build_object('record', p_record, 'kind', v_rec.kind, 'title', v_rec.title));
  END
  $$;

-- 분석 등에 연결된 버전은 하나만 따로 지우지 못한다(당시 버전 유지, D-35).
CREATE FUNCTION app_guard_pinned_version() RETURNS trigger
  LANGUAGE plpgsql
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF current_setting('fvoci.purging_files', true) = 'on' THEN
      RETURN OLD;
    END IF;
    IF EXISTS (SELECT 1 FROM record_files WHERE version_id = OLD.id) THEN
      RAISE EXCEPTION 'this version is linked from a record; remove the link first' USING ERRCODE = '55000';
    END IF;
    RETURN OLD;
  END
  $$;

CREATE OR REPLACE FUNCTION app_delete_file_rows(p_ids uuid[]) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
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

CREATE TRIGGER file_versions_pinned BEFORE DELETE ON file_versions
  FOR EACH ROW EXECUTE FUNCTION app_guard_pinned_version();

ALTER TABLE item_records ENABLE ROW LEVEL SECURITY;
CREATE POLICY item_records_select ON item_records FOR SELECT USING (app_item_access(item_id) IS NOT NULL);
ALTER TABLE record_files ENABLE ROW LEVEL SECURITY;
CREATE POLICY record_files_select ON record_files FOR SELECT
  USING (EXISTS (SELECT 1 FROM item_records r WHERE r.id = record_id));
GRANT SELECT ON item_records, record_files TO fvoci_app;

REVOKE ALL ON FUNCTION app_record_write_check(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_set_record_files(uuid, uuid, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_create_record(uuid, uuid, text, text, date, text, jsonb, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_update_record(uuid, integer, text, date, text, jsonb, jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_delete_record(uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_guard_pinned_version() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION
  app_create_record(uuid, uuid, text, text, date, text, jsonb, jsonb),
  app_update_record(uuid, integer, text, date, text, jsonb, jsonb),
  app_delete_record(uuid)
TO fvoci_app;
