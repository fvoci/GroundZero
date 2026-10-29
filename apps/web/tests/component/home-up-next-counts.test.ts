import { activityApi } from '$lib/features/activity/api';
import type { Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView, HomeTaskView, HomeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { homeExtras } from './fixtures/home';
import { setLocale } from '$lib/paraglide/runtime.js';
import { myTasksView } from '../../stories/fixtures/personal';

beforeEach(homeExtras);

// 감사 W23: Up Next는 서버가 묶음마다 50개까지 주고 묶음별 전체 수(`up_next_counts`)를 함께 준다. 화면은 목록의
// 길이가 아니라 그 수를 보이고, 잘렸으면 알린다.
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
};

function task(n: number, due: string, bucket: HomeTaskView['bucket']): HomeTaskView {
  return {
    id: `t-${bucket ?? 'none'}-${String(n)}`,
    item_id: 'i1',
    workspace_id: 'ws-1',
    reference: `DEV-${String(n)}`,
    title: `${bucket ?? ''} ${String(n)}`,
    status: 'todo',
    status_category: 'todo',
    due_date: due,
    bucket,
  };
}

const calendar: CalendarView = {
  tasks: [],
  projected_tasks: [],
  events: [],
  occurrences: [],
  dates: [],
  items: [],
};

describe('home up next counts', () => {
  it('shows the server totals and says when a bucket is cut short', async () => {
    await setLocale('ko', { reload: false });
    const overdue = Array.from({ length: 50 }, (_, n) => task(n, '2026-10-01', 'overdue'));
    const home: HomeView = {
      today: '2026-10-21',
      up_next: [...overdue, task(1, '2026-10-21', 'today'), task(2, '2026-10-21', 'today')],
      up_next_counts: { overdue: 73, today: 2, upcoming: 0 },
      recent_tasks: [],
      recent_documents: [],
      active_items: [],
      pins: [],
      favorites: [],
    };
    vi.spyOn(api, 'home').mockResolvedValue(home);
    vi.mocked(api.myTasks).mockResolvedValue(myTasksView(home));
    vi.spyOn(api, 'calendar').mockResolvedValue(calendar);
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    const headings = await screen.findAllByRole('heading', { level: 3 });
    const texts = headings.map((h) => h.textContent.replace(/\s+/g, ' ').trim());
    expect(texts).toContain('기한 지남 73');
    expect(texts).toContain('오늘 2');
    expect(screen.getByRole('link', { name: '내 업무 모두 보기' })).toHaveAttribute(
      'href',
      '/tasks',
    );
    // 홈 미리보기 수(5)가 아니라 서버의 전체 수(73)를 표시한다.
    expect(screen.getAllByRole('link', { name: /^overdue / })).toHaveLength(5);
  });
});
