import { activityApi } from '$lib/features/activity/api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import { org } from '$lib/org.svelte';
import './fixtures/task-statuses';
import { readFileSync } from 'node:fs';
import path from 'node:path';

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
      access: 'owner',
      id: 'ws-1',
      organization_id: 'org-1',
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
    vi.spyOn(activityApi, 'notifications').mockResolvedValue({ notifications: [], next: null });
    render(HomeDashboard, { props: { workspaces: [workspace] } });

    const grid = document.querySelector('[aria-labelledby="week-heading"]')?.parentElement;
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
    vi.spyOn(work, 'tasks').mockResolvedValue([]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });

    const board = await screen.findByTestId('task-board');
    expect(board).toHaveClass('@2xl:grid-cols-3');
    expect(board.className).not.toMatch(viewportColumns);
  });
});

// UI 감사: 항목 화면의 내용 패널(항목 정보·강의·스터디·기록 편집)과 문서 충돌 비교도 창 폭(sm·md)으로 나뉘어, 사이드바를 연
// 640~767px 창에서 칸이 좁게 눌렸다. 모두 본문 안에 그려지므로(대화상자가 아니다) 본문 폭 기준으로 나눈다.
describe('content panels split by the content width', () => {
  const panels = [
    'src/lib/features/courses/CoursePanel.svelte',
    'src/lib/features/study/StudyPanel.svelte',
    'src/lib/features/modules/ItemProfilePanel.svelte',
    'src/lib/features/records/RecordEditor.svelte',
    'src/lib/features/documents/DocumentEditor.svelte',
  ];
  it.each(panels)('%s uses container variants for its columns', (file) => {
    const source = readFileSync(path.resolve(import.meta.dirname, '../..', file), 'utf8');
    expect(source).not.toMatch(/(^|[\s'"`])(sm|md|lg):(grid-cols|col-span)-/m);
    expect(source).toMatch(/@(xl|2xl):grid-cols-/);
  });
});

// UI 감사: 업무 목록 보기의 여섯 칸 표가 좁은 화면에서 본문 밖으로 넘쳤다. 기록 표처럼 표만 가로로 스크롤한다.
describe('task list table on narrow screens', () => {
  it('scrolls the table sideways instead of widening the page', async () => {
    window.sessionStorage.setItem(org.key('board:i1'), JSON.stringify({ view: 'list' }));
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
    vi.spyOn(work, 'tasks').mockResolvedValue([]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const table = await screen.findByTestId('task-list');
    expect(table.parentElement).toHaveClass('overflow-x-auto');
  });
});
