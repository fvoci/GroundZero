import type { Workspace } from '@fvoci/api-client';
import { render } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { HomeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 리뷰: 늦은 응답은 새 요청이 시작됐을 때만 버렸다. 홈이 사라진 뒤 도착한 응답은 이어서 달력·알림을 묻고 사라진
// 화면의 상태를 고쳤다. 화면이 사라지면 응답을 버리고 다음 요청도 보내지 않는다.
afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
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
    const notifications = vi.spyOn(api, 'notifications');
    const { unmount } = render(HomeDashboard, { props: { workspaces: [workspace] } });

    unmount();
    finish(home);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(calendar).not.toHaveBeenCalled();
    expect(notifications).not.toHaveBeenCalled();
  });
});
