-- 이름 해석을 고정한다. 함수의 순환 호출 본문 검사는 설치 트랜잭션 안에서만 늦춘다.
-- 설정은 모두 SET LOCAL이며 SQLx가 커밋·롤백하면 접속의 원래 값으로 복구된다.
SET LOCAL search_path = '';
SET LOCAL check_function_bodies = false;
SET LOCAL default_tablespace = '';
SET LOCAL default_table_access_method = heap;

REVOKE ALL ON FUNCTION fvoci.app_file_ref_in_scope(p_doc_ws uuid, p_doc_owner uuid, p_file_ws uuid, p_file_owner uuid) FROM PUBLIC;

REVOKE ALL ON FUNCTION fvoci.app_require_file_write(p_file uuid) FROM PUBLIC;

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

REVOKE ALL ON FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[], p_template uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_create_personal_note(p_request_id uuid, p_title text, p_schema_version integer, p_body jsonb, p_parent uuid, p_max_depth integer, p_ids uuid[], p_template uuid) TO fvoci_app;

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


REVOKE ALL ON FUNCTION fvoci.app_authorized_file_refs(p_doc uuid, p_body jsonb, p_editors jsonb, p_author uuid, p_revision integer) FROM PUBLIC;
REVOKE ALL ON FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[], p_file_ref_editors jsonb) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_system_collab_store(p_doc uuid, p_revision integer, p_state bytea, p_pending boolean, p_body jsonb, p_editors uuid[], p_file_ref_editors jsonb) TO fvoci_app;

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

REVOKE ALL ON FUNCTION fvoci.app_template_body(p_template uuid, p_workspace uuid, p_item uuid, p_request_id uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_template_body(p_template uuid, p_workspace uuid, p_item uuid, p_request_id uuid, p_ids uuid[]) TO fvoci_app;

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
REVOKE ALL ON FUNCTION fvoci.app_document_share_recipient(p_doc uuid, p_user uuid, p_organization uuid) FROM PUBLIC;
REVOKE ALL ON FUNCTION fvoci.app_document_share_recipient(p_doc uuid, p_email text, p_organization uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_share_recipient(p_doc uuid, p_email text, p_organization uuid) TO fvoci_app;
REVOKE ALL ON FUNCTION fvoci.app_document_share_recipient_for_share(p_share uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_document_share_recipient_for_share(p_share uuid) TO fvoci_app;
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

-- 홈 집계는 호출자 권한과 기존 RLS로 읽는다.
REVOKE ALL ON FUNCTION fvoci.app_home(p_organization uuid, p_tz text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_home(p_organization uuid, p_tz text) TO fvoci_app;

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

REVOKE ALL ON FUNCTION fvoci.app_set_organization_member_ids(p_organization uuid, p_user uuid, p_role text) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_set_organization_member_ids(p_organization uuid, p_user uuid, p_role text) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_organization_member_preview(p_organization uuid, p_user uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_organization_member_preview(p_organization uuid, p_user uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_organization_member_ids(p_organization uuid, p_user uuid) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_organization_member_ids(p_organization uuid, p_user uuid) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_remove_organization_member(p_organization uuid, p_user uuid, p_ids uuid[]) FROM PUBLIC;
GRANT ALL ON FUNCTION fvoci.app_remove_organization_member(p_organization uuid, p_user uuid, p_ids uuid[]) TO fvoci_app;

REVOKE ALL ON FUNCTION fvoci.app_invitation_recipient(p_email text) FROM PUBLIC;

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

-- 검색 집계 읽기(호출자 RLS, SQL01).
REVOKE ALL ON FUNCTION fvoci.app_search_titles(p_pattern text, p_kinds text[], p_workspace uuid, p_module uuid, p_status_category text, p_assignee uuid, p_from date, p_to date, p_start timestamp with time zone, p_end timestamp with time zone, p_author uuid, p_organization uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.app_search_titles(p_pattern text, p_kinds text[], p_workspace uuid, p_module uuid, p_status_category text, p_assignee uuid, p_from date, p_to date, p_start timestamp with time zone, p_end timestamp with time zone, p_author uuid, p_organization uuid) TO fvoci_app;

-- 달력 집계(D-80): 기존 권한 덩어리와 분리해 추가한다.
REVOKE ALL ON FUNCTION fvoci.app_calendar(uuid, date, date, text, uuid, uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION fvoci.app_calendar(uuid, date, date, text, uuid, uuid) TO fvoci_app;

-- SQLx는 본문 실행 뒤 같은 트랜잭션에서 스키마 없는 _sqlx_migrations 이름에 기록한다.
-- bootstrap의 스키마를 다시 보이게 해 새 버전·체크섬도 이 원장에 남긴다.
SET LOCAL search_path = fvoci;

-- 앱은 원장 SELECT 권한 없이 카탈로그 주석으로 기준 세대를 확인한다.
-- 실제 버전·체크섬 검증은 migrator의 SQLx 검사를 그대로 사용한다.
COMMENT ON TABLE fvoci._sqlx_migrations IS 'fvoci baseline 202610280001';
