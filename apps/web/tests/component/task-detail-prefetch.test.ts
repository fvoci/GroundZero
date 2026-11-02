import './fixtures/authenticated';
import { TEST_STATUSES } from './fixtures/task-statuses';
import { render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { taskData } from '$lib/features/tasks/detail-data';
import { taskScreenApi, type DetailRead } from '$lib/features/tasks/screen-api';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import { activityApi } from '$lib/features/activity/api';
import { timeData } from '$lib/features/time/data';
import { api } from '$lib/api/client';
import { TASK_ITEM, taskViews } from '../../stories/fixtures/synthetic';

afterEach(() => {
  vi.restoreAllMocks();
});

it('starts the authoritative detail read before waiting for recurrence synchronization', async () => {
  const task = taskViews()[0];
  if (task === undefined) throw new Error('missing task fixture');
  const order: string[] = [];
  vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
  vi.spyOn(taskData, 'syncRecurrences').mockImplementation(() => {
    order.push('sync');
    return new Promise<number>(() => undefined);
  });
  vi.spyOn(taskScreenApi, 'detail').mockImplementation(() => {
    order.push('detail');
    return new Promise<DetailRead>(() => undefined);
  });
  render(TaskBoard, { item: TASK_ITEM, workspaces: [], openTaskId: task.id, aggregate: true });
  await waitFor(() => {
    expect(taskScreenApi.detail).toHaveBeenCalledOnce();
    expect(taskData.syncRecurrences).toHaveBeenCalledOnce();
  });
  expect(order).toEqual(['detail', 'sync']);
});

it('fetches a fresh detail when the startup task is closed and reopened', async () => {
  const task = taskViews()[0];
  if (task === undefined) throw new Error('missing task fixture');
  const read: DetailRead = {
    viewer_id: 'user-a',
    task,
    item: TASK_ITEM,
    archived: false,
    tasks: [task],
    assignees: [],
    dependencies: [],
    series: [],
    checklist: [],
  };
  vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
  vi.spyOn(taskData, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(taskScreenApi, 'board').mockResolvedValue({
    ...read,
    item_access: 'write',
    statuses: TEST_STATUSES,
  });
  const detail = vi
    .spyOn(taskScreenApi, 'detail')
    .mockResolvedValueOnce(read)
    .mockResolvedValue({
      ...read,
      task: { ...task, title: '재열기 최신 업무' },
    });
  vi.spyOn(taskScreenApi, 'options').mockResolvedValue({
    viewer_id: 'user-a',
    item_id: TASK_ITEM.id,
    statuses: { state: 'ready', value: TEST_STATUSES },
    assignable: { state: 'ready', value: [] },
  });
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(timeData, 'taskTime').mockResolvedValue({ estimate_minutes: null, mine_seconds: 0 });
  const props = { item: TASK_ITEM, workspaces: [], aggregate: true };
  const view = render(TaskBoard, { ...props, openTaskId: task.id });
  expect(await screen.findByLabelText('제목')).toHaveValue(task.title);
  expect(detail).toHaveBeenCalledOnce();
  await view.rerender({ ...props, openTaskId: undefined });
  await waitFor(() => {
    expect(screen.queryByLabelText('제목')).toBeNull();
  });
  await view.rerender({ ...props, openTaskId: task.id });
  expect(await screen.findByLabelText('제목')).toHaveValue('재열기 최신 업무');
  expect(detail).toHaveBeenCalledTimes(2);
});
