import type { Activity, Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { HomeView, MyTasksView } from '$lib/api/client';
import { api } from '$lib/api/client';
import ActivityList from '$lib/features/activity/ActivityList.svelte';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import MyTasksScreen from '$lib/features/home/MyTasksScreen.svelte';
import { org } from '$lib/org.svelte';
import { ACTIVITY, myTasksView } from '../../stories/fixtures/personal';
import { homeView } from '../../stories/fixtures/synthetic';
import { TEST_ORGANIZATION } from './fixtures/organization';
import { homeExtras } from './fixtures/home';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(homeExtras);

// 리뷰: 늦은 응답은 새 요청이 시작됐을 때만 버렸다. 홈이 사라진 뒤 도착한 응답은 이어서 달력·알림을 묻고 사라진
// 화면의 상태를 고쳤다. 화면이 사라지면 응답을 버리고 다음 요청도 보내지 않는다.
afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
  access: 'owner',
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  is_owner: true,
  realtime_enabled: false,
};

const home: HomeView = {
  today: '2026-10-20',
  up_next: [],
  up_next_counts: { overdue: 0, today: 0, upcoming: 0 },
  recent_tasks: [],
  recent_documents: [],
  active_items: [],
  pins: [],
  favorites: [],
};

describe('home answer after unmount', () => {
  it('drops a home answer that arrives after the dashboard is gone', async () => {
    await setLocale('ko', { reload: false });
    let finish: (value: HomeView) => void = () => undefined;
    vi.spyOn(api, 'home').mockImplementation(
      () =>
        new Promise<HomeView>((resolve) => {
          finish = resolve;
        }),
    );
    const calendar = vi.spyOn(api, 'calendar');
    const notifications = vi
      .spyOn(api, 'notifications')
      .mockResolvedValue({ notifications: [], next: null });
    const { unmount } = render(HomeDashboard, { props: { workspaces: [workspace] } });

    unmount();
    finish(home);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(calendar).not.toHaveBeenCalled();
    expect(notifications).not.toHaveBeenCalled();
  });
});

// R5 B 최종 FC-19: 옛 조직의 응답이 새 조직 응답 뒤에 와도 새 조직 화면을 덮지 않는다. 홈과 홈 안의 내 업무·최근 활동이
// 저마다 요청 순번으로 늦은 응답을 버린다. A 응답을 붙잡고 B로 바꾼 뒤, B 응답이 보이고 나서 A 응답을 푼다.
describe('an earlier organization answer that arrives after switching', () => {
  const second = { ...TEST_ORGANIZATION, id: 'org-2', name: '다른 조직' };
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    org.load('user-a', [TEST_ORGANIZATION, second], TEST_ORGANIZATION.id);
  });

  function held<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
    let resolve: ((value: T) => void) | undefined;
    const promise = new Promise<T>((ok) => {
      resolve = ok;
    });
    if (resolve === undefined) throw new Error('promise');
    return { promise, resolve };
  }
  const later = (): Promise<void> => new Promise((r) => setTimeout(r, 0));

  it('home dashboard', async () => {
    const titled = (title: string): HomeView => {
      const view = homeView();
      const [first, ...rest] = view.active_items;
      if (first === undefined) throw new Error('active item');
      return { ...view, active_items: [{ ...first, title }, ...rest] };
    };
    const first = held<HomeView>();
    vi.spyOn(api, 'home').mockImplementation((organizationId) =>
      organizationId === TEST_ORGANIZATION.id
        ? first.promise
        : Promise.resolve(titled('다른 조직 항목')),
    );
    vi.spyOn(api, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { props: { workspaces: [] } });
    org.select(second.id);
    expect(await screen.findAllByText(/다른 조직 항목/)).not.toHaveLength(0);
    first.resolve(titled('옛 조직 항목'));
    await later();
    expect(screen.queryAllByText(/옛 조직 항목/)).toHaveLength(0);
    expect(screen.getAllByText(/다른 조직 항목/)).not.toHaveLength(0);
  });

  it('my tasks', async () => {
    const titled = (title: string): MyTasksView => {
      const view = myTasksView();
      const [ws, ...others] = view.workspaces;
      const [task, ...tasks] = ws?.tasks ?? [];
      if (ws === undefined || task === undefined) throw new Error('task');
      return { ...view, workspaces: [{ ...ws, tasks: [{ ...task, title }, ...tasks] }, ...others] };
    };
    const first = held<MyTasksView>();
    vi.spyOn(api, 'myTasks').mockImplementation((organizationId) =>
      organizationId === TEST_ORGANIZATION.id
        ? first.promise
        : Promise.resolve(titled('다른 조직 업무')),
    );
    render(MyTasksScreen);
    org.select(second.id);
    expect(await screen.findByText('다른 조직 업무')).toBeInTheDocument();
    first.resolve(titled('옛 조직 업무'));
    await later();
    expect(screen.queryByText('옛 조직 업무')).toBeNull();
    expect(screen.getByText('다른 조직 업무')).toBeInTheDocument();
  });

  it('recent activity', async () => {
    const titled = (title: string): Activity[] =>
      ACTIVITY.slice(0, 1).map((a) => ({ ...a, detail: { title } }));
    const first = held<Activity[]>();
    vi.spyOn(api, 'recentActivity').mockImplementation((organizationId) =>
      organizationId === TEST_ORGANIZATION.id
        ? first.promise
        : Promise.resolve(titled('다른 조직 활동')),
    );
    render(ActivityList);
    org.select(second.id);
    expect(await screen.findByText(/다른 조직 활동/)).toBeInTheDocument();
    first.resolve(titled('옛 조직 활동'));
    await later();
    expect(screen.queryByText(/옛 조직 활동/)).toBeNull();
    expect(screen.getByText(/다른 조직 활동/)).toBeInTheDocument();
  });
});
