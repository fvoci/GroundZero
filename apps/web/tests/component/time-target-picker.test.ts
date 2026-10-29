import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { api, type ReferenceCandidateView } from '$lib/api/client';
import TargetPicker from '$lib/features/time/TargetPicker.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';

const task: ReferenceCandidateView = {
  kind: 'task',
  id: 'task-1',
  title: '분석',
  reference: 'TIM-2',
  workspace_id: 'ws-1',
  workspace_name: '연구',
};
beforeEach(async () => {
  await setLocale('ko', { reload: false });
  vi.useFakeTimers();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});
async function search(value: string): Promise<void> {
  await fireEvent.input(screen.getByLabelText(m.time_target_search()), { target: { value } });
  await vi.advanceTimersByTimeAsync(200);
}
function deferred() {
  let resolve!: (value: ReferenceCandidateView[]) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<ReferenceCandidateView[]>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

it('uses common candidates, hides users and preserves the displayed target when picked', async () => {
  const request = vi
    .spyOn(api, 'referenceCandidates')
    .mockResolvedValue([task, { ...task, kind: 'user', id: 'user-1', title: '사람' }]);
  const onpick = vi.fn();
  render(TargetPicker, { idPrefix: 'time', onpick });
  await search('  분석  ');
  expect(request).toHaveBeenCalledWith('분석', null);
  expect(screen.queryByRole('button', { name: /사람/ })).not.toBeInTheDocument();
  await fireEvent.click(screen.getByRole('button', { name: 'TIM-2 분석 · 연구' }));
  expect(onpick).toHaveBeenCalledWith({ kind: 'task', id: 'task-1', label: 'TIM-2 분석 · 연구' });
  expect(screen.getByLabelText(m.time_target_search())).toHaveValue('');
  expect(screen.queryByRole('list')).not.toBeInTheDocument();
});

it('does not restore old candidates after the search is cleared', async () => {
  const pending = deferred();
  vi.spyOn(api, 'referenceCandidates').mockReturnValue(pending.promise);
  render(TargetPicker, { idPrefix: 'time', onpick: vi.fn() });
  await search('분석');
  await search('');
  pending.resolve([task]);
  await vi.advanceTimersByTimeAsync(0);
  expect(screen.queryByRole('list')).not.toBeInTheDocument();
});

it('does not let an older rejected search clear the latest candidates', async () => {
  const old = deferred();
  vi.spyOn(api, 'referenceCandidates')
    .mockReturnValueOnce(old.promise)
    .mockResolvedValueOnce([task]);
  render(TargetPicker, { idPrefix: 'time', onpick: vi.fn() });
  await search('예전');
  await search('분석');
  expect(screen.getByRole('button', { name: 'TIM-2 분석 · 연구' })).toBeInTheDocument();
  old.reject(new Error('older search failed'));
  await vi.advanceTimersByTimeAsync(0);
  expect(screen.getByRole('button', { name: 'TIM-2 분석 · 연구' })).toBeInTheDocument();
});
