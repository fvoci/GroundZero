import { fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import { EMPTY_FILTER, hiddenConditionCount } from '$lib/features/tasks/filters';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

// 375px에서 찾기·상태·담당·우선순위·태그·마감 범위·정렬·저장한 보기와 예상 설명이 업무 카드보다 먼저 한 화면을 차지했다.
// 찾기와 "필터·정렬" 단추만 늘 보이고 나머지는 펼친다. 접혀 있어도 걸린 조건 수를 보이고, 접어도 조건 값은 남는다.
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

function task(id: string, title: string, priority: TaskView['priority']): TaskView {
  return {
    id,
    item_id: 'i1',
    module_id: 'm1',
    workspace_id: 'ws-1',
    parent_id: null,
    reference: `KED-${id}`,
    title,
    description: '',
    status: 'todo',
    priority,
    start_date: null,
    due_date: null,
    estimate_minutes: null,
    tags: [],
    milestone: false,
    predecessors: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    move_version: 0,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

describe('hidden condition count', () => {
  it('counts the folded filters and a changed sort, not the visible search text', () => {
    expect(hiddenConditionCount({ ...EMPTY_FILTER, text: '자료' }, 'position')).toBe(0);
    expect(
      hiddenConditionCount({ ...EMPTY_FILTER, status: 'done', dueFrom: '2026-10-01' }, 'due'),
    ).toBe(3);
  });
});

describe('task board filter disclosure', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([
      task('1', '자료 정리', 'high'),
      task('2', '문서 검토', 'low'),
    ]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  });
  afterEach(() => {
    vi.restoreAllMocks();
    window.sessionStorage.clear();
  });

  const toggle = (): HTMLElement => screen.getByRole('button', { name: /필터·정렬/ });

  it('shows only the search and a real toggle button until it is opened', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByText('자료 정리');
    const region = screen.getByRole('search', { name: '업무 거르기' });
    expect(within(region).getByPlaceholderText('제목·번호')).toBeInTheDocument();
    expect(screen.queryByRole('combobox', { name: '우선순위로 거르기' })).toBeNull();
    expect(screen.queryByRole('combobox', { name: '저장한 보기' })).toBeNull();
    expect(toggle().tagName).toBe('BUTTON');
    expect(toggle()).toHaveAttribute('aria-expanded', 'false');

    await fireEvent.click(toggle());
    expect(toggle()).toHaveAttribute('aria-expanded', 'true');
    const controls = toggle().getAttribute('aria-controls');
    expect(controls).not.toBeNull();
    const panel = document.getElementById(controls ?? '');
    if (panel === null) throw new Error('no panel');
    expect(within(panel).getByRole('combobox', { name: '우선순위로 거르기' })).toBeInTheDocument();
    expect(within(panel).getByRole('combobox', { name: '저장한 보기' })).toBeInTheDocument();
  });

  it('keeps a condition after folding and says how many are on', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByText('자료 정리');
    await fireEvent.click(toggle());
    await fireEvent.change(screen.getByRole('combobox', { name: '우선순위로 거르기' }), {
      target: { value: 'high' },
    });
    await fireEvent.click(toggle());

    expect(screen.queryByRole('combobox', { name: '우선순위로 거르기' })).toBeNull();
    expect(toggle()).toHaveAccessibleName('필터·정렬 1개 적용 중');
    expect(screen.getByText('1/2개 보임')).toBeInTheDocument();
    expect(screen.queryByText('문서 검토')).toBeNull();

    await fireEvent.click(toggle());
    expect(screen.getByRole('combobox', { name: '우선순위로 거르기' })).toHaveValue('high');
  });

  it('puts the lane estimate explanation after the board, not before the cards', async () => {
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const board = await screen.findByTestId('task-board');
    const hint = screen.getByText(/열 머리의 예상은 카드에 적은 예상 시간을 더한 값입니다/);
    expect(board.compareDocumentPosition(hint) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
