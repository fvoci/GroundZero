import type { HomeView } from '$lib/features/home/api';
import { homeApi } from '$lib/features/home/api';
import { activityApi } from '$lib/features/activity/api';
import type { Workspace } from '@fvoci/api-client';
import { render, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { homeExtras } from './fixtures/home';
import { setLocale } from '$lib/paraglide/runtime.js';

beforeEach(homeExtras);

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

const LONG_URL =
  'https://example.test/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/tasks/ffffffff-0000-1111-2222-333333333333';

const home: HomeView = {
  my_tasks: { counts: { overdue: 0, today: 0, upcoming: 0, undated: 0, done: 0 }, workspaces: [] },
  today: '2026-10-21',
  up_next: [],
  up_next_counts: { overdue: 0, today: 0, upcoming: 0 },
  recent_tasks: [],
  recent_documents: [],
  active_items: [],
  pins: [],
  favorites: [],
};

const calendar: CalendarView = {
  tasks: [],
  projected_tasks: [],
  events: [],
  occurrences: [],
  dates: [
    {
      kind: 'submission',
      id: 'sub-1',
      item_id: 'i1',
      title: LONG_URL,
      date: '2026-10-21',
      personal: false,
      done: false,
    },
  ],
  items: [],
};

// jsdom은 배치를 계산하지 않는다. 넘침은 실제 브라우저(375px)에서 확인했고, 여기서는 그 원인이 된 구조만 지킨다.
describe('home layout at narrow widths', () => {
  it('gives the dashboard grid a shrinkable column and lets long week entries break', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(homeApi, 'home').mockResolvedValue(home);
    vi.spyOn(api, 'calendar').mockResolvedValue(calendar);
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    const week = document.querySelector('[aria-labelledby="week-heading"]');
    if (!(week instanceof HTMLElement)) throw new Error('no week section');
    // 열 정의 없는 grid는 암묵 auto 열이 되어 띄어쓰기 없는 긴 글만큼 넓어진다(375px에서 906px).
    expect(week.parentElement).toHaveClass('grid-cols-1');

    await waitFor(() => {
      expect(week.textContent).toContain(LONG_URL);
    });
    const entry = [...week.querySelectorAll('li span')].find((s) =>
      s.textContent.includes(LONG_URL),
    );
    expect(entry).toHaveClass('[overflow-wrap:anywhere]');
  });
});
