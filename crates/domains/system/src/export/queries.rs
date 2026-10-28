//! 반출 구조 데이터. 각 SELECT 한 문장이 목록과 파일 키를 같은 스냅샷으로 읽는다.

pub(super) const WORKSPACE_JSON: &str = "
SELECT json_build_object(
  'format', 1,
  'exported_at', now(),
  'workspace', (SELECT row_to_json(w) FROM (SELECT id, name, created_at, trash_retention_days FROM workspaces WHERE id = $1) w),
  'members', (SELECT coalesce(json_agg(m ORDER BY m.display_name), '[]') FROM (
      SELECT wm.user_id, u.display_name, wm.role FROM workspace_members wm JOIN users u ON u.id = wm.user_id
      WHERE wm.workspace_id = $1) m),
  'modules', (SELECT coalesce(json_agg(m ORDER BY m.created_at), '[]') FROM (
      SELECT md.*, (SELECT key FROM module_keys k WHERE k.module_id = md.id AND k.retired_at IS NULL) AS key
      FROM modules md WHERE md.workspace_id = $1) m),
  'items', (SELECT coalesce(json_agg(i ORDER BY i.created_at), '[]') FROM (
      SELECT it.*, app_item_reference(it.id) AS reference FROM items it
      WHERE it.workspace_id = $1 AND it.deleted_at IS NULL) i),
  'item_members', (SELECT coalesce(json_agg(im ORDER BY im.item_id, im.added_at), '[]') FROM (
      SELECT x.item_id, x.user_id, x.group_id, x.added_at FROM item_members x JOIN items it ON it.id = x.item_id
      WHERE it.workspace_id = $1 AND it.deleted_at IS NULL) im),
  'item_records', (SELECT coalesce(json_agg(r ORDER BY r.item_id, r.created_at), '[]') FROM (
      SELECT rec.id, rec.item_id, rec.kind, rec.title, rec.occurred_on, rec.body, rec.fields, rec.created_by,
             rec.created_at, rec.updated_at,
             (SELECT coalesce(json_agg(json_build_object('role', rf.role, 'file_id', rf.file_id, 'file_name', rf.file_name,
                                                        'version', rf.version, 'sha256', rf.sha256)
                                       ORDER BY rf.position), '[]')
              FROM record_files rf WHERE rf.record_id = rec.id) AS files
      FROM item_records rec JOIN items it ON it.id = rec.item_id
      WHERE rec.workspace_id = $1 AND it.deleted_at IS NULL) r),
  'documents', (SELECT coalesce(json_agg(d ORDER BY d.created_at), '[]') FROM (
      SELECT doc.id, doc.title, app_document_reference(doc.id) AS reference, doc.item_id, doc.parent_id,
             doc.visibility, doc.schema_version, doc.revision, doc.body, doc.created_at, doc.updated_at
      FROM documents doc WHERE doc.workspace_id = $1 AND doc.deleted_at IS NULL) d),
  'tasks', (SELECT coalesce(json_agg(t ORDER BY t.created_at), '[]') FROM (
      SELECT tk.*, s.name AS status, s.category AS status_category, app_task_reference(tk.id) AS reference,
             (SELECT coalesce(json_agg(a.user_id), '[]') FROM task_assignees a WHERE a.task_id = tk.id) AS assignees,
             (SELECT coalesce(json_agg(c ORDER BY c.position), '[]') FROM checklist_items c WHERE c.task_id = tk.id)
               AS checklist
      FROM tasks tk JOIN task_statuses s ON s.id = tk.status_id WHERE tk.workspace_id = $1 AND tk.deleted_at IS NULL) t),
  'task_statuses', (SELECT coalesce(json_agg(s ORDER BY s.item_id, s.position), '[]')
      FROM task_statuses s JOIN items i ON i.id = s.item_id WHERE i.workspace_id = $1),
  'task_dependencies', (SELECT coalesce(json_agg(dep), '[]') FROM (
      SELECT dp.* FROM task_dependencies dp JOIN items it ON it.id = dp.item_id WHERE it.workspace_id = $1) dep),
  'task_series', (SELECT coalesce(json_agg(s), '[]') FROM (SELECT * FROM task_series WHERE workspace_id = $1) s),
  'events', (SELECT coalesce(json_agg(e ORDER BY e.created_at), '[]') FROM (
      SELECT * FROM events WHERE workspace_id = $1 AND deleted_at IS NULL) e),
  'comments', (SELECT coalesce(json_agg(c ORDER BY c.created_at), '[]') FROM (
      SELECT cm.id, cm.document_id, cm.task_id, cm.parent_id, cm.body, cm.created_by, cm.created_at, cm.edited,
             cm.deleted_at, cm.resolved_at
      FROM comments cm
      LEFT JOIN documents cd ON cd.id = cm.document_id LEFT JOIN tasks ct ON ct.id = cm.task_id
      WHERE coalesce(cd.workspace_id, ct.workspace_id) = $1) c),
  'course_details', (SELECT coalesce(json_agg(c), '[]') FROM (
      SELECT cd.* FROM course_details cd JOIN items it ON it.id = cd.item_id
      WHERE cd.workspace_id = $1 AND it.deleted_at IS NULL) c),
  'study_units', (SELECT coalesce(json_agg(s ORDER BY s.item_id, s.number), '[]') FROM (
      SELECT su.* FROM study_units su JOIN items it ON it.id = su.item_id
      WHERE su.workspace_id = $1 AND it.deleted_at IS NULL) s),
  'course_episodes', (SELECT coalesce(json_agg(c ORDER BY c.item_id, c.number), '[]') FROM (
      SELECT ce.* FROM course_episodes ce JOIN items it ON it.id = ce.item_id
      WHERE ce.workspace_id = $1 AND it.deleted_at IS NULL) c),
  'files', (SELECT coalesce(json_agg(f ORDER BY f.created_at), '[]') FROM (
      SELECT fl.id, fl.name, fl.document_id, fl.item_id, fl.current_version, fl.created_at,
             (SELECT json_agg(json_build_object('version', v.version, 'size', v.size, 'sha256', v.sha256,
                       'content_type', v.content_type, 'note', v.note, 'uploaded_by', v.uploaded_by,
                       'created_at', v.created_at) ORDER BY v.version)
              FROM file_versions v WHERE v.file_id = fl.id) AS versions
      FROM files fl WHERE fl.workspace_id = $1 AND fl.deleted_at IS NULL) f),
  'audit_log', (SELECT coalesce(json_agg(a ORDER BY a.created_at), '[]') FROM (
      SELECT * FROM audit_log WHERE workspace_id = $1) a)
,
  '_objects', (SELECT coalesce(json_agg(json_build_object('file', fl.id, 'name', fl.name, 'version', v.version,
                                                          'key', v.storage_key) ORDER BY fl.id, v.version), '[]')
      FROM files fl JOIN file_versions v ON v.file_id = fl.id
      WHERE fl.workspace_id = $1 AND fl.deleted_at IS NULL)
)";

pub(super) const PERSONAL_JSON: &str = "
SELECT json_build_object(
  'format', 1,
  'exported_at', now(),
  'user', (SELECT row_to_json(u) FROM (SELECT id, display_name, email, created_at FROM users WHERE id = $1) u),
  'notes', (SELECT coalesce(json_agg(d ORDER BY d.created_at), '[]') FROM (
      SELECT id, title, parent_id, schema_version, revision, body, created_at, updated_at
      FROM documents WHERE owner_user_id = $1 AND deleted_at IS NULL) d),
  'files', (SELECT coalesce(json_agg(f ORDER BY f.created_at), '[]') FROM (
      SELECT fl.id, fl.name, fl.document_id, fl.current_version, fl.created_at,
             (SELECT json_agg(json_build_object('version', v.version, 'size', v.size, 'sha256', v.sha256,
                       'content_type', v.content_type, 'note', v.note, 'created_at', v.created_at)
                       ORDER BY v.version)
              FROM file_versions v WHERE v.file_id = fl.id) AS versions
      FROM files fl WHERE fl.owner_user_id = $1 AND fl.deleted_at IS NULL) f),
  'time_entries', (SELECT coalesce(json_agg(t ORDER BY t.started_at), '[]') FROM (
      SELECT id, started_at, ended_at, paused_seconds, kind, memo, target_kind, target_id, workspace_id,
             created_at, updated_at
      FROM time_entries WHERE user_id = $1 AND deleted_at IS NULL) t),
  'marks', (SELECT coalesce(json_agg(m), '[]') FROM (SELECT * FROM user_marks WHERE user_id = $1) m),
  'episode_progress', (SELECT coalesce(json_agg(p), '[]') FROM (
      SELECT * FROM episode_progress WHERE user_id = $1) p),
  'unit_progress', (SELECT coalesce(json_agg(p), '[]') FROM (
      SELECT * FROM unit_progress WHERE user_id = $1) p),
  'saved_views', (SELECT coalesce(json_agg(v), '[]') FROM (
      SELECT id, item_id, name, config, created_at FROM saved_views WHERE user_id = $1) v),
  'audit_log', (SELECT coalesce(json_agg(a ORDER BY a.created_at), '[]') FROM (
      SELECT * FROM audit_log WHERE owner_user_id = $1 AND workspace_id IS NULL) a)
,
  '_objects', (SELECT coalesce(json_agg(json_build_object('file', fl.id, 'name', fl.name, 'version', v.version,
                                                          'key', v.storage_key) ORDER BY fl.id, v.version), '[]')
      FROM files fl JOIN file_versions v ON v.file_id = fl.id
      WHERE fl.owner_user_id = $1 AND fl.deleted_at IS NULL)
)";
