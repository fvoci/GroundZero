-- 리뷰 반영(항목 멤버 이후 접근 규칙, 버전 보존):
-- 1. 그룹에서 빠지거나 그룹이 지워져 비공개 항목을 볼 수 없게 되면 그 항목 업무의 담당·리더에서도 빠진다.
--    워크스페이스를 떠나면 항목 멤버에서도 빠진다(다시 초대돼도 예전 비공개 항목이 저절로 열리지 않게).
-- 2. 이동 미리보기의 대상 사람과 담당자 후보도 항목 멤버를 센다(app_item_access_as 한 규칙).
-- 3. 문서 리비전의 버전 참조는 저장할 때 표에 적고, 같은 워크스페이스(개인 노트는 같은 소유자)의 파일만 붙잡는다.
-- 4. 이전 버전 보존 기간은 다음 버전이 올라온 때부터 센다. 붙잡힌 버전은 정리 후보에서 미리 뺀다.

------------------------------------------------------------------------------
-- 1. 담당·리더 정리
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_drop_unseen_assignees(p_item uuid, p_workspace uuid, p_user uuid, p_reason text)
  RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row record;
    v_ws  uuid := coalesce(p_workspace, (SELECT workspace_id FROM items WHERE id = p_item));
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
      IF app_item_access_as(v_row.item_id, v_row.user_id) IS NULL THEN
        DELETE FROM task_assignees WHERE task_id = v_row.task_id AND user_id = v_row.user_id;
        IF FOUND THEN
          PERFORM app_audit(v_row.workspace_id, 'unassign', 'task', v_row.task_id,
                            jsonb_build_object('user', v_row.user_id, 'reason', p_reason));
        END IF;
      END IF;
    END LOOP;
    -- 리더도 그 항목을 볼 수 있는 사람이어야 한다. 휴지통의 항목은 되살릴 때를 위해 둔다.
    FOR v_row IN
      SELECT i.id, i.workspace_id, i.leader_id FROM items i
      WHERE i.leader_id IS NOT NULL AND i.deleted_at IS NULL AND i.workspace_id = v_ws
        AND (p_item IS NULL OR i.id = p_item)
        AND (p_user IS NULL OR i.leader_id = p_user)
      FOR UPDATE
    LOOP
      IF app_item_access_as(v_row.id, v_row.leader_id) IS NULL THEN
        UPDATE items SET leader_id = NULL WHERE id = v_row.id;
        PERFORM app_audit(v_row.workspace_id, 'unset_leader', 'item', v_row.id,
                          jsonb_build_object('user', v_row.leader_id, 'reason', p_reason));
      END IF;
    END LOOP;
  END
  $$;

CREATE OR REPLACE FUNCTION app_delete_group(p_group uuid) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    -- 멤버 행을 먼저 잠근다: 동시에 담당자로 지정하는 쪽(멤버 행 공유 잠금)이 정리가 끝날 때까지 기다린다.
    PERFORM 1 FROM workspace_members m JOIN group_members gm ON gm.user_id = m.user_id
    WHERE m.workspace_id = v_ws AND gm.group_id = p_group
    ORDER BY m.user_id
    FOR UPDATE OF m;
    DELETE FROM groups WHERE id = p_group;
    PERFORM app_drop_unseen_assignees(NULL, v_ws, NULL, 'group_deleted');
    PERFORM app_audit(v_ws, 'delete', 'group', p_group, '{}'::jsonb);
  END
  $$;

CREATE OR REPLACE FUNCTION app_set_group_member(p_group uuid, p_member uuid, p_present boolean) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    SELECT workspace_id INTO v_ws FROM groups WHERE id = p_group;
    IF v_ws IS NULL OR app_member_role(v_ws) IS NULL THEN
      RAISE EXCEPTION 'group not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_require_admin(v_ws);
    IF p_present THEN
      IF NOT EXISTS (SELECT 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id = p_member) THEN
        RAISE EXCEPTION 'only workspace members can join a group' USING ERRCODE = '22023';
      END IF;
      INSERT INTO group_members (group_id, user_id) VALUES (p_group, p_member) ON CONFLICT DO NOTHING;
      PERFORM app_audit(v_ws, 'add_group_member', 'group', p_group, jsonb_build_object('user', p_member));
    ELSE
      PERFORM 1 FROM workspace_members WHERE workspace_id = v_ws AND user_id = p_member FOR UPDATE;
      DELETE FROM group_members WHERE group_id = p_group AND user_id = p_member;
      PERFORM app_drop_unseen_assignees(NULL, v_ws, p_member, 'group_member_removed');
      PERFORM app_audit(v_ws, 'remove_group_member', 'group', p_group, jsonb_build_object('user', p_member));
    END IF;
  END
  $$;

CREATE OR REPLACE FUNCTION app_remove_member_internal(p_workspace uuid, p_member uuid, p_action text) RETURNS void
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    IF p_member = (SELECT owner_id FROM workspaces WHERE id = p_workspace) THEN
      RAISE EXCEPTION 'the owner cannot leave or be removed; transfer ownership first' USING ERRCODE = '55000';
    END IF;
    DELETE FROM group_members gm USING groups g
    WHERE gm.group_id = g.id AND g.workspace_id = p_workspace AND gm.user_id = p_member;
    -- 항목 멤버도 끝난다. 다시 초대되면 비공개 항목은 다시 멤버로 넣어야 열린다.
    DELETE FROM item_members im USING items i
    WHERE im.item_id = i.id AND i.workspace_id = p_workspace AND im.user_id = p_member;
    DELETE FROM ownership_offers WHERE workspace_id = p_workspace AND to_user = p_member;
    -- 멤버 행을 먼저 잠근다: 동시에 담당자로 지정하는 쪽(멤버 행 공유 잠금)이 이 제거가 끝날 때까지 기다리게 한다.
    PERFORM 1 FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member FOR UPDATE;
    DELETE FROM workspace_members WHERE workspace_id = p_workspace AND user_id = p_member;
    IF NOT FOUND THEN
      RAISE EXCEPTION 'member not found' USING ERRCODE = 'P0002';
    END IF;
    PERFORM app_drop_unseen_assignees(NULL, p_workspace, p_member, p_action);
    PERFORM app_audit(p_workspace, p_action, 'member', p_member, '{}'::jsonb);
  END
  $$;

-- 이미 떠난 사람의 항목 멤버 행도 정리한다.
DELETE FROM item_members im USING items i
WHERE im.item_id = i.id AND im.user_id IS NOT NULL
  AND NOT EXISTS (SELECT 1 FROM workspace_members m WHERE m.workspace_id = i.workspace_id AND m.user_id = im.user_id);

------------------------------------------------------------------------------
-- 2. 이동 미리보기·담당자 후보: 항목을 볼 수 있는지는 app_item_access_as 하나로
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_document_audience(
  p_workspace uuid, p_item uuid, p_owner uuid, p_creator uuid, p_visibility text, p_doc uuid
) RETURNS TABLE (user_id uuid)
  LANGUAGE sql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT p_owner WHERE p_owner IS NOT NULL
    UNION
    SELECT m.user_id FROM workspace_members m
    WHERE p_workspace IS NOT NULL AND m.workspace_id = p_workspace
      AND (m.role = 'admin' OR p_visibility = 'workspace' OR m.user_id = p_creator)
      AND (p_item IS NULL OR app_item_access_as(p_item, m.user_id) IS NOT NULL)
    UNION
    SELECT u.id FROM document_shares s JOIN users u
      ON u.email_verified AND lower(u.email) = s.grantee_email
    WHERE s.document_id = p_doc
  $$;

CREATE OR REPLACE FUNCTION app_item_assignable(p_item uuid) RETURNS TABLE (user_id uuid, display_name text)
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_ws uuid;
  BEGIN
    IF app_item_access(p_item) IS NULL THEN
      RAISE EXCEPTION 'item not found' USING ERRCODE = 'P0002';
    END IF;
    SELECT workspace_id INTO v_ws FROM items WHERE id = p_item;
    RETURN QUERY
    SELECT u.id, u.display_name
    FROM workspace_members m JOIN users u ON u.id = m.user_id
    WHERE m.workspace_id = v_ws AND app_item_access_as(p_item, m.user_id) IS NOT NULL
    ORDER BY u.display_name, u.id;
  END
  $$;

------------------------------------------------------------------------------
-- 3. 리비전의 파일 버전 참조
------------------------------------------------------------------------------

-- 리비전 본문의 첨부 주소에서 (파일, 버전)을 뽑는다. 서버는 새 저장에 표준형만 받지만(documents.rs
-- `is_file_url`), 예전 리비전을 위해 대소문자·하이픈 없는 id·매개변수 순서·앞의 0도 알아본다.
CREATE FUNCTION app_file_refs(p_body jsonb) RETURNS TABLE (file_id uuid, version integer)
  LANGUAGE sql IMMUTABLE
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT DISTINCT lower(replace(m[1], '-', ''))::uuid, (substring(m[2] FROM '(?:^|&)version=0*([0-9]{1,9})(?:&|$)'))::integer
    FROM regexp_matches(p_body::text, '/api/files/([0-9A-Fa-f-]{32,36})/content\?([^"]*)', 'g') AS m
    WHERE lower(replace(m[1], '-', '')) ~ '^[0-9a-f]{32}$'
      AND substring(m[2] FROM '(?:^|&)version=0*([0-9]{1,9})(?:&|$)') IS NOT NULL
  $$;

CREATE TABLE document_file_refs (
  document_id uuid NOT NULL,
  revision    integer NOT NULL,
  file_id     uuid NOT NULL,
  version     integer NOT NULL,
  PRIMARY KEY (document_id, revision, file_id, version),
  FOREIGN KEY (document_id, revision) REFERENCES document_revisions (document_id, revision) ON DELETE CASCADE
);
CREATE INDEX document_file_refs_file ON document_file_refs (file_id, version);
ALTER TABLE document_file_refs ENABLE ROW LEVEL SECURITY;

CREATE FUNCTION app_record_file_refs() RETURNS trigger
  LANGUAGE plpgsql SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  BEGIN
    INSERT INTO document_file_refs (document_id, revision, file_id, version)
    SELECT NEW.document_id, NEW.revision, r.file_id, r.version FROM app_file_refs(NEW.body) r
    ON CONFLICT DO NOTHING;
    RETURN NULL;
  END
  $$;

CREATE TRIGGER document_revisions_file_refs AFTER INSERT ON document_revisions
  FOR EACH ROW EXECUTE FUNCTION app_record_file_refs();

INSERT INTO document_file_refs (document_id, revision, file_id, version)
SELECT d.document_id, d.revision, r.file_id, r.version
FROM document_revisions d CROSS JOIN LATERAL app_file_refs(d.body) r
ON CONFLICT DO NOTHING;

-- 붙잡는 것: 기록 연결(D-35), 또는 그 파일과 같은 곳(워크스페이스, 개인 영역은 같은 소유자)에 있는 문서의
-- 리비전(D-56). 다른 곳의 문서가 주소를 적어도 남의 버전을 붙잡지 못한다. 문서가 옮겨지면 지금 자리로 본다.
CREATE OR REPLACE FUNCTION app_version_referenced(p_file uuid, p_version integer, p_version_id uuid) RETURNS boolean
  LANGUAGE sql STABLE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
    SELECT EXISTS (SELECT 1 FROM record_files WHERE version_id = p_version_id)
        OR EXISTS (SELECT 1 FROM document_file_refs r
                   JOIN documents d ON d.id = r.document_id
                   JOIN files f ON f.id = r.file_id
                   WHERE r.file_id = p_file AND r.version = p_version
                     AND ((f.workspace_id IS NOT NULL AND d.workspace_id = f.workspace_id)
                       OR (f.owner_user_id IS NOT NULL AND d.owner_user_id = f.owner_user_id)))
  $$;

------------------------------------------------------------------------------
-- 4. 이전 버전 정리: 다음 버전이 올라온 때부터 세고, 붙잡힌 버전은 후보에서 뺀다
------------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION app_system_purge_old_versions() RETURNS integer
  LANGUAGE plpgsql VOLATILE SECURITY DEFINER
  SET search_path = fvoci, pg_temp
  AS $$
  DECLARE
    v_row   record;
    v_count integer := 0;
  BEGIN
    IF app_user_id() IS NOT NULL THEN
      RETURN 0;
    END IF;
    FOR v_row IN
      SELECT v.id
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
      UPDATE uploads SET version_id = NULL WHERE version_id = v_row.id;
      DELETE FROM file_versions WHERE id = v_row.id;
      v_count := v_count + 1;
    END LOOP;
    RETURN v_count;
  END
  $$;

REVOKE ALL ON FUNCTION app_file_refs(jsonb) FROM PUBLIC;
REVOKE ALL ON FUNCTION app_record_file_refs() FROM PUBLIC;
