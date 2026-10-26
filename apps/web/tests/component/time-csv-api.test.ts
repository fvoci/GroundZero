import { afterEach, expect, it, vi } from 'vitest';
import { api } from '$lib/api/client';
import { connection } from '$lib/features/feedback/feedback.svelte';
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
        new Response(JSON.stringify({ ...running, stopped_entry_id: 'old-entry' })),
      ),
  );
  expect(await api.taskTime('task-1')).toEqual({ mine_seconds: 30, estimate_minutes: null });
  expect(await api.itemTime('item-1')).toEqual({
    estimate_minutes: 10,
    kinds: [{ kind: 'work', mine_seconds: 30 }],
  });
  expect(
    (await api.startTimer('request-1', 'work', '', { kind: 'task', id: 'task-1' }))
      .stopped_entry_id,
  ).toBe('old-entry');
});
it('passes cancellation to fetch without reporting an offline connection', async () => {
  connection.lost = false;
  const controller = new AbortController();
  let received: AbortSignal | null | undefined;
  vi.stubGlobal(
    'fetch',
    vi.fn((_url: string, init?: RequestInit) => {
      received = init?.signal;
      return new Promise<Response>((_resolve, reject) =>
        init?.signal?.addEventListener('abort', () => {
          reject(new DOMException('Aborted', 'AbortError'));
        }),
      );
    }),
  );
  const creating = api.createTask(
    'user-a',
    'item-1',
    'request-1',
    '제목',
    null,
    undefined,
    controller.signal,
  );
  controller.abort();
  await expect(creating).rejects.toThrow('Aborted');
  expect(received).toBe(controller.signal);
  expect(connection.lost).toBe(false);
});
