import type { Activity } from '@fvoci/api-client';
import { m } from '$lib/paraglide/messages.js';
import { itemPath, itemTaskPath, pathFor } from '$lib/router.svelte';

export function activityText(activity: Activity): string {
  const who = activity.actor_name ?? m.references_notification_someone();
  const detail: unknown = activity.detail;
  const title =
    typeof detail === 'object' &&
    detail !== null &&
    'title' in detail &&
    typeof detail.title === 'string'
      ? detail.title
      : '';
  const target =
    title ||
    (activity.target_kind === 'task'
      ? m.references_kind_task()
      : activity.target_kind === 'document'
        ? m.references_kind_document()
        : m.references_kind_item());
  switch (activity.kind) {
    case 'comment':
      return m.home_activity_comment({ who, target });
    case 'status':
      return m.home_activity_status({ who, target });
    case 'kanban_move':
      return m.home_activity_kanban({ who, target });
    case 'assign':
      return m.home_activity_assign({ who, target });
    case 'unassign':
      if (
        typeof detail === 'object' &&
        detail !== null &&
        'reason' in detail &&
        detail.reason === 'remove'
      )
        return m.home_activity_member_removed({ who, target });
      return m.home_activity_unassign({ who, target });
    case 'create':
      return m.home_activity_create({ who, target });
    case 'move':
      return m.home_activity_move({ who, target });
    case 'detach':
      return m.home_activity_detach({ who, target });
    default:
      return m.home_activity_other({ who, target });
  }
}

export function activityPath(activity: Activity, workspaceId?: string): string | undefined {
  const ws = activity.workspace_id ?? workspaceId;
  if (activity.target_kind === 'document')
    return pathFor(
      { id: activity.target_id, workspace_id: ws ?? null, item_id: activity.item_id ?? null },
      // 활동 접근은 서버가 검사한다. 공유 문서는 멤버십을 추정하지 않고 독립 문서 경로로 연다.
      () => false,
    );
  if (ws === undefined) return undefined;
  if (activity.target_kind === 'item') return itemPath(ws, activity.target_id);
  if (activity.target_kind === 'task' && activity.item_id != null)
    return itemTaskPath(ws, activity.item_id, activity.target_id);
  if (activity.item_id != null) return itemPath(ws, activity.item_id);
  return undefined;
}
