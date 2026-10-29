// 본문 보존 시험은 실제 Y.Doc을 고치되 소켓은 열지 않는다. 서버 확인은 시험이 명시적으로 보낸다.
import { afterEach, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';
import type { LiveSession } from '$lib/features/documents/live';

export const sessions: LiveSession[] = [];
vi.mock('$lib/features/documents/TiptapEditor.svelte', async () => ({
  default: (await import('../stubs/EditorStub.svelte')).default,
}));
vi.mock('$lib/features/documents/live', async (original) => ({
  ...(await original<typeof import('$lib/features/documents/live')>()),
  openLive: (_id: string, user: LiveSession['user']): LiveSession => {
    const document = new Y.Doc();
    const session: LiveSession = {
      document,
      provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
      user,
      persisted: { on: () => undefined, off: () => undefined },
    };
    sessions.push(session);
    queueMicrotask(() => session.provider.emit('sync', [true]));
    return session;
  },
}));
afterEach(() => {
  sessions.length = 0;
});

export function editUnconfirmedBody(): void {
  const session = sessions.at(-1);
  if (session === undefined) throw new Error('no live session');
  session.document.getText('test-input').insert(0, '확인받지 못한 문장');
}
