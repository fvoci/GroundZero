import { afterEach, expect, it, vi } from 'vitest';
import { startTaskReads } from '$lib/features/tasks/startup-reads';
import type { ContextCapture } from '$lib/features/tasks/context-read';
import { taskData } from '$lib/features/tasks/detail-data';
import { accountSafety } from '$lib/features/feedback/feedback.svelte';
import { taskScreenApi, type BoardRead, type DetailRead } from '$lib/features/tasks/screen-api';
import { PROJECT_MODULE, TASK_ITEM } from '../../stories/fixtures/synthetic';

const context = { viewer_id: 'viewer', item: TASK_ITEM, module: PROJECT_MODULE, documents: [] };
const capture = (): ContextCapture => ({
  userId: 'viewer',
  sequence: 1,
  navigationGeneration: 1,
  sessionGeneration: 1,
  safetyGeneration: 1,
  path: '/tasks',
});
const board: BoardRead = {
  viewer_id: 'viewer',
  archived: false,
  item_access: 'write',
  tasks: [],
  statuses: [],
  assignees: [],
  dependencies: [],
  series: [],
  checklist: [],
};
function held<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
afterEach(() => {
  vi.restoreAllMocks();
  accountSafety.switching = false;
  accountSafety.restoring = false;
});

it('starts detail and sync immediately, then waits for sync before the board', async () => {
  const sync = held<number>();
  const detail = held<DetailRead>();
  const order: string[] = [];
  vi.spyOn(taskScreenApi, 'detail').mockImplementation(() => {
    order.push('detail');
    return detail.promise;
  });
  vi.spyOn(taskData, 'syncRecurrences').mockImplementation(() => {
    order.push('sync');
    return sync.promise;
  });
  vi.spyOn(taskScreenApi, 'board').mockImplementation(() => {
    order.push('board');
    return Promise.resolve(board);
  });
  const current = capture();
  const reads = startTaskReads(current, () => current, TASK_ITEM.id, context, 'task');
  expect(order).toEqual(['detail', 'sync']);
  sync.resolve(0);
  expect(await reads.board.take()).toEqual(board);
  expect(order).toEqual(['detail', 'sync', 'board']);
  expect(taskData.syncRecurrences).toHaveBeenCalledWith('viewer', TASK_ITEM.id);
});

it.each([
  'userId',
  'sequence',
  'navigationGeneration',
  'sessionGeneration',
  'safetyGeneration',
  'path',
] as const)('does not start a late board after %s changes during sync', async (field) => {
  const sync = held<number>();
  let current = capture();
  vi.spyOn(taskData, 'syncRecurrences').mockReturnValue(sync.promise);
  vi.spyOn(taskScreenApi, 'board').mockResolvedValue(board);
  const reads = startTaskReads(current, () => current, TASK_ITEM.id, context);
  current = { ...current, [field]: typeof current[field] === 'number' ? 99 : 'changed' };
  sync.resolve(0);
  await expect(reads.board.take()).rejects.toThrow();
  expect(taskScreenApi.board).not.toHaveBeenCalled();
});

it.each(['viewer', 'scope'] as const)(
  'rejects a mismatched context %s before any task request',
  (kind) => {
    vi.spyOn(taskData, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(taskScreenApi, 'board').mockResolvedValue(board);
    const current = capture();
    expect(() =>
      startTaskReads(
        current,
        () => current,
        kind === 'scope' ? 'other' : TASK_ITEM.id,
        kind === 'viewer' ? { ...context, viewer_id: 'other' } : context,
      ),
    ).toThrow();
    expect(taskData.syncRecurrences).not.toHaveBeenCalled();
    expect(taskScreenApi.board).not.toHaveBeenCalled();
  },
);

it.each(['read', 'archived'] as const)(
  'keeps %s contexts free of synchronization writes',
  async (kind) => {
    vi.spyOn(taskData, 'syncRecurrences').mockResolvedValue(0);
    vi.spyOn(taskScreenApi, 'board').mockResolvedValue(board);
    const current = capture();
    const reads = startTaskReads(current, () => current, TASK_ITEM.id, {
      ...context,
      item: {
        ...TASK_ITEM,
        access: kind === 'read' ? 'read' : 'write',
        archived: kind === 'archived',
      },
    });
    expect(await reads.board.take()).toEqual(board);
    expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  },
);

it.each(['switching', 'restoring'] as const)('does not start writes while %s', (flag) => {
  vi.spyOn(taskData, 'syncRecurrences').mockResolvedValue(0);
  vi.spyOn(taskScreenApi, 'board').mockResolvedValue(board);
  accountSafety[flag] = true;
  const current = capture();
  expect(() => startTaskReads(current, () => current, TASK_ITEM.id, context)).toThrow();
  expect(taskData.syncRecurrences).not.toHaveBeenCalled();
  expect(taskScreenApi.board).not.toHaveBeenCalled();
});

it.each(['switching', 'restoring'] as const)(
  'does not start a board if %s begins during sync',
  async (flag) => {
    const sync = held<number>();
    vi.spyOn(taskData, 'syncRecurrences').mockReturnValue(sync.promise);
    vi.spyOn(taskScreenApi, 'board').mockResolvedValue(board);
    const current = capture();
    const reads = startTaskReads(current, () => current, TASK_ITEM.id, context);
    accountSafety[flag] = true;
    sync.resolve(0);
    await expect(reads.board.take()).rejects.toThrow();
    expect(taskScreenApi.board).not.toHaveBeenCalled();
  },
);
