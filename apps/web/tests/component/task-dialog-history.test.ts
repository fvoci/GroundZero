import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { ItemView, TaskView } from '$lib/api/client';
import { api } from '$lib/api/client';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

afterEach(() => {
  vi.restoreAllMocks();
});

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
  updated_at: '2026-10-19T00:00:00Z',
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
    predecessor_ids: [],
    series: null,
    occurrence_date: null,
    assignees: [],
    checklist_done: 0,
    checklist_total: 0,
    version: 1,
    completed_at: null,
    access: 'write',
    updated_at: '2026-10-19T00:00:00Z',
  };
}

// 칸반 맥락(목표 3): 카드를 열었다 닫은 뒤 브라우저 뒤로 가기는 보드를 떠나야 한다. 닫기가 새 기록을 쌓으면 뒤로 가기가
// 방금 닫은 카드를 다시 연다.
describe('task dialog and browser history', () => {
  it('closing a card opened from the board goes back instead of adding an entry', async () => {
    await setLocale('ko', { reload: false });
    window.history.replaceState(null, '', '/w/ws-1/i/i1');
    vi.spyOn(api, 'tasks').mockResolvedValue([task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(api, 'task').mockReturnValue(new Promise(() => undefined));
    vi.spyOn(api, 'assignable').mockResolvedValue([]);
    const push = vi.spyOn(window.history, 'pushState');
    const back = vi.spyOn(window.history, 'back').mockImplementation(() => undefined);

    const view = render(TaskBoard, {
      props: { item, workspaces: [], openTaskId: undefined },
    });
    await fireEvent.click(await screen.findByRole('button', { name: /API 연결/ }));
    expect(push).toHaveBeenCalledTimes(1);
    // 앱에서는 주소가 바뀌면 부모가 연 업무를 넘긴다.
    await view.rerender({ item, workspaces: [], openTaskId: '2' });
    await waitFor(() => {
      expect(screen.getByRole('dialog')).toBeTruthy();
    });
    await fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    await waitFor(() => {
      expect(back).toHaveBeenCalledTimes(1);
    });
    expect(push).toHaveBeenCalledTimes(1);
  });
});

// 칸반 맥락(목표 3): 다른 화면에 갔다가 돌아와도(보드가 새로 그려짐) 보기·필터·정렬이 그대로다.
describe('board context across navigation', () => {
  it('restores the view and filter when the board is shown again', async () => {
    await setLocale('ko', { reload: false });
    window.sessionStorage.clear();
    vi.spyOn(api, 'tasks').mockResolvedValue([task('2', 'API 연결')]);
    vi.spyOn(api, 'savedViews').mockResolvedValue([]);
    vi.spyOn(api, 'syncRecurrences').mockResolvedValue(0);
    const first = render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const status = await screen.findByLabelText('상태');
    await fireEvent.change(status, { target: { value: 'in_progress' } });
    await fireEvent.click(screen.getByRole('button', { name: '목록' }));
    first.unmount();

    render(TaskBoard, { props: { item, workspaces: [], openTaskId: undefined } });
    const again = await screen.findByLabelText('상태');
    expect(again instanceof HTMLSelectElement ? again.value : '').toBe('in_progress');
    expect(screen.getByRole('button', { name: '목록' }).getAttribute('aria-pressed')).toBe('true');
  });
});
