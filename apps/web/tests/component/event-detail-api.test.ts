import './fixtures/event-user';
import { afterEach, expect, it, vi } from 'vitest';
import { eventsApi } from '$lib/features/events/api';
import { DiscardedTaskRead } from '$lib/features/tasks/context-read';
import { readSessionGeneration } from '$lib/features/auth/session-signal';
import { me } from '$lib/me.svelte';
import { recurring } from '../../stories/time/fixtures';

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
function response(viewer: string, archived: unknown = false) {
  return new Response(JSON.stringify({ ...recurring, archived }), {
    status: 200,
    headers: { 'Content-Type': 'application/json', 'x-fvoci-viewer-id': viewer },
  });
}
it('takes viewer and archived proof from the existing single event read', async () => {
  const fetch = vi.fn().mockResolvedValue(response(me.userId, true));
  vi.stubGlobal('fetch', fetch);
  expect(await eventsApi.event(recurring.id)).toMatchObject({
    viewer_id: me.userId,
    archived: true,
  });
  expect(fetch).toHaveBeenCalledOnce();
});
it('does not publish a single event returned for another authenticated viewer', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response('different-user')));
  await expect(eventsApi.event(recurring.id)).rejects.toMatchObject({ code: 'account_changed' });
});
it('rejects incomplete archived metadata', async () => {
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response(me.userId, null)));
  await expect(eventsApi.event(recurring.id)).rejects.toBeInstanceOf(TypeError);
});
it.each([200, 500])('discards a late %s read after an A→B→A session epoch', async (status) => {
  let finish!: (value: Response) => void;
  vi.stubGlobal(
    'fetch',
    vi.fn(
      () =>
        new Promise<Response>((resolve) => {
          finish = resolve;
        }),
    ),
  );
  const pending = eventsApi.event(recurring.id);
  const assertion = expect(pending).rejects.toBeInstanceOf(DiscardedTaskRead);
  for (const sequence of ['B', 'A'])
    localStorage.setItem(
      'fvoci:auth-session',
      JSON.stringify({ state: 'settled', generation: readSessionGeneration() + 1, sequence }),
    );
  finish(status === 200 ? response(me.userId) : new Response('{}', { status }));
  await assertion;
});
