import './fixtures/authenticated';
import './fixtures/task-statuses';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, ApiError, type ShiftProposalView } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { taskViews } from '../../stories/fixtures/synthetic';

const seed = taskViews()[0];
if (seed === undefined) throw new Error('missing task fixture');
const predecessor = { ...seed, id: 'predecessor', reference: 'PRE', title: '선행' };
const base = {
  ...seed,
  start_date: null,
  due_date: null,
  checklist: [],
  predecessors: [{ task_id: predecessor.id, kind: 'FS' as const, lag_days: 0 }],
};
const next = { ...base, id: 'next-task', title: '새 업무', description: '새 업무의 입력' };
const oldShift: ShiftProposalView = {
  task_id: 'old-follower',
  reference: 'OLD',
  title: '옛 업무의 이동',
  base_version: 1,
  from_start_date: '2026-11-01',
  from_due_date: '2026-11-02',
  start_date: '2026-11-10',
  due_date: '2026-11-11',
  causes: [],
};
const newShift = {
  ...oldShift,
  task_id: 'new-follower',
  reference: 'NEW',
  title: '새 업무의 이동',
};
type Plan = Awaited<ReturnType<typeof work.previewSchedule>>;
const plan = (shifts: ShiftProposalView[]): Plan => ({ shifts, undated: [], blocked: null });
function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
}
async function settled() {
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
  await tick();
}
beforeEach(() => {
  vi.spyOn(work, 'task').mockResolvedValue(base);
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function mount() {
  const view = render(TaskDetail, {
    taskId: base.id,
    tasks: [base, predecessor],
    workspaces: [],
    onchanged: vi.fn(),
    onopen: vi.fn(),
  });
  await screen.findByLabelText('설명');
  return view;
}
async function edit(label: string, value: string) {
  await fireEvent.input(screen.getByLabelText(label), { target: { value } });
}
async function editLink(lag: string) {
  await fireEvent.click(screen.getByRole('button', { name: 'PRE 선행 관계 고치기' }));
  await edit('지연(일)', lag);
  await fireEvent.click(screen.getByRole('button', { name: '관계 저장' }));
}

const cases = [
  { moves: true, own: false },
  { moves: true, own: true },
  { moves: false, own: true },
  { moves: false, own: false },
];
it.each(cases)(
  'discards an obsolete task shift preview (moves=$moves, own=$own)',
  async ({ moves, own }) => {
    const pending = held<Plan>();
    const preview = vi
      .spyOn(work, 'previewSchedule')
      .mockResolvedValueOnce(plan([]))
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(plan([newShift]));
    const update = vi
      .spyOn(work, 'updateTask')
      .mockResolvedValueOnce({ kind: 'shift_conflict', current: { ...base, assignee_ids: [] } })
      .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
    const view = await mount();
    await edit('설명', '옛 업무의 입력');
    await edit('마감일', '2026-11-10');
    await fireEvent.click(screen.getByRole('button', { name: '저장' }));
    await waitFor(() => {
      expect(preview).toHaveBeenCalledTimes(2);
    });
    vi.mocked(work.task).mockResolvedValueOnce(next);
    await view.rerender({ taskId: next.id, tasks: [next, predecessor] });
    await waitFor(() => {
      expect(screen.getByLabelText('설명')).toHaveValue(next.description);
    });
    if (own) {
      await edit('마감일', '2026-11-20');
      await fireEvent.click(screen.getByRole('button', { name: '저장' }));
      await waitFor(() => {
        expect(document.querySelector('[data-shift-task="new-follower"]')).not.toBeNull();
      });
    } else screen.getByLabelText('설명').focus();
    pending.resolve(plan(moves ? [oldShift] : []));
    await settled();
    expect(document.querySelector('[data-shift-task="old-follower"]')).toBeNull();
    expect(screen.getByLabelText('설명')).toHaveValue(next.description);
    expect(update).toHaveBeenCalledOnce();
    if (own) {
      await fireEvent.click(screen.getByRole('button', { name: '후속 업무도 옮겨 저장' }));
      await waitFor(() => {
        expect(update).toHaveBeenCalledTimes(2);
      });
      expect(update.mock.calls[1]?.slice(0, 4)).toEqual([
        'user-a',
        next.id,
        next.version,
        { start_date: null, due_date: '2026-11-20', milestone: false },
      ]);
    } else {
      expect(screen.queryByRole('button', { name: '후속 업무도 옮겨 저장' })).toBeNull();
      expect(screen.getByLabelText('설명')).toHaveFocus();
    }
  },
);

it.each(cases)(
  'discards an obsolete link shift preview (moves=$moves, own=$own)',
  async ({ moves, own }) => {
    const pending = held<Plan>();
    const preview = vi
      .spyOn(work, 'previewLink')
      .mockResolvedValueOnce(plan([]))
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValueOnce(plan([newShift]));
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockResolvedValueOnce({ kind: 'shift_conflict', current: base })
      .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
    const view = await mount();
    await editLink('3');
    await waitFor(() => {
      expect(preview).toHaveBeenCalledTimes(2);
    });
    vi.mocked(work.task).mockResolvedValueOnce(next);
    await view.rerender({ taskId: next.id, tasks: [next, predecessor] });
    await waitFor(() => {
      expect(screen.getByLabelText('설명')).toHaveValue(next.description);
    });
    if (own) {
      await editLink('7');
      await waitFor(() => {
        expect(document.querySelector('[data-shift-task="new-follower"]')).not.toBeNull();
      });
    } else screen.getByLabelText('설명').focus();
    pending.resolve(plan(moves ? [oldShift] : []));
    await settled();
    expect(document.querySelector('[data-shift-task="old-follower"]')).toBeNull();
    expect(screen.getByLabelText('설명')).toHaveValue(next.description);
    expect(set).toHaveBeenCalledOnce();
    if (own) {
      await fireEvent.click(screen.getByRole('button', { name: '관계 저장하고 후속 업무 옮기기' }));
      await waitFor(() => {
        expect(set).toHaveBeenCalledTimes(2);
      });
      expect(set.mock.calls[1]?.slice(0, 6)).toEqual([
        'user-a',
        next.id,
        predecessor.id,
        'FS',
        7,
        { kind: 'FS', lag_days: 0 },
      ]);
    } else {
      expect(screen.queryByRole('button', { name: '관계 저장하고 후속 업무 옮기기' })).toBeNull();
      expect(screen.getByLabelText('설명')).toHaveFocus();
    }
  },
);

it.each([false, true])(
  'also discards an obsolete initial link preview (moves=%s)',
  async (moves) => {
    const pending = held<Plan>();
    const preview = vi.spyOn(work, 'previewLink').mockReturnValueOnce(pending.promise);
    const set = vi
      .spyOn(work, 'setPredecessor')
      .mockRejectedValue(new ApiError(503, 'unavailable', 'failed'));
    const view = await mount();
    await editLink('3');
    expect(preview).toHaveBeenCalledOnce();
    vi.mocked(work.task).mockResolvedValueOnce(next);
    await view.rerender({ taskId: next.id, tasks: [next, predecessor] });
    await waitFor(() => {
      expect(screen.getByLabelText('설명')).toHaveValue(next.description);
    });
    pending.resolve(plan(moves ? [oldShift] : []));
    await settled();
    expect(document.querySelector('[data-shift-task="old-follower"]')).toBeNull();
    expect(set).not.toHaveBeenCalled();
  },
);

it('clears the old link approval when another task opens during recalculation', async () => {
  const pending = held<Plan>();
  const preview = vi
    .spyOn(work, 'previewLink')
    .mockResolvedValueOnce(plan([oldShift]))
    .mockReturnValueOnce(pending.promise);
  const set = vi
    .spyOn(work, 'setPredecessor')
    .mockResolvedValueOnce({ kind: 'shift_conflict', current: base });
  const view = await mount();
  await editLink('3');
  await fireEvent.click(
    await screen.findByRole('button', { name: '관계 저장하고 후속 업무 옮기기' }),
  );
  await waitFor(() => {
    expect(preview).toHaveBeenCalledTimes(2);
  });
  vi.mocked(work.task).mockResolvedValueOnce(next);
  await view.rerender({ taskId: next.id, tasks: [next, predecessor] });
  await waitFor(() => {
    expect(screen.getByLabelText('설명')).toHaveValue(next.description);
  });
  pending.resolve(plan([oldShift]));
  await settled();
  expect(document.querySelector('[data-shift-task="old-follower"]')).toBeNull();
  expect(screen.queryByRole('button', { name: '관계 저장하고 후속 업무 옮기기' })).toBeNull();
  expect(set).toHaveBeenCalledOnce();
});
