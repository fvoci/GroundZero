import { timeApi as timeData } from '$lib/features/time/api';
import { afterEach, expect, it, vi } from 'vitest';

import { running } from '../../stories/time/fixtures';

afterEach(() => vi.unstubAllGlobals());
it('parses the actual personal-only time response and stopped entry id', async () => {
  vi.stubGlobal(
    'fetch',
    vi
      .fn()
      .mockResolvedValueOnce(
        new Response(JSON.stringify({ mine_seconds: 30, estimate_minutes: null })),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({ estimate_minutes: 10, kinds: [{ kind: 'work', mine_seconds: 30 }] }),
        ),
      )
      .mockResolvedValueOnce(
        new Response(
          JSON.stringify({
            ...running,
            target: { kind: 'task', id: 'task-1' },
            stopped_entry_id: 'old-entry',
          }),
        ),
      ),
  );
  expect(await timeData.taskTime('task-1')).toEqual({ mine_seconds: 30, estimate_minutes: null });
  expect(await timeData.itemTime('item-1')).toEqual({
    estimate_minutes: 10,
    kinds: [{ kind: 'work', mine_seconds: 30 }],
  });
  expect(
    (await timeData.startTimer('request-1', 'work', '', { kind: 'task', id: 'task-1' }))
      .stopped_entry_id,
  ).toBe('old-entry');
});
