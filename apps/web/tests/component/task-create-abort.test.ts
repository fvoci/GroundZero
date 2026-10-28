import { workApi as work } from '$lib/features/tasks/api';
import { afterEach, expect, it, vi } from 'vitest';

import { connection } from '$lib/features/feedback/feedback.svelte';

afterEach(() => vi.unstubAllGlobals());
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
  const creating = work.createTask(
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
