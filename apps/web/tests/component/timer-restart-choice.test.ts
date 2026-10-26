import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TimeEntryView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import { timer } from '$lib/features/time/timer.svelte';
import TimerRestartDialog from '$lib/features/time/TimerRestartDialog.svelte';
import { markAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import { me } from '$lib/me.svelte';
import { setLocale } from '$lib/paraglide/runtime.js';

const running: TimeEntryView & { stopped_entry_id: string | null } = {
  id: 'running',
  started_at: '2026-10-26T00:00:00Z',
  ended_at: null,
  paused_at: null,
  paused_seconds: 0,
  seconds: 10,
  kind: 'work',
  memo: '',
  target: null,
  version: 1,
  stopped_entry_id: null,
};

afterEach(async () => {
  timer.chooseRestart(undefined);
  cleanup();
  vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
  await timer.load();
  vi.restoreAllMocks();
  me.userId = '';
  vi.unstubAllGlobals();
});

describe('same-target timer restart choice', () => {
  it('waits for an explicit choice and reuses the request ID for continue', async () => {
    me.userId = 'user-a';
    const start = vi
      .spyOn(api, 'startTimer')
      .mockRejectedValueOnce(new ApiError(409, 'timer_running', 'already running'))
      .mockResolvedValue(running);
    const pending = timer.start('work').catch((error: unknown) => error);
    await waitFor(() => {
      expect(timer.restartPending).toBe(true);
    });
    expect(start).toHaveBeenCalledTimes(1);
    timer.chooseRestart('continue');
    await pending;
    expect(start).toHaveBeenCalledTimes(2);
    expect(start.mock.calls[1]).toEqual([start.mock.calls[0]?.[0], 'work', '', null, 'continue']);
    expect(timer.entry?.id).toBe('running');
  });

  it('focuses Continue by default and sends New entry only when selected', async () => {
    await setLocale('ko', { reload: false });
    me.userId = 'user-a';
    const start = vi
      .spyOn(api, 'startTimer')
      .mockRejectedValueOnce(new ApiError(409, 'timer_running', 'already running'))
      .mockResolvedValue({ ...running, id: 'new-entry' });
    render(TimerRestartDialog);
    const pending = timer.start('work', null, 'memo');
    const keep = await screen.findByRole('button', { name: '이어가기' });
    await waitFor(() => {
      expect(keep).toHaveFocus();
    });
    expect(start).toHaveBeenCalledTimes(1);
    await fireEvent.click(screen.getByRole('button', { name: '새 기록' }));
    await pending;
    expect(start.mock.calls[1]?.slice(1)).toEqual(['work', 'memo', null, 'new']);
    expect(timer.entry?.id).toBe('new-entry');
  });

  it('cancels without sending another write', async () => {
    const start = vi
      .spyOn(api, 'startTimer')
      .mockRejectedValueOnce(new ApiError(409, 'timer_running', 'already running'));
    const pending = timer.start('work');
    await waitFor(() => {
      expect(timer.restartPending).toBe(true);
    });
    timer.chooseRestart(undefined);
    await pending;
    expect(start).toHaveBeenCalledTimes(1);
  });

  it('dismisses the pending choice when an account switch begins', async () => {
    me.userId = 'user-a';
    const start = vi
      .spyOn(api, 'startTimer')
      .mockRejectedValueOnce(new ApiError(409, 'timer_running', 'already running'));
    render(TimerRestartDialog);
    const pending = timer.start('work');
    await waitFor(() => {
      expect(timer.restartPending).toBe(true);
    });
    markAccountSwitch();
    await pending;
    expect(timer.restartPending).toBe(false);
    expect(start).toHaveBeenCalledTimes(1);
  });

  it('cancels an older choice when another target starts', async () => {
    const start = vi
      .spyOn(api, 'startTimer')
      .mockRejectedValueOnce(new ApiError(409, 'timer_running', 'already running'))
      .mockResolvedValue({ ...running, id: 'other-target' });
    const first = timer.start('work');
    await waitFor(() => {
      expect(timer.restartPending).toBe(true);
    });
    await timer.start('study', { kind: 'study_unit', id: 'unit-1' });
    await first;
    expect(timer.restartPending).toBe(false);
    expect(start).toHaveBeenCalledTimes(2);
    expect(timer.entry?.id).toBe('other-target');
  });

  it('serializes the chosen mode in the HTTP request body', async () => {
    const fetch = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(running), {
        status: 201,
        headers: { 'Content-Type': 'application/json' },
      }),
    );
    vi.stubGlobal('fetch', fetch);
    await api.startTimer('request-1', 'work', '', null, 'continue');
    expect(fetch).toHaveBeenCalledWith(
      '/api/me/timer',
      expect.objectContaining({
        body: JSON.stringify({
          request_id: 'request-1',
          kind: 'work',
          memo: '',
          target: null,
          mode: 'continue',
        }),
      }),
    );
  });
});
