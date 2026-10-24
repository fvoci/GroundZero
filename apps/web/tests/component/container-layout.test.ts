import type { Workspace } from '@fvoci/api-client';
import { render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { HomeView, ItemView } from '$lib/api/client';
import { api } from '$lib/api/client';
import HomeDashboard from '$lib/features/home/HomeDashboard.svelte';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 루트 글꼴 200%(데스크톱 창 1009px)에서 사이드바가 512px을 차지해 본문이 497px인데도, 창 폭 기준(md·lg) 여러 열이
// 남아 업무 제목·문서 번호가 한 글자씩 섰다. 여러 열은 본문 폭(App의 `@container`) 기준이어야 한다. jsdom은 컨테이너
// 쿼리를 계산하지 않으므로 클래스만 확인하고, 배치는 실제 브라우저에서 확인한다.
const viewportColumns = /(^|\s)(sm|md|lg|xl):grid-cols-/;

afterEach(() => {
  vi.restoreAllMocks();
  window.sessionStorage.clear();
});

beforeEach(async () => {
  await setLocale('ko', { reload: false });
});

describe('multi-column layouts follow the content width', () => {
  it('splits the home dashboard by the container, not the window', async () => {
    const workspace: Workspace = {
      id: 'ws-1',
      name: '공간',
      role: 'admin',
      is_owner: true,
      realtime_enabled: false,
    };
    const home: HomeView = {
      today: '2026-10-21',
      up_next: [],
      up_next_counts: { overdue: 0, today: 0, upcoming: 0 },
      recent_tasks: [],
      recent_documents: [],
      active_items: [],
      pins: [],
      favorites: [],
    };
    vi.spyOn(api, 'home').mockResolvedValue(home);
    vi.spyOn(api, 'calendar').mockResolvedValue({
      tasks: [],
      projected_tasks: [],
      events: [],
      occurrences: [],
      dates: [],
      items: [],
    });
    vi.spyOn(api, 'notifications').mockResolvedValue([]);
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    const grid = document.querySelector('[aria-labelledby="up-next-heading"]')?.parentElement;
    if (!(grid instanceof HTMLElement)) throw new Error('no dashboard grid');
    await waitFor(() => {
      expect(grid).toHaveClass('@2xl:grid-cols-2');
    });
    expect(grid.className).not.toMatch(viewportColumns);
  });

  it('lays out the task board lanes by the container, not the window', async () => {
    const item: ItemView = {
      id: 'i1',
      module_id: 'm1',
      workspace_id: 'ws-1',
      reference: 'KED-1',
      title: '항목',
      description: '',
      visibility: 'workspace',
      access: 'write',
      archived: false,
      updated_at: '2026-10-21T00:00:00Z',
      status: 'active',
      is_template: false,
      version: 1,
      starts_on: null,
      ends_on: null,
    };
    vi.spyOn(api, 'tasks').mockResolvedValue([]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });

    const board = await screen.findByTestId('task-board');
    expect(board).toHaveClass('@2xl:grid-cols-3');
    expect(board.className).not.toMatch(viewportColumns);
  });
});
