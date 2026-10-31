import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';

import { NetworkError, type AssigneeView } from '$lib/api/client';
import { taskData as work } from '$lib/features/tasks/detail-data';
import TaskCsvImport from '$lib/features/tasks/TaskCsvImport.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';
import { taskViews } from '../../stories/fixtures/synthetic';
import { TEST_STATUSES } from './fixtures/task-statuses';

function held<T>() {
  let resolve: (value: T) => void = () => undefined;
  let reject: (error: Error) => void = () => undefined;
  const promise = new Promise<T>((ok, fail) => {
    resolve = ok;
    reject = fail;
  });
  return { promise, resolve, reject };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'csv-reader';
  vi.spyOn(work, 'assignable').mockResolvedValue([]);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
  sessionStorage.clear();
  me.userId = '';
});

function mount() {
  const onimported = vi.fn(() => Promise.resolve());
  const view = render(TaskCsvImport, { itemId: 'item-1', statuses: TEST_STATUSES, onimported });
  return { ...view, onimported };
}

async function toggle() {
  const button = screen.getByRole('button', { name: '여러 업무 한꺼번에 등록' });
  button.focus();
  await fireEvent.click(button);
  return button;
}

it('defers candidates until expansion and retains input, focus and a pending read across collapse', async () => {
  const candidates = held<AssigneeView[]>();
  vi.mocked(work.assignable).mockReturnValue(candidates.promise);
  mount();
  await tick();
  expect(work.assignable).not.toHaveBeenCalled();
  const button = await toggle();
  await waitFor(() => {
    expect(work.assignable).toHaveBeenCalledWith('item-1');
  });
  expect(button).toHaveFocus();
  expect(screen.getByRole('status')).toHaveTextContent('불러오는 중');
  const input = screen.getByLabelText(/한 줄에 한 업무/);
  await fireEvent.input(input, { target: { value: '보존한 제목' } });
  await toggle();
  await toggle();
  expect(screen.getByLabelText(/한 줄에 한 업무/)).toHaveValue('보존한 제목');
  expect(work.assignable).toHaveBeenCalledOnce();
  candidates.resolve([]);
  await waitFor(() => {
    expect(screen.getByRole('status')).toBeEmptyDOMElement();
  });
  expect(screen.getByRole('button', { name: '등록' })).toBeEnabled();
  await toggle();
  await toggle();
  expect(work.assignable).toHaveBeenCalledOnce();
});

it('keeps a failed candidate read visible, retries without losing input and restores keyboard focus', async () => {
  const retry = held<AssigneeView[]>();
  vi.mocked(work.assignable)
    .mockRejectedValueOnce(new NetworkError('offline'))
    .mockReturnValueOnce(retry.promise);
  mount();
  await toggle();
  const button = await screen.findByRole('button', { name: '다시 시도' });
  expect(screen.getByRole('status')).toHaveTextContent('담당');
  const input = screen.getByLabelText(/한 줄에 한 업무/);
  await fireEvent.input(input, { target: { value: '다시 시도할 제목' } });
  button.focus();
  await fireEvent.click(button);
  expect(button).toBeDisabled();
  retry.resolve([]);
  await waitFor(() => {
    expect(input).toHaveFocus();
  });
  expect(input).toHaveValue('다시 시도할 제목');
  expect(work.assignable).toHaveBeenCalledTimes(2);
});

it('does not steal focus moved elsewhere during a candidate retry', async () => {
  const retry = held<AssigneeView[]>();
  vi.mocked(work.assignable)
    .mockRejectedValueOnce(new NetworkError('offline'))
    .mockReturnValueOnce(retry.promise);
  mount();
  const toggleButton = await toggle();
  const button = await screen.findByRole('button', { name: '다시 시도' });
  button.focus();
  await fireEvent.click(button);
  toggleButton.focus();
  retry.resolve([]);
  await waitFor(() => {
    expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
  });
  expect(toggleButton).toHaveFocus();
});

it('still accepts rows without assignees while candidates are pending or unavailable', async () => {
  const candidates = held<AssigneeView[]>();
  vi.mocked(work.assignable).mockReturnValue(candidates.promise);
  const task = taskViews()[0];
  if (task === undefined) throw new Error('fixture task missing');
  vi.spyOn(work, 'createTask').mockImplementation(
    (_user, itemId, _request, title, _parent, fields) =>
      Promise.resolve({
        ...task,
        item_id: itemId,
        title,
        description: fields?.description ?? '',
        priority: fields?.priority ?? 'none',
        start_date: fields?.start_date ?? null,
        due_date: fields?.due_date ?? null,
        estimate_minutes: fields?.estimate_minutes ?? null,
        tags: fields?.tags ?? [],
        milestone: false,
        assignees: [],
        checklist: [],
      }),
  );
  const { onimported } = mount();
  await toggle();
  await fireEvent.input(screen.getByLabelText(/한 줄에 한 업무/), {
    target: { value: '후보 없는 업무' },
  });
  await fireEvent.click(screen.getByRole('button', { name: '등록' }));
  await waitFor(() => {
    expect(onimported).toHaveBeenCalledOnce();
  });
  candidates.reject(new NetworkError('offline'));
  expect(await screen.findByRole('button', { name: '다시 시도' })).toBeVisible();
  expect(work.createTask).toHaveBeenCalledOnce();
  await fireEvent.input(screen.getByLabelText(/한 줄에 한 업무/), {
    target: { value: '후보 실패 뒤 업무' },
  });
  await fireEvent.click(screen.getByRole('button', { name: '등록' }));
  await waitFor(() => {
    expect(onimported).toHaveBeenCalledTimes(2);
  });
  expect(work.createTask).toHaveBeenCalledTimes(2);
});

it('drops an old scope response and waits until the new closed scope is expanded', async () => {
  const old = held<AssigneeView[]>();
  vi.mocked(work.assignable).mockReturnValueOnce(old.promise);
  const view = mount();
  await toggle();
  await toggle();
  await view.rerender({ itemId: 'item-2' });
  old.reject(new NetworkError('old scope failed'));
  await tick();
  expect(work.assignable).toHaveBeenCalledOnce();
  await toggle();
  await waitFor(() => {
    expect(work.assignable).toHaveBeenLastCalledWith('item-2');
  });
  expect(screen.queryByRole('button', { name: '다시 시도' })).toBeNull();
  expect(screen.getByLabelText(/한 줄에 한 업무/)).toHaveValue('');
});

it('does not start a hidden old-account read', async () => {
  mount();
  me.userId = 'next-account';
  await tick();
  expect(screen.queryByRole('button', { name: '여러 업무 한꺼번에 등록' })).toBeNull();
  expect(work.assignable).not.toHaveBeenCalled();
});
