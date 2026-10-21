// 감사 기록의 동작·대상 이름(D-57). 모르는 코드는 그대로 보인다.
// 표시 언어가 바뀔 수 있어 메시지 함수를 담아 두고 부를 때 계산한다(D-81).
import { m } from '$lib/paraglide/messages.js';

const ACTION: Record<string, () => string> = {
  create: m.audit_action_create,
  rename: m.audit_action_rename,
  move_in: m.audit_action_move_in,
  move_out: m.audit_action_move_out,
  reparent: m.audit_action_reparent,
  copy: m.audit_action_copy,
  delete: m.audit_action_delete,
  restore: m.audit_action_restore,
  purge: m.audit_action_purge,
  restore_revision: m.audit_action_restore_revision,
  upload: m.audit_action_upload,
  delete_version: m.audit_action_delete_version,
  share: m.audit_action_share,
  unshare: m.audit_action_unshare,
  create_public_link: m.audit_action_create_public_link,
  revoke_public_link: m.audit_action_revoke_public_link,
  change_visibility: m.audit_action_change_visibility,
  change_copy_policy: m.audit_action_change_copy_policy,
  change_key: m.audit_action_change_key,
  invite: m.audit_action_invite,
  revoke: m.audit_action_revoke,
  join: m.audit_action_join,
  decline: m.audit_action_decline,
  change_role: m.audit_action_change_role,
  leave: m.audit_action_leave,
  remove: m.audit_action_remove,
  offer_ownership: m.audit_action_offer_ownership,
  cancel_ownership_offer: m.audit_action_cancel_ownership_offer,
  accept_ownership: m.audit_action_accept_ownership,
  add_group_member: m.audit_action_add_group_member,
  remove_group_member: m.audit_action_remove_group_member,
  assign: m.audit_action_assign,
  unassign: m.audit_action_unassign,
  link_predecessor: m.audit_action_link_predecessor,
  unlink_predecessor: m.audit_action_unlink_predecessor,
  change_predecessor: m.audit_action_change_predecessor,
  reschedule: m.audit_action_reschedule,
  set_recurrence: m.audit_action_set_recurrence,
  exclude_occurrence: m.audit_action_exclude_occurrence,
  set_trash_retention: m.audit_action_set_trash_retention,
  set_version_retention: m.audit_action_set_version_retention,
  set_members: m.audit_action_set_members,
  change_leader: m.audit_action_change_leader,
  unset_leader: m.audit_action_unset_leader,
  change_status: m.audit_action_change_status,
  create_record: m.audit_action_create_record,
  delete_record: m.audit_action_delete_record,
  add_episodes: m.audit_action_add_episodes,
  delete_episode: m.audit_action_delete_episode,
  update_course: m.audit_action_update_course,
  add_study_units: m.audit_action_add_study_units,
  delete_study_unit: m.audit_action_delete_study_unit,
  export: m.audit_action_export,
};

const TARGET: Record<string, () => string> = {
  document: m.audit_target_document,
  task: m.audit_target_task,
  item: m.audit_target_item,
  module: m.audit_target_module,
  event: m.audit_target_event,
  file: m.audit_target_file,
  member: m.audit_target_member,
  invitation: m.audit_target_invitation,
  group: m.audit_target_group,
  workspace: m.audit_target_workspace,
  user: m.audit_target_user,
};

/** 동작 코드의 표시 이름. 모르는 코드는 그대로. */
export function auditAction(code: string): string {
  return Object.hasOwn(ACTION, code) ? (ACTION[code]?.() ?? code) : code;
}

/** 대상 종류의 표시 이름. 모르는 종류는 그대로. */
export function auditTarget(type: string): string {
  return Object.hasOwn(TARGET, type) ? (TARGET[type]?.() ?? type) : type;
}

/** 기록의 detail에서 사람이 알아볼 이름(제목·이름·이메일). */
export function auditSubject(detail: unknown): string {
  if (typeof detail !== 'object' || detail === null) return '';
  for (const key of ['title', 'name', 'email', 'to']) {
    const value: unknown = Reflect.get(detail, key);
    if (typeof value === 'string' && value !== '') return value;
  }
  return '';
}
