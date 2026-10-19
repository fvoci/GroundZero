import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TimeEntryView } from '$lib/api/client';
import { api, ApiError } from '$lib/api/client';
import { timer } from '$lib/features/time/timer.svelte';

afterEach(() => {
  vi.restoreAllMocks();
});

const running: TimeEntryView = {
  id: 't1',
  started_at: '2026-10-19T00:00:00Z',
  ended_at: null,
  paused_at: null,
  paused_seconds: 0,
  seconds: 10,
  kind: 'work',
  memo: '',
  target: null,
  version: 1,
};

// 탭을 다시 볼 때 읽은 옛 응답이 그 뒤의 멈춤을 되돌리지 않는다(제품 로직 점검 8번 불변식).
describe('timer', () => {
  it('ignores a stale read that lands after a stop', async () => {
    vi.spyOn(api, 'currentTimer').mockResolvedValueOnce(running);
    await timer.load();
    expect(timer.entry?.id).toBe('t1');

    let resolveRead: (entry: TimeEntryView | null) => void = () => undefined;
    vi.spyOn(api, 'currentTimer').mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveRead = resolve;
        }),
    );
    vi.spyOn(api, 'timerAction').mockResolvedValue({
      ...running,
      ended_at: '2026-10-19T00:01:00Z',
    });
    const reading = timer.load();
    await timer.act('stop');
    expect(timer.entry).toBeNull();
    resolveRead(running);
    await reading;
    expect(timer.entry).toBeNull();
  });

  it('does not let a read started during a start hide the started timer', async () => {
    let resolveStart: (entry: TimeEntryView) => void = () => undefined;
    vi.spyOn(api, 'startTimer').mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveStart = resolve;
        }),
    );
    // 시작 응답 전에 탭을 다시 봐서 읽었고, 서버는 그 읽기에 먼저 "없음"으로 답했다.
    vi.spyOn(api, 'currentTimer').mockResolvedValueOnce(null);
    const starting = timer.start('work');
    await timer.load();
    resolveStart({ ...running, id: 't2' });
    await starting;
    expect(timer.entry?.id).toBe('t2');
  });

  // 시작을 두 번 눌러 먼저 보낸 것이 늦게 성공하고 뒤의 것이 409로 먼저 실패하면, 서버에서는 타이머가 돈다. 앞의 성공을
  // 버린 채 비워 두지 않고 서버 상태로 맞춘다.
  it('shows the running timer when a later start fails while an earlier one succeeds', async () => {
    vi.spyOn(api, 'currentTimer').mockResolvedValue(null);
    await timer.load();
    let resolveFirst: (entry: TimeEntryView) => void = () => undefined;
    vi.spyOn(api, 'startTimer')
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            resolveFirst = resolve;
          }),
      )
      .mockRejectedValueOnce(new ApiError(409, 'conflict', 'already running'));
    const first = timer.start('work');
    await expect(timer.start('work')).rejects.toThrow();
    vi.spyOn(api, 'currentTimer').mockResolvedValue({ ...running, id: 't3' });
    resolveFirst({ ...running, id: 't3' });
    await first;
    expect(timer.entry?.id).toBe('t3');
  });

  // 옛 읽기의 실패가 그 뒤에 성공한 읽기의 상태 위에 오류를 남기지 않는다.
  it('does not show the error of a read that was overtaken', async () => {
    let rejectOld: (e: Error) => void = () => undefined;
    vi.spyOn(api, 'currentTimer')
      .mockImplementationOnce(
        () =>
          new Promise((_, reject) => {
            rejectOld = reject;
          }),
      )
      .mockResolvedValueOnce(running);
    const old = timer.load();
    await timer.load();
    rejectOld(new Error('network'));
    await old;
    expect(timer.error).toBeUndefined();
    expect(timer.entry?.id).toBe('t1');
  });
});
