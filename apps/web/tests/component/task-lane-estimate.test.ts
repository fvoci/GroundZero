import { workApi as work } from '$lib/features/tasks/api';
import { org } from '$lib/org.svelte';
import './fixtures/task-statuses';
import { render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskStatus, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { laneEstimate } from '$lib/features/tasks/board';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 열 머리의 예상 시간 합계(D-31 예상 시간): 보이는 카드를 더하고, 예상이 없는 카드는 따로 세고, 거르기 중이면 열 전체도 보인다.
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

function task(id: string, title: string, status: TaskStatus, estimate: number | null): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `KED-${id}`,
    title,
    description: '',
    status,
    priority: 'none',
    start_date: null,
    due_date: null,
    estimate_minutes: estimate,
    tags: [],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    status_category: 'todo',
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 0,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

const list = [
  task('a', '자료 정리', '할 일', 30),
  task('b', 'API 연결', '할 일', 90),
  task('c', '문서 검토', '할 일', null),
  task('d', '배포 점검', '진행 중', null),
];

describe('lane estimate helper', () => {
  it('adds estimates and counts the cards without one', () => {
    expect(laneEstimate(list.filter((t) => t.status === '할 일'))).toEqual({
      minutes: 120,
      missing: 1,
      count: 3,
    });
    expect(laneEstimate([])).toEqual({ minutes: 0, missing: 0, count: 0 });
  });
});

describe('lane estimate in the board', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(work, 'tasks').mockResolvedValue(list);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    window.sessionStorage.clear();
  });

  const estimateOf = (lane: TaskStatus): string | null =>
    document.querySelector(`[data-lane-estimate="${lane}"]`)?.textContent.trim() ?? null;

  it('shows the total in hours and minutes, the cards without an estimate, and nothing for an empty column', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByText('자료 정리');
    expect(estimateOf('할 일')).toBe('예상 2시간 · 예상 없음 1장');
    expect(estimateOf('진행 중')).toBe('예상 시간 없음');
    expect(estimateOf('완료')).toBeNull();
    expect(screen.getByText(/열 머리의 예상은 카드에 적은 예상 시간을 더한 값입니다/)).toBeTruthy();
    // 빈 열은 비어 있다고 글로 말한다(카드가 있는 열에는 없다).
    expect(document.querySelector('[data-lane-empty="완료"]')?.textContent.trim()).toBe(
      '카드 없음',
    );
    expect(document.querySelector('[data-lane-empty="할 일"]')).toBeNull();
  });

  it('says it counts the shown cards when filtered, and gives the whole column too', async () => {
    window.sessionStorage.setItem(
      org.key('board:i1'),
      JSON.stringify({ view: 'board', sort: 'position', filter: { text: '자료' } }),
    );
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByText('자료 정리');
    expect(estimateOf('할 일')).toBe('보이는 카드 예상 30분 · 열 전체 2시간');
    expect(document.querySelector('[data-lane-empty="진행 중"]')?.textContent.trim()).toBe(
      '거르기에 맞는 카드 없음',
    );
  });
});
