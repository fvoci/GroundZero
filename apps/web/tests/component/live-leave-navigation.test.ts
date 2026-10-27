import { cleanup, fireEvent, render, screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { WebsocketProvider } from 'y-websocket';
import * as Y from 'yjs';

import type { DocumentView } from '$lib/api/client';
import { markAccountSwitch } from '$lib/features/feedback/feedback.svelte';
import type { LiveSession } from '$lib/features/documents/live';
import LiveDocumentEditor from '$lib/features/documents/LiveDocumentEditor.svelte';
import { me } from '$lib/me.svelte';
import { m } from '$lib/paraglide/messages.js';
import { setLocale } from '$lib/paraglide/runtime.js';
import { router } from '$lib/router.svelte';
import { scopedKey } from '$lib/storage';

// D-27 개정 2026-10-27(#141 R8 A): 실시간 문서도 서버에 반영되지 않은(확인 응답 전) 편집이 있으면 앱 안 이동과 창 닫기를 다른 폼과
// 같은 규칙으로 경고한다. 모두 반영됐으면 묻지 않는다. 떠나도 반영되지 않은 편집은 초안으로 남는다.

/** 편집기가 연 연결과, 서버의 저장 확인을 대신 보낼 콜백. */
const opened: { session: LiveSession; persisted: ((snapshot: Y.Snapshot) => void)[] }[] = [];

vi.mock('$lib/features/documents/TiptapEditor.svelte', async () => ({
  default: (await import('./stubs/EditorStub.svelte')).default,
}));

vi.mock('$lib/features/documents/live', async (original) => {
  const real = await original<typeof import('$lib/features/documents/live')>();
  return {
    ...real,
    openLive: (_id: string, user: LiveSession['user']): LiveSession => {
      const document = new Y.Doc();
      const persisted: ((snapshot: Y.Snapshot) => void)[] = [];
      const session: LiveSession = {
        document,
        provider: new WebsocketProvider('ws://localhost', 'collab', document, { connect: false }),
        user,
        persisted: {
          on: (handler: (snapshot: Y.Snapshot) => void) => {
            persisted.push(handler);
          },
          off: () => undefined,
        },
      };
      opened.push({ session, persisted });
      return session;
    },
  };
});

const doc: DocumentView = {
  id: 'd1',
  workspace_id: 'ws-1',
  placement: 'wiki',
  item_id: null,
  parent_id: null,
  reference: 'WIKI-1',
  title: '문서',
  body: { type: 'doc', content: [{ type: 'paragraph' }] },
  schema_version: 1,
  revision: 3,
  updated_at: '2026-10-19T00:00:00Z',
  access: 'write',
  can_edit: true,
  visibility: 'workspace',
  allow_copy: false,
  can_manage: false,
  can_delete: false,
  is_template: false,
  realtime: true,
  can_export: true,
};

const drafts = (): string[] =>
  Object.keys(window.localStorage).filter((k) =>
    k.startsWith(scopedKey('draft:d1', 'user-a', undefined)),
  );

function unload(): boolean {
  const event = new Event('beforeunload', { cancelable: true });
  window.dispatchEvent(event);
  return event.defaultPrevented;
}

/** 서버가 받았다고 확인하지 않은 본문 편집. */
async function editUnconfirmed(): Promise<{ document: Y.Doc; confirm: () => void }> {
  await fireEvent.click(screen.getByRole('button', { name: 'stub-type' }));
  const last = opened.at(-1);
  if (last === undefined) throw new Error('no connection');
  last.session.document.getText('t').insert(0, '확인받지 못한 문장');
  return {
    document: last.session.document,
    confirm: () => {
      const snapshot = Y.snapshot(last.session.document);
      for (const handler of last.persisted) handler(snapshot);
    },
  };
}

beforeEach(async () => {
  await setLocale('ko', { reload: false });
  me.userId = 'user-a';
  window.history.replaceState(null, '', '/w/ws-1/d/d1');
  router.path = '/w/ws-1/d/d1';
  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  opened.length = 0;
  router.guard = undefined;
  router.navigate('/');
  window.localStorage.clear();
  me.userId = '';
});

describe('leaving a live document with edits the server has not confirmed', () => {
  it('asks before an in-app move, stays on keep, and leaves with the draft kept', async () => {
    render(LiveDocumentEditor, { props: { initial: doc } });
    await editUnconfirmed();

    router.navigate('/notes');
    const box = await screen.findByRole('alertdialog');
    expect(box).toHaveTextContent(m.documents_live_leave_description());
    expect(router.path).toBe('/w/ws-1/d/d1');

    await fireEvent.click(within(box).getByRole('button', { name: m.common_discard_keep() }));
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(router.path).toBe('/w/ws-1/d/d1');

    router.navigate('/notes');
    const again = await screen.findByRole('alertdialog');
    await fireEvent.click(within(again).getByRole('button', { name: m.documents_live_leave() }));
    expect(router.path).toBe('/notes');
    expect(drafts().length).toBeGreaterThan(0);
  });

  it('warns on closing the window like other forms', async () => {
    render(LiveDocumentEditor, { props: { initial: doc } });
    await editUnconfirmed();
    expect(unload()).toBe(true);
    expect(drafts().length).toBeGreaterThan(0);
  });

  it('does not ask once the server confirmed every edit', async () => {
    render(LiveDocumentEditor, { props: { initial: doc } });
    const { confirm } = await editUnconfirmed();
    confirm();
    expect(unload()).toBe(false);
    router.navigate('/notes');
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(router.path).toBe('/notes');
  });

  it('does not block an account switch reload when the draft was kept (D-96)', async () => {
    render(LiveDocumentEditor, { props: { initial: doc } });
    await editUnconfirmed();
    markAccountSwitch();
    expect(unload()).toBe(false);
    expect(drafts().length).toBeGreaterThan(0);
  });
});
