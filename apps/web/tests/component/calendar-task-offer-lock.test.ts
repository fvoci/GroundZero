import { taskData as work } from '$lib/features/tasks/detail-data';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TaskDetailView } from '$lib/api/client';

import { formDraftTabKey } from '$lib/features/common/form-draft';
import CalendarTaskDialog from '$lib/features/events/CalendarTaskDialog.svelte';
import { m } from '$lib/paraglide/messages.js';

import './fixtures/event-user';

// R5 delta2 D2-F-1: 날짜 칸 업무 만들기에서 다른 탭의 초안 되살리기 단추가 저장 중에도 눌렸다. 되살리면 스냅숏이 바뀌어
// 서버에 생긴 업무를 화면이 버리고(`oncreated` 없음), 되살린 입력으로 다시 내면 새 요청 ID로 업무가 하나 더 생겼다.
// 저장하는 동안은 초안 제안도 입력과 함께 잠근다.

afterEach(() => {
  vi.restoreAllMocks();
});

function held<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  return { promise, resolve };
}

const created: TaskDetailView = {
  id: 't1',
  item_id: 'i1',
  module_id: 'm1',
  workspace_id: 'ws-1',
  parent_id: null,
  reference: 'KED-9',
  title: '내 업무',
  description: '',
  status: 'todo',
  priority: 'none',
  start_date: null,
  due_date: '2026-10-20',
  estimate_minutes: null,
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
  move_version: 1,
  completed_at: null,
  access: 'write',
  updated_at: '2026-10-21T00:00:00Z',
  checklist: [],
};

describe('calendar task creation with another tab draft', () => {
  it('locks the draft offer while saving so a restore cannot create a second task', async () => {
    const own = formDraftTabKey('calendar-task:ws-1:i1:2026-10-20');
    if (own === undefined) throw new Error('draft key');
    localStorage.setItem(
      own.replace(/:tab:.*$/, ':tab:other-tab'),
      JSON.stringify({
        savedAt: '2026-10-19T00:00:00Z',
        input: { date: '2026-10-20', title: '다른 탭', module: '', item: 'i1', retry: null },
      }),
    );
    const request = held<TaskDetailView>();
    const create = vi.spyOn(work, 'createTask').mockReturnValueOnce(request.promise);
    const oncreated = vi.fn();
    render(CalendarTaskDialog, {
      props: {
        workspaceId: 'ws-1',
        day: '2026-10-20',
        item: { id: 'i1', title: '항목' },
        onclose: vi.fn(),
        oncreated,
      },
    });
    const restore = await screen.findByRole('button', { name: m.common_draft_restore() });
    const discard = screen.getByRole('button', { name: m.common_draft_discard() });
    const title = screen.getByLabelText(m.events_task_name());
    await fireEvent.input(title, { target: { value: '내 업무' } });
    await fireEvent.click(screen.getByRole('button', { name: m.events_task_create() }));
    await waitFor(() => {
      expect(create).toHaveBeenCalledOnce();
    });

    expect(restore).toBeDisabled();
    expect(discard).toBeDisabled();

    request.resolve(created);
    await waitFor(() => {
      expect(oncreated).toHaveBeenCalledOnce();
    });
    expect(create).toHaveBeenCalledOnce();
  });
});
