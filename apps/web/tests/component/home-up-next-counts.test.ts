import type { Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { CalendarView, HomeTaskView, HomeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 감사 W23: Up Next는 서버가 묶음마다 50개까지 주고 묶음별 전체 수(`up_next_counts`)를 함께 준다. 화면은 목록의
// 길이가 아니라 그 수를 보이고, 잘렸으면 알린다.
afterEach(() => {
  vi.restoreAllMocks();
});

const workspace: Workspace = {
  id: 'ws-1',
  organization_id: 'org-1',
  name: '공간',
  role: 'admin',
  access: 'owner',
  is_owner: true,
  realtime_enabled: false,
};

function task(n: number, due: string, bucket: HomeTaskView['bucket']): HomeTaskView {
  return {
    id: `t-${bucket ?? 'none'}-${String(n)}`,
    item_id: 'i1',
    workspace_id: 'ws-1',
    reference: `DEV-${String(n)}`,
    title: `${bucket ?? ''} ${String(n)}`,
    status: 'todo',
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
    vi.spyOn(api, 'calendar').mockResolvedValue(calendar);
    vi.spyOn(api, 'notifications').mockResolvedValue([]);
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    const headings = await screen.findAllByRole('heading', { level: 3 });
    const texts = headings.map((h) => h.textContent.replace(/\s+/g, ' ').trim());
    expect(texts).toContain('기한 지남 73');
    expect(texts).toContain('오늘 2');
    expect(screen.getByText('마감이 이른 50개만 보입니다(모두 73개).')).toBeTruthy();
    // 잘리지 않은 묶음에는 알림이 없다.
    expect(screen.getAllByText(/개만 보입니다/)).toHaveLength(1);
  });
});
