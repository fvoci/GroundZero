import './fixtures/authenticated';
import { TEST_STATUSES } from './fixtures/task-statuses';
import { cleanup, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { ApiError, api } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { savedViewsApi } from '$lib/features/events/saved-views-api';
import {
  taskScreenApi,
  type BoardRead,
  type DetailRead,
  type DetailOptionsRead,
} from '$lib/features/tasks/screen-api';
import TaskBoard from '$lib/features/tasks/TaskBoard.svelte';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { DiscardedTaskRead } from '$lib/features/tasks/context-read';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import {
  createTaskSessionRecovery,
  TASK_SESSION_RECOVERY,
} from '$lib/features/tasks/session-recovery';
import { timeData } from '$lib/features/time/data';
import { TASK_ITEM, taskViews } from '../../stories/fixtures/synthetic';

function held<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}
function changeSession(): void {
  for (const sequence of ['B', 'A']) {
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({
        state: 'settled',
        generation: readSessionGeneration() + 1,
        sequence,
      }),
    );
  }
}
async function settled(): Promise<void> {
  // Drain the response's promise chain before asserting that no state was adopted.
  await new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
  await tick();
}
const task = taskViews()[0];
if (task === undefined) throw new Error('missing task fixture');
const detail: DetailRead = {
  viewer_id: 'user-a',
  task: { ...task, title: 'stale session task' },
  item: TASK_ITEM,
  tasks: [task],
  archived: false,
  assignees: [],
  dependencies: [],
  series: [],
  checklist: [],
};
const board: BoardRead = {
  ...detail,
  tasks: [detail.task],
  item_access: 'read',
  statuses: TEST_STATUSES,
};
const options: DetailOptionsRead = {
  viewer_id: 'user-a',
  item_id: TASK_ITEM.id,
  statuses: {
    state: 'ready',
    value: [
      {
        id: 'old-status',
        name: 'stale session status',
        category: 'todo',
        color: '',
        position: 0,
        task_count: 0,
      },
    ],
  },
  assignable: {
    state: 'ready',
    value: [{ user_id: 'stale-person', display_name: 'stale session person' }],
  },
};
function recoveryContext() {
  vi.spyOn(api, 'me').mockResolvedValue({
    id: 'user-a',
    display_name: 'A',
    is_server_admin: false,
    hour_cycle: '',
    locale: 'ko',
    time_zone: '',
    week_start: '',
    theme: 'system',
    limits: {
      max_title_chars: 200,
      max_document_bytes: 2_000_000,
      max_references_per_document: 500,
      max_document_depth: 10,
    },
  });
  return new Map([
    [
      TASK_SESSION_RECOVERY,
      createTaskSessionRecovery(async (userId) => {
        const generation = readSessionGeneration();
        const safetyGeneration = accountSafety.generation;
        const checked = await api.me();
        if (checked?.id !== userId) return;
        return { userId, sessionGeneration: generation, safetyGeneration };
      }),
    ],
  ]);
}
beforeEach(() => {
  vi.spyOn(savedViewsApi, 'savedViews').mockResolvedValue([]);
  vi.spyOn(activityApi, 'comments').mockResolvedValue([]);
  vi.spyOn(activityApi, 'mark').mockResolvedValue({ pin: false, favorite: false });
  vi.spyOn(api, 'taskBacklinks').mockResolvedValue([]);
  vi.spyOn(timeData, 'taskTime').mockResolvedValue({ estimate_minutes: null, mine_seconds: 0 });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  accountSafety.switching = false;
  accountSafety.restoring = false;
});

it.each(['success', 'failure'] as const)(
  'SR-298-2 discards a direct board %s after A → B → A',
  async (outcome) => {
    const pending = held<BoardRead>();
    vi.spyOn(taskScreenApi, 'board').mockReturnValue(pending.promise);
    render(TaskBoard, {
      item: { ...TASK_ITEM, access: 'read' },
      workspaces: [],
      aggregate: true,
      openTaskId: undefined,
    });
    await waitFor(() => {
      expect(taskScreenApi.board).toHaveBeenCalledOnce();
    });
    changeSession();
    if (outcome === 'success') pending.resolve(board);
    else pending.reject(new ApiError(403, 'forbidden', 'B session failure'));
    await settled();
    expect(screen.queryByText('stale session task')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

it.each(['success', 'failure'] as const)(
  'SR-298-2 discards a fallback detail %s after A → B → A',
  async (outcome) => {
    const pending = held<DetailRead>();
    vi.spyOn(taskScreenApi, 'detail').mockReturnValue(pending.promise);
    vi.spyOn(taskScreenApi, 'options').mockResolvedValue(options);
    render(TaskDetail, {
      taskId: task.id,
      tasks: [],
      workspaces: [],
      aggregate: true,
      onchanged: () => undefined,
      onopen: () => undefined,
    });
    await waitFor(() => {
      expect(taskScreenApi.detail).toHaveBeenCalledOnce();
    });
    changeSession();
    if (outcome === 'success') pending.resolve(detail);
    else pending.reject(new ApiError(403, 'forbidden', 'B session failure'));
    await settled();
    expect(screen.queryByLabelText('제목')).toBeNull();
    expect(taskScreenApi.options).not.toHaveBeenCalled();
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

it.each(['success', 'failure'] as const)(
  'SR-298-2 discards detail options %s after A → B → A',
  async (outcome) => {
    const pending = held<DetailOptionsRead>();
    vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
    vi.spyOn(taskScreenApi, 'options').mockReturnValue(pending.promise);
    render(TaskDetail, {
      taskId: task.id,
      tasks: [],
      workspaces: [],
      aggregate: true,
      onchanged: () => undefined,
      onopen: () => undefined,
    });
    await screen.findByLabelText('제목');
    await waitFor(() => {
      expect(taskScreenApi.options).toHaveBeenCalledOnce();
    });
    changeSession();
    if (outcome === 'success') pending.resolve(options);
    else pending.reject(new ApiError(403, 'forbidden', 'B session failure'));
    await settled();
    expect(screen.getByLabelText('제목')).toHaveValue(detail.task.title);
    expect(screen.queryByText('stale session person')).toBeNull();
    expect(screen.queryByText('stale session status')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

it.each(['success', 'failure'] as const)(
  'SR-298-2 keeps the latest board when an older request finishes with %s',
  async (outcome) => {
    const pending = held<BoardRead>();
    vi.spyOn(taskScreenApi, 'board')
      .mockReturnValueOnce(pending.promise)
      .mockResolvedValue({
        ...board,
        tasks: [{ ...detail.task, title: 'current request task' }],
      });
    const item = { ...TASK_ITEM, access: 'read' as const };
    const view = render(TaskBoard, {
      item,
      workspaces: [],
      aggregate: true,
      openTaskId: undefined,
    });
    await waitFor(() => {
      expect(taskScreenApi.board).toHaveBeenCalledOnce();
    });
    await view.rerender({ item: { ...item, title: 'refreshed item' } });
    await screen.findByText('current request task');
    if (outcome === 'success') pending.resolve(board);
    else pending.reject(new ApiError(403, 'forbidden', 'old request failure'));
    await settled();
    expect(screen.getByText('current request task')).toBeVisible();
    expect(screen.queryByText('stale session task')).toBeNull();
    expect(screen.queryByRole('alert')).toBeNull();
  },
);

it('CR-1 retries a discarded initial detail once in the current safe screen', async () => {
  const read = vi.fn(() => Promise.reject<DetailRead>(new DiscardedTaskRead()));
  vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
  vi.spyOn(taskScreenApi, 'options').mockResolvedValue(options);
  render(TaskDetail, {
    context: recoveryContext(),
    props: {
      taskId: task.id,
      tasks: [],
      workspaces: [],
      aggregate: true,
      initialRead: () => ({ take: read }),
      onchanged: () => undefined,
      onopen: () => undefined,
    },
  });
  expect(await screen.findByLabelText('제목')).toHaveValue(detail.task.title);
  expect(read).toHaveBeenCalledOnce();
  expect(taskScreenApi.detail).toHaveBeenCalledOnce();
  expect(api.me).toHaveBeenCalledOnce();
  expect(screen.queryByRole('alert')).toBeNull();
});

it.each(['switching', 'restoring', 'stale-session', 'hidden'] as const)(
  'CR-1 does not retry a discarded initial detail while %s',
  async (state) => {
    const pending = held<DetailRead>();
    const take = vi.fn(() => pending.promise);
    vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
    render(TaskDetail, {
      taskId: task.id,
      tasks: [],
      workspaces: [],
      aggregate: true,
      initialRead: () => ({ take }),
      onchanged: () => undefined,
      onopen: () => undefined,
    });
    await waitFor(() => {
      expect(take).toHaveBeenCalledOnce();
    });
    if (state === 'stale-session') changeSession();
    else if (state === 'hidden') me.userId = 'other-user';
    else accountSafety[state] = true;
    pending.reject(new DiscardedTaskRead());
    await settled();
    expect(taskScreenApi.detail).not.toHaveBeenCalled();
    expect(screen.queryByLabelText('제목')).toBeNull();
    accountSafety.switching = false;
    accountSafety.restoring = false;
  },
);

it('CR-1 propagates a current failure from the single fresh detail retry', async () => {
  vi.spyOn(taskScreenApi, 'detail').mockRejectedValue(
    new ApiError(403, 'forbidden', 'fresh failure'),
  );
  render(TaskDetail, {
    context: recoveryContext(),
    props: {
      taskId: task.id,
      tasks: [],
      workspaces: [],
      aggregate: true,
      initialRead: () => ({ take: () => Promise.reject(new DiscardedTaskRead()) }),
      onchanged: () => undefined,
      onopen: () => undefined,
    },
  });
  await screen.findByRole('alert');
  expect(taskScreenApi.detail).toHaveBeenCalledOnce();
  expect(api.me).toHaveBeenCalledOnce();
  expect(screen.queryByLabelText('제목')).toBeNull();
});

it('CR-1 preserves an ordinary initial permission failure without retrying', async () => {
  vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(detail);
  render(TaskDetail, {
    taskId: task.id,
    tasks: [],
    workspaces: [],
    aggregate: true,
    initialRead: () => ({
      take: () => Promise.reject(new ApiError(403, 'forbidden', 'initial failure')),
    }),
    onchanged: () => undefined,
    onopen: () => undefined,
  });
  await screen.findByRole('alert');
  expect(taskScreenApi.detail).not.toHaveBeenCalled();
  expect(screen.queryByLabelText('제목')).toBeNull();
});
