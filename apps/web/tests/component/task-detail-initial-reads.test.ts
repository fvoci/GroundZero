import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { api, ApiError, NetworkError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { workApi } from '$lib/features/tasks/api';
import { taskData } from '$lib/features/tasks/detail-data';
import {
  taskScreenApi,
  type DetailRead,
  type DetailOptionsRead,
} from '$lib/features/tasks/screen-api';
import type { TaskResource } from '$lib/features/tasks/model';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { timeData } from '$lib/features/time/data';
import { setLocale } from '$lib/paraglide/runtime.js';
import { TASK_ITEM } from '../../stories/fixtures/synthetic';

function held<T>() {
  let resolve: (value: T) => void = () => undefined;
  let reject: (error: Error) => void = () => undefined;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

const resource: TaskResource = {
  id: 'parallel-task',
  item_id: TASK_ITEM.id,
  module_id: TASK_ITEM.module_id,
  workspace_id: TASK_ITEM.workspace_id,
  title: '병렬 상세',
  description: '',
  status: '할 일',
  status_category: 'todo',
  priority: 'none',
  milestone: false,
  tags: [],
  version: 1,
  move_version: 0,
  updated_at: '2026-10-31T00:00:00Z',
};

function aggregateRead(): DetailRead {
  return {
    viewer_id: 'viewer',
    task: resource,
    item: TASK_ITEM,
    archived: false,
    tasks: [resource],
    assignees: [],
    dependencies: [],
    series: [],
    checklist: [],
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.spyOn(workApi, 'task').mockResolvedValue(resource);
  vi.spyOn(workApi, 'tasks').mockResolvedValue([]);
  vi.spyOn(workApi, 'assignees').mockResolvedValue([]);
  vi.spyOn(workApi, 'dependencies').mockResolvedValue([]);
  vi.spyOn(workApi, 'series').mockResolvedValue([]);
  vi.spyOn(workApi, 'checklist').mockResolvedValue([]);
  vi.spyOn(api, 'item').mockResolvedValue(TASK_ITEM);
  vi.spyOn(taskData, 'taskStatuses').mockResolvedValue([]);
  vi.spyOn(taskData, 'assignable').mockResolvedValue([]);
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(timeData, 'taskTime').mockResolvedValue({ estimate_minutes: null, mine_seconds: 0 });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  sessionStorage.clear();
});

function mount(itemId = resource.item_id) {
  // A parent ID is a hint: a task URL may name a task belonging to another item.
  const props = {
    taskId: resource.id,
    itemId,
    tasks: [],
    workspaces: [],
    onchanged: () => undefined,
    onopen: () => undefined,
  };
  return render(TaskDetail, props);
}

it.each([401, 403, 404])(
  'clears the entire detail on an options permission error %s',
  async (status) => {
    vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(aggregateRead());
    vi.spyOn(taskScreenApi, 'options').mockRejectedValue(
      new ApiError(status, 'not_found', 'denied'),
    );
    render(TaskDetail, {
      taskId: resource.id,
      tasks: [],
      workspaces: [],
      aggregate: true,
      onchanged: () => undefined,
      onopen: () => undefined,
    });
    await screen.findByRole('alert');
    expect(screen.queryByLabelText('제목')).toBeNull();
    expect(taskData.taskStatuses).not.toHaveBeenCalled();
    expect(taskData.assignable).not.toHaveBeenCalled();
  },
);

it('starts every item-scope read while the task is pending, then composes an empty collection', async () => {
  const task = held<TaskResource>();
  vi.mocked(workApi.task).mockReturnValue(task.promise);
  mount();
  await waitFor(() => {
    expect(workApi.tasks).toHaveBeenCalledWith(resource.item_id);
    expect(api.item).toHaveBeenCalledWith(resource.item_id);
    for (const read of [workApi.assignees, workApi.dependencies, workApi.series, workApi.checklist])
      expect(read).toHaveBeenCalledWith({ item: resource.item_id });
  });
  expect(screen.queryByLabelText('제목')).toBeNull();
  expect(screen.queryByText('댓글이 없습니다.')).toBeNull();
  expect(activityApi.comments).not.toHaveBeenCalled();
  task.resolve(resource);
  expect(await screen.findByLabelText('제목')).toHaveValue(resource.title);
  expect(await screen.findByText('댓글이 없습니다.')).toBeVisible();
  expect(workApi.tasks).toHaveBeenCalledOnce();
});

it('keeps the detail loading until every parallel arm completes', async () => {
  const checklist = held<Awaited<ReturnType<typeof workApi.checklist>>>();
  vi.mocked(workApi.checklist).mockReturnValue(checklist.promise);
  mount();
  await waitFor(() => {
    expect(workApi.checklist).toHaveBeenCalledOnce();
  });
  expect(screen.queryByLabelText('제목')).toBeNull();
  expect(activityApi.comments).not.toHaveBeenCalled();
  checklist.resolve([]);
  expect(await screen.findByLabelText('제목')).toHaveValue(resource.title);
});

it.each(['tasks', 'assignees', 'dependencies', 'series', 'checklist', 'item'] as const)(
  'preserves a %s failure, ignores it until the task succeeds, and can reopen',
  async (arm) => {
    const task = held<TaskResource>();
    vi.mocked(workApi.task).mockReturnValueOnce(task.promise);
    const failed = arm === 'item' ? vi.mocked(api.item) : vi.mocked(workApi[arm]);
    failed.mockRejectedValueOnce(new NetworkError('scope failed'));
    const view = mount();
    await waitFor(() => {
      expect(failed).toHaveBeenCalledOnce();
    });
    await tick();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(screen.queryByLabelText('제목')).toBeNull();
    task.resolve(resource);
    expect(await screen.findByRole('alert')).toBeVisible();
    expect(screen.queryByLabelText('제목')).toBeNull();
    expect(activityApi.comments).not.toHaveBeenCalled();
    view.unmount();
    mount();
    expect(await screen.findByLabelText('제목')).toHaveValue(resource.title);
  },
);

it('keeps task 404 ahead of an early scope failure', async () => {
  const task = held<TaskResource>();
  vi.mocked(workApi.task).mockReturnValueOnce(task.promise);
  vi.mocked(workApi.tasks).mockRejectedValueOnce(new NetworkError('scope failed'));
  mount();
  await waitFor(() => {
    expect(workApi.tasks).toHaveBeenCalledOnce();
  });
  task.reject(new ApiError(404, 'not_found', 'missing'));
  expect(await screen.findByRole('alert')).toHaveTextContent('업무가 없거나 볼 권한이 없습니다.');
});

it('discards a failed hint scope and uses the task response item without narrowing accepted URLs', async () => {
  vi.mocked(api.item).mockImplementation((id) =>
    id === 'wrong-item'
      ? Promise.reject(new ApiError(404, 'not_found', 'hidden hint'))
      : Promise.resolve({ ...TASK_ITEM, access: 'read' }),
  );
  mount('wrong-item');
  expect(await screen.findByLabelText('제목')).toHaveValue(resource.title);
  expect(screen.getByLabelText('제목')).toBeDisabled();
  expect(workApi.tasks).toHaveBeenNthCalledWith(1, 'wrong-item');
  expect(workApi.tasks).toHaveBeenNthCalledWith(2, resource.item_id);
  expect(screen.queryByRole('alert')).toBeNull();
});

it('keeps standalone reads without a hint behind the authoritative task response', async () => {
  const task = held<TaskResource>();
  vi.mocked(workApi.task).mockReturnValueOnce(task.promise);
  const result = taskData.task(resource.id);
  expect(workApi.tasks).not.toHaveBeenCalled();
  task.resolve(resource);
  expect(await result).toMatchObject({
    id: resource.id,
    checklist: [],
    predecessors: [],
    assignees: [],
  });
});

it('ignores an old task response after the detail is replaced', async () => {
  const old = held<TaskResource>();
  vi.mocked(workApi.task).mockReturnValueOnce(old.promise);
  const view = mount();
  await waitFor(() => {
    expect(workApi.tasks).toHaveBeenCalledOnce();
  });
  vi.mocked(workApi.task).mockResolvedValue({ ...resource, id: 'next-task', title: '다음 상세' });
  await view.rerender({ taskId: 'next-task' });
  expect(await screen.findByLabelText('제목')).toHaveValue('다음 상세');
  old.resolve(resource);
  await tick();
  expect(screen.getByLabelText('제목')).toHaveValue('다음 상세');
});

it('does not keep another task scope status choices when the aggregate section fails', async () => {
  const read: DetailRead = {
    viewer_id: 'viewer',
    task: resource,
    item: TASK_ITEM,
    archived: false,
    tasks: [resource],
    assignees: [],
    dependencies: [],
    series: [],
    checklist: [],
  };
  const options: DetailOptionsRead = {
    viewer_id: 'viewer',
    item_id: TASK_ITEM.id,
    statuses: {
      state: 'ready',
      value: [
        {
          id: 'old-status',
          name: 'previous scope choice',
          color: '',
          position: 0,
          category: 'todo',
          task_count: 1,
        },
      ],
    },
    assignable: { state: 'ready' as const, value: [] },
  };
  vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(read);
  const optional = vi.spyOn(taskScreenApi, 'options').mockResolvedValue(options);
  const view = render(TaskDetail, {
    taskId: resource.id,
    tasks: [],
    workspaces: [],
    aggregate: true,
    onchanged: () => undefined,
    onopen: () => undefined,
  });
  expect(await screen.findByLabelText('제목')).toHaveValue(resource.title);
  await fireEvent.keyDown(screen.getByLabelText('상태'), { key: 'Enter' });
  expect(await screen.findByRole('option', { name: 'previous scope choice' })).toBeVisible();
  await fireEvent.keyDown(screen.getByLabelText('상태'), { key: 'Escape' });
  vi.mocked(taskScreenApi.detail).mockResolvedValue({
    ...read,
    task: { ...resource, id: 'next-task', title: '다음 집계 상세' },
  });
  optional.mockResolvedValue({ ...options, statuses: { state: 'failed', error: 'unavailable' } });
  await view.rerender({ taskId: 'next-task' });
  await waitFor(() => {
    expect(screen.getByLabelText('제목')).toHaveValue('다음 집계 상세');
  });
  expect(screen.getByRole('alert')).toHaveTextContent('업무를 불러오지 못했습니다.');
  await fireEvent.keyDown(screen.getByLabelText('상태'), { key: 'Enter' });
  expect(screen.queryByRole('option', { name: 'previous scope choice' })).toBeNull();
  expect(taskData.taskStatuses).not.toHaveBeenCalled();
  expect(taskData.assignable).not.toHaveBeenCalled();
});
