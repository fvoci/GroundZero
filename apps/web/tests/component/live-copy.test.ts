import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { getSchema } from '@tiptap/core';
import { prosemirrorJSONToYXmlFragment } from '@tiptap/y-tiptap';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';
import type { LiveSession } from '$lib/features/documents/live';
import { saveLiveCopy } from '$lib/features/documents/live-copy';
import { schemaExtensions } from '$lib/features/documents/editor-schema';
import { setLocale } from '$lib/paraglide/runtime.js';

const opened: { session: LiveSession; listeners: Set<(snapshot: Y.Snapshot) => void> }[] = [];
vi.mock('$lib/features/documents/live', async (original) => ({
  ...(await original<typeof import('$lib/features/documents/live')>()),
  openLive: (_id: string, user: LiveSession['user']): LiveSession => {
    const document = new Y.Doc();
    const listeners = new Set<(snapshot: Y.Snapshot) => void>();
    const session: LiveSession = {
      document,
      provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
      user,
      persisted: {
        on: (listener) => {
          listeners.add(listener);
        },
        off: (listener) => {
          listeners.delete(listener);
        },
      },
    };
    opened.push({ session, listeners });
    return session;
  },
}));
beforeEach(async () => {
  await setLocale('ko', { reload: false });
});
afterEach(() => {
  opened.length = 0;
  vi.restoreAllMocks();
  vi.useRealTimers();
});
const body = {
  type: 'doc',
  content: [{ type: 'paragraph', content: [{ type: 'text', text: '보존할 초안' }] }],
};

it('waits for synchronization and an acknowledgement that contains the copied body', async () => {
  let confirmed = false;
  const copy = saveLiveCopy('note', body, 'original-user').then(() => {
    confirmed = true;
  });
  const current = opened.at(-1);
  if (current === undefined) throw new Error('missing session');
  const { session, listeners } = current;
  const close = vi.spyOn(session.provider, 'destroy');
  expect(session.document.getXmlFragment('default').length).toBe(0);
  const empty = new Y.Doc();
  for (const listener of listeners) listener(Y.snapshot(empty));
  session.provider.emit('sync', [true]);
  expect(session.document.getXmlFragment('default').toJSON()).toContain('보존할 초안');
  for (const listener of listeners) listener(Y.snapshot(empty));
  empty.destroy();
  await Promise.resolve();
  expect(confirmed).toBe(false);
  for (const listener of listeners) listener(Y.snapshot(session.document));
  await copy;
  expect(confirmed).toBe(true);
  expect(close).toHaveBeenCalledOnce();
  expect(listeners.size).toBe(0);
});

it.each([
  { name: 'an empty draft', draft: { type: 'doc', content: [] } },
  { name: 'a retry with the same body', draft: body },
])('accepts the pre-sync persisted snapshot for $name', async ({ draft }) => {
  vi.useFakeTimers();
  const outcome = saveLiveCopy('note', draft, 'original-user').then(
    () => 'confirmed',
    (error: unknown) => (error instanceof Error ? error.message : 'failed'),
  );
  const current = opened.at(-1);
  if (current === undefined) throw new Error('missing session');
  const { session, listeners } = current;
  const close = vi.spyOn(session.provider, 'destroy');
  prosemirrorJSONToYXmlFragment(
    getSchema(schemaExtensions({ collaborative: true })),
    draft,
    session.document.getXmlFragment('default'),
  );
  const persisted = Y.encodeStateAsUpdate(session.document);
  for (const listener of listeners) listener(Y.snapshot(session.document));
  session.provider.emit('sync', [true]);
  expect(Y.encodeStateAsUpdate(session.document)).toEqual(persisted);
  await vi.advanceTimersByTimeAsync(20_000);
  expect(await outcome).toBe('confirmed');
  expect(close).toHaveBeenCalledOnce();
  expect(listeners.size).toBe(0);
});

it('rejects an account refusal and closes the copy session without confirming the draft', async () => {
  const copy = saveLiveCopy('note', body, 'original-user');
  const current = opened.at(-1);
  if (current === undefined) throw new Error('missing session');
  const close = vi.spyOn(current.session.provider, 'destroy');
  current.session.provider.emit('connection-close', [
    new CloseEvent('close', { code: 4412 }),
    current.session.provider,
  ]);
  await expect(copy).rejects.toThrow('refused');
  expect(close).toHaveBeenCalledOnce();
  expect(current.listeners.size).toBe(0);
});
