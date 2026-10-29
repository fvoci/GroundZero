import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';
import type { LiveSession } from '$lib/features/documents/live';
import { saveLiveCopy } from '$lib/features/documents/live-copy';
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
  session.provider.emit('sync', [true]);
  expect(session.document.getXmlFragment('default').toString()).toContain('보존할 초안');
  const empty = new Y.Doc();
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

it('rejects an account refusal and closes the copy session without confirming the draft', async () => {
  const copy = saveLiveCopy('note', body, 'original-user');
  const current = opened.at(-1);
  if (current === undefined) throw new Error('missing session');
  const close = vi.spyOn(current.session.provider, 'destroy');
  current.session.provider.emit('connection-close', [new CloseEvent('close', { code: 4412 })]);
  await expect(copy).rejects.toThrow('refused');
  expect(close).toHaveBeenCalledOnce();
  expect(current.listeners.size).toBe(0);
});
