import { type MyTasksView } from '$lib/features/home/api';
import { homeApi } from '$lib/features/home/api';
import { activityApi, type NotificationPageView } from '$lib/features/activity/api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Activity, ReceivedInvitation } from '@fvoci/api-client';
import { api, NetworkError } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import MyTasksScreen from '$lib/features/home/MyTasksScreen.svelte';
import NotificationsScreen from '$lib/features/references/NotificationsScreen.svelte';
import ReceivedInvitations from '$lib/features/members/ReceivedInvitations.svelte';
import ActivityList from '$lib/features/activity/ActivityList.svelte';
import SharedScreen from '$lib/features/notes/SharedScreen.svelte';
import { org } from '$lib/org.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { TEST_ORGANIZATION } from './fixtures/organization';
import {
  ACTIVITY,
  INVITATION,
  INVITE_NOTIFICATION,
  INVITED_ORGANIZATION,
} from '../../stories/fixtures/personal';
import { emptyHomeView, homeView, taskViews } from '../../stories/fixtures/synthetic';
import { homeExtras } from './fixtures/home';

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  router.path = '/';
  router.guard = undefined;
  localStorage.clear();
  homeExtras();
});
afterEach(() => {
  vi.restoreAllMocks();
});

const second = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };
const task = {
  id: 't1',
  item_id: 'i1',
  title: '날짜 없음 업무',
  status: '검토 대기',
  status_category: 'doing' as const,
  priority: 'none',
  reference: null,
  start_date: null,
  due_date: null,
  bucket: 'none' as const,
  updated_at: '2026-10-26T01:00:00Z',
};
const mine: MyTasksView = {
  counts: { overdue: 0, today: 2, upcoming: 0, undated: 17, done: 0 },
  workspaces: [
    {
      workspace_id: 'ws1',
      workspace_name: '연구실',
      tasks: [
        task,
        { ...task, id: 't2', title: '오늘 먼저', due_date: '2026-10-26', bucket: 'today' },
        {
          ...task,
          id: 't3',
          title: '오늘 나중',
          due_date: '2026-10-26',
          bucket: 'today',
          updated_at: '2026-10-25T00:00:00Z',
        },
      ],
    },
  ],
};

describe('organization summaries', () => {
  it('makes no personal requests until an organization is selected', async () => {
    org.reset();
    const home = vi.spyOn(homeApi, 'home');
    const { unmount } = render(HomeDashboard, { workspaces: [] });
    expect(await screen.findByText(m.app_organization_required())).toBeVisible();
    expect(screen.queryByRole('heading')).toBeNull();
    expect(home).not.toHaveBeenCalled();
    expect(homeApi.myTasks).not.toHaveBeenCalled();
    expect(activityApi.recentActivity).not.toHaveBeenCalled();
    unmount();
  });

  it('groups my tasks by workspace, sorts due date then update, and keeps undated custom statuses', async () => {
    vi.mocked(homeApi.myTasks).mockResolvedValue(mine);
    render(MyTasksScreen);
    const links = await screen.findAllByRole('link');
    expect(links.map((a) => a.textContent)).toEqual([
      expect.stringContaining('오늘 먼저'),
      expect.stringContaining('오늘 나중'),
      expect.stringContaining('날짜 없음 업무'),
    ]);
    expect(links[2]).toHaveAttribute('href', '/w/ws1/i/i1/tasks/board/t/t1');
    expect(screen.getByTestId('my-tasks-counts')).toHaveTextContent('17');
    expect(screen.getByRole('heading', { name: '연구실 · 3' })).toBeVisible();
  });

  it('puts undated tasks under a "날짜 없음" heading inside their workspace (D-120)', async () => {
    vi.mocked(homeApi.myTasks).mockResolvedValue(mine);
    render(MyTasksScreen);
    const section = (await screen.findByRole('heading', { name: '연구실 · 3' })).closest('section');
    if (section === null) throw new Error('workspace section');
    // 요약과 같은 묶음 제목(이름과 수)을 날짜 순서대로, 업무가 있는 묶음만 둔다.
    const headings = within(section)
      .getAllByRole('heading', { level: 3 })
      .map((h) => h.textContent.trim());
    expect(headings).toEqual(['오늘 2', '날짜 없음 1']);
    const undated = within(section)
      .getByRole('heading', { name: '날짜 없음 1' })
      .closest('section');
    if (undated === null) throw new Error('undated group');
    expect(within(undated).getByRole('link', { name: /날짜 없음 업무/ })).toBeVisible();
    expect(within(undated).queryByRole('link', { name: /오늘 먼저/ })).toBeNull();
  });

  it('reloads on organization change and discards a late task response', async () => {
    org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
    let finish: (value: MyTasksView) => void = () => undefined;
    vi.mocked(homeApi.myTasks)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      )
      .mockResolvedValue({
        counts: { overdue: 0, today: 0, upcoming: 0, undated: 0, done: 0 },
        workspaces: [],
      });
    render(MyTasksScreen);
    org.select(second.id);
    await screen.findByText(m.home_my_tasks_empty());
    finish(mine);
    await waitFor(() => {
      expect(homeApi.myTasks).toHaveBeenLastCalledWith(second.id, expect.any(String));
    });
    expect(screen.queryByText(task.title)).toBeNull();
    org.reset();
    await screen.findByText(m.app_organization_required());
    expect(screen.queryByTestId('my-tasks-counts')).toBeNull();
  });

  it('quick add starts collapsed, assigns me, remembers the item per organization and preserves failed input', async () => {
    me.userId = 'user-a';
    org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
    const home = homeView();
    const item = home.active_items.find((i) => i.can_write);
    const created = taskViews()[0];
    if (item === undefined || created === undefined) throw new Error('missing synthetic task');
    localStorage.setItem(org.key('quick-add-item'), item.id);
    vi.spyOn(homeApi, 'home').mockResolvedValue(home);
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
    const add = vi
      .spyOn(work, 'createQuickTask')
      .mockRejectedValueOnce(new NetworkError('offline'))
      .mockResolvedValue({ ...created, checklist: [] });
    render(HomeDashboard, { workspaces: [] });
    expect(screen.queryByRole('textbox')).toBeNull();
    await screen.findAllByText(home.active_items[0]?.title ?? '');
    await fireEvent.click(screen.getByRole('button', { name: m.home_quick_add() }));
    const title = await screen.findByLabelText(m.home_title());
    await fireEvent.input(title, { target: { value: '새 내 업무' } });
    await fireEvent.click(screen.getByRole('button', { name: m.home_add() }));
    await screen.findByRole('alert');
    expect(title).toHaveValue('새 내 업무');
    const request = add.mock.calls[0]?.[2];
    await fireEvent.click(screen.getByRole('button', { name: m.home_add() }));
    await waitFor(() => {
      expect(add).toHaveBeenCalledTimes(2);
    });
    expect(add).toHaveBeenLastCalledWith('user-a', item.id, request, '새 내 업무', 'user-a');
    expect(localStorage.getItem(org.key('quick-add-item'))).toBe(item.id);
    org.select(second.id);
    await waitFor(() => {
      expect(homeApi.home).toHaveBeenLastCalledWith(second.id, expect.any(String));
    });
    expect(localStorage.getItem(org.key('quick-add-item'))).toBeNull();
  });

  it('clears shared documents while switching organizations', async () => {
    org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
    const shared = vi
      .spyOn(api, 'sharedWithMe')
      .mockResolvedValueOnce([
        {
          id: 'd1',
          title: '이전 공유 문서',
          permission: 'read',
          shared_by: '민수',
          updated_at: '2026-10-26T00:00:00Z',
        },
      ])
      .mockImplementation(() => new Promise(() => undefined));
    render(SharedScreen);
    await screen.findByText('이전 공유 문서');
    org.select(second.id);
    await waitFor(() => {
      expect(shared).toHaveBeenLastCalledWith(second.id);
    });
    expect(screen.queryByText('이전 공유 문서')).toBeNull();
    expect(screen.getByText(m.notes_organization_scope({ name: second.name }))).toBeVisible();
  });
});

describe('notification details', () => {
  it('follows a direct detail URL through cursors and retries a failed read inline', async () => {
    router.path = `/notifications/${INVITE_NOTIFICATION.id}`;
    const target = {
      ...INVITE_NOTIFICATION,
      kind: 'mention',
      invitation_id: null,
      document_title: '직접 연결 문서',
      document_id: 'doc1',
    };
    const cursor = { before: '2026-10-25T00:00:00Z', before_id: 'first' };
    const notes = vi
      .spyOn(activityApi, 'notifications')
      .mockResolvedValueOnce({
        notifications: [{ ...target, id: 'first', read: true }],
        next: cursor,
      })
      .mockResolvedValue({ notifications: [target], next: null });
    const count = vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(42);
    const read = vi
      .spyOn(activityApi, 'markNotificationsRead')
      .mockRejectedValueOnce(new NetworkError('offline'))
      .mockResolvedValue(1);
    const changed = vi.fn();
    render(NotificationsScreen, { workspaces: [], onchanged: changed });
    await screen.findByRole('alert');
    expect(notes).toHaveBeenLastCalledWith('org-1', cursor);
    expect(read).toHaveBeenCalledWith('org-1', target.id);
    expect(changed).not.toHaveBeenCalled();
    expect(screen.getByText(m.references_unread_count({ count: 42 }))).toBeVisible();
    await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
    await waitFor(() => {
      expect(changed).toHaveBeenCalledOnce();
    });
    expect(count).toHaveBeenCalledTimes(3);
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('loads more without marking the list read and ignores previous organization responses', async () => {
    router.path = '/notifications';
    org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
    const cursor = { before: '2026-10-25T00:00:00Z', before_id: 'first' };
    let finish: (page: NotificationPageView) => void = () => undefined;
    const load = vi
      .spyOn(activityApi, 'notifications')
      .mockResolvedValueOnce({ notifications: [INVITE_NOTIFICATION], next: cursor })
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      )
      .mockResolvedValue({ notifications: [], next: null });
    vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(20);
    const read = vi.spyOn(activityApi, 'markNotificationsRead').mockResolvedValue(1);
    render(NotificationsScreen, { workspaces: [], onchanged: () => undefined });
    await fireEvent.click(await screen.findByRole('button', { name: m.references_load_more() }));
    expect(load).toHaveBeenLastCalledWith('org-1', cursor);
    expect(read).not.toHaveBeenCalled();
    org.select(second.id);
    await screen.findByText(m.references_notifications_empty());
    finish({ notifications: [{ ...INVITE_NOTIFICATION, id: 'late' }], next: null });
    await waitFor(() => {
      expect(load).toHaveBeenLastCalledWith(second.id, undefined);
    });
    expect(within(screen.getByTestId('notifications')).queryAllByRole('listitem')).toHaveLength(0);
  });
});

describe.each(['home', 'inbox'] as const)('same invitation flow in %s', (host) => {
  it.each(['organization', 'workspace'] as const)(
    'accepts %s by the Accepted result',
    async (kind) => {
      const invitation: ReceivedInvitation = {
        ...INVITATION,
        kind,
        workspace_id: kind === 'workspace' ? 'new-workspace' : null,
        workspace_name: kind === 'workspace' ? '새 공간' : null,
      };
      vi.spyOn(api, 'myInvitations').mockResolvedValue([invitation]);
      vi.spyOn(api, 'acceptInvitation').mockResolvedValue({
        kind,
        organization_id: invitation.organization_id,
        workspace_id: invitation.workspace_id ?? null,
      });
      vi.spyOn(activityApi, 'notifications').mockResolvedValue({
        notifications: [INVITE_NOTIFICATION],
        next: null,
      });
      vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(1);
      vi.spyOn(activityApi, 'markNotificationsRead').mockResolvedValue(1);
      const accepted = vi.fn(() => {
        org.load('user-a', [TEST_ORGANIZATION, INVITED_ORGANIZATION]);
        return Promise.resolve();
      });
      const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
      if (host === 'home') render(ReceivedInvitations, { onaccepted: accepted });
      else {
        router.path = `/notifications/${INVITE_NOTIFICATION.id}`;
        render(NotificationsScreen, {
          workspaces: [],
          onchanged: () => undefined,
          onaccepted: accepted,
        });
      }
      await fireEvent.click(await screen.findByRole('button', { name: m.members_accept() }));
      await waitFor(() => {
        expect(accepted).toHaveBeenCalledOnce();
      });
      expect(api.acceptInvitation).toHaveBeenCalledWith(invitation.id);
      if (kind === 'organization') {
        expect(org.currentId).toBe(invitation.organization_id);
        expect(navigate).toHaveBeenCalledWith('/');
      } else expect(navigate).toHaveBeenCalledWith('/w/new-workspace');
    },
  );

  it('declines and reloads the same invitation source', async () => {
    let pending = true;
    const received = vi
      .spyOn(api, 'myInvitations')
      .mockImplementation(() => Promise.resolve(pending ? [INVITATION] : []));
    vi.spyOn(api, 'declineInvitation').mockImplementation(() => {
      pending = false;
      return Promise.resolve(undefined);
    });
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({
      notifications: [INVITE_NOTIFICATION],
      next: null,
    });
    vi.spyOn(activityApi, 'unreadCount').mockResolvedValue(1);
    vi.spyOn(activityApi, 'markNotificationsRead').mockResolvedValue(1);
    if (host === 'home') render(ReceivedInvitations, { onaccepted: () => Promise.resolve() });
    else {
      router.path = `/notifications/${INVITE_NOTIFICATION.id}`;
      render(NotificationsScreen, { workspaces: [], onchanged: () => undefined });
    }
    await fireEvent.click(await screen.findByRole('button', { name: m.members_decline() }));
    await waitFor(() => {
      expect(screen.queryByRole('button', { name: m.members_accept() })).toBeNull();
    });
    expect(received.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(api.declineInvitation).toHaveBeenCalledWith(INVITATION.id);
    expect(screen.queryByRole('button', { name: m.members_accept() })).toBeNull();
  });
});

describe('activity feed', () => {
  it('uses target cursors, readable verbs and target links', async () => {
    const activity: Activity[] = ACTIVITY.map((a) => ({ ...a, workspace_id: null }));
    const load = vi
      .spyOn(activityApi, 'targetActivity')
      .mockResolvedValueOnce({
        activity: activity.slice(0, 4),
        next_before: '2026-10-25T00:00:00Z',
        next_before_id: 'cursor',
      })
      .mockResolvedValue({ activity: activity.slice(4), next_before: null, next_before_id: null });
    render(ActivityList, { kind: 'item', targetId: 'item-1', workspaceId: 'ws1' });
    await fireEvent.click(await screen.findByRole('button', { name: m.references_load_more() }));
    expect(load).toHaveBeenLastCalledWith('item', 'item-1', {
      before: '2026-10-25T00:00:00Z',
      before_id: 'cursor',
    });
    await waitFor(() => {
      expect(within(screen.getByRole('list')).getAllByRole('link')).toHaveLength(8);
    });
    expect(screen.getAllByRole('link')[0]).toHaveAttribute('href', '/w/ws1/i/item-1');
    expect(screen.getByRole('list')).toHaveTextContent('댓글');
    expect(screen.getByRole('list')).toHaveTextContent('담당을 해제');
    // 서버가 내는 종류만 쓴다(SQL activity_kind_check, R5 B 최종 FB-3). 공유 철회도 읽을 수 있는 문장이고, 모르는
    // 종류의 "변경했습니다"로 떨어지는 항목은 없다.
    expect(screen.getByRole('list')).toHaveTextContent('공유를 철회');
    expect(screen.getByRole('list')).not.toHaveTextContent('변경했습니다');
  });

  it('reloads recent activity after organization changes', async () => {
    org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
    vi.mocked(activityApi.recentActivity).mockResolvedValueOnce(ACTIVITY).mockResolvedValue([]);
    render(ActivityList);
    await screen.findAllByRole('link');
    org.select(second.id);
    await screen.findByText(m.home_activity_empty());
    expect(activityApi.recentActivity).toHaveBeenLastCalledWith(second.id);
  });
});

it('refreshes the home notification preview when its invitation is declined', async () => {
  let pending = true;
  vi.spyOn(homeApi, 'home').mockResolvedValue(emptyHomeView());
  vi.spyOn(api, 'myInvitations').mockImplementation(() =>
    Promise.resolve(pending ? [INVITATION] : []),
  );
  const notifications = vi
    .spyOn(activityApi, 'notifications')
    .mockImplementation(() =>
      Promise.resolve({ notifications: pending ? [INVITE_NOTIFICATION] : [], next: null }),
    );
  vi.spyOn(api, 'declineInvitation').mockImplementation(() => {
    pending = false;
    return Promise.resolve(undefined);
  });
  render(HomeDashboard, { workspaces: [] });
  render(ReceivedInvitations, { onaccepted: () => Promise.resolve() });
  await screen.findByRole('link', { name: /Second User님이 초대/ });
  await fireEvent.click(screen.getByRole('button', { name: m.members_decline() }));
  await screen.findByText(m.home_notifications_empty());
  expect(notifications.mock.calls.length).toBeGreaterThanOrEqual(2);
  expect(screen.queryByRole('link', { name: /Second User님이 초대/ })).toBeNull();
});

it('retries membership refresh after acceptance without accepting the invitation twice', async () => {
  let pending = true;
  vi.spyOn(api, 'myInvitations').mockImplementation(() =>
    Promise.resolve(pending ? [INVITATION] : []),
  );
  const accept = vi.spyOn(api, 'acceptInvitation').mockImplementation(() => {
    pending = false;
    return Promise.resolve({
      kind: 'organization',
      organization_id: INVITATION.organization_id,
      workspace_id: null,
    });
  });
  const refresh = vi
    .fn()
    .mockRejectedValueOnce(new NetworkError('offline'))
    .mockImplementation(() => {
      org.load('user-a', [TEST_ORGANIZATION, INVITED_ORGANIZATION]);
      return Promise.resolve();
    });
  const navigate = vi.spyOn(router, 'navigate').mockImplementation(() => undefined);
  render(ReceivedInvitations, { onaccepted: refresh });
  await fireEvent.click(await screen.findByRole('button', { name: m.members_accept() }));
  await screen.findByRole('alert');
  expect(navigate).not.toHaveBeenCalled();
  await fireEvent.click(screen.getByRole('button', { name: m.common_retry() }));
  await waitFor(() => {
    expect(navigate).toHaveBeenCalledWith('/');
  });
  expect(accept).toHaveBeenCalledOnce();
  expect(org.currentId).toBe(INVITATION.organization_id);
});
