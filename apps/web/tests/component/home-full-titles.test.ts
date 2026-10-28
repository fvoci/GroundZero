import { activityApi } from '$lib/features/activity/api';
import type { Workspace } from '@fvoci/api-client';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { HomeTaskView, HomeView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import { homeExtras } from './fixtures/home';
import { setLocale } from '$lib/paraglide/runtime.js';
import { myTasksView } from '../../stories/fixtures/personal';

beforeEach(homeExtras);

// 홈의 다음 할 일·최근 문서·최근 업무는 제목을 한 줄로 줄여(truncate) 좁은 화면이나 큰 글자에서 전체 제목을 읽을 길이
// 없었다. 줄인 줄의 단추에 전체 제목을 둔다(사이드바 문서 트리와 같은 방식).
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

const LONG = '2026학년도 2학기 대학원 연구실 공동 세미나 발표 자료와 회의록을 한곳에 모아 정리하기';

function task(id: string, title: string, bucket: HomeTaskView['bucket']): HomeTaskView {
  return {
    id,
    item_id: 'i1',
    workspace_id: 'ws-1',
    reference: 'DEV-2',
    title,
    status: 'todo',
    status_category: 'todo',
    due_date: '2026-10-21',
    bucket,
  };
}

const home: HomeView = {
  today: '2026-10-20',
  up_next: [task('t1', `${LONG} (다음)`, 'upcoming')],
  up_next_counts: { overdue: 0, today: 0, upcoming: 1 },
  recent_tasks: [task('t2', `${LONG} (업무)`, null)],
  recent_documents: [
    {
      id: 'd1',
      title: `${LONG} (문서)`,
      reference: 'WIKI-1',
      workspace_id: 'ws-1',
      item_id: null,
      updated_at: '2026-10-20T00:00:00Z',
    },
  ],
  active_items: [],
  pins: [],
  favorites: [],
};

describe('home full titles', () => {
  it('keeps the full title on rows that truncate it', async () => {
    await setLocale('ko', { reload: false });
    vi.spyOn(api, 'home').mockResolvedValue(home);
    vi.mocked(api.myTasks).mockResolvedValue(myTasksView(home));
    vi.spyOn(api, 'calendar').mockResolvedValue({
      tasks: [],
      projected_tasks: [],
      events: [],
      occurrences: [],
      dates: [],
      items: [],
    });
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    expect(await screen.findByRole('link', { name: `${LONG} (다음)` })).toHaveTextContent(
      `${LONG} (다음)`,
    );
    for (const suffix of ['(업무)', '(문서)']) {
      const full = `${LONG} ${suffix}`;
      const row = await screen.findByRole('button', { name: (name) => name.includes(full) });
      expect(row).toHaveAttribute('title', full);
    }
  });
});
