import { afterEach, describe, expect, it, vi } from 'vitest';

import type { TimeEntryView } from '$lib/api/client';
import { api } from '$lib/api/client';
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
});
