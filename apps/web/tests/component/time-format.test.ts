import { beforeEach, describe, expect, it } from 'vitest';

import type { TimeEntryView } from '$lib/api/client';
import { clockText, durationText, liveSeconds } from '$lib/features/time/format';
import { setLocale } from '$lib/paraglide/runtime.js';

const entry = (over: Partial<TimeEntryView>): TimeEntryView => ({
  id: 'e',
  started_at: '2026-10-17T00:00:00Z',
  ended_at: null,
  paused_at: null,
  paused_seconds: 0,
  seconds: 100,
  kind: 'work',
  memo: '',
  target: null,
  version: 1,
  ...over,
});

describe('time display (D-32)', () => {
  beforeEach(async () => {
    await setLocale('ko', { reload: false });
  });

  it('formats a stopwatch and totals', () => {
    expect(clockText(3725)).toBe('1:02:05');
    expect(durationText(59)).toBe('0분');
    expect(durationText(3600)).toBe('1시간');
    expect(durationText(3600 + 5 * 60)).toBe('1시간 5분');
  });

  it('keeps counting only while running', () => {
    const fetched = 1_000_000;
    expect(liveSeconds(entry({}), fetched, fetched + 5_500)).toBe(105);
    expect(
      liveSeconds(entry({ paused_at: '2026-10-17T00:01:00Z' }), fetched, fetched + 5_500),
    ).toBe(100);
    expect(liveSeconds(entry({ ended_at: '2026-10-17T00:02:00Z' }), fetched, fetched + 5_500)).toBe(
      100,
    );
  });
});
