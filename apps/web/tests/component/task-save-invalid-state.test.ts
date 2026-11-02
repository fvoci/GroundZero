import './fixtures/authenticated';
import './fixtures/task-statuses';
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, ApiError } from '$lib/api/client';
import { activityApi } from '$lib/features/activity/api';
import { taskData as work } from '$lib/features/tasks/detail-data';
import { taskScreenApi, type DetailRead } from '$lib/features/tasks/screen-api';
import TaskDetail from '$lib/features/tasks/TaskDetail.svelte';
import { me } from '$lib/me.svelte';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { TASK_ITEM, taskViews } from '../../stories/fixtures/synthetic';

const seed = taskViews()[0];
if (seed === undefined) throw new Error('missing task fixture');
const base = { ...seed, start_date: null, due_date: null, checklist: [] };
const mine = '보존할 내 설명';
function state(archived = false, access: DetailRead['item']['access'] = 'write'): DetailRead {
  return {
    viewer_id: 'user-a',
    task: base,
    item: { ...TASK_ITEM, access },
    archived,
    tasks: [base],
    assignees: [],
    dependencies: [],
    series: [],
    checklist: [],
  };
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
    tasks: [base],
    workspaces: [],
    onchanged: vi.fn(),
    onopen: vi.fn(),
  });
  await fireEvent.input(await screen.findByLabelText('설명'), { target: { value: mine } });
  return view;
}
function saveButton() {
  const button = document.getElementById('task-save');
  if (button === null) throw new Error('missing save button');
  return button;
}
const retryable = [
  'the change collided with another one; try again', // 40001 / 40P01
  'the data changed while saving; try again', // FV001
] as const;
it.each(retryable)('permits an explicit retry after invalid_state: %s', async (message) => {
  const read = vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(state());
  const update = vi
    .spyOn(work, 'updateTask')
    .mockRejectedValueOnce(new ApiError(409, 'invalid_state', message))
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  expect(read).not.toHaveBeenCalled();
  await fireEvent.click(saveButton());
  await waitFor(() => {
    expect(saveButton()).toBeEnabled();
  });
  expect(read).toHaveBeenCalledExactlyOnceWith(base.id);
  expect(update).toHaveBeenCalledTimes(1);
  expect(screen.getByLabelText('설명')).toHaveValue(mine);
  expect(Object.values(localStorage).join()).toContain(mine);
  await fireEvent.click(saveButton());
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(2);
  });
  expect(update.mock.calls[1]?.slice(0, 4)).toEqual([
    'user-a',
    base.id,
    base.version,
    { description: mine },
  ]);
  expect(read).toHaveBeenCalledTimes(1);
});

it.each(['archived', 'readonly'] as const)(
  'keeps input locked when invalid_state is confirmed as %s',
  async (kind) => {
    const read = vi
      .spyOn(taskScreenApi, 'detail')
      .mockResolvedValue(state(kind === 'archived', kind === 'readonly' ? 'read' : 'write'));
    const update = vi
      .spyOn(work, 'updateTask')
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', 'state changed'));
    await mount();
    await fireEvent.click(saveButton());
    await waitFor(() => {
      expect(read).toHaveBeenCalledOnce();
    });
    expect(document.getElementById('task-save')).toBeNull();
    expect(screen.getByLabelText('설명')).toBeDisabled();
    expect(screen.getByLabelText('설명')).toHaveValue(mine);
    expect(Object.values(localStorage).join()).toContain(mine);
    expect(update).toHaveBeenCalledOnce();
  },
);

it.each([403, 404])(
  'still blocks a direct %s without an extra read and preserves input',
  async (status) => {
    const read = vi.spyOn(taskScreenApi, 'detail');
    const update = vi
      .spyOn(work, 'updateTask')
      .mockRejectedValueOnce(
        new ApiError(status, status === 403 ? 'forbidden' : 'not_found', 'revoked'),
      );
    await mount();
    await fireEvent.click(saveButton());
    await waitFor(() => {
      expect(update).toHaveBeenCalledOnce();
    });
    await tick();
    if (status === 403) {
      expect(document.getElementById('task-save')).toBeNull();
      expect(screen.getByLabelText('설명')).toBeDisabled();
      expect(screen.getByLabelText('설명')).toHaveValue(mine);
    } else expect(screen.getByLabelText('설명')).not.toBeVisible();
    expect(Object.values(localStorage).join()).toContain(mine);
    expect(read).not.toHaveBeenCalled();
  },
);

it('keeps the chosen candidate retryable after invalid_state without resending it automatically', async () => {
  const read = vi.spyOn(taskScreenApi, 'detail').mockResolvedValue(state());
  const update = vi
    .spyOn(work, 'updateTask')
    .mockResolvedValueOnce({
      kind: 'conflict',
      current: { ...base, version: 2, description: '서버 설명', assignee_ids: [] },
    })
    .mockRejectedValueOnce(new ApiError(409, 'invalid_state', retryable[0]))
    .mockRejectedValueOnce(new ApiError(503, 'unavailable', 'failed'));
  await mount();
  await fireEvent.click(saveButton());
  const panel = await screen.findByRole('region', { name: '바뀐 내용 확인' });
  await fireEvent.click(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' }));
  const selected = within(panel).getByRole('button', { name: '선택한 내용으로 저장' });
  await fireEvent.click(selected);
  await waitFor(() => {
    expect(selected).toBeEnabled();
  });
  expect(read).toHaveBeenCalledOnce();
  expect(update).toHaveBeenCalledTimes(2);
  expect(within(panel).getByRole('radio', { name: '설명: 내 입력 사용' })).toBeChecked();
  expect(Object.values(localStorage).join()).toContain(mine);
  await fireEvent.click(selected);
  await waitFor(() => {
    expect(update).toHaveBeenCalledTimes(3);
  });
  expect(update.mock.calls[2]?.[2]).toBe(2);
  expect(update.mock.calls[2]?.[3]).toEqual({ description: mine });
});

it.each([403, 404, 503])(
  'handles a failed access recheck %s while preserving input',
  async (status) => {
    const read = vi
      .spyOn(taskScreenApi, 'detail')
      .mockRejectedValueOnce(
        new ApiError(status, status === 503 ? 'unavailable' : 'forbidden', 'read failed'),
      );
    const update = vi
      .spyOn(work, 'updateTask')
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', retryable[0]));
    await mount();
    await fireEvent.click(saveButton());
    await waitFor(() => {
      expect(read).toHaveBeenCalledOnce();
      if (status === 503) expect(saveButton()).toBeEnabled();
      else expect(document.getElementById('task-save')).toBeNull();
    });
    if (status === 503) expect(saveButton()).toBeEnabled();
    else expect(screen.getByLabelText('설명')).toBeDisabled();
    if (status === 404) expect(screen.getByLabelText('설명')).not.toBeVisible();
    expect(screen.getByLabelText('설명')).toHaveValue(mine);
    expect(Object.values(localStorage).join()).toContain(mine);
    expect(update).toHaveBeenCalledOnce();
  },
);

it.each(['viewer', 'session', 'task'] as const)(
  'discards an access recheck from an obsolete %s context',
  async (kind) => {
    let resolve!: (value: DetailRead) => void;
    const pending = new Promise<DetailRead>((yes) => {
      resolve = yes;
    });
    const read = vi.spyOn(taskScreenApi, 'detail').mockReturnValueOnce(pending);
    const update = vi
      .spyOn(work, 'updateTask')
      .mockRejectedValueOnce(new ApiError(409, 'invalid_state', retryable[0]));
    const view = await mount();
    await fireEvent.click(saveButton());
    await waitFor(() => {
      expect(read).toHaveBeenCalledOnce();
    });
    let description = mine;
    if (kind === 'task') {
      description = '다른 업무 입력';
      const other = { ...base, id: 'other-task', description };
      vi.mocked(work.task).mockResolvedValueOnce(other);
      await view.rerender({ taskId: other.id, tasks: [other] });
      await waitFor(() => {
        expect(screen.getByLabelText('설명')).toHaveValue(description);
      });
      screen.getByLabelText('설명').focus();
    } else if (kind === 'session') {
      for (const sequence of ['B', 'A'])
        localStorage.setItem(
          'fvoci:auth-session',
          JSON.stringify({
            state: 'settled',
            generation: readSessionGeneration() + 1,
            sequence,
          }),
        );
    }
    resolve({ ...state(true, 'read'), viewer_id: kind === 'viewer' ? 'user-b' : 'user-a' });
    await pending;
    await waitFor(() => {
      expect(saveButton()).toBeEnabled();
    });
    expect(screen.getByLabelText('설명')).toHaveValue(description);
    if (kind === 'task') expect(screen.getByLabelText('설명')).toHaveFocus();
    expect(me.userId).toBe('user-a');
    expect(update).toHaveBeenCalledOnce();
  },
);
