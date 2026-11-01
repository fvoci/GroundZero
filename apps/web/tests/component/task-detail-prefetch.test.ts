import './fixtures/authenticated';
import './fixtures/task-statuses';
import { render, waitFor } from '@testing-library/svelte';
import { afterEach, expect, it, vi } from 'vitest';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import { taskData } from '$lib/features/tasks/detail-data';
import { taskScreenApi, type DetailRead } from '$lib/features/tasks/screen-api';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
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
