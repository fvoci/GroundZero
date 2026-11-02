import './fixtures/task-statuses';
import './fixtures/authenticated';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import { taskData as work } from '$lib/features/tasks/detail-data';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { router } from '$lib/router.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { TASK_ITEM, taskViews } from '../../stories/fixtures/synthetic';

function required<T>(value: T | null | undefined): T {
  if (value == null) throw new Error('Expected title fixture element');
  return value;
}

afterEach(() => {
  vi.restoreAllMocks();
  router.guard = undefined;
});

it.each(['form', 'quick'] as const)(
  '#289 releases the hidden %s title after losing write permission',
  async (kind) => {
    await setLocale('ko', { reload: false });
    vi.spyOn(work, 'tasks').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
    router.navigate('/title-before');
    const view = render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: undefined });
    await screen.findByTestId('task-board');
    if (kind === 'quick')
      await fireEvent.click(required(screen.getAllByRole('button', { name: /^업무 추가$/ })[0]));
    const input =
      kind === 'form'
        ? screen.getByLabelText('새 업무 제목', { exact: true })
        : view.container.querySelector<HTMLInputElement>('input[id^="new-task-"]');
    expect(input).not.toBeNull();
    await fireEvent.input(required(input), { target: { value: 'hidden title' } });
    await view.rerender({
      item: { ...TASK_ITEM, access: 'read' },
      workspaces: [],
      openTaskId: undefined,
    });
    expect(screen.queryByDisplayValue('hidden title')).toBeNull();
    router.navigate('/title-after');
    expect(router.path).toBe('/title-after');
    expect(screen.queryByRole('alertdialog')).toBeNull();
  },
);

it.each(['form', 'quick'] as const)(
  '#289 keeps the %s save lock until the response after permission loss',
  async (kind) => {
    await setLocale('ko', { reload: false });
    vi.spyOn(work, 'tasks').mockResolvedValue([]);
    vi.spyOn(work, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
    let resolve!: (value: Awaited<ReturnType<typeof work.createTask>>) => void;
    vi.spyOn(work, 'createTask').mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    router.navigate('/saving-title-before');
    const view = render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: undefined });
    await screen.findByTestId('task-board');
    if (kind === 'quick')
      await fireEvent.click(required(screen.getAllByRole('button', { name: /^업무 추가$/ })[0]));
    const input =
      kind === 'form'
        ? screen.getByLabelText('새 업무 제목', { exact: true })
        : view.container.querySelector<HTMLInputElement>('input[id^="new-task-"]');
    await fireEvent.input(required(input), { target: { value: 'saving title' } });
    await fireEvent.submit(required(required(input).closest('form')));
    await waitFor(() => {
      expect(work.createTask).toHaveBeenCalledOnce();
    });
    await view.rerender({
      item: { ...TASK_ITEM, access: 'read' },
      workspaces: [],
      openTaskId: undefined,
    });
    router.navigate('/saving-title-after');
    expect(router.path).toBe('/saving-title-before');
    resolve({ ...required(taskViews()[0]), checklist: [] });
    await waitFor(() => {
      router.navigate('/saving-title-after');
      expect(router.path).toBe('/saving-title-after');
    });
  },
);
