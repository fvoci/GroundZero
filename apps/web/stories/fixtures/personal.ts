import type { HomeView, MyTasksView } from '$lib/features/home/api';
import { type NotificationView } from '$lib/features/activity/api';
import type { Activity, ReceivedInvitation } from '@fvoci/api-client';
import { homeView, ORGANIZATION, WORKSPACE } from './synthetic';

/** 홈과 내 업무가 공유하는 합성 업무. 수는 서버 응답 자리에서만 정한다. */
export function myTasksView(home: HomeView = homeView()): MyTasksView {
  const tasks = home.up_next.map((t) => ({
    ...t,
    bucket: t.bucket ?? ('none' as const),
    priority: 'none',
    start_date: null,
    updated_at: '2026-10-26T00:00:00Z',
  }));
  return {
    counts: { ...home.up_next_counts, undated: 0, done: 0 },
    workspaces:
      tasks.length === 0
        ? []
        : [{ workspace_id: WORKSPACE.id, workspace_name: WORKSPACE.name, tasks }],
  };
}

export const ACTIVITY: Activity[] = [
  'comment',
  'status',
  'kanban_move',
  'assign',
  'unassign',
  'create',
  'move',
  'unshare',
].map((kind, index) => ({
  id: `activity-${String(index)}`,
  kind,
  actor_id: 'story-user',
  actor_name: 'Test User',
  target_kind: 'item',
  target_id: 'item-1',
  workspace_id: WORKSPACE.id,
  item_id: 'item-1',
  created_at: '2026-10-26T00:00:00Z',
  detail: { title: '프로젝트 일정 조정 · Project planning' },
}));

export const INVITATION: ReceivedInvitation = {
  id: 'invitation-1',
  kind: 'organization',
  organization_id: 'invited-org',
  organization_name: '새 연구소 · New lab',
  workspace_id: null,
  workspace_name: null,
  role: 'member',
  invited_by: 'Second User',
  expires_at: '2026-11-30T00:00:00Z',
};

export const INVITE_NOTIFICATION: NotificationView = {
  id: '00000000-0000-4000-8000-000000000118',
  kind: 'invite',
  organization_id: INVITATION.organization_id,
  invitation_id: INVITATION.id,
  document_id: null,
  task_id: null,
  document_title: null,
  document_reference: null,
  workspace_id: null,
  item_id: null,
  comment_id: null,
  excerpt: null,
  due_date: null,
  subject: INVITATION.organization_name,
  actor_name: INVITATION.invited_by,
  created_at: '2026-10-26T00:00:00Z',
  read: false,
};

export const INVITED_ORGANIZATION = {
  ...ORGANIZATION,
  id: INVITATION.organization_id,
  name: INVITATION.organization_name,
};
