import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

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
  starts_on: null,
  ends_on: null,
};

function task(id: string, title: string): TaskView {
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
    priority: 'none',
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
    move_version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-21T00:00:00Z',
  };
}

// 카드 메뉴의 같은 열 안 순서 바꾸기: 화살표 줄은 aria-hidden이고 Alt+위·아래는 키보드에서만 되므로, 터치 화면의
// 낭독기 사용자도 메뉴로 순서를 바꿀 수 있어야 한다(같은 이동 저장 경로).
describe('move menu within a column', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([task('1', '문서 검토'), task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('moves a card up in its column, and offers no move past the top', async () => {
    const move = vi
      .spyOn(api, 'moveTask')
      .mockResolvedValue({ ...task('2', 'API 연결'), checklist: [] });
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await fireEvent.click(await screen.findByRole('button', { name: '문서 검토 옮기기' }));
    const up = await screen.findByRole('menuitem', { name: '이 열에서 위로' });
    expect(up.getAttribute('aria-disabled')).toBe('true');
    await fireEvent.keyDown(up, { key: 'Escape' });
    await fireEvent.click(await screen.findByRole('button', { name: 'API 연결 옮기기' }));
    await fireEvent.click(await screen.findByRole('menuitem', { name: '이 열에서 위로' }));
    await waitFor(() => {
      expect(move).toHaveBeenCalledTimes(1);
    });
    // 같은 열(할 일)의 "문서 검토" 앞으로.
    expect(move.mock.calls[0]?.slice(0, 3)).toEqual(['2', 'todo', '1']);
  });
});

// 기본 순서가 아닌 정렬(제목순)에서는 같은 열 안 재정렬을 막는다(D-79). 단추·메뉴·키보드가 같은 진입점의 규칙을 따른다.
describe('reordering under a non-default sort', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([task('1', '문서 검토'), task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('Alt+Up/Down sends no move in title order, while Alt+Right still changes the column', async () => {
    const move = vi
      .spyOn(api, 'moveTask')
      .mockResolvedValue({ ...task('1', '문서 검토'), status: 'in_progress', checklist: [] });
    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    await screen.findByRole('button', { name: '문서 검토 옮기기' });
    const card = document.querySelector<HTMLElement>('[data-task-card="1"]');
    if (card === null) throw new Error('card');
    await fireEvent.change(screen.getByRole('combobox', { name: '정렬' }), {
      target: { value: 'title' },
    });
    // 제목순에서 "문서 검토"는 두 번째다. 위로 옮기면 저장된 기본 순서(첫 번째)를 바꾸게 되므로 보내지 않는다.
    await fireEvent.keyDown(card, { key: 'ArrowUp', altKey: true });
    await fireEvent.keyDown(card, { key: 'ArrowDown', altKey: true });
    expect(move).not.toHaveBeenCalled();
    expect(
      [...document.querySelectorAll('[aria-live="polite"]')].map((e) => e.textContent).join(' '),
    ).toContain('열 안 순서를 바꿀 수 없습니다');
    await fireEvent.keyDown(card, { key: 'ArrowRight', altKey: true });
    await waitFor(() => {
      expect(move).toHaveBeenCalledTimes(1);
    });
    expect(move.mock.calls[0]?.slice(0, 3)).toEqual(['1', 'in_progress', null]);
  });
});
